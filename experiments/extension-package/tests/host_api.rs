//! Real disabled installation with explicit host compatibility; no new saved grants.
use extension_api::{
    ApiRange, ApiVersion, Capability, Contribution, ContributionKind, HOST_API, Manifest,
    PackageVersion,
};
use qusor_package_probe::{
    package::{
        PackageError, PackageErrorCode, PackageFile, build_signed_for_host, publisher_key,
        verify_for_host,
    },
    store::Store,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const APIS: [ApiVersion; 2] = [HOST_API, ApiVersion { major: 0, minor: 2 }];
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-store-api-{}-{}",
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
fn surface_package() -> qusor_package_probe::package::VerifiedPackage {
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.surface".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        required_capabilities: vec![Capability::ReaderSurface],
        optional_capabilities: vec![Capability::SelectionRead],
        contributions: vec![Contribution {
            id: "org.test.surface.reader".into(),
            title: "Reader".into(),
            kind: ContributionKind::ReaderWorkspace,
            required_capabilities: vec![Capability::ReaderSurface],
        }],
    };
    let seed = [17; 32];
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: b"\0asm\x01\0\0\0".to_vec(),
        }],
        &seed,
        &APIS,
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&seed), &APIS).unwrap()
}
#[test]
fn explicit_host_versions_survive_reopen_without_enabling_or_granting() {
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let package = surface_package();
    let receipt = {
        let mut store = Store::open_for_host(&path, &APIS).unwrap();
        let r = store.install("install", 0, &package, None).unwrap();
        assert_eq!(r.status, "disabled");
        r
    };
    assert!(
        Store::open(&path).is_err(),
        "legacy host must reject an installed min-0.2 package"
    );
    let mut store = Store::open_for_host(&path, &APIS).unwrap();
    let mut status = receipt.clone();
    status.operation_id.clear();
    assert_eq!(store.status().unwrap(), vec![status]);
    assert_eq!(
        store.install("install", 0, &package, None).unwrap(),
        receipt
    );
    let (current, frozen) = store.frozen_package_for_action("org.test.surface").unwrap();
    assert_eq!(current.status, "disabled");
    assert_eq!(frozen.digest(), package.digest());
    assert_eq!(frozen.manifest(), package.manifest());
    let db = rusqlite::Connection::open(path.join("state.sqlite3")).unwrap();
    let grants: i64 = db
        .query_row("SELECT count(*) FROM grants", [], |r| r.get(0))
        .unwrap();
    assert_eq!(grants, 0);
}
#[test]
fn legacy_install_reverifies_and_rolls_back_a_verified_new_api_package() {
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let package = surface_package();
    let mut store = Store::open(&path).unwrap();
    let error = store.install("install", 0, &package, None).unwrap_err();
    assert_eq!(
        error.downcast_ref::<PackageError>().unwrap().code,
        PackageErrorCode::Incompatible
    );
    assert!(store.status().unwrap().is_empty());
    let db = rusqlite::Connection::open(path.join("state.sqlite3")).unwrap();
    for table in ["content", "trust", "operations"] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
#[test]
fn invalid_host_version_configuration_does_not_create_a_store() {
    let scratch = Scratch::new();
    for (index, apis) in [
        vec![],
        vec![HOST_API, HOST_API],
        vec![ApiVersion {
            major: 99,
            minor: 0,
        }],
    ]
    .iter()
    .enumerate()
    {
        let path = scratch.0.join(format!("invalid-{index}"));
        assert!(Store::open_for_host(&path, apis).is_err());
        assert!(!path.exists());
    }
}

#[test]
fn new_api_action_reverifies_the_trusted_stored_bytes() {
    let scratch = Scratch::new();
    let path = scratch.0.join("store");
    let package = surface_package();
    let mut store = Store::open_for_host(&path, &APIS).unwrap();
    store.install("install", 0, &package, None).unwrap();
    let db = rusqlite::Connection::open(path.join("state.sqlite3")).unwrap();
    db.execute("UPDATE content SET bytes=?1", [b"{}".as_slice()])
        .unwrap();
    assert!(store.frozen_package_for_action("org.test.surface").is_err());
    db.execute("UPDATE content SET bytes=?1", [package.raw_bytes()])
        .unwrap();
    assert_eq!(
        store
            .frozen_package_for_action("org.test.surface")
            .unwrap()
            .1
            .digest(),
        package.digest()
    );
}
