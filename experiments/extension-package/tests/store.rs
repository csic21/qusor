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
        optional_capabilities: vec![],
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
