use qusor_capture_probe::{
    contract::{CaptureDraft, sha256},
    core::{Core, Provenance},
    pdf_contract::*,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-pdf-core-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn core(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn original() -> Vec<u8> {
    b"%PDF-1.7\nSynthetic immutable Core fixture, parser covered independently.\n".to_vec()
}
fn snapshot() -> PdfSnapshot {
    let original = original();
    serde_json::from_value(serde_json::json!({"protocol_version":1,"original_sha256":sha256(&original),"original_bytes":original.len(),"extraction":{"provider":"pdfium","algorithm":"qusor-pdfium-char-map-v1","wrapper":"pdfium-render-0.9.5","abi":"pdfium_8125","library_sha256":"a".repeat(64),"mapping_check":"unicode_scalar_validity_only","normalization":PDF_NORMALIZATION},"engine_report_sha256":"b".repeat(64),"pages":[{"page_index":0,"engine_text":"hello","text_sha256":sha256(b"hello"),"rotation_degrees":0,"media_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"crop_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"characters":"hello".chars().enumerate().map(|(i,c)|serde_json::json!({"pdf_char_index":i,"unicode_value":c as u32,"mapping":"scalar","generated":false,"scalar_start":i,"scalar_end":i+1,"utf8_start":i,"utf8_end":i+1,"utf16_start":i,"utf16_end":i+1,"pdf_bounds":{"left":i as f32,"bottom":1.0,"right":i as f32+1.0,"top":2.0}})).collect::<Vec<_>>()}]})).unwrap()
}
fn draft() -> PdfDraft {
    let snap = snapshot();
    PdfDraft {
        protocol_version: 1,
        original_sha256: snap.original_sha256.clone(),
        extraction_revision: snap.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(b"hello"),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 5,
        exact: "hello".into(),
    }
}
fn origin() -> Provenance {
    Provenance {
        package_id: "org.test.reader".into(),
        publisher_id: "org.test".into(),
        package_digest: "c".repeat(64),
        installation_id: 1,
    }
}
fn text() -> CaptureDraft {
    serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"test","reference":"source","title":"Source","public_uri":null,"external_return":"none"},"snapshot_text":"legacy text","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":6,"exact":"legacy"}})).unwrap()
}
fn downgrade_fixture_to_schema_one(path: &Path) {
    let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN; DROP TABLE pdf_receipt; DROP TABLE pdf_outbox; DROP TABLE pdf_index_job; DROP TABLE pdf_operation; DROP TABLE pdf_capture; DROP TABLE pdf_extraction; DROP TABLE pdf_document; DROP TABLE commit_log; PRAGMA user_version=1; COMMIT;").unwrap();
}

#[test]
fn binary_original_global_revisions_and_distinct_capture_intents_survive_reopen() {
    let scratch = Scratch::new();
    let path = scratch.core("vault");
    let (document, capture, export);
    {
        let mut core = Core::open(&path).unwrap();
        let legacy = core
            .commit("legacy", "vault", &text(), &origin(), "my text note", None)
            .unwrap();
        assert_eq!(legacy.commit_rev, 1);
        document = core
            .import_document(
                "import",
                "vault",
                "PDF source",
                &original(),
                &snapshot(),
                None,
            )
            .unwrap();
        assert_eq!(document.commit_rev, 2);
        assert_eq!(
            core.import_document(
                "import",
                "vault",
                "PDF source",
                &original(),
                &snapshot(),
                None
            )
            .unwrap(),
            document
        );
        assert!(
            core.import_document("import", "vault", "Changed", &original(), &snapshot(), None)
                .is_err()
        );
        capture = core
            .capture_pdf(
                "capture",
                document.document_id,
                &draft(),
                &origin(),
                "My own note",
                None,
            )
            .unwrap();
        assert_eq!(capture.commit_rev, 3);
        assert_eq!(
            core.capture_pdf(
                "capture",
                document.document_id,
                &draft(),
                &origin(),
                "My own note",
                None
            )
            .unwrap(),
            capture
        );
        assert!(
            core.capture_pdf(
                "capture",
                document.document_id,
                &draft(),
                &origin(),
                "Changed",
                None
            )
            .is_err()
        );
        let another = core
            .capture_pdf(
                "separate intent",
                document.document_id,
                &draft(),
                &origin(),
                "My own note",
                None,
            )
            .unwrap();
        assert_ne!(another.capture_id, capture.capture_id);
        let after = core
            .commit("text-after-PDF", "vault", &text(), &origin(), "", None)
            .unwrap();
        assert_eq!(after.capture_id, 2);
        assert_eq!(after.commit_rev, 5);
        assert_eq!(core.query(after.capture_id).unwrap().receipt, after);
        assert!(core.export().unwrap_err().to_string().contains("schema 2"));
        assert_eq!(
            core.read_original(document.document_id).unwrap(),
            original()
        );
        assert_eq!(
            core.resolve_pdf(capture.capture_id, &draft())
                .unwrap()
                .exact,
            "hello"
        );
        let mut wrong = draft();
        wrong.start = 1;
        wrong.exact = "ello".into();
        assert!(core.resolve_pdf(capture.capture_id, &wrong).is_err());
        assert!(
            core.capture_pdf(
                "legacy",
                document.document_id,
                &draft(),
                &origin(),
                "",
                None
            )
            .is_err()
        );
        assert!(
            core.commit("import", "vault", &text(), &origin(), "", None)
                .is_err()
        );
        export = core.export_all().unwrap();
        assert_eq!(export.commits.len(), 5);
        assert_eq!(export.blobs.len(), 2);
    }
    let core = Core::open(&path).unwrap();
    assert_eq!(core.export_all().unwrap(), export);
    assert_eq!(core.document_receipt("import").unwrap(), Some(document));
    assert_eq!(core.pdf_receipt("capture").unwrap(), Some(capture));
}

#[test]
fn mixed_archive_restores_atomically_and_rejects_relationship_or_blob_damage() {
    let scratch = Scratch::new();
    let mut core = Core::open(scratch.core("source")).unwrap();
    core.commit("text", "v", &text(), &origin(), "", None)
        .unwrap();
    let document = core
        .import_document("import", "v", "PDF", &original(), &snapshot(), None)
        .unwrap();
    core.capture_pdf(
        "capture",
        document.document_id,
        &draft(),
        &origin(),
        "quote != user note",
        None,
    )
    .unwrap();
    let archive = core.export_all().unwrap();
    let mut restored = Core::open(scratch.core("restored")).unwrap();
    for kind in ["blob", "revision", "snapshot", "operation", "missing"] {
        let mut broken = archive.clone();
        match kind {
            "blob" => broken.blobs[0].base64.push('A'),
            "revision" => broken.commits[1].commit_rev = 99,
            "snapshot" => broken.pdf_documents[0].snapshot.pages[0].engine_text = "changed".into(),
            "operation" => broken.pdf_operations[0].outbox = "different".into(),
            _ => {
                broken.blobs.pop();
            }
        };
        assert!(restored.restore_all(&broken, None).is_err(), "{kind}");
        assert!(restored.export_all().unwrap().commits.is_empty());
    }
    let bytes = serde_json::to_vec(&archive).unwrap();
    let duplicate = format!(
        "{{\"schema_version\":2,{}",
        &String::from_utf8(bytes.clone()).unwrap()[1..]
    );
    assert!(
        restored
            .restore_all_json(duplicate.as_bytes(), None)
            .is_err()
    );
    let mut value = serde_json::to_value(&archive).unwrap();
    value["grants"] = serde_json::json!(["all"]);
    assert!(
        restored
            .restore_all_json(&serde_json::to_vec(&value).unwrap(), None)
            .is_err()
    );
    restored.restore_all_json(&bytes, None).unwrap();
    assert_eq!(restored.export_all().unwrap(), archive);
    assert!(restored.restore_all(&archive, None).is_err());
    assert_eq!(restored.read_original(1).unwrap(), original());
}

#[test]
fn actual_schema_one_database_and_archive_upgrade_preserve_existing_content() {
    let scratch = Scratch::new();
    let path = scratch.core("legacy");
    let archive;
    {
        let mut core = Core::open(&path).unwrap();
        core.commit("old-op", "v", &text(), &origin(), "older note", None)
            .unwrap();
        archive = core.export().unwrap();
    }
    downgrade_fixture_to_schema_one(&path);
    let mut upgraded = Core::open(&path).unwrap();
    assert_eq!(upgraded.export().unwrap(), archive);
    let imported = upgraded
        .import_document(
            "new-pdf",
            "v",
            "Imported after migration",
            &original(),
            &snapshot(),
            None,
        )
        .unwrap();
    assert_eq!(imported.commit_rev, 2);
    let mut empty = Core::open(scratch.core("v1-restore")).unwrap();
    empty.restore(&archive, None).unwrap();
    assert_eq!(empty.export().unwrap(), archive);
    assert_eq!(
        empty.query_operation("old-op").unwrap().unwrap().commit_rev,
        1
    );
}

#[test]
fn missing_original_and_sql_relationship_tampering_are_not_repaired() {
    let scratch = Scratch::new();
    let path = scratch.core("missing");
    let mut core = Core::open(&path).unwrap();
    let document = core
        .import_document("import", "v", "PDF", &original(), &snapshot(), None)
        .unwrap();
    let hash = document.original_sha256.clone();
    fs::remove_file(path.join("blobs").join(&hash)).unwrap();
    assert!(core.query_document(1).is_err());
    drop(core);
    assert!(Core::open(&path).is_err());
    for table in [
        "pdf_document",
        "pdf_extraction",
        "pdf_capture",
        "pdf_operation",
        "pdf_outbox",
        "commit_log",
    ] {
        let path = scratch.core(table);
        let mut core = Core::open(&path).unwrap();
        core.import_document("import", "v", "PDF", &original(), &snapshot(), None)
            .unwrap();
        core.capture_pdf("capture", 1, &draft(), &origin(), "", None)
            .unwrap();
        drop(core);
        let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        let sql = match table {
            "pdf_document" => "UPDATE pdf_document SET title='tampered'",
            "pdf_extraction" => "UPDATE pdf_extraction SET snapshot='{}'",
            "pdf_capture" => "UPDATE pdf_capture SET anchor='{}'",
            "pdf_operation" => "UPDATE pdf_operation SET input_hash='tampered'",
            "pdf_outbox" => "DELETE FROM pdf_outbox",
            _ => "DELETE FROM commit_log WHERE commit_rev=1",
        };
        db.execute_batch(sql).unwrap();
        drop(db);
        assert!(Core::open(&path).is_err(), "{table}");
    }
}

#[test]
fn process_crash_child() {
    if let Ok(path) = std::env::var("QUSOR_PDF_CORE_CRASH_PATH") {
        let stage = std::env::var("QUSOR_PDF_CORE_CRASH_STAGE").unwrap();
        let point = std::env::var("QUSOR_PDF_CORE_CRASH_POINT").unwrap();
        if stage == "migration" {
            Core::open_migration_probe(&path, Some(&point)).unwrap();
            panic!("did not hit migration crashpoint");
        }
        let mut core = Core::open(&path).unwrap();
        match stage.as_str() {
            "import" => {
                core.import_document("import", "v", "PDF", &original(), &snapshot(), Some(&point))
                    .unwrap();
            }
            "capture" => {
                core.capture_pdf("capture", 1, &draft(), &origin(), "note", Some(&point))
                    .unwrap();
            }
            "restore" => {
                let input = fs::read(std::env::var("QUSOR_PDF_CORE_ARCHIVE").unwrap()).unwrap();
                core.restore_all_json(&input, Some(&point)).unwrap();
            }
            _ => panic!("unknown stage"),
        };
        panic!("did not hit crashpoint");
    }
}

#[test]
fn real_schema_migration_crashes_keep_schema_and_content_in_one_commit() {
    let scratch = Scratch::new();
    for point in ["before_migration_commit", "after_migration_commit"] {
        let path = scratch.core(point);
        let original_archive;
        {
            let mut core = Core::open(&path).unwrap();
            core.commit("old-op", "v", &text(), &origin(), "original note", None)
                .unwrap();
            original_archive = core.export().unwrap();
        }
        downgrade_fixture_to_schema_one(&path);
        crash_child(&path, "migration", point, None);
        let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            version,
            if point == "after_migration_commit" {
                3
            } else {
                1
            }
        );
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='commit_log'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count,
            if point == "after_migration_commit" {
                1
            } else {
                0
            }
        );
        drop(db);
        let core = Core::open(&path).unwrap();
        assert_eq!(core.export().unwrap(), original_archive);
        assert_eq!(
            core.query_operation("old-op").unwrap().unwrap().commit_rev,
            1
        );
    }
}
fn crash_child(path: &Path, stage: &str, point: &str, archive: Option<&Path>) {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "process_crash_child", "--nocapture"])
        .env("QUSOR_PDF_CORE_CRASH_PATH", path)
        .env("QUSOR_PDF_CORE_CRASH_STAGE", stage)
        .env("QUSOR_PDF_CORE_CRASH_POINT", point);
    if let Some(archive) = archive {
        command.env("QUSOR_PDF_CORE_ARCHIVE", archive);
    }
    assert_eq!(
        command.status().unwrap().code(),
        Some(86),
        "{stage}/{point}"
    );
}
#[test]
fn real_import_capture_and_mixed_restore_process_crashes_preserve_global_prefix() {
    let scratch = Scratch::new();
    for point in [
        "after_blob_write",
        "before_blob_publish",
        "after_blob_publish",
        "before_transaction",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let path = scratch.core(&format!("import-{point}"));
        crash_child(&path, "import", point, None);
        let mut core = Core::open(path).unwrap();
        assert_eq!(
            core.export_all().unwrap().commits.len(),
            usize::from(point == "after_commit")
        );
        let receipt = core
            .import_document("import", "v", "PDF", &original(), &snapshot(), None)
            .unwrap();
        assert_eq!(receipt.document_id, 1);
        assert_eq!(receipt.commit_rev, 1);
    }
    for point in [
        "before_transaction",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let path = scratch.core(&format!("capture-{point}"));
        {
            let mut core = Core::open(&path).unwrap();
            core.import_document("import", "v", "PDF", &original(), &snapshot(), None)
                .unwrap();
        }
        crash_child(&path, "capture", point, None);
        let mut core = Core::open(path).unwrap();
        assert_eq!(
            core.export_all().unwrap().commits.len(),
            1 + usize::from(point == "after_commit")
        );
        let receipt = core
            .capture_pdf("capture", 1, &draft(), &origin(), "note", None)
            .unwrap();
        assert_eq!(receipt.capture_id, 1);
        assert_eq!(receipt.commit_rev, 2);
    }
    let mut source = Core::open(scratch.core("archive-source")).unwrap();
    source
        .commit("text", "v", &text(), &origin(), "", None)
        .unwrap();
    source
        .import_document("import", "v", "PDF", &original(), &snapshot(), None)
        .unwrap();
    source
        .capture_pdf("capture", 1, &draft(), &origin(), "", None)
        .unwrap();
    let archive = source.export_all().unwrap();
    let input = scratch.core("archive.json");
    fs::write(&input, serde_json::to_vec(&archive).unwrap()).unwrap();
    for point in [
        "after_blob_publish",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let path = scratch.core(&format!("restore-{point}"));
        crash_child(&path, "restore", point, Some(&input));
        let mut core = Core::open(path).unwrap();
        assert_eq!(
            core.export_all().unwrap().commits.len(),
            if point == "after_commit" { 3 } else { 0 }
        );
        if point != "after_commit" {
            core.restore_all(&archive, None).unwrap();
        }
        assert_eq!(core.export_all().unwrap(), archive);
    }
}
