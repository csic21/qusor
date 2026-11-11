#![cfg(target_os = "linux")]
use super::*;
use crate::{Receipt, Store};
use extension_api::{HOST_API, HOST_API_V0_3, Manifest, composition::*};
use extension_host::durable::{AuthorityRead, ComponentAuthoritySource};
use extension_package::package::{
    PackageFile, build_signed_for_host, publisher_key, verify_for_host,
};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-verified-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn package(name: &str, label: &str) -> VerifiedPackage {
    let id = format!("ordinary.{name}");
    let manifest: Manifest=serde_json::from_value(serde_json::json!({"manifest_version":1,"package_id":id,"publisher_id":"ordinary.author","package_version":{"major":1,"minor":0,"patch":0},"host_api":{"major":0,"min_minor":3,"max_minor":3},"required_capabilities":["note.draft.edit"],"optional_capabilities":[],"contributions":[{"id":format!("{id}.input"),"title":"Editor","kind":"component_provider","required_capabilities":["note.draft.edit"]}]})).unwrap();
    let descriptor = CompositionDescriptor {
        composition_protocol: 1,
        api_version: HOST_API_V0_3,
        contribution_id: format!("{id}.input"),
        role: ComponentRole::AnnotationPlainTextEditor,
        contract: ANNOTATION_EDITOR_CONTRACT.into(),
        view: NativeEditorView::TextEditor {
            label: label.into(),
            placeholder: "Write".into(),
        },
    };
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![
            PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: b"\0asm\x01\0\0\0".to_vec(),
            },
            PackageFile {
                path: "assets/ui-composition.json".into(),
                mime: "application/json".into(),
                bytes: descriptor.to_json().unwrap().into_bytes(),
            },
        ],
        &[44; 32],
        &[HOST_API_V0_3],
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&[44; 32]), &[HOST_API_V0_3]).unwrap()
}
fn installed(root: &Scratch) -> (Store, Receipt, VerifiedPackage, Connection) {
    let p = package("cache-a", "alpha");
    let path = root.0.join("store");
    let mut s = Store::open_for_host(&path, &[HOST_API_V0_3]).unwrap();
    let item = s.install("install", 0, &p).unwrap();
    let item = s
        .bind_component_vault("bind", item.revision, &item.package_id, "vault")
        .unwrap();
    let db = Connection::open(path.join("state.sqlite3")).unwrap();
    (s, item, p, db)
}
#[test]
fn exact_success_reuses_arc_but_every_binding_is_fresh_sql() {
    let scratch = Scratch::new();
    let (s, item, p, db) = installed(&scratch);
    let first = s.current_package_shared(&item).unwrap();
    let again = s.current_package_shared(&item).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    let read =
        || s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None });
    read().unwrap();
    db.execute("UPDATE trust SET publisher='ordinary.attacker'", [])
        .unwrap();
    assert!(read().is_err());
    db.execute("UPDATE trust SET publisher='ordinary.author'", [])
        .unwrap();
    db.execute("UPDATE trust SET key=?1", [publisher_key(&[45; 32])])
        .unwrap();
    assert!(read().is_err());
    db.execute("UPDATE trust SET key=?1", [publisher_key(&[44; 32])])
        .unwrap();
    db.execute(
        "UPDATE content SET bytes=zeroblob(length(bytes)) WHERE digest=?1",
        [&item.digest],
    )
    .unwrap();
    assert!(read().is_err());
    db.execute(
        "UPDATE content SET bytes=?1 WHERE digest=?2",
        params![p.raw_bytes(), item.digest],
    )
    .unwrap();
    assert!(read().is_ok());
    assert!(Arc::ptr_eq(
        &first,
        &s.current_package_shared(&item).unwrap()
    ));
    assert_eq!(s.verified_cache.borrow().entries.len(), 1);
}
#[test]
fn same_length_valid_signed_substitute_never_uses_the_old_receipt() {
    let scratch = Scratch::new();
    let (s, item, p, db) = installed(&scratch);
    s.current_package_shared(&item).unwrap();
    let alternate = package("cache-a", "bravo");
    assert_eq!(p.raw_bytes().len(), alternate.raw_bytes().len());
    assert_ne!(p.digest(), alternate.digest());
    db.execute(
        "UPDATE content SET bytes=?1 WHERE digest=?2",
        params![alternate.raw_bytes(), item.digest],
    )
    .unwrap();
    assert!(s.current_package_shared(&item).is_err());
    assert!(s.current_package(&item).is_err());
    assert!(
        s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
            .is_err()
    );
    assert_eq!(
        s.verified_cache.borrow().entries.len(),
        1,
        "a binding failure must not insert a success"
    );
}
#[test]
fn supported_api_full_vector_and_verifier_policy_are_cache_key_material() {
    let scratch = Scratch::new();
    let (mut s, item, _, _) = installed(&scratch);
    let first = s.current_package_shared(&item).unwrap();
    s.supported_apis = vec![HOST_API];
    assert!(s.current_package_shared(&item).is_err());
    s.supported_apis = vec![HOST_API_V0_3, HOST_API];
    let reordered = s.current_package_shared(&item).unwrap();
    assert!(!Arc::ptr_eq(&first, &reordered));
    s.supported_apis = vec![HOST_API, HOST_API_V0_3];
    let reordered2 = s.current_package_shared(&item).unwrap();
    assert!(!Arc::ptr_eq(&reordered, &reordered2));
    let mut cache = VerifiedCache::default();
    cache.insert(first.clone(), &[HOST_API_V0_3]);
    cache.entries[0].verifier_policy = VERIFIER_POLICY + 1;
    assert!(
        cache
            .get(first.raw_bytes(), first.publisher_key(), &[HOST_API_V0_3])
            .is_none()
    );
}
#[test]
fn bounded_lru_evicts_oldest_and_open_has_no_restored_verifications() {
    let a = Arc::new(package("cache-a", "alpha"));
    let b = Arc::new(package("cache-b", "bravo"));
    let c = Arc::new(package("cache-c", "charm"));
    let mut cache = VerifiedCache::default();
    cache.insert(a.clone(), &[HOST_API_V0_3]);
    cache.insert(b.clone(), &[HOST_API_V0_3]);
    cache
        .get(a.raw_bytes(), a.publisher_key(), &[HOST_API_V0_3])
        .unwrap();
    cache.insert(c.clone(), &[HOST_API_V0_3]);
    assert_eq!(cache.entries.len(), 2);
    assert!(cache.retained_bytes <= MAX_RETAINED_BYTES);
    assert_eq!(
        cache.retained_bytes,
        cache.entries.iter().map(|e| e.charge).sum::<usize>()
    );
    assert!(
        cache
            .get(b.raw_bytes(), b.publisher_key(), &[HOST_API_V0_3])
            .is_none()
    );
    assert!(
        cache
            .get(a.raw_bytes(), a.publisher_key(), &[HOST_API_V0_3])
            .is_some()
    );
    assert!(
        cache
            .get(c.raw_bytes(), c.publisher_key(), &[HOST_API_V0_3])
            .is_some()
    );
    let scratch = Scratch::new();
    let (s, item, _, _db) = installed(&scratch);
    s.current_package_shared(&item).unwrap();
    assert_eq!(s.verified_cache.borrow().entries.len(), 1);
    drop(s);
    let reopened = Store::open_for_host(&scratch.0.join("store"), &[HOST_API_V0_3]).unwrap();
    assert!(reopened.verified_cache.borrow().entries.is_empty());
    reopened
        .component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(reopened.verified_cache.borrow().entries.len(), 1);
}
#[test]
fn warm_authentication_does_not_memoize_installation_generation_or_removal() {
    let scratch = Scratch::new();
    let (mut s, item, _, _) = installed(&scratch);
    let before = s
        .component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
        .unwrap();
    let next = s
        .stop("stop", item.revision, &item.package_id, "disable")
        .unwrap();
    let after = s
        .component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
        .unwrap();
    assert!(after.generation > before.generation);
    s.uninstall("remove", next.revision, &item.package_id)
        .unwrap();
    assert!(
        s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
            .is_err()
    );
}

#[test]
fn api_support_input_cannot_create_an_unbounded_retained_vector() {
    use extension_api::{SUPPORTED_HOST_APIS, validate_supported_apis};
    // The implemented set is a published contract: every version it names is accepted, and
    // the budget keeps that list bounded as versions are added. Reaudit `METADATA_ALLOWANCE`
    // below when adding an API version; never let this list grow without a wire schema.
    assert!(!SUPPORTED_HOST_APIS.is_empty() && SUPPORTED_HOST_APIS.len() <= 8);
    validate_supported_apis(&SUPPORTED_HOST_APIS).unwrap();
    assert!(validate_supported_apis(&[HOST_API_V0_3; 1024]).is_err());
    assert!(
        validate_supported_apis(&[ApiVersion {
            major: 0,
            minor: u16::MAX
        }])
        .is_err()
    );
    // A successful configuration has unique elements from the implemented set.
    // The retained Vec is at most `SUPPORTED_HOST_APIS.len()` * size_of(ApiVersion),
    // within the charged metadata allowance. Reaudit alongside limits on every new version.
    assert!(SUPPORTED_HOST_APIS.len() * std::mem::size_of::<ApiVersion>() < METADATA_ALLOWANCE);
}

struct PreflightCounter(Arc<AtomicU64>);
impl crate::PrivateStateExecutor for PreflightCounter {
    fn preflight(&self, _: &[u8], schema: u32, _: &[u8]) -> crate::Result<()> {
        assert_eq!(schema, 1);
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn migrate(&self, _: &[u8], _: u32, _: u32, _: &[u8]) -> crate::Result<Vec<u8>> {
        panic!("cache freshness test never requests migration")
    }
}
fn private_package() -> VerifiedPackage {
    let plain = package("private-cache", "alpha");
    let (mime, composition) = plain.file("assets/ui-composition.json").unwrap();
    let files = vec![
        PackageFile { path: "module.wasm".into(), mime: "application/wasm".into(), bytes: plain.module_bytes().to_vec() },
        PackageFile { path: "assets/ui-composition.json".into(), mime: mime.into(), bytes: composition.to_vec() },
        PackageFile { path: "assets/lifecycle.json".into(), mime: "application/json".into(), bytes: br#"{"protocol":1,"private_schema":1,"migration_module":null,"accepted_from_schemas":[]}"#.to_vec() },
    ];
    let bytes = build_signed_for_host(
        plain.manifest().clone(),
        "module.wasm",
        files,
        &[44; 32],
        &[HOST_API_V0_3],
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&[44; 32]), &[HOST_API_V0_3]).unwrap()
}

#[test]
fn warm_component_rechecks_private_pair_and_pending_without_current_vm_execution() {
    use sha2::{Digest, Sha256};
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let count = Arc::new(AtomicU64::new(0));
    // Trusted test preflight instrumentation, not a Wasm/runtime qualification.
    let mut s = Store::open_with_services(
        &path,
        &[HOST_API_V0_3],
        crate::StoreServices {
            private_executor: Arc::new(PreflightCounter(count.clone())),
            ..crate::StoreServices::default()
        },
    )
    .unwrap();
    let p = private_package();
    let installed = s.install("install", 0, &p).unwrap();
    let initialized = s
        .initialize_private(
            "initialize",
            installed.revision,
            &installed.package_id,
            b"state",
        )
        .unwrap();
    let item = s
        .bind_component_vault(
            "bind",
            initialized.installation.revision,
            &installed.package_id,
            "vault",
        )
        .unwrap();
    let warm = s.current_package_shared(&item).unwrap();
    let read =
        || s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None });
    read().unwrap();
    assert!(Arc::ptr_eq(
        &warm,
        &s.current_package_shared(&item).unwrap()
    ));
    let preflights = count.load(Ordering::Relaxed);
    let db = Connection::open(path.join("state.sqlite3")).unwrap();
    db.execute(
        "UPDATE private_snapshots SET schema=2 WHERE id=?1",
        [initialized.snapshot_id],
    )
    .unwrap();
    assert!(
        read().is_err(),
        "warm authentication cannot conceal private schema drift"
    );
    db.execute(
        "UPDATE private_snapshots SET schema=1 WHERE id=?1",
        [initialized.snapshot_id],
    )
    .unwrap();
    db.execute(
        "UPDATE private_snapshots SET bytes=?1 WHERE id=?2",
        params![b"State", initialized.snapshot_id],
    )
    .unwrap();
    assert!(
        read().is_err(),
        "warm authentication cannot conceal private byte/hash drift"
    );
    db.execute(
        "UPDATE private_snapshots SET bytes=?1,state_digest=?2 WHERE id=?3",
        params![
            b"state",
            format!("{:x}", Sha256::digest(b"state")),
            initialized.snapshot_id
        ],
    )
    .unwrap();
    read().unwrap();
    db.execute(
        "INSERT INTO private_pending VALUES (?1,'pending','malformed-pending-record')",
        [item.installation_id],
    )
    .unwrap();
    assert!(
        read().is_err(),
        "pending existence must reject before parsing or using cached authentication"
    );
    db.execute(
        "DELETE FROM private_pending WHERE installation_id=?1",
        [item.installation_id],
    )
    .unwrap();
    read().unwrap();
    assert_eq!(
        count.load(Ordering::Relaxed),
        preflights,
        "Current never executes the private guest, even when authentication is warm"
    );
}

#[test]
fn warm_component_checks_unbound_and_wrong_worker_runs_before_reusing_authentication() {
    use extension_runtime_api::ProcessIdentity;
    let scratch = Scratch::new();
    let (mut s, item, _, _) = installed(&scratch);
    let cached = s.current_package_shared(&item).unwrap();
    s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
        .unwrap();
    let owner = ProcessIdentity {
        pid: 1,
        boot_id: "00000000-0000-0000-0000-000000000001".into(),
        start_time_ticks: 1,
        uid: 1,
        exe_device: 1,
        exe_inode: 1,
        run_id: String::new(),
    };
    let boot = "a".repeat(32);
    let run = format!("{boot}-1");
    // Durable lease fixture only; no actual OS identity/reap assertion is made.
    s.prepare_worker_lease(&item, &boot, &run, &owner).unwrap();
    assert!(
        s.component_installation(&item.package_id, AuthorityRead::Current { run_id: None })
            .is_err()
    );
    assert!(
        s.component_installation(
            &item.package_id,
            AuthorityRead::Current { run_id: Some(&run) }
        )
        .is_err(),
        "prepared/unbound own run is insufficient"
    );
    s.mark_worker_spawn_attempted(&run).unwrap();
    assert!(
        s.component_installation(
            &item.package_id,
            AuthorityRead::Current { run_id: Some(&run) }
        )
        .is_err()
    );
    let worker = ProcessIdentity {
        pid: 2,
        start_time_ticks: 2,
        run_id: run.clone(),
        ..owner
    };
    s.bind_worker_lease(&run, worker).unwrap();
    assert!(
        s.component_installation(
            &item.package_id,
            AuthorityRead::Current {
                run_id: Some("wrong-run")
            }
        )
        .is_err()
    );
    assert!(
        s.component_installation(&item.package_id, AuthorityRead::Activation)
            .is_err()
    );
    s.component_installation(
        &item.package_id,
        AuthorityRead::Current { run_id: Some(&run) },
    )
    .unwrap();
    assert!(
        Arc::ptr_eq(&cached, &s.current_package_shared(&item).unwrap()),
        "correct bound run still uses the same immutable verified material"
    );
}
