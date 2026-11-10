mod common;

use core_api::knowledge::KnowledgeErrorCode;
use learning_application::pdf_notes::{MAX_NOTE_QUERY, NOTE_PREVIEW_BYTES};

#[test]
fn lists_newest_first_and_isolates_vault_and_document() {
    let (mut app, draft) = common::imported();
    let first = app
        .capture_pdf("note-first", 1, &draft, &common::origin(), "first thought")
        .unwrap();
    let second = app
        .capture_pdf(
            "note-second",
            1,
            &draft,
            &common::origin(),
            "second thought",
        )
        .unwrap();
    let (original, snapshot, other_draft) = common::pdf_fixture();
    app.import_document(
        "import-other",
        "other-vault",
        "Other source",
        &original,
        &snapshot,
    )
    .unwrap();
    app.capture_pdf(
        "note-other",
        2,
        &other_draft,
        &common::origin(),
        "other vault thought",
    )
    .unwrap();

    let notes = app.pdf_notes("vault", None, 20).unwrap();
    assert_eq!(
        notes.iter().map(|note| note.capture_id).collect::<Vec<_>>(),
        vec![second.capture_id, first.capture_id]
    );
    assert_eq!(notes[0].document_title, "Source");
    assert_eq!(notes[0].quote_preview, "hello");
    assert_eq!(notes[0].note_preview, "second thought");
    assert_eq!(notes[0].page_index, 0);
    assert!(!notes[0].compound);

    let current = app.pdf_notes("vault", Some(1), 20).unwrap();
    assert_eq!(current.len(), 2);
    assert!(
        app.pdf_notes("other-vault", Some(1), 20)
            .unwrap()
            .is_empty()
    );
    assert_eq!(app.pdf_notes("other-vault", None, 20).unwrap().len(), 1);
}

#[test]
fn list_previews_are_bounded_without_changing_the_stored_note() {
    let (mut app, draft) = common::imported();
    let long_note = "你".repeat(300);
    let receipt = app
        .capture_pdf("long-note", 1, &draft, &common::origin(), &long_note)
        .unwrap();

    let notes = app.pdf_notes("vault", Some(1), 1).unwrap();
    assert_eq!(notes[0].note_bytes, long_note.len());
    assert!(notes[0].note_truncated);
    assert!(notes[0].note_preview.len() <= NOTE_PREVIEW_BYTES);
    assert_eq!(notes[0].note_preview.len(), 510);
    assert_eq!(
        app.query_pdf(receipt.capture_id).unwrap().user_note,
        long_note
    );
}

#[test]
fn query_limits_and_vault_are_validated() {
    let (app, _) = common::imported();
    for limit in [0, MAX_NOTE_QUERY + 1] {
        let error = app.pdf_notes("vault", None, limit).unwrap_err();
        assert_eq!(error.code, KnowledgeErrorCode::BudgetExceeded);
    }
    let error = app.pdf_notes("", None, 1).unwrap_err();
    assert_eq!(error.code, KnowledgeErrorCode::InvalidInput);
    assert!(
        app.pdf_notes(&"v".repeat(257), None, 1).unwrap_err().code
            == KnowledgeErrorCode::InvalidInput
    );
}
