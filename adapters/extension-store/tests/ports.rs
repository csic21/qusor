#![cfg(target_os = "linux")]
use extension_api::{Capability, HOST_API, Manifest};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed, publisher_key, verify,
};
use extension_store::{
    GrantScope, GrantValidator, PrivateStateExecutor, Result, Store, StoreErrorCode, StoreServices,
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-install-ports-{}-{}",
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
fn package(version: u32, schema: u32) -> VerifiedPackage {
    let manifest:Manifest=Manifest::from_json(&format!(r#"{{"manifest_version":1,"package_id":"org.test.persist","publisher_id":"org.test","package_version":{{"major":{version},"minor":0,"patch":0}},"host_api":{{"major":0,"min_minor":1,"max_minor":1}},"required_capabilities":["document.metadata.read"],"optional_capabilities":[],"contributions":[{{"id":"org.test.persist.metadata","title":"Metadata","kind":"command","required_capabilities":["document.metadata.read"]}}]}}"#)).unwrap();
    let mut files = vec![PackageFile {
        path: "module.wasm".into(),
        mime: "application/wasm".into(),
        bytes: b"\0asm\x01\0\0\0".to_vec(),
    }];
    let lifecycle = if version == 1 {
        format!(
            r#"{{"protocol":1,"private_schema":{schema},"migration_module":null,"accepted_from_schemas":[]}}"#
        )
    } else {
        files.push(PackageFile {
            path: "migration.wasm".into(),
            mime: "application/wasm".into(),
            bytes: b"\0asm\x01\0\0\0".to_vec(),
        });
        format!(
            r#"{{"protocol":1,"private_schema":{schema},"migration_module":"migration.wasm","accepted_from_schemas":[1]}}"#
        )
    };
    files.push(PackageFile {
        path: "assets/lifecycle.json".into(),
        mime: "application/json".into(),
        bytes: lifecycle.into_bytes(),
    });
    let seed = [11; 32];
    let bytes = build_signed(manifest, "module.wasm", files, &seed).unwrap();
    verify(&bytes, &publisher_key(&seed)).unwrap()
}
fn scope() -> GrantScope {
    GrantScope {
        capability: Capability::DocumentMetadataRead,
        library: "vault".into(),
        document: "doc".into(),
        version: "version".into(),
        expires_at: u64::MAX,
    }
}
#[test]
fn unavailable_services_fail_closed_without_committing_intent_or_state() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0.join("store")).unwrap();
    let package = package(1, 1);
    let receipt = store.install("install", 0, &package).unwrap();
    assert_eq!(
        store
            .initialize_private("initialize", 1, &receipt.package_id, b"state")
            .unwrap_err()
            .code(),
        StoreErrorCode::External
    );
    // A package without private state cannot even reach grant validation. Check both failures preserve revision.
    assert_eq!(store.current(&receipt.package_id).unwrap().revision, 1);
    let seed = [11; 32];
    let plain_bytes = build_signed(
        package.manifest().clone(),
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: b"\0asm\x01\0\0\0".to_vec(),
        }],
        &seed,
    )
    .unwrap();
    let plain = verify(&plain_bytes, &publisher_key(&seed)).unwrap();
    let mut other = Store::open(&scratch.0.join("plain")).unwrap();
    other.install("install", 0, &plain).unwrap();
    assert_eq!(
        other
            .grant("grant", 1, &plain.manifest().package_id, scope())
            .unwrap_err()
            .code(),
        StoreErrorCode::External
    );
    assert_eq!(
        other
            .current(&plain.manifest().package_id)
            .unwrap()
            .revision,
        1
    );
    let db = rusqlite::Connection::open(scratch.0.join("plain/state.sqlite3")).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM grants", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(store.worker_leases().unwrap().len(), 0);
    assert!(
        store
            .private_status(&receipt.package_id)
            .unwrap()
            .current
            .is_none()
    );
}
struct Services {
    database: PathBuf,
    calls: Mutex<Vec<String>>,
}
impl Services {
    fn outside_transaction(&self) {
        let db = rusqlite::Connection::open(&self.database).unwrap();
        db.busy_timeout(std::time::Duration::ZERO).unwrap();
        db.execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
            .expect("executor must never run inside a SQL write transaction");
    }
}
impl GrantValidator for Services {
    fn validate(&self, _: &VerifiedPackage, _: &GrantScope) -> Result<()> {
        self.outside_transaction();
        self.calls.lock().unwrap().push("grant".into());
        Ok(())
    }
}
impl PrivateStateExecutor for Services {
    fn preflight(&self, _: &[u8], schema: u32, _: &[u8]) -> Result<()> {
        self.outside_transaction();
        self.calls
            .lock()
            .unwrap()
            .push(format!("preflight-{schema}"));
        Ok(())
    }
    fn migrate(&self, _: &[u8], from: u32, to: u32, state: &[u8]) -> Result<Vec<u8>> {
        self.outside_transaction();
        assert_eq!((from, to), (1, 2));
        let db = rusqlite::Connection::open(&self.database).unwrap();
        let pending: i64 = db
            .query_row("SELECT count(*) FROM private_pending", [], |r| r.get(0))
            .unwrap();
        let grants: i64 = db
            .query_row("SELECT count(*) FROM grants", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 1, "prepare must already be durable");
        assert_eq!(grants, 0, "old grant must be revoked before execution");
        self.calls.lock().unwrap().push("migrate".into());
        Ok([state, b"-migrated"].concat())
    }
}
#[test]
fn migration_runs_after_durable_revocation_outside_sql_transaction() {
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let services = Arc::new(Services {
        database: path.join("state.sqlite3"),
        calls: Mutex::new(Vec::new()),
    });
    let mut store = Store::open_with_services(
        &path,
        &[HOST_API],
        StoreServices {
            grant_validator: services.clone(),
            private_executor: services.clone(),
            ..StoreServices::default()
        },
    )
    .unwrap();
    let first = package(1, 1);
    let second = package(2, 2);
    store.install("install", 0, &first).unwrap();
    let initialized = store
        .initialize_private("initialize", 1, &first.manifest().package_id, b"state")
        .unwrap();
    let granted = store
        .grant(
            "grant",
            initialized.installation.revision,
            &first.manifest().package_id,
            scope(),
        )
        .unwrap();
    let migrated = store
        .upgrade_private("upgrade", granted.revision, &second)
        .unwrap();
    assert_eq!(migrated.outcome, "succeeded");
    assert_eq!(migrated.installation.digest, second.digest());
    assert!(
        store
            .execution_snapshot(&first.manifest().package_id, None)
            .is_err(),
        "no implicit grant after migration"
    );
    assert_eq!(
        services.calls.lock().unwrap().as_slice(),
        [
            "preflight-1",
            "preflight-1",
            "grant",
            "migrate",
            "preflight-2"
        ]
    );
    assert_eq!(
        store
            .private_status(&first.manifest().package_id)
            .unwrap()
            .current
            .unwrap()
            .state_hex,
        "73746174652d6d69677261746564"
    );
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened
            .current(&first.manifest().package_id)
            .unwrap()
            .digest,
        second.digest()
    );
}

#[test]
fn current_authority_rechecks_binding_without_executing_private_guest() {
    use extension_ports::{AuthorityRead, AuthoritySource};
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let services = Arc::new(Services {
        database: path.join("state.sqlite3"),
        calls: Mutex::new(Vec::new()),
    });
    let mut store = Store::open_with_services(
        &path,
        &[HOST_API],
        StoreServices {
            grant_validator: services.clone(),
            private_executor: services.clone(),
            ..StoreServices::default()
        },
    )
    .unwrap();
    let package = package(1, 1);
    let id = &package.manifest().package_id;
    store.install("install", 0, &package).unwrap();
    let initialized = store
        .initialize_private("initialize", 1, id, b"state")
        .unwrap();
    let granted = store
        .grant("grant", initialized.installation.revision, id, scope())
        .unwrap();
    services.calls.lock().unwrap().clear();
    let activated = store
        .authority_snapshot(id, AuthorityRead::Activation)
        .unwrap();
    assert_eq!(services.calls.lock().unwrap().as_slice(), ["preflight-1"]);
    let current = store
        .authority_snapshot(id, AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(current, activated);
    assert_eq!(
        current.grants[0].resource_version.as_deref(),
        Some("version")
    );
    assert_eq!(
        services.calls.lock().unwrap().as_slice(),
        ["preflight-1"],
        "per-message gate must not execute private guest code"
    );
    let db = rusqlite::Connection::open(&services.database).unwrap();
    db.execute("UPDATE trust SET publisher='org.attacker'", [])
        .unwrap();
    assert!(
        store
            .authority_snapshot(id, AuthorityRead::Current { run_id: None })
            .is_err()
    );
    db.execute("UPDATE trust SET publisher='org.test'", [])
        .unwrap();
    let original: Vec<u8> = db
        .query_row(
            "SELECT bytes FROM content WHERE digest=?1",
            [package.digest()],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE content SET bytes=zeroblob(length(bytes)) WHERE digest=?1",
        [package.digest()],
    )
    .unwrap();
    assert!(
        store
            .authority_snapshot(id, AuthorityRead::Current { run_id: None })
            .is_err(),
        "a previous verified read must not hide current package byte corruption"
    );
    db.execute(
        "UPDATE content SET bytes=?1 WHERE digest=?2",
        rusqlite::params![original, package.digest()],
    )
    .unwrap();
    store
        .authority_snapshot(id, AuthorityRead::Current { run_id: None })
        .unwrap();
    store
        .stop("revoke", granted.revision, id, "revoke")
        .unwrap();
    assert!(
        store
            .authority_snapshot(id, AuthorityRead::Current { run_id: None })
            .is_err()
    );
}
