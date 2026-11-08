use qusor_capture_probe::{
    contract::CaptureDraft,
    core::{Core, Provenance},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}
impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn temp(label: &str) -> Scratch {
    let p = std::env::temp_dir().join(format!(
        "qusor-core-{label}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
    Scratch(p)
}
fn draft() -> CaptureDraft {
    serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"test","reference":"local-reference","title":"Source title","public_uri":null,"external_return":"none"},"snapshot_text":"Hello world","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":6,"end":11,"exact":"world"}})).unwrap()
}
fn origin() -> Provenance {
    Provenance {
        package_id: "third.party".into(),
        publisher_id: "publisher".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    }
}
#[test]
fn durable_idempotent_distinct_intents_and_exact_resolution() {
    let dir = temp("durable");
    let receipt;
    {
        let mut c = Core::open(&dir).unwrap();
        assert!(Core::open(&dir).is_err());
        receipt = c
            .commit("intent-a", "vault", &draft(), &origin(), "My view", None)
            .unwrap();
        assert_eq!(
            receipt,
            c.commit("intent-a", "vault", &draft(), &origin(), "My view", None)
                .unwrap()
        );
        assert!(
            c.commit(
                "intent-a",
                "vault",
                &draft(),
                &origin(),
                "Changed view",
                None
            )
            .is_err()
        );
        let second = c
            .commit("intent-b", "vault", &draft(), &origin(), "My view", None)
            .unwrap();
        assert_ne!(receipt.capture_id, second.capture_id);
        assert_eq!(receipt.snapshot_hash, second.snapshot_hash);
        assert_eq!(fs::read_dir(dir.join("blobs")).unwrap().count(), 1);
    }
    let c = Core::open(&dir).unwrap();
    assert_eq!(c.receipt("intent-a").unwrap(), Some(receipt.clone()));
    assert_eq!(c.query(receipt.capture_id).unwrap().user_note, "My view");
    assert!(
        c.resolve_version(
            receipt.capture_id,
            "wrong-version",
            &receipt.snapshot_hash,
            "qusor-raw-text-v1",
            "unicode_scalar"
        )
        .is_err()
    );
    assert_eq!(
        c.resolve(
            receipt.capture_id,
            &receipt.snapshot_hash,
            "qusor-raw-text-v1",
            "unicode_scalar"
        )
        .unwrap()
        .text,
        "world"
    );
    assert!(
        c.resolve(
            receipt.capture_id,
            &"0".repeat(64),
            "qusor-raw-text-v1",
            "unicode_scalar"
        )
        .is_err()
    );
    assert!(
        c.resolve(
            receipt.capture_id,
            &receipt.snapshot_hash,
            "other",
            "unicode_scalar"
        )
        .is_err()
    );
    assert!(
        c.resolve(
            receipt.capture_id,
            &receipt.snapshot_hash,
            "qusor-raw-text-v1",
            "utf16"
        )
        .is_err()
    );
}
#[test]
fn export_restore_is_complete_and_rejects_damage() {
    let dir = temp("export");
    let mut c = Core::open(&dir).unwrap();
    let r = c
        .commit(
            "intent",
            "vault",
            &draft(),
            &origin(),
            "trusted user note",
            None,
        )
        .unwrap();
    let b = c.export().unwrap();
    let dst = temp("restore");
    let mut restored = Core::open(&dst).unwrap();
    let mut broken = b.clone();
    broken.blobs[0].text.push('!');
    assert!(restored.restore(&broken, None).is_err());
    assert!(restored.receipt("intent").unwrap().is_none());
    broken = b.clone();
    broken.entries[0].record.receipt.commit_rev = 77;
    assert!(restored.restore(&broken, None).is_err());
    broken = b.clone();
    broken.schema_version = 99;
    assert!(restored.restore(&broken, None).is_err());
    broken = b.clone();
    broken.blobs.clear();
    assert!(restored.restore(&broken, None).is_err());
    restored.restore(&b, None).unwrap();
    assert_eq!(restored.export().unwrap(), b);
    assert_eq!(restored.receipt("intent").unwrap(), Some(r));
    assert!(restored.restore(&b, None).is_err());
    drop(restored);
    assert!(Core::open(&dst).is_ok());
}
#[test]
fn actual_blob_corruption_is_never_fuzzy_repaired() {
    let dir = temp("corrupt");
    let mut c = Core::open(&dir).unwrap();
    let r = c
        .commit("intent", "vault", &draft(), &origin(), "", None)
        .unwrap();
    fs::write(
        dir.join("blobs").join(&r.snapshot_hash),
        "Hello similar world",
    )
    .unwrap();
    assert!(c.query(r.capture_id).is_err());
    assert!(c.export().is_err());
    drop(c);
    assert!(Core::open(&dir).is_err());
}
#[test]
fn crash_child() {
    if let Ok(path) = std::env::var("QUSOR_CORE_CRASH_PATH") {
        let mut c = Core::open(path).unwrap();
        let point = std::env::var("QUSOR_CORE_CRASH_POINT").unwrap();
        let _ = c.commit(
            "crash-intent",
            "vault",
            &draft(),
            &origin(),
            "note",
            Some(&point),
        );
        panic!("crash did not exit");
    }
}
#[test]
fn real_process_crashes_leave_atomic_transactions() {
    for point in [
        "after_blob_write",
        "before_blob_publish",
        "after_blob_publish",
        "before_transaction",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let dir = temp(point);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_child", "--nocapture"])
            .env("QUSOR_CORE_CRASH_PATH", &dir)
            .env("QUSOR_CORE_CRASH_POINT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86), "{point}");
        let mut c = Core::open(&dir).unwrap();
        let b = c.export().unwrap();
        assert_eq!(
            b.entries.len(),
            usize::from(point == "after_commit"),
            "{point}"
        );
        let r = c
            .commit("crash-intent", "vault", &draft(), &origin(), "note", None)
            .unwrap();
        assert_eq!(r.capture_id, 1);
        assert_eq!(c.export().unwrap().entries.len(), 1);
    }
}
#[test]
fn sql_identity_and_atomic_record_damage_are_rejected() {
    for mutation in [
        "UPDATE capture SET id=9 WHERE id=1",
        "UPDATE document_version SET source_id=2 WHERE id=1",
        "DELETE FROM outbox",
    ] {
        let dir = temp("sql-tamper");
        {
            let mut c = Core::open(&dir).unwrap();
            c.commit("one", "vault", &draft(), &origin(), "note", None)
                .unwrap();
            c.commit("two", "vault", &draft(), &origin(), "note", None)
                .unwrap();
        }
        let db = rusqlite::Connection::open(dir.join("core.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        db.execute_batch(mutation).unwrap();
        drop(db);
        assert!(Core::open(&dir).is_err(), "{mutation}");
    }
}
#[test]
fn failed_restore_crash_remains_empty() {
    for point in ["after_blob_publish", "after_entities", "before_commit"] {
        let dir = temp("restore-source");
        let mut c = Core::open(&dir).unwrap();
        c.commit("one", "vault", &draft(), &origin(), "note", None)
            .unwrap();
        let bundle = c.export().unwrap();
        let dst = temp("restore-crash");
        let bundle_file = dir.join("export.json");
        fs::write(&bundle_file, serde_json::to_vec(&bundle).unwrap()).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "restore_crash_child", "--nocapture"])
            .env("QUSOR_RESTORE_PATH", &dst)
            .env("QUSOR_RESTORE_BUNDLE", bundle_file)
            .env("QUSOR_CORE_CRASH_POINT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        let mut restored = Core::open(&dst).unwrap();
        assert!(restored.export().unwrap().entries.is_empty());
        restored.restore(&bundle, None).unwrap();
        assert_eq!(restored.export().unwrap(), bundle);
    }
}
#[test]
fn restore_crash_child() {
    if let Ok(path) = std::env::var("QUSOR_RESTORE_PATH") {
        let mut c = Core::open(path).unwrap();
        let point = std::env::var("QUSOR_CORE_CRASH_POINT").unwrap();
        let bytes = fs::read(std::env::var("QUSOR_RESTORE_BUNDLE").unwrap()).unwrap();
        let _ = c.restore_json(&bytes, Some(&point));
        panic!("restore did not crash");
    }
}
#[test]
fn missing_blob_unknown_schema_and_symlink_are_rejected() {
    let dir = temp("missing");
    let receipt = {
        let mut c = Core::open(&dir).unwrap();
        c.commit("one", "vault", &draft(), &origin(), "note", None)
            .unwrap()
    };
    fs::rename(
        dir.join("blobs").join(receipt.snapshot_hash),
        dir.join("removed-blob"),
    )
    .unwrap();
    assert!(Core::open(&dir).is_err());
    let schema = temp("schema");
    drop(Core::open(&schema).unwrap());
    let db = rusqlite::Connection::open(schema.join("core.sqlite")).unwrap();
    db.execute_batch("PRAGMA user_version=999").unwrap();
    drop(db);
    assert!(Core::open(&schema).is_err());
    let sym = temp("symlink");
    std::os::unix::fs::symlink("/dev/null", sym.join("writer.lock")).unwrap();
    assert!(Core::open(&sym).is_err());
}
