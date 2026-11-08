#![cfg(unix)]
use core_api::{contract::CaptureDraft, knowledge::*};
use learning_application::KnowledgeApplication;
use learning_ports::{CommitPlan, KnowledgeStore};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use storage_sqlite::{SqliteStore, StorageStage};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-atomic-port-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn store(&self) -> SqliteStore {
        SqliteStore::open(&self.0).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn draft() -> CaptureDraft {
    serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"test","reference":"local","title":"Source","public_uri":null,"external_return":"none"},"snapshot_text":"Hello world","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":6,"end":11,"exact":"world"}})).unwrap()
}
fn origin() -> Provenance {
    Provenance {
        package_id: "third.party".into(),
        publisher_id: "publisher".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    }
}
fn commit<S: KnowledgeStore>(app: &mut KnowledgeApplication<S>, op: &str) -> Receipt {
    app.commit(op, "vault", &draft(), &origin(), "My view")
        .unwrap()
}
fn archive(count: usize) -> ArchiveV2 {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    for i in 0..count {
        commit(&mut app, &format!("op-{i}"));
    }
    app.export_all().unwrap()
}
#[test]
fn stale_and_bulk_plans_cannot_partially_commit() {
    let scratch = Scratch::new();
    let mut store = scratch.store();
    let one = archive(1);
    store
        .commit(CommitPlan {
            expected_revision: 0,
            next: one.clone(),
        })
        .unwrap();
    for (expected, next) in [(0, one.clone()), (1, archive(3))] {
        let error = store
            .commit(CommitPlan {
                expected_revision: expected,
                next,
            })
            .unwrap_err();
        assert_eq!(error.code, KnowledgeErrorCode::Conflict);
        assert_eq!(store.snapshot().unwrap(), one);
    }
}
#[test]
fn self_consistent_forged_history_is_not_an_append() {
    let scratch = Scratch::new();
    let mut store = scratch.store();
    let one = archive(1);
    store.restore(&one).unwrap();
    let mut next = archive(2);
    next.text_entries[0].record.user_note = "Rewritten past".into();
    next.text_entries[0].input_hash = input_hash(&next.text_entries[0].record).unwrap();
    next.commits[0].input_hash = next.text_entries[0].input_hash.clone();
    validate_archive(&next).unwrap();
    assert_eq!(
        store
            .commit(CommitPlan {
                expected_revision: 1,
                next
            })
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(store.snapshot().unwrap(), one);
}
#[test]
fn malformed_plan_is_rejected_before_blob_publication() {
    let scratch = Scratch::new();
    let mut store = scratch.store();
    let mut next = archive(1);
    next.text_entries[0].outbox = "different".into();
    assert_eq!(
        store
            .commit(CommitPlan {
                expected_revision: 0,
                next
            })
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Integrity
    );
    assert_eq!(fs::read_dir(scratch.0.join("blobs")).unwrap().count(), 0);
    assert!(store.snapshot().unwrap().commits.is_empty());
}
#[test]
fn idempotence_ignores_later_global_revisions_and_detects_changed_payload() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let first = commit(&mut app, "first");
    commit(&mut app, "second");
    assert_eq!(commit(&mut app, "first"), first);
    assert_eq!(
        app.commit("first", "vault", &draft(), &origin(), "new note")
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(app.export_all().unwrap().commits.len(), 2);
}
#[test]
fn corrupt_and_missing_referenced_blobs_prevent_reads_and_retries() {
    for missing in [false, true] {
        let scratch = Scratch::new();
        let mut app = KnowledgeApplication::new(scratch.store());
        let receipt = commit(&mut app, "first");
        let path = scratch.0.join("blobs").join(receipt.snapshot_hash);
        if missing {
            fs::remove_file(path).unwrap();
        } else {
            fs::write(path, b"corrupt").unwrap();
        }
        assert_eq!(
            app.query(receipt.capture_id).unwrap_err().code,
            KnowledgeErrorCode::Integrity
        );
        assert_eq!(
            app.commit("first", "vault", &draft(), &origin(), "My view")
                .unwrap_err()
                .code,
            KnowledgeErrorCode::Integrity
        );
    }
}
#[test]
fn restore_rechecks_empty_inside_its_transaction() {
    let scratch = Scratch::new();
    let path = scratch.0.join("core.sqlite");
    let mut store = scratch.store();
    store.set_observer(Arc::new(move |stage| {
        if stage == StorageStage::BeforeTransaction {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute(
                    "INSERT INTO commit_log VALUES(1,'other','text',1,'hash','receipt')",
                    [],
                )
                .unwrap();
        }
    }));
    assert_eq!(
        store.restore(&archive(1)).unwrap_err().code,
        KnowledgeErrorCode::Conflict
    );
    let db = rusqlite::Connection::open(scratch.0.join("core.sqlite")).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM capture", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn application_reports_actionable_error_categories() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    assert_eq!(app.query(1).unwrap_err().code, KnowledgeErrorCode::NotFound);
    assert_eq!(
        app.commit("", "vault", &draft(), &origin(), "")
            .unwrap_err()
            .code,
        KnowledgeErrorCode::InvalidInput
    );
    let receipt = commit(&mut app, "first");
    assert_eq!(
        app.resolve(
            receipt.capture_id,
            "wrong",
            "qusor-raw-text-v1",
            "unicode_scalar"
        )
        .unwrap_err()
        .code,
        KnowledgeErrorCode::Conflict
    );
    let mut export = app.export_all().unwrap();
    export.schema_version = 99;
    assert_eq!(
        validate_archive(&export).unwrap_err().code,
        KnowledgeErrorCode::UnsupportedVersion
    );
}

fn pdf_fixture() -> (
    Vec<u8>,
    core_api::pdf_contract::PdfSnapshot,
    core_api::pdf_contract::PdfDraft,
) {
    use core_api::{contract::sha256, pdf_contract::*};
    let original =
        b"%PDF-1.7\nSynthetic immutable Core fixture, parser validated independently.\n".to_vec();
    let snapshot:PdfSnapshot=serde_json::from_value(serde_json::json!({"protocol_version":1,"original_sha256":sha256(&original),"original_bytes":original.len(),"extraction":{"provider":"pdfium","algorithm":"qusor-pdfium-char-map-v1","wrapper":"pdfium-render-0.9.5","abi":"pdfium_8125","library_sha256":"a".repeat(64),"mapping_check":"unicode_scalar_validity_only","normalization":PDF_NORMALIZATION},"engine_report_sha256":"b".repeat(64),"pages":[{"page_index":0,"engine_text":"hello","text_sha256":sha256(b"hello"),"rotation_degrees":0,"media_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"crop_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"characters":"hello".chars().enumerate().map(|(i,c)|serde_json::json!({"pdf_char_index":i,"unicode_value":c as u32,"mapping":"scalar","generated":false,"scalar_start":i,"scalar_end":i+1,"utf8_start":i,"utf8_end":i+1,"utf16_start":i,"utf16_end":i+1,"pdf_bounds":{"left":i as f32,"bottom":1.0,"right":i as f32+1.0,"top":2.0}})).collect::<Vec<_>>()}]})).unwrap();
    let draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(b"hello"),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 5,
        exact: "hello".into(),
    };
    (original, snapshot, draft)
}
#[test]
fn mixed_operations_share_identity_and_retries_keep_original_receipts() {
    use core_api::pdf_compound::{
        PDF_COMPOUND_PROTOCOL_VERSION, PDF_SELECTION_PROFILE, PdfCompoundDraft,
    };
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let text = commit(&mut app, "text");
    let (original, snapshot, single) = pdf_fixture();
    assert_eq!(
        app.import_document("text", "vault", "PDF", &original, &snapshot)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );
    let imported = app
        .import_document("import", "vault", "PDF", &original, &snapshot)
        .unwrap();
    let one = app
        .capture_pdf(
            "single",
            imported.document_id,
            &single,
            &origin(),
            "My view",
        )
        .unwrap();
    let mut first = single.clone();
    first.end = 2;
    first.exact = "he".into();
    let mut last = single.clone();
    last.start = 2;
    last.exact = "llo".into();
    let compound = PdfCompoundDraft {
        protocol_version: PDF_COMPOUND_PROTOCOL_VERSION,
        original_sha256: single.original_sha256.clone(),
        extraction_revision: single.extraction_revision.clone(),
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments: vec![first, last],
        exact: "he\nllo".into(),
    };
    let multiple = app
        .capture_pdf_compound(
            "compound",
            imported.document_id,
            &compound,
            &origin(),
            "My view",
        )
        .unwrap();
    assert_eq!(commit(&mut app, "text"), text);
    assert_eq!(
        app.import_document("import", "vault", "PDF", &original, &snapshot)
            .unwrap(),
        imported
    );
    assert_eq!(
        app.capture_pdf(
            "single",
            imported.document_id,
            &single,
            &origin(),
            "My view"
        )
        .unwrap(),
        one
    );
    assert_eq!(
        app.capture_pdf_compound(
            "compound",
            imported.document_id,
            &compound,
            &origin(),
            "My view"
        )
        .unwrap(),
        multiple
    );
    assert_eq!(
        app.commit("import", "vault", &draft(), &origin(), "My view")
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(
        app.capture_pdf(
            "import",
            imported.document_id,
            &single,
            &origin(),
            "My view"
        )
        .unwrap_err()
        .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(
        app.capture_pdf_compound(
            "single",
            imported.document_id,
            &compound,
            &origin(),
            "My view"
        )
        .unwrap_err()
        .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(
        app.capture_pdf(
            "compound",
            imported.document_id,
            &single,
            &origin(),
            "My view"
        )
        .unwrap_err()
        .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(
        app.query_pdf(multiple.capture_id).unwrap_err().code,
        KnowledgeErrorCode::UnsupportedVersion
    );
    assert_eq!(app.export_all().unwrap().commits.len(), 4);
}
#[test]
fn oversized_restore_is_a_budget_failure_before_json_parsing() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let oversized = vec![0; LIMIT + 1];
    assert_eq!(
        app.restore_json(&oversized).unwrap_err().code,
        KnowledgeErrorCode::BudgetExceeded
    );
    assert_eq!(
        app.restore_all_json(&oversized).unwrap_err().code,
        KnowledgeErrorCode::BudgetExceeded
    );
    assert!(app.export_all().unwrap().commits.is_empty());
}

#[test]
fn transaction_rechecks_the_validated_input_hash_history() {
    let scratch = Scratch::new();
    let mut store = scratch.store();
    let original = archive(1);
    store.restore(&original).unwrap();
    let mut forged = original.clone();
    forged.text_entries[0].record.user_note = "Changed between snapshot and transaction".into();
    forged.text_entries[0].input_hash = input_hash(&forged.text_entries[0].record).unwrap();
    forged.commits[0].input_hash = forged.text_entries[0].input_hash.clone();
    validate_archive(&forged).unwrap();
    let raw = serde_json::to_string(&forged.text_entries[0].record).unwrap();
    let input = forged.text_entries[0].input_hash.clone();
    let path = scratch.0.join("core.sqlite");
    store.set_observer(Arc::new(move |stage| {
        if stage == StorageStage::BeforeTransaction {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute("UPDATE capture SET record=? WHERE id=1", [&raw])
                .unwrap();
            db.execute(
                "UPDATE operation SET input_hash=? WHERE id='op-0'",
                [&input],
            )
            .unwrap();
            db.execute(
                "UPDATE commit_log SET input_hash=? WHERE commit_rev=1",
                [&input],
            )
            .unwrap();
        }
    }));
    assert_eq!(
        store
            .commit(CommitPlan {
                expected_revision: 1,
                next: archive(2)
            })
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );
    assert_eq!(store.snapshot().unwrap(), forged);
}
