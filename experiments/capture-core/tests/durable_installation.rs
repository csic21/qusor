//! Actual production journal + single Host + signed Wasm + retained Core metadata.
//! Controlled text fixture, not a native reader or process-supervision acceptance.
use extension_api::*;
use extension_host::{
    Grant,
    durable::{DurableHost, DurableSession, VersionedRequest},
};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_runtime_api::{ExecutionLimits, ModuleExecutor, RuntimeError};
use extension_store::{GrantScope, GrantValidator, Store, StoreError, StoreServices};
use qusor_capture_probe::{
    contract::CaptureDraft,
    core::{Core, Provenance},
};
use runtime_wasm::WasmiRuntime;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Policy(Arc<Mutex<DurableHost>>);
impl GrantValidator for Policy {
    fn validate(
        &self,
        package: &VerifiedPackage,
        scope: &GrantScope,
    ) -> extension_store::Result<()> {
        let api = package
            .manifest()
            .negotiate_api(&SUPPORTED_HOST_APIS)
            .ok_or_else(|| StoreError::invalid("no supported API"))?;
        self.0
            .lock()
            .unwrap()
            .validate_activation_intent(
                package.manifest(),
                package.digest(),
                &[Grant {
                    capability: scope.capability,
                    resource: scope.resource(),
                }],
                api,
            )
            .map_err(StoreError::from)
    }
}
fn authority() -> Arc<Mutex<DurableHost>> {
    Arc::new(Mutex::new(
        DurableHost::new("vault", [Capability::DocumentMetadataRead]).unwrap(),
    ))
}
fn services(host: &Arc<Mutex<DurableHost>>) -> StoreServices {
    StoreServices {
        grant_validator: Arc::new(Policy(host.clone())),
        ..StoreServices::default()
    }
}
fn request(api: ApiVersion) -> CapabilityRequest {
    CapabilityRequest {
        api_version: api,
        request_id: "read-metadata".into(),
        capability: Capability::DocumentMetadataRead,
        resource: Resource::Document {
            vault_id: "vault".into(),
            document_id: "1".into(),
        },
    }
}
fn package(id: &str, api: ApiVersion, patch: u32) -> VerifiedPackage {
    let request = request(api).to_json().unwrap();
    let data = request
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect::<String>();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "qusor" "call" (func $call(param i32 i32 i32 i32)(result i32)))
        (memory(export "memory")1 2)(data(i32.const 0)"{data}")
        (func(export "run")(result i32 i32)(local $n i32)
        i32.const 0 i32.const {} i32.const 4096 i32.const 4096 call $call local.set $n
        i32.const 4096 local.get $n))"#,
        request.len()
    ))
    .unwrap();
    let manifest = Manifest {
        manifest_version: 1,
        package_id: id.into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: api.minor,
            max_minor: api.minor,
        },
        required_capabilities: vec![Capability::DocumentMetadataRead],
        optional_capabilities: vec![],
        contributions: vec![],
    };
    let seed = [31; 32];
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &seed,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&seed), &SUPPORTED_HOST_APIS).unwrap()
}
fn run(
    host: &Arc<Mutex<DurableHost>>,
    store: &Store,
    core: &Core,
    session: &DurableSession,
    package: &VerifiedPackage,
    now: u64,
) -> extension_runtime_api::Result<String> {
    let mut broker = |bytes: &[u8]| {
        let request = CapabilityRequest::from_slice(bytes)
            .map_err(|_| RuntimeError::new("invalid_request"))?;
        // The test's exclusive thread is the shared gate: no journal/Core mutation
        // can occur between this fresh version lookup, policy dispatch and response.
        let record = core
            .query(1)
            .map_err(|_| RuntimeError::new("source_unavailable"))?;
        let current_version = record.receipt.snapshot_hash.clone();
        host.lock().unwrap().dispatch(store,session,now,None,
            VersionedRequest{request,resource_version:Some(current_version)},|_| {
                serde_json::to_vec(&serde_json::json!({"title":record.draft.source.title,
                    "document_version_id":record.receipt.document_version_id,"version":record.receipt.snapshot_hash,
                    "commit_rev":record.receipt.commit_rev}))
                    .map_err(|_|ApiError::new(ErrorCode::InvalidState,"metadata encoding failed"))
            }).map_err(|_|RuntimeError::new("permission_denied"))
    };
    WasmiRuntime
        .execute(
            package.module_bytes(),
            &ExecutionLimits::default(),
            MAX_REQUEST_BYTES,
            &mut broker,
        )
        .map(|r| r.output)
}

#[test]
fn two_signed_installations_restore_into_one_host_and_read_actual_core() {
    let path = std::env::temp_dir().join(format!(
        "qusor-durable-consumer-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut core = Core::open(scratch.0.join("knowledge")).unwrap();
    let draft:CaptureDraft=serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"local","reference":"controlled-text","title":"Retained local source","public_uri":null,"external_return":"none"},"snapshot_text":"Local immutable source","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":5,"exact":"Local"}})).unwrap();
    let receipt = core
        .commit(
            "source",
            "vault",
            &draft,
            &Provenance {
                package_id: "org.test.source".into(),
                publisher_id: "org.test".into(),
                package_digest: "a".repeat(64),
                installation_id: 1,
            },
            "",
            None,
        )
        .unwrap();
    let a = package("org.test.a", HOST_API, 0);
    let b = package("org.test.b", HOST_API_V0_2, 0);
    let host = authority();
    let data = scratch.0.join("packages");
    let mut journal =
        Store::open_with_services(&data, &SUPPORTED_HOST_APIS, services(&host)).unwrap();
    for p in [&a, &b] {
        journal
            .install(
                &format!("install-{}", p.manifest().package_id.replace('.', "-")),
                0,
                p,
            )
            .unwrap();
        assert!(
            host.lock()
                .unwrap()
                .activate(
                    &journal,
                    &p.manifest().package_id,
                    p.manifest().negotiate_api(&SUPPORTED_HOST_APIS).unwrap(),
                    now
                )
                .is_err(),
            "installation must not auto-grant"
        );
        journal
            .grant(
                &format!("grant-{}", p.manifest().package_id.replace('.', "-")),
                1,
                &p.manifest().package_id,
                GrantScope {
                    capability: Capability::DocumentMetadataRead,
                    library: "vault".into(),
                    document: "1".into(),
                    version: receipt.snapshot_hash.clone(),
                    expires_at: now + 3600,
                },
            )
            .unwrap();
    }
    let sa = host
        .lock()
        .unwrap()
        .activate(&journal, "org.test.a", HOST_API, now)
        .unwrap();
    let sb = host
        .lock()
        .unwrap()
        .activate(&journal, "org.test.b", HOST_API_V0_2, now)
        .unwrap();
    let first = run(&host, &journal, &core, &sa, &a, now).unwrap();
    assert_eq!(first, run(&host, &journal, &core, &sb, &b, now).unwrap());
    let metadata: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(metadata["title"], "Retained local source");
    assert_eq!(metadata["version"], receipt.snapshot_hash);
    assert!(metadata.get("synthetic").is_none());
    // A real foreign vault can use the same local installation id and package.
    // Its newer, valid uninstall proof must not even fence this active session.
    let foreign_host = Arc::new(Mutex::new(
        DurableHost::new("other-vault", [Capability::DocumentMetadataRead]).unwrap(),
    ));
    let mut foreign = Store::open_with_services(
        &scratch.0.join("foreign-packages"),
        &SUPPORTED_HOST_APIS,
        services(&foreign_host),
    )
    .unwrap();
    let foreign_install = foreign.install("install-a", 0, &a).unwrap();
    let local_id = journal.current("org.test.a").unwrap().installation_id;
    assert_eq!(foreign_install.installation_id, local_id);
    foreign
        .grant(
            "grant-a",
            1,
            "org.test.a",
            GrantScope {
                capability: Capability::DocumentMetadataRead,
                library: "other-vault".into(),
                document: "1".into(),
                version: receipt.snapshot_hash.clone(),
                expires_at: now + 3600,
            },
        )
        .unwrap();
    foreign.uninstall("uninstall-a", 2, "org.test.a").unwrap();
    assert!(
        host.lock()
            .unwrap()
            .retire_removed(&foreign, local_id, "org.test.a")
            .is_err()
    );
    assert_eq!(run(&host, &journal, &core, &sa, &a, now).unwrap(), first);
    drop(foreign);
    drop(foreign_host);
    journal.stop("revoke-a", 2, "org.test.a", "revoke").unwrap();
    assert!(run(&host, &journal, &core, &sa, &a, now).is_err());
    assert!(run(&host, &journal, &core, &sb, &b, now).is_ok());
    journal
        .grant(
            "grant-a-again",
            3,
            "org.test.a",
            GrantScope {
                capability: Capability::DocumentMetadataRead,
                library: "vault".into(),
                document: "1".into(),
                version: receipt.snapshot_hash.clone(),
                expires_at: now + 3600,
            },
        )
        .unwrap();
    let newer = host
        .lock()
        .unwrap()
        .activate(&journal, "org.test.a", HOST_API, now)
        .unwrap();
    assert!(run(&host, &journal, &core, &sa, &a, now).is_err());
    assert!(run(&host, &journal, &core, &newer, &a, now).is_ok());
    let update = package("org.test.a", HOST_API, 1);
    journal.install("update-a", 4, &update).unwrap();
    assert!(run(&host, &journal, &core, &newer, &a, now).is_err());
    assert!(
        host.lock()
            .unwrap()
            .activate(&journal, "org.test.a", HOST_API, now)
            .is_err()
    );
    drop(journal);
    drop(host);
    drop(core);
    let core = Core::open(scratch.0.join("knowledge")).unwrap();
    let restarted = authority();
    let mut journal =
        Store::open_with_services(&data, &SUPPORTED_HOST_APIS, services(&restarted)).unwrap();
    assert!(
        run(&restarted, &journal, &core, &sb, &b, now).is_err(),
        "old process session cannot revive"
    );
    let fresh = restarted
        .lock()
        .unwrap()
        .activate(&journal, "org.test.b", HOST_API_V0_2, now)
        .unwrap();
    assert_eq!(
        run(&restarted, &journal, &core, &fresh, &b, now).unwrap(),
        first
    );
    let removed_id = journal.current("org.test.b").unwrap().installation_id;
    journal.uninstall("uninstall-b", 2, "org.test.b").unwrap();
    restarted
        .lock()
        .unwrap()
        .retire_removed(&journal, removed_id, "org.test.b")
        .unwrap();
    assert!(run(&restarted, &journal, &core, &fresh, &b, now).is_err());
    assert_eq!(core.query(receipt.capture_id).unwrap().draft, draft);
    drop(journal);
    drop(restarted);
    drop(core);
    drop(scratch);
}
