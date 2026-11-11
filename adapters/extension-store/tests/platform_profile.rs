//! The durable store is qualified on Unix only. On a platform without a qualified
//! native filesystem profile it must fail closed instead of opening a store whose
//! 0700 directory, symlink refusal and exclusive writer lease are not enforced.
//! Windows private storage and file adapters remain a recorded gap; this test keeps
//! the gap explicit in the Windows CI job rather than letting it pass as coverage.

use extension_store::Store;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "qusor-platform-store-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[cfg(not(unix))]
#[test]
fn an_unqualified_platform_refuses_to_open_a_store() {
    use extension_store::StoreErrorCode;

    let path = scratch();
    let error = Store::open(&path.join("store")).unwrap_err();
    assert_eq!(error.code(), StoreErrorCode::Unsupported);
    assert!(
        error
            .to_string()
            .contains("qualified native filesystem profile")
    );
    fs::remove_dir_all(&path).unwrap();
}

#[cfg(unix)]
#[test]
fn a_qualified_platform_opens_a_store_and_holds_the_writer_lease() {
    let path = scratch();
    let store_path = path.join("store");
    let store = Store::open(&store_path).unwrap();
    assert!(Store::open(&store_path).is_err());
    drop(store);
    Store::open(&store_path).unwrap();
    fs::remove_dir_all(&path).unwrap();
}
