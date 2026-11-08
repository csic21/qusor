use extension_api::{ApiRange, Capability, Manifest, PackageVersion};
use qusor_capture_probe::host::CaptureHost;
use qusor_package_probe::{
    package::{PackageFile, build_signed, publisher_key, verify},
    store::Store,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Barrier, mpsc},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture() -> Fixture {
    fixture_with_private(false)
}
fn fixture_with_private(private: bool) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "capture-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let request=serde_json::json!({"capture_protocol":1,"request_id":"guest-1","draft":{"protocol_version":1,"source":{"namespace":"org.source.test","reference":"article-1","title":"Article","public_uri":null,"external_return":"none"},"snapshot_text":"hello world","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":5,"exact":"hello"}}}).to_string();
    let escaped = request
        .bytes()
        .map(|v| format!("\\{v:02x}"))
        .collect::<String>();
    let private_exports = if private {
        r#"(func(export "alloc")(param i32)(result i32)i32.const 1024)(func(export "qusor_abi_version")(result i32)i32.const 1)(func(export "qusor_state_check")(param i32 i32 i32)(result i32)local.get 0 i32.const 1 i32.ne)"#
    } else {
        ""
    };
    let wasm=wat::parse_str(format!(r#"(module(import "qusor" "call"(func $call(param i32 i32 i32 i32)(result i32)))(memory(export "memory") 2 2){private_exports}(data(i32.const 0)"{escaped}")(func(export "run")(result i32 i32)i32.const 0 i32.const {} i32.const 65536 i32.const 4096 call $call drop i32.const 65536 i32.const 0))"#,request.len())).unwrap();
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.capture".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        required_capabilities: vec![],
        optional_capabilities: vec![Capability::CapturePropose],
        contributions: vec![],
    };
    let mut files = vec![PackageFile {
        path: "module.wasm".into(),
        mime: "application/wasm".into(),
        bytes: wasm,
    }];
    if private {
        files.push(PackageFile{path:"assets/lifecycle.json".into(),mime:"application/json".into(),bytes:serde_json::to_vec(&serde_json::json!({"protocol":1,"private_schema":1,"migration_module":null,"accepted_from_schemas":[]})).unwrap()});
    }
    let signed = build_signed(manifest, "module.wasm", files, &[3; 32]).unwrap();
    let package = verify(&signed, &publisher_key(&[3; 32])).unwrap();
    Store::open(&root.join("packages"))
        .unwrap()
        .install("install", 0, &package, None)
        .unwrap();
    if private {
        Store::open(&root.join("packages"))
            .unwrap()
            .initialize_private("init", 1, "org.test.capture", b"theme=dark", None)
            .unwrap();
    }
    Fixture(root)
}
fn open(f: &Fixture) -> CaptureHost {
    CaptureHost::open(
        &f.0.join("packages"),
        &f.0.join("core"),
        "vault-a",
        "org.test.capture",
    )
    .unwrap()
}
#[test]
fn explicit_grant_review_and_epoch_are_required() {
    let f = fixture();
    let h = open(&f);
    assert!(h.propose().is_err());
    h.grant().unwrap();
    let p = h.propose().unwrap();
    assert!(h.approve(p, "wrong", "op", "note").is_err());
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let approval = h.approve(p, &hash, "op", "note").unwrap();
    h.revoke().unwrap();
    assert!(h.accept(approval).is_err());
    assert!(h.receipt("op").unwrap().is_none());
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let approval = h.approve(p, &hash, "op", "note").unwrap();
    let receipt = h.accept(approval).unwrap();
    assert_eq!(receipt.draft_hash, hash);
}
#[test]
fn reboot_does_not_restore_native_approval() {
    let f = fixture();
    let h = open(&f);
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "op", "note").unwrap();
    drop(h);
    let h = open(&f);
    h.grant().unwrap();
    assert!(h.accept(a).is_err());
    assert!(h.receipt("op").unwrap().is_none());
}
#[test]
fn revoke_first_and_commit_first_have_serial_gate_order() {
    let f = fixture();
    let h = open(&f);
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "revoke-first", "").unwrap();
    let barrier = Arc::new(Barrier::new(2)); // Run actual concurrent order with a notification after revoke acquired the gate.
    let other = h.clone();
    let b = barrier.clone();
    let t = std::thread::spawn(move || {
        other.revoke().unwrap();
        b.wait();
    });
    barrier.wait();
    assert!(h.accept(a).is_err());
    t.join().unwrap();
    assert!(h.receipt("revoke-first").unwrap().is_none());
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "commit-first", "").unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (revoked_tx, revoked_rx) = mpsc::channel();
    let commit = h.clone();
    let t = std::thread::spawn(move || {
        commit
            .accept_observed(a, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .map(|r| r.capture_id)
            .unwrap()
    });
    entered_rx.recv().unwrap();
    let revoke = h.clone();
    let r = std::thread::spawn(move || {
        revoke.revoke().unwrap();
        revoked_tx.send(()).unwrap();
    });
    assert!(revoked_rx.recv_timeout(Duration::from_millis(30)).is_err());
    release_tx.send(()).unwrap();
    let id = t.join().unwrap();
    r.join().unwrap();
    assert_eq!(h.receipt("commit-first").unwrap().unwrap().capture_id, id);
}

#[test]
fn native_approval_is_bound_to_origin_vault_and_host_instance() {
    let f = fixture();
    let g = fixture();
    let first = open(&f);
    let second = CaptureHost::open(
        &g.0.join("packages"),
        &g.0.join("core"),
        "vault-b",
        "org.test.capture",
    )
    .unwrap();
    first.grant().unwrap();
    second.grant().unwrap();
    let proposal = first.propose().unwrap();
    let hash = proposal.hash().to_owned();
    let approval = first.approve(proposal, &hash, "cross-host", "").unwrap();
    assert!(second.accept(approval).is_err());
    assert!(second.receipt("cross-host").unwrap().is_none());
    // Holding the trusted package writer lease excludes concurrent uninstall/update.
    assert!(Store::open(&f.0.join("packages")).is_err());
}

#[test]
fn saved_knowledge_survives_same_installation_upgrade_and_private_rollback() {
    use qusor_capture_probe::core::Core;
    let f = fixture_with_private(true);
    let host = open(&f);
    host.grant().unwrap();
    let proposal = host.propose().unwrap();
    let hash = proposal.hash().to_owned();
    let approval = host
        .approve(
            proposal,
            &hash,
            "durable-before-upgrade",
            "my separate viewpoint",
        )
        .unwrap();
    let receipt = host.accept(approval).unwrap();
    let installation_id = host.provenance().installation_id;
    drop(host);
    let core = Core::open(f.0.join("core")).unwrap();
    let original = core.query(receipt.capture_id).unwrap();
    let exported = core.export().unwrap();
    drop(core);
    let mut store = Store::open(&f.0.join("packages")).unwrap();
    let (before, package) = store.frozen_package_for_action("org.test.capture").unwrap();
    assert_eq!(before.installation_id, installation_id);
    let original_digest = package.digest().to_owned();
    let mut manifest = package.manifest().clone();
    manifest.package_version.major = 2;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../extension-package/fixtures");
    let files=vec![PackageFile{path:"module.wasm".into(),mime:"application/wasm".into(),bytes:wat::parse_file(fixtures.join("private-run-v2.wat")).unwrap()},PackageFile{path:"migration.wasm".into(),mime:"application/wasm".into(),bytes:wat::parse_file(fixtures.join("private-migrate-good.wat")).unwrap()},PackageFile{path:"assets/lifecycle.json".into(),mime:"application/json".into(),bytes:serde_json::to_vec(&serde_json::json!({"protocol":1,"private_schema":2,"migration_module":"migration.wasm","accepted_from_schemas":[1]})).unwrap()}];
    let signed = build_signed(manifest, "module.wasm", files, &[3; 32]).unwrap();
    let v2 = verify(&signed, &publisher_key(&[3; 32])).unwrap();
    let upgraded = store
        .upgrade_private("real-upgrade", before.revision, &v2, None)
        .unwrap();
    assert_eq!(upgraded.outcome, "succeeded");
    assert_eq!(upgraded.installation.installation_id, installation_id);
    let state = store.private_status("org.test.capture").unwrap();
    assert_eq!(state.current.as_ref().unwrap().schema, 2);
    assert_eq!(
        state.current.as_ref().unwrap().state_hex,
        "7468656d653d6461726b0a6c61796f75743d77696465"
    );
    {
        let core = Core::open(f.0.join("core")).unwrap();
        assert_eq!(core.query(receipt.capture_id).unwrap(), original);
        assert_eq!(core.export().unwrap(), exported);
    }
    let rolled = store
        .rollback_private(
            "real-rollback",
            upgraded.installation.revision,
            "org.test.capture",
            state.retained[0].pair_id,
            None,
        )
        .unwrap();
    assert_eq!(rolled.outcome, "succeeded");
    assert_eq!(rolled.installation.installation_id, installation_id);
    let head = store
        .private_status("org.test.capture")
        .unwrap()
        .current
        .unwrap();
    assert_eq!(head.package_digest, original_digest);
    assert_eq!(head.schema, 1);
    assert_eq!(head.state_hex, "7468656d653d6461726b");
    drop(store);
    let core = Core::open(f.0.join("core")).unwrap();
    assert_eq!(
        core.receipt("durable-before-upgrade").unwrap().unwrap(),
        receipt
    );
    assert_eq!(core.query(receipt.capture_id).unwrap(), original);
    assert_eq!(core.export().unwrap(), exported);
    assert_eq!(
        core.resolve(
            receipt.capture_id,
            &receipt.snapshot_hash,
            "qusor-raw-text-v1",
            "unicode_scalar"
        )
        .unwrap()
        .text,
        "hello"
    );
}
