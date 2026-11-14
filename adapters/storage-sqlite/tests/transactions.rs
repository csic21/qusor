#![cfg(unix)]
#![allow(deprecated)]
use core_api::{contract::CaptureDraft, knowledge::*};
use learning_application::KnowledgeApplication;
use learning_application::pdf_notes::{MAX_NOTE_CHUNK_BYTES, NoteBodyField};
use learning_ports::archive::{input_hash, validate_archive};
use learning_ports::{CommitPlan, KnowledgeCommand, KnowledgeStore};
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
fn commit<S: KnowledgeCommand>(app: &mut KnowledgeApplication<S>, op: &str) -> Receipt {
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
fn note_index_survives_sqlite_reopen_with_bounded_preview() {
    let scratch = Scratch::new();
    let (original, snapshot, selection) = pdf_fixture();
    let long_note = "想法😀".repeat(200);
    let receipt = {
        let mut app = KnowledgeApplication::new(scratch.store());
        let imported = app
            .import_document("import-note-index", "vault", "PDF", &original, &snapshot)
            .unwrap();
        app.capture_pdf(
            "note-index",
            imported.document_id,
            &selection,
            &origin(),
            &long_note,
        )
        .unwrap()
    };

    let reopened = KnowledgeApplication::new(scratch.store());
    let page = reopened.pdf_notes_page("vault", None, None, 10).unwrap();
    assert_eq!(page.notes.len(), 1);
    assert!(page.next_cursor.is_none());
    assert_eq!(page.notes[0].capture_id, receipt.capture_id);
    assert_eq!(page.notes[0].document_title, "PDF");
    assert_eq!(page.notes[0].quote_preview, "hello");
    assert_eq!(page.notes[0].note_bytes, long_note.len());
    assert!(page.notes[0].note_truncated);
    assert!(page.notes[0].note_preview.len() <= 512);
    let chunk = reopened
        .pdf_note_body(
            "vault",
            receipt.capture_id,
            NoteBodyField::Note,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(chunk.text, long_note);
    assert_eq!(chunk.total_bytes, long_note.len());
    assert_eq!(chunk.next_offset, None);
    assert_eq!(
        reopened.query_pdf(receipt.capture_id).unwrap().user_note,
        long_note
    );
}

#[test]
fn library_index_survives_sqlite_reopen_with_stable_pages() {
    let scratch = Scratch::new();
    let (original, snapshot, _) = pdf_fixture();
    {
        let mut app = KnowledgeApplication::new(scratch.store());
        app.import_document("library-first", "vault", "First", &original, &snapshot)
            .unwrap();
        app.import_document("library-second", "vault", "Second", &original, &snapshot)
            .unwrap();
        app.import_document(
            "library-other",
            "other-vault",
            "Other",
            &original,
            &snapshot,
        )
        .unwrap();
    }

    let reopened = KnowledgeApplication::new(scratch.store());
    let first = reopened.pdf_documents_page("vault", None, 1).unwrap();
    assert_eq!(first.documents[0].title, "Second");
    assert_eq!(first.documents[0].original_bytes, original.len() as u64);
    let cursor = first.next_cursor.unwrap();
    let second = reopened
        .pdf_documents_page("vault", Some(&cursor), 1)
        .unwrap();
    assert_eq!(second.documents[0].title, "First");
    assert!(second.next_cursor.is_none());
    assert!(
        reopened
            .pdf_documents_page("vault", Some(&cursor), 1)
            .is_ok()
    );
    assert!(
        reopened
            .pdf_documents_page("other-vault", Some(&cursor), 1)
            .is_err()
    );
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

#[test]
fn update_pdf_capture_note_validates_scopes_audits_and_persists() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let (original, snapshot, draft) = pdf_fixture();
    let doc = app
        .import_document("op-pdf-edit-import", "vault", "PDF", &original, &snapshot)
        .unwrap();
    let cap = app
        .capture_pdf(
            "op-pdf-edit-cap",
            doc.document_id,
            &draft,
            &origin(),
            "Initial note",
        )
        .unwrap();

    // Wrong vault -> NotFound
    let wrong_vault = app
        .update_pdf_capture_note("other", cap.capture_id, "Updated")
        .unwrap_err();
    assert_eq!(wrong_vault.code, KnowledgeErrorCode::NotFound);

    // Control char -> InvalidInput
    let ctrl = app
        .update_pdf_capture_note("vault", cap.capture_id, "bad\u{0000}note")
        .unwrap_err();
    assert_eq!(ctrl.code, KnowledgeErrorCode::InvalidInput);

    // Missing capture -> NotFound
    let missing = app
        .update_pdf_capture_note("vault", 9999, "Updated")
        .unwrap_err();
    assert_eq!(missing.code, KnowledgeErrorCode::NotFound);

    // Valid update persists across reopen and appends knowledge_audit
    app.update_pdf_capture_note("vault", cap.capture_id, "Revised PDF note")
        .unwrap();
    let body = app
        .pdf_note_body(
            "vault",
            cap.capture_id,
            NoteBodyField::Note,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(body.text, "Revised PDF note");
    drop(app);

    let db = rusqlite::Connection::open(scratch.0.join("core.sqlite")).unwrap();
    let audit_count: i64 = db
        .query_row(
            "SELECT count(*) FROM knowledge_audit WHERE vault='vault' AND action='update_pdf_capture_note' AND entity_kind='pdf_capture' AND entity_id=?1",
            [cap.capture_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);

    let reopened = KnowledgeApplication::new(scratch.store());
    let page = reopened.pdf_notes_page("vault", None, None, 10).unwrap();
    assert_eq!(page.notes.len(), 1);
    assert_eq!(page.notes[0].note_preview, "Revised PDF note");
}

#[test]
fn delete_pdf_capture_audits_exports_and_restores_into_empty_core() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let (original, snapshot, draft) = pdf_fixture();
    let doc = app
        .import_document("op-pdf-delcap-import", "vault", "PDF", &original, &snapshot)
        .unwrap();
    let cap1 = app
        .capture_pdf(
            "op-pdf-delcap-1",
            doc.document_id,
            &draft,
            &origin(),
            "First",
        )
        .unwrap();
    let mut draft2 = draft.clone();
    draft2.end = 2;
    draft2.exact = "he".into();
    let cap2 = app
        .capture_pdf(
            "op-pdf-delcap-2",
            doc.document_id,
            &draft2,
            &origin(),
            "Second",
        )
        .unwrap();

    assert_eq!(
        app.delete_pdf_capture("other", cap1.capture_id)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    app.delete_pdf_capture("vault", cap1.capture_id).unwrap();
    assert_eq!(
        app.delete_pdf_capture("vault", cap1.capture_id)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );

    let page = app.pdf_notes_page("vault", None, None, 10).unwrap();
    assert_eq!(page.notes.len(), 1);
    assert_eq!(page.notes[0].capture_id, cap2.capture_id);

    let db = rusqlite::Connection::open(scratch.0.join("core.sqlite")).unwrap();
    let audit_count: i64 = db
        .query_row(
            "SELECT count(*) FROM knowledge_audit WHERE vault='vault' AND action='delete_pdf_capture' AND entity_kind='pdf_capture' AND entity_id=?1",
            [cap1.capture_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);

    // Export and restore into empty Core, then capture a third note to prove ID allocation & commit_rev continue cleanly.
    let exported = app.export_all().unwrap();
    let scratch2 = Scratch::new();
    let mut restored = KnowledgeApplication::new(scratch2.store());
    restored.restore_all(&exported).unwrap();
    let restored_notes = restored.pdf_notes_page("vault", None, None, 10).unwrap();
    assert_eq!(restored_notes.notes.len(), 1);
    assert_eq!(restored_notes.notes[0].capture_id, cap2.capture_id);

    let cap3 = restored
        .capture_pdf(
            "op-pdf-delcap-3",
            doc.document_id,
            &draft,
            &origin(),
            "Third",
        )
        .unwrap();
    assert!(cap3.capture_id > cap2.capture_id);
}

#[test]
fn delete_pdf_document_cascades_audits_exports_and_restores_into_empty_core() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let (original, snapshot, draft) = pdf_fixture();
    let doc1 = app
        .import_document("op-pdf-deldoc-1", "vault", "PDF 1", &original, &snapshot)
        .unwrap();
    let _cap1 = app
        .capture_pdf(
            "op-pdf-deldoc-cap1",
            doc1.document_id,
            &draft,
            &origin(),
            "Note 1",
        )
        .unwrap();
    let doc2 = app
        .import_document("op-pdf-deldoc-2", "other", "PDF 2", &original, &snapshot)
        .unwrap();
    let cap2 = app
        .capture_pdf(
            "op-pdf-deldoc-cap2",
            doc2.document_id,
            &draft,
            &origin(),
            "Note 2",
        )
        .unwrap();

    assert_eq!(
        app.delete_pdf_document("other", doc1.document_id)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    app.delete_pdf_document("vault", doc1.document_id).unwrap();

    assert!(
        app.pdf_documents_page("vault", None, 10)
            .unwrap()
            .documents
            .is_empty()
    );
    assert!(
        app.pdf_notes_page("vault", None, None, 10)
            .unwrap()
            .notes
            .is_empty()
    );
    assert_eq!(
        app.pdf_documents_page("other", None, 10)
            .unwrap()
            .documents
            .len(),
        1
    );
    assert_eq!(
        app.pdf_notes_page("other", None, None, 10)
            .unwrap()
            .notes
            .len(),
        1
    );

    let db = rusqlite::Connection::open(scratch.0.join("core.sqlite")).unwrap();
    let audit_count: i64 = db
        .query_row(
            "SELECT count(*) FROM knowledge_audit WHERE vault='vault' AND action='delete_pdf_document' AND entity_kind='pdf_document' AND entity_id=?1",
            [doc1.document_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);

    let exported = app.export_all().unwrap();
    let scratch2 = Scratch::new();
    let mut restored = KnowledgeApplication::new(scratch2.store());
    restored.restore_all(&exported).unwrap();
    assert!(
        restored
            .pdf_documents_page("vault", None, 10)
            .unwrap()
            .documents
            .is_empty()
    );
    let other_notes = restored.pdf_notes_page("other", None, None, 10).unwrap();
    assert_eq!(other_notes.notes.len(), 1);
    assert_eq!(other_notes.notes[0].capture_id, cap2.capture_id);
}
