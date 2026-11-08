use extension_api::{ApiRange, Manifest, PackageVersion};
use qusor_package_probe::{
    package::{PackageFile, build_signed, publisher_key, verify},
    store::{Store, read_package},
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
            "qusor-package-store-{}-{}",
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
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn package(version: u32, seed: u8) -> qusor_package_probe::package::VerifiedPackage {
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.plugin".into(),
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
        optional_capabilities: vec![extension_api::Capability::DocumentMetadataRead],
        contributions: vec![],
    };
    let bytes = build_signed(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: b"\0asm\x01\0\0\0".to_vec(),
        }],
        &[seed; 32],
    )
    .unwrap();
    verify(&bytes, &publisher_key(&[seed; 32])).unwrap()
}
#[test]
fn restart_update_replay_and_stale_revision() {
    let temp = Scratch::new();
    let first;
    {
        let mut store = Store::open(&temp.store()).unwrap();
        first = store.install("first", 0, &package(1, 7), None).unwrap();
        assert_eq!(first.status, "disabled");
        assert!(Store::open(&temp.store()).is_err(), "single writer lock");
    }
    let mut store = Store::open(&temp.store()).unwrap();
    assert_eq!(store.status().unwrap()[0].digest, first.digest);
    assert_eq!(
        store.install("first", 0, &package(1, 7), None).unwrap(),
        first
    );
    assert!(
        store.install("first", 0, &package(2, 7), None).is_err(),
        "operation conflict"
    );
    assert!(store.install("stale", 0, &package(2, 7), None).is_err());
    let second = store.install("second", 1, &package(2, 7), None).unwrap();
    assert_eq!(second.installation_id, first.installation_id);
    assert_eq!(second.revision, 2);
    assert_eq!(second.generation, 2);
    assert_eq!(second.status, "disabled");
    assert!(store.install("downgrade", 2, &package(1, 7), None).is_err());
}
#[test]
fn publisher_binding_survives_uninstall_and_old_commands_do_not_match_reinstall() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    let first = store.install("first", 0, &package(1, 7), None).unwrap();
    assert!(store.install("takeover", 1, &package(2, 8), None).is_err());
    let removed = store
        .uninstall("remove", 1, "org.test.plugin", None)
        .unwrap();
    assert_eq!(removed.status, "uninstalled");
    assert_eq!(
        store
            .uninstall("remove", 1, "org.test.plugin", None)
            .unwrap(),
        removed
    );
    assert!(
        store
            .install("reinstall-stranger", 0, &package(1, 8), None)
            .is_err()
    );
    let next = store.install("reinstall", 0, &package(1, 7), None).unwrap();
    assert_ne!(first.installation_id, next.installation_id);
    assert!(next.revision > removed.revision);
    assert!(next.generation > removed.generation);
    assert!(
        store
            .uninstall("stale-remove", first.revision, "org.test.plugin", None)
            .is_err()
    );
    assert!(
        store
            .install("stale-update", first.revision, &package(2, 7), None)
            .is_err()
    );
    assert_eq!(
        store.status().unwrap()[0].installation_id,
        next.installation_id
    );
}
#[test]
fn package_input_replacement_after_verification_cannot_change_installed_bytes() {
    let temp = Scratch::new();
    let first = package(1, 7);
    let path = temp.0.join("input.json");
    fs::write(&path, first.raw_bytes()).unwrap();
    let frozen = verify(&read_package(&path).unwrap(), first.publisher_key()).unwrap();
    fs::write(path, package(2, 7).raw_bytes()).unwrap();
    Store::open(&temp.store())
        .unwrap()
        .install("first", 0, &frozen, None)
        .unwrap();
    let reopened = Store::open(&temp.store()).unwrap();
    assert_eq!(reopened.status().unwrap()[0].digest, first.digest());
}
#[test]
fn corrupt_content_unknown_schema_and_missing_content_fail_closed() {
    for scenario in ["content", "schema", "missing"] {
        let temp = Scratch::new();
        Store::open(&temp.store())
            .unwrap()
            .install("first", 0, &package(1, 7), None)
            .unwrap();
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        match scenario {
            "content" => {
                db.execute("UPDATE content SET bytes=x'00'", []).unwrap();
            }
            "schema" => {
                db.execute_batch("PRAGMA user_version=99").unwrap();
            }
            _ => {
                db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM content")
                    .unwrap();
            }
        }
        drop(db);
        assert!(Store::open(&temp.store()).is_err(), "{scenario}");
    }
}
#[cfg(unix)]
#[test]
fn symlink_inputs_and_store_are_rejected() {
    use std::os::unix::fs::symlink;
    let temp = Scratch::new();
    let input = temp.0.join("real.json");
    fs::write(&input, package(1, 7).raw_bytes()).unwrap();
    let link = temp.0.join("input-link");
    symlink(&input, &link).unwrap();
    assert!(read_package(&link).is_err());
    let store = Store::open(&temp.store()).unwrap();
    drop(store);
    let linked_store = temp.0.join("linked-store");
    symlink(temp.store(), &linked_store).unwrap();
    assert!(Store::open(&linked_store).is_err());
}

#[test]
fn valid_large_container_uses_the_shared_package_input_budget() {
    let temp = Scratch::new();
    let small = package(1, 7);
    let mut files = vec![PackageFile {
        path: "module.wasm".into(),
        mime: "application/wasm".into(),
        bytes: small.module_bytes().to_vec(),
    }];
    for index in 0..3 {
        files.push(PackageFile {
            path: format!("assets/{index}.bin"),
            mime: "application/octet-stream".into(),
            bytes: vec![0; 800_000],
        });
    }
    let bytes = build_signed(small.manifest().clone(), "module.wasm", files, &[7; 32]).unwrap();
    assert!(bytes.len() > 4 * 1024 * 1024);
    let path = temp.0.join("large.qpkg");
    fs::write(&path, &bytes).unwrap();
    assert_eq!(read_package(&path).unwrap(), bytes);
}

fn scope() -> qusor_package_probe::store::GrantScope {
    qusor_package_probe::store::GrantScope {
        capability: extension_api::Capability::DocumentMetadataRead,
        library: "library-a".into(),
        document: "document-a".into(),
        version: "version-1".into(),
        expires_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
    }
}
#[test]
fn recovery_uses_new_native_sessions_and_exact_durable_scopes() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &package(1, 7), None).unwrap();
    let grant = scope();
    let receipt = store
        .grant("grant", 1, "org.test.plugin", grant.clone(), None)
        .unwrap();
    assert_eq!(receipt.generation, 2);
    assert_eq!(
        store
            .grant("grant", 1, "org.test.plugin", grant, None)
            .unwrap(),
        receipt
    );
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    store
        .check_session(&token, "library-a", "document-a", "version-1")
        .unwrap();
    for (library, document, version) in [
        ("other", "document-a", "version-1"),
        ("library-a", "other", "version-1"),
        ("library-a", "document-a", "version-2"),
    ] {
        assert!(
            store
                .check_session(&token, library, document, version)
                .is_err()
        );
    }
    drop(store);
    let mut store = Store::open(&temp.store()).unwrap();
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
    let fresh = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    assert_ne!(token, fresh);
    store
        .check_session(&fresh, "library-a", "document-a", "version-1")
        .unwrap();
    store
        .stop("revoke", 2, "org.test.plugin", "revoke", None)
        .unwrap();
    assert!(
        store
            .check_session(&fresh, "library-a", "document-a", "version-1")
            .is_err()
    );
    assert!(
        store
            .resume("org.test.plugin", "library-a", "document-a", "version-1")
            .is_err()
    );
}
#[test]
fn transient_capabilities_expiry_update_and_disable_do_not_restore_grants() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &package(1, 7), None).unwrap();
    for capability in [
        extension_api::Capability::SelectionRead,
        extension_api::Capability::CapturePropose,
    ] {
        let mut grant = scope();
        grant.capability = capability;
        assert!(
            store
                .grant("bad", 1, "org.test.plugin", grant, None)
                .is_err()
        );
    }
    let mut expired = scope();
    expired.expires_at = 1;
    assert!(
        store
            .grant("expired", 1, "org.test.plugin", expired, None)
            .is_err()
    );
    store
        .grant("grant", 1, "org.test.plugin", scope(), None)
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    store.install("update", 2, &package(2, 7), None).unwrap();
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
    assert!(
        store
            .resume("org.test.plugin", "library-a", "document-a", "version-1")
            .is_err()
    );
    store
        .grant("grant-again", 3, "org.test.plugin", scope(), None)
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    store
        .stop("disable", 4, "org.test.plugin", "disable", None)
        .unwrap();
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
}
#[test]
fn expired_grant_replay_returns_committed_receipt_without_reactivation() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &package(1, 7), None).unwrap();
    let mut grant = scope();
    grant.expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 1;
    let receipt = store
        .grant("grant", 1, "org.test.plugin", grant.clone(), None)
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
    assert_eq!(
        store
            .grant("grant", 1, "org.test.plugin", grant, None)
            .unwrap(),
        receipt
    );
    assert!(
        store
            .resume("org.test.plugin", "library-a", "document-a", "version-1")
            .is_err()
    );
}

#[test]
fn corrupt_persisted_grant_bindings_fail_closed() {
    for field in [
        "digest",
        "generation",
        "policy_version",
        "installation_id",
        "scope",
    ] {
        let temp = Scratch::new();
        let mut store = Store::open(&temp.store()).unwrap();
        store.install("install", 0, &package(1, 7), None).unwrap();
        store
            .grant("grant", 1, "org.test.plugin", scope(), None)
            .unwrap();
        drop(store);
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        let json: String = db
            .query_row("SELECT record FROM grants", [], |r| r.get(0))
            .unwrap();
        let mut record: serde_json::Value = serde_json::from_str(&json).unwrap();
        if field == "digest" {
            record[field] = serde_json::json!("0".repeat(64));
        } else if field == "scope" {
            record[field]["capability"] = serde_json::json!("selection.read");
        } else {
            record[field] = serde_json::json!(999);
        }
        db.execute("UPDATE grants SET record=?1", [record.to_string()])
            .unwrap();
        drop(db);
        assert!(Store::open(&temp.store()).is_err(), "{field}");
    }
}
#[test]
fn grant_replacement_and_uninstall_invalidate_live_native_sessions() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &package(1, 7), None).unwrap();
    store
        .grant("grant", 1, "org.test.plugin", scope(), None)
        .unwrap();
    let first_token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    assert!(
        store
            .check_session(&first_token, "library-a", "document-a", "version-1")
            .is_err()
    );
    store
        .grant("replace", 2, "org.test.plugin", scope(), None)
        .unwrap();
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    store
        .uninstall("uninstall", 3, "org.test.plugin", None)
        .unwrap();
    assert!(
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .is_err()
    );
    store.install("reinstall", 0, &package(1, 7), None).unwrap();
    assert!(
        store
            .resume("org.test.plugin", "library-a", "document-a", "version-1")
            .is_err()
    );
}

#[test]
fn updating_other_package_preserves_live_session() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    let first = package(1, 7);
    store.install("install-a", 0, &first, None).unwrap();
    store
        .grant("grant-a", 1, "org.test.plugin", scope(), None)
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    for version in [1, 2] {
        let source = package(version, 7);
        let mut manifest = source.manifest().clone();
        manifest.package_id = "org.test.second".into();
        let bytes = build_signed(
            manifest,
            "module.wasm",
            vec![PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: source.module_bytes().to_vec(),
            }],
            &[7; 32],
        )
        .unwrap();
        let second = verify(&bytes, &publisher_key(&[7; 32])).unwrap();
        store
            .install(
                &format!("install-b-{version}"),
                i64::from(version - 1),
                &second,
                None,
            )
            .unwrap();
        store
            .check_session(&token, "library-a", "document-a", "version-1")
            .unwrap();
    }
    store
        .uninstall("remove-b", 2, "org.test.second", None)
        .unwrap();
    store
        .check_session(&token, "library-a", "document-a", "version-1")
        .unwrap();
}
#[test]
fn schema_one_upgrade_preserves_installation_and_operation_receipt() {
    let temp = Scratch::new();
    let first = package(1, 7);
    let receipt = Store::open(&temp.store())
        .unwrap()
        .install("install", 0, &first, None)
        .unwrap();
    let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE worker_leases; DROP TABLE private_pending; DROP TABLE retained_pairs; DROP TABLE private_heads; DROP TABLE private_snapshots; DROP TABLE private_operations; DROP TABLE grants; DROP TABLE installation_library; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let mut store = Store::open(&temp.store()).unwrap();
    assert_eq!(
        store.status().unwrap()[0].installation_id,
        receipt.installation_id
    );
    assert_eq!(store.install("install", 0, &first, None).unwrap(), receipt);
    store
        .grant("grant", 1, "org.test.plugin", scope(), None)
        .unwrap();
    let token = store
        .resume("org.test.plugin", "library-a", "document-a", "version-1")
        .unwrap();
    store
        .check_session(&token, "library-a", "document-a", "version-1")
        .unwrap();
}

#[test]
fn installation_library_binding_survives_stop_and_update_until_reinstall() {
    let temp = Scratch::new();
    let mut store = Store::open(&temp.store()).unwrap();
    store.install("install", 0, &package(1, 7), None).unwrap();
    store
        .grant("grant", 1, "org.test.plugin", scope(), None)
        .unwrap();
    let mut another = scope();
    another.library = "other-library".into();
    assert!(
        store
            .grant("switch", 2, "org.test.plugin", another.clone(), None)
            .is_err()
    );
    store
        .stop("disable", 2, "org.test.plugin", "disable", None)
        .unwrap();
    assert!(
        store
            .grant(
                "switch-after-disable",
                3,
                "org.test.plugin",
                another.clone(),
                None
            )
            .is_err()
    );
    store.install("update", 3, &package(2, 7), None).unwrap();
    assert!(
        store
            .grant(
                "switch-after-update",
                4,
                "org.test.plugin",
                another.clone(),
                None
            )
            .is_err()
    );
    store
        .uninstall("remove", 4, "org.test.plugin", None)
        .unwrap();
    store.install("reinstall", 0, &package(1, 7), None).unwrap();
    store
        .grant("new-library", 6, "org.test.plugin", another, None)
        .unwrap();
}
#[test]
fn missing_mismatched_and_orphaned_library_bindings_fail_closed() {
    for sql in [
        "DELETE FROM installation_library",
        "UPDATE installation_library SET library='other'",
        "PRAGMA foreign_keys=OFF; INSERT INTO installation_library VALUES (999,'orphan')",
    ] {
        let temp = Scratch::new();
        let mut store = Store::open(&temp.store()).unwrap();
        store.install("install", 0, &package(1, 7), None).unwrap();
        store
            .grant("grant", 1, "org.test.plugin", scope(), None)
            .unwrap();
        drop(store);
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        assert!(Store::open(&temp.store()).is_err(), "{sql}");
    }
}

#[test]
fn recovered_execution_consumes_native_session_on_success_and_vm_failure() {
    // Minimal Wasm ABI: one bounded memory and run() -> (0,0). No native service
    // calls. Independent author metadata fixtures exercise the broker separately.
    let wasm = b"\0asm\x01\0\0\0\x01\x06\x01\x60\0\x02\x7f\x7f\x03\x02\x01\0\x05\x03\x01\0\x01\x07\x10\x02\x06memory\x02\0\x03run\0\0\x0a\x08\x01\x06\0\x41\0\x41\0\x0b";
    for (module, succeeds) in [
        (wasm.as_slice(), true),
        (b"\0asm\x01\0\0\0".as_slice(), false),
    ] {
        let temp = Scratch::new();
        let manifest = package(1, 7).manifest().clone();
        let signed = build_signed(
            manifest,
            "module.wasm",
            vec![PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: module.to_vec(),
            }],
            &[7; 32],
        )
        .unwrap();
        let verified = verify(&signed, &publisher_key(&[7; 32])).unwrap();
        let mut store = Store::open(&temp.store()).unwrap();
        store.install("install", 0, &verified, None).unwrap();
        store
            .grant("grant", 1, "org.test.plugin", scope(), None)
            .unwrap();
        let token = store
            .resume("org.test.plugin", "library-a", "document-a", "version-1")
            .unwrap();
        let result = store.execute(&token, "library-a", "document-a", "version-1");
        assert_eq!(result.is_ok(), succeeds);
        if let Ok(result) = result {
            assert_eq!(result.output, "");
            assert_eq!(result.calls, 0);
        }
        assert!(
            store
                .execute(&token, "library-a", "document-a", "version-1")
                .is_err()
        );
        assert!(
            store
                .check_session(&token, "library-a", "document-a", "version-1")
                .is_err()
        );
    }
}
