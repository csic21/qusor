use extension_api::{ApiRange, Capability, Manifest, PackageVersion};
use qusor_package_probe::{
    package::{PackageFile, VerifiedPackage, build_signed, publisher_key, verify},
    store::{GrantScope, Store},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-private-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> PathBuf {
        self.0.join("store")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap()
    }
}
fn package(version: u32, migration: &str, run: &str) -> VerifiedPackage {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.private".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: version,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        required_capabilities: vec![],
        optional_capabilities: vec![Capability::DocumentMetadataRead],
        contributions: vec![],
    };
    let descriptor = serde_json::json!({"protocol":1,"private_schema":version,"migration_module":if version==1 {None}else{Some("migration.wasm")},"accepted_from_schemas":if version==1 {vec![]}else{vec![1]}});
    let mut files = vec![
        PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wat::parse_file(fixtures.join(run)).unwrap(),
        },
        PackageFile {
            path: "assets/lifecycle.json".into(),
            mime: "application/json".into(),
            bytes: serde_json::to_vec(&descriptor).unwrap(),
        },
    ];
    if version > 1 {
        files.push(PackageFile {
            path: "migration.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wat::parse_file(fixtures.join(migration)).unwrap(),
        })
    }
    let bytes = build_signed(manifest, "module.wasm", files, &[7; 32]).unwrap();
    verify(&bytes, &publisher_key(&[7; 32])).unwrap()
}
fn v1() -> VerifiedPackage {
    package(1, "private-migrate-good.wat", "private-run-v1.wat")
}
fn v2() -> VerifiedPackage {
    package(2, "private-migrate-good.wat", "private-run-v2.wat")
}
fn scope() -> GrantScope {
    GrantScope {
        capability: Capability::DocumentMetadataRead,
        library: "library".into(),
        document: "document".into(),
        version: "v1".into(),
        expires_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
    }
}
#[test]
fn true_migration_and_rollback_keep_matching_package_and_original_bytes() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    let initial = store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    assert_eq!(initial.outcome, "succeeded");
    let grant = store
        .grant("grant", 2, "org.test.private", scope(), None)
        .unwrap();
    let token = store
        .resume("org.test.private", "library", "document", "v1")
        .unwrap();
    assert!(
        store
            .install("bypass", grant.revision, &v2(), None)
            .is_err()
    );
    let upgraded = store
        .upgrade_private("upgrade", grant.revision, &v2(), None)
        .unwrap();
    assert_eq!(upgraded.outcome, "succeeded");
    assert_eq!(upgraded.installation.revision, grant.revision + 2);
    assert!(
        store
            .check_session(&token, "library", "document", "v1")
            .is_err()
    );
    assert!(
        store
            .resume("org.test.private", "library", "document", "v1")
            .is_err()
    );
    let state = store.private_status("org.test.private").unwrap();
    let head = state.current.unwrap();
    assert_eq!(head.package_digest, v2().digest());
    assert_eq!(head.schema, 2);
    assert_eq!(
        head.state_hex,
        "7468656d653d6461726b0a6c61796f75743d77696465"
    );
    assert_eq!(state.retained.len(), 1);
    let rolled = store
        .rollback_private(
            "rollback",
            upgraded.installation.revision,
            "org.test.private",
            state.retained[0].pair_id,
            None,
        )
        .unwrap();
    assert_eq!(rolled.outcome, "succeeded");
    assert!(rolled.installation.generation > upgraded.installation.generation);
    let head = store
        .private_status("org.test.private")
        .unwrap()
        .current
        .unwrap();
    assert_eq!(head.package_digest, v1().digest());
    assert_eq!(head.state_hex, "7468656d653d6461726b");
    assert_eq!(
        store
            .upgrade_private("upgrade", grant.revision, &v2(), None)
            .unwrap(),
        upgraded
    );
    assert!(store.install("upgrade", 0, &v1(), None).is_err());
    assert!(
        store
            .initialize_private("install", 0, "org.test.private", b"theme=dark", None)
            .is_err()
    );
    drop(store);
    let store = Store::open(&temp.store()).unwrap();
    assert_eq!(
        store
            .private_status("org.test.private")
            .unwrap()
            .current
            .unwrap()
            .snapshot_id,
        initial.snapshot_id
    );
}
#[test]
fn migration_failures_preserve_old_pair_but_revoke_authority() {
    for (migration, run) in [
        ("private-migrate-trap.wat", "private-run-v2.wat"),
        ("private-migrate-loop.wat", "private-run-v2.wat"),
        ("private-migrate-bad-pointer.wat", "private-run-v2.wat"),
        ("private-migrate-oversize.wat", "private-run-v2.wat"),
        ("private-migrate-import.wat", "private-run-v2.wat"),
        ("private-migrate-wrong-schema.wat", "private-run-v2.wat"),
        ("private-migrate-good.wat", "private-run-bad-abi.wat"),
        ("private-migrate-good.wat", "private-run-host-call.wat"),
    ] {
        let temp = Scratch::new();
        let mut store = Store::open(&temp.store()).unwrap();
        store.install("install", 0, &v1(), None).unwrap();
        let initial = store
            .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
            .unwrap();
        let grant = store
            .grant("grant", 2, "org.test.private", scope(), None)
            .unwrap();
        let token = store
            .resume("org.test.private", "library", "document", "v1")
            .unwrap();
        let candidate = package(2, migration, run);
        let failed = store
            .upgrade_private("upgrade", grant.revision, &candidate, None)
            .unwrap();
        assert_eq!(failed.outcome, "failed", "{migration}/{run}");
        assert_eq!(failed.snapshot_id, initial.snapshot_id);
        assert_eq!(failed.installation.digest, v1().digest());
        assert!(failed.installation.generation > grant.generation);
        assert!(
            store
                .check_session(&token, "library", "document", "v1")
                .is_err()
        );
        assert!(
            store
                .resume("org.test.private", "library", "document", "v1")
                .is_err()
        );
        assert_eq!(
            store
                .upgrade_private("upgrade", grant.revision, &candidate, None)
                .unwrap(),
            failed
        );
        drop(store);
        assert_eq!(
            Store::open(&temp.store())
                .unwrap()
                .private_status("org.test.private")
                .unwrap()
                .current
                .unwrap()
                .snapshot_id,
            initial.snapshot_id
        );
    }
}
#[test]
fn initialization_requires_no_grant_and_reinstall_never_reuses_private_head() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    assert!(
        store
            .grant("early-grant", 1, "org.test.private", scope(), None)
            .is_err()
    );
    let old = store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    store
        .uninstall("remove", 2, "org.test.private", None)
        .unwrap();
    let new = store.install("reinstall", 0, &v1(), None).unwrap();
    assert_ne!(new.installation_id, old.installation.installation_id);
    assert!(
        store
            .private_status("org.test.private")
            .unwrap()
            .current
            .is_none()
    );
    assert_eq!(
        store
            .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
            .unwrap(),
        old
    );
    assert!(
        store
            .private_status("org.test.private")
            .unwrap()
            .current
            .is_none()
    );
    assert!(
        store
            .grant("grant", new.revision, "org.test.private", scope(), None)
            .is_err()
    );
}
#[test]
fn snapshot_corruption_missing_head_or_cross_binding_fail_closed() {
    for sql in [
        "UPDATE private_snapshots SET bytes=x'00'",
        "DELETE FROM private_heads",
        "UPDATE private_snapshots SET schema=2",
        "PRAGMA foreign_keys=OFF;UPDATE private_heads SET snapshot_id=999",
    ] {
        let temp = Scratch::new();
        let mut store = Store::open(&temp.store()).unwrap();
        store.install("install", 0, &v1(), None).unwrap();
        store
            .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
            .unwrap();
        drop(store);
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        assert!(Store::open(&temp.store()).is_err(), "{sql}");
    }
}
#[test]
fn regular_and_private_receipts_share_a_single_budget() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    drop(store);
    let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
    db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<8190) INSERT INTO operations SELECT 'budget-'||x,'hash','{}' FROM n;").unwrap();
    drop(db);
    let mut store = Store::open(&temp.store()).unwrap();
    assert!(
        store
            .stop("over", 2, "org.test.private", "disable", None)
            .is_err()
    );
    assert!(
        store
            .rollback_private("over", 2, "org.test.private", 1, None)
            .is_err()
    );
}

#[test]
fn aggregate_snapshot_budget_is_checked_when_reopening() {
    use sha2::{Digest, Sha256};
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    drop(store);
    let mut db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
    let tx = db.transaction().unwrap();
    let digest = format!("{:x}", Sha256::digest(vec![0; 65536]));
    tx.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1024) INSERT INTO private_snapshots(installation_id,digest,schema,state_digest,bytes) SELECT i.id,i.digest,1,?1,zeroblob(65536) FROM n,installations i",[digest]).unwrap();
    tx.execute("INSERT INTO retained_pairs(installation_id,snapshot_id) SELECT installation_id,id FROM private_snapshots WHERE id>1",[]).unwrap();
    tx.commit().unwrap();
    drop(db);
    assert!(
        Store::open(&temp.store())
            .err()
            .unwrap()
            .to_string()
            .contains("snapshot budget")
    );
}

#[test]
fn orphaned_retained_snapshot_and_overfull_operation_history_fail_closed() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    store.upgrade_private("upgrade", 2, &v2(), None).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
    db.execute("DELETE FROM retained_pairs", []).unwrap();
    drop(db);
    assert!(
        Store::open(&temp.store())
            .err()
            .unwrap()
            .to_string()
            .contains("orphaned private snapshot")
    );
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &v1(), None).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
    db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<8192) INSERT INTO operations SELECT 'over-'||x,'hash','{}' FROM n").unwrap();
    drop(db);
    assert!(
        Store::open(&temp.store())
            .err()
            .unwrap()
            .to_string()
            .contains("store budget")
    );
}

#[test]
fn schema_three_upgrade_preserves_private_pair_grant_and_historical_receipts() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    let installed = store.install("install", 0, &v1(), None).unwrap();
    let initialized = store
        .initialize_private("init", 1, "org.test.private", b"theme=dark", None)
        .unwrap();
    let explicit_grant = scope();
    let granted = store
        .grant("grant", 2, "org.test.private", explicit_grant.clone(), None)
        .unwrap();
    drop(store);
    {
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute_batch("DROP TABLE worker_leases; PRAGMA user_version=3;")
            .unwrap();
    }
    let mut store = Store::open(&temp.store()).unwrap();
    assert!(store.worker_leases().unwrap().is_empty());
    assert_eq!(
        store
            .private_status("org.test.private")
            .unwrap()
            .current
            .unwrap()
            .snapshot_id,
        initialized.snapshot_id
    );
    assert_eq!(store.install("install", 0, &v1(), None).unwrap(), installed);
    assert_eq!(
        store
            .grant("grant", 2, "org.test.private", explicit_grant, None)
            .unwrap(),
        granted
    );
    let session = store
        .resume("org.test.private", "library", "document", "v1")
        .unwrap();
    store
        .check_session(&session, "library", "document", "v1")
        .unwrap();
}
