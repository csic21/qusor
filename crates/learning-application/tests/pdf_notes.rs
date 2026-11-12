#[allow(dead_code)]
mod common;

use core_api::knowledge::KnowledgeErrorCode;
use learning_application::pdf_notes::{
    MAX_NOTE_CHUNK_BYTES, MAX_NOTE_PAGE, NOTE_PREVIEW_BYTES, NoteBodyField,
};

fn capture(
    app: &mut learning_application::KnowledgeApplication<common::Memory>,
    operation: &str,
    document_id: i64,
    note: &str,
) -> i64 {
    let (_, _, draft) = common::pdf_fixture();
    app.capture_pdf(operation, document_id, &draft, &common::origin(), note)
        .unwrap()
        .capture_id
}

fn compound_capture(
    app: &mut learning_application::KnowledgeApplication<common::Memory>,
    operation: &str,
    document_id: i64,
    note: &str,
) -> i64 {
    use core_api::pdf_compound::{PDF_COMPOUND_PROTOCOL_VERSION, PDF_SELECTION_PROFILE};
    let (_, _, base) = common::pdf_fixture();
    let mut first = base.clone();
    first.start = 0;
    first.end = 2;
    first.exact = "he".into();
    let mut second = base.clone();
    second.start = 3;
    second.end = 5;
    second.exact = "lo".into();
    let compound = core_api::pdf_compound::PdfCompoundDraft {
        protocol_version: PDF_COMPOUND_PROTOCOL_VERSION,
        original_sha256: base.original_sha256.clone(),
        extraction_revision: base.extraction_revision.clone(),
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments: vec![first, second],
        exact: "he\nlo".into(),
    };
    app.capture_pdf_compound(operation, document_id, &compound, &common::origin(), note)
        .unwrap()
        .capture_id
}

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

    let page = app.pdf_notes_page("vault", None, None, 20).unwrap();
    assert_eq!(
        page.notes
            .iter()
            .map(|note| note.capture_id)
            .collect::<Vec<_>>(),
        vec![second.capture_id, first.capture_id]
    );
    assert!(page.next_cursor.is_none());
    assert_eq!(page.notes[0].document_title, "Source");
    assert_eq!(page.notes[0].quote_preview, "hello");
    assert_eq!(page.notes[0].note_preview, "second thought");
    assert_eq!(page.notes[0].page_index, 0);
    assert!(!page.notes[0].compound);

    let current = app.pdf_notes_page("vault", Some(1), None, 20).unwrap();
    assert_eq!(current.notes.len(), 2);
    assert!(
        app.pdf_notes_page("other-vault", Some(1), None, 20)
            .unwrap()
            .notes
            .is_empty()
    );
    assert_eq!(
        app.pdf_notes_page("other-vault", None, None, 20)
            .unwrap()
            .notes
            .len(),
        1
    );
}

#[test]
fn list_previews_are_bounded_without_changing_the_stored_note() {
    let (mut app, draft) = common::imported();
    let long_note = "你".repeat(300);
    let receipt = app
        .capture_pdf("long-note", 1, &draft, &common::origin(), &long_note)
        .unwrap();

    let page = app.pdf_notes_page("vault", Some(1), None, 1).unwrap();
    assert_eq!(page.notes[0].note_bytes, long_note.len());
    assert!(page.notes[0].note_truncated);
    assert!(page.notes[0].note_preview.len() <= NOTE_PREVIEW_BYTES);
    assert_eq!(page.notes[0].note_preview.len(), 510);
    assert_eq!(
        app.query_pdf(receipt.capture_id).unwrap().user_note,
        long_note
    );
}

#[test]
fn query_limits_and_vault_are_validated() {
    let (app, _) = common::imported();
    for limit in [0, MAX_NOTE_PAGE + 1] {
        let error = app.pdf_notes_page("vault", None, None, limit).unwrap_err();
        assert_eq!(error.code, KnowledgeErrorCode::BudgetExceeded);
    }
    let error = app.pdf_notes_page("", None, None, 1).unwrap_err();
    assert_eq!(error.code, KnowledgeErrorCode::InvalidInput);
    assert_eq!(
        app.pdf_notes_page(&"v".repeat(257), None, None, 1)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::InvalidInput
    );
}

#[test]
fn keyset_pages_stay_stable_while_new_notes_arrive() {
    let (mut app, _) = common::imported();
    let mut ids = Vec::new();
    for index in 1..=5 {
        ids.push(capture(&mut app, &format!("note-{index}"), 1, "thought"));
    }
    let first = app.pdf_notes_page("vault", None, None, 2).unwrap();
    assert_eq!(
        first
            .notes
            .iter()
            .map(|note| note.capture_id)
            .collect::<Vec<_>>(),
        vec![ids[4], ids[3]]
    );
    let first_cursor = first.next_cursor.clone().unwrap();
    assert_eq!(
        first_cursor,
        format!("n1:*:{}:{}", first.notes[1].commit_rev, ids[3])
    );

    let late = capture(&mut app, "note-late", 1, "arrived during paging");
    let second = app
        .pdf_notes_page("vault", None, Some(&first_cursor), 2)
        .unwrap();
    assert_eq!(
        second
            .notes
            .iter()
            .map(|note| note.capture_id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[1]]
    );
    let second_cursor = second.next_cursor.clone().unwrap();
    let third = app
        .pdf_notes_page("vault", None, Some(&second_cursor), 2)
        .unwrap();
    assert_eq!(
        third
            .notes
            .iter()
            .map(|note| note.capture_id)
            .collect::<Vec<_>>(),
        vec![ids[0]]
    );
    assert!(third.next_cursor.is_none());

    let refreshed = app.pdf_notes_page("vault", None, None, 2).unwrap();
    assert_eq!(
        refreshed
            .notes
            .iter()
            .map(|note| note.capture_id)
            .collect::<Vec<_>>(),
        vec![late, ids[4]]
    );
}

#[test]
fn cursors_are_rejected_outside_their_exact_scope() {
    let (mut app, _) = common::imported();
    capture(&mut app, "note-one", 1, "one");
    capture(&mut app, "note-two", 1, "two");
    let (original, snapshot, other_draft) = common::pdf_fixture();
    app.import_document("import-other", "other-vault", "Other", &original, &snapshot)
        .unwrap();
    app.capture_pdf(
        "note-other-one",
        2,
        &other_draft,
        &common::origin(),
        "other one",
    )
    .unwrap();
    app.capture_pdf(
        "note-other-two",
        2,
        &other_draft,
        &common::origin(),
        "other two",
    )
    .unwrap();

    let vault_cursor = app
        .pdf_notes_page("vault", None, None, 1)
        .unwrap()
        .next_cursor
        .unwrap();
    let document_cursor = app
        .pdf_notes_page("vault", Some(1), None, 1)
        .unwrap()
        .next_cursor
        .unwrap();
    let other_cursor = app
        .pdf_notes_page("other-vault", None, None, 1)
        .unwrap()
        .next_cursor
        .unwrap();

    for (cursor, document) in [
        (vault_cursor.as_str(), Some(1)),
        (document_cursor.as_str(), None),
        (other_cursor.as_str(), None),
        ("n1:*:99:99", None),
        ("n1:*:0:1", None),
        ("n1:*:1", None),
        ("n1:1:1:1", None),
    ] {
        assert_eq!(
            app.pdf_notes_page("vault", document, Some(cursor), 1)
                .unwrap_err()
                .code,
            KnowledgeErrorCode::InvalidInput,
            "{cursor}"
        );
    }
}

#[test]
fn body_chunks_rebuild_utf8_notes_without_rewriting() {
    let (mut app, _) = common::imported();
    let note = "想法😀\u{0}".repeat(200);
    let capture_id = capture(&mut app, "long-utf8", 1, &note);
    let mut offset = 0;
    let mut chunks = 0;
    let mut rebuilt = String::new();
    loop {
        let chunk = app
            .pdf_note_body("vault", capture_id, NoteBodyField::Note, offset, 7)
            .unwrap();
        assert_eq!(chunk.capture_id, capture_id);
        assert_eq!(chunk.document_id, 1);
        assert_eq!(chunk.field, NoteBodyField::Note);
        assert_eq!(chunk.total_bytes, note.len());
        assert!(!chunk.text.is_empty());
        rebuilt.push_str(&chunk.text);
        chunks += 1;
        match chunk.next_offset {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(rebuilt, note);
    assert!(chunks > 1);
}

#[test]
fn one_mib_body_is_read_in_bounded_chunks() {
    let (mut app, _) = common::imported();
    let note = "😀".repeat(1024 * 1024 / 4);
    assert_eq!(note.len(), 1024 * 1024);
    let capture_id = capture(&mut app, "one-mib", 1, &note);
    let mut offset = 0;
    let mut chunks = 0;
    let mut rebuilt = String::new();
    loop {
        let chunk = app
            .pdf_note_body(
                "vault",
                capture_id,
                NoteBodyField::Note,
                offset,
                MAX_NOTE_CHUNK_BYTES,
            )
            .unwrap();
        assert!(chunk.text.len() <= MAX_NOTE_CHUNK_BYTES);
        rebuilt.push_str(&chunk.text);
        chunks += 1;
        match chunk.next_offset {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(rebuilt, note);
    assert_eq!(chunks, 1024 * 1024 / MAX_NOTE_CHUNK_BYTES);
}

#[test]
fn body_offsets_and_budgets_are_explicit() {
    let (mut app, _) = common::imported();
    let note = "a😀你b";
    let capture_id = capture(&mut app, "utf8-bounds", 1, note);

    let end = app
        .pdf_note_body("vault", capture_id, NoteBodyField::Note, note.len(), 4)
        .unwrap();
    assert!(end.text.is_empty());
    assert_eq!(end.next_offset, None);

    for offset in [note.len() + 1, 2] {
        assert_eq!(
            app.pdf_note_body("vault", capture_id, NoteBodyField::Note, offset, 4)
                .unwrap_err()
                .code,
            KnowledgeErrorCode::InvalidInput,
            "{offset}"
        );
    }
    for max_bytes in [0, 3, MAX_NOTE_CHUNK_BYTES + 1] {
        assert_eq!(
            app.pdf_note_body("vault", capture_id, NoteBodyField::Note, 0, max_bytes)
                .unwrap_err()
                .code,
            KnowledgeErrorCode::BudgetExceeded,
            "{max_bytes}"
        );
    }
    assert_eq!(
        app.pdf_note_body("", capture_id, NoteBodyField::Note, 0, 4)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::InvalidInput
    );
}

#[test]
fn body_isolates_vaults_and_reports_missing_captures() {
    let (mut app, _) = common::imported();
    let capture_id = capture(&mut app, "note-one", 1, "one");
    let (original, snapshot, other_draft) = common::pdf_fixture();
    app.import_document("import-other", "other-vault", "Other", &original, &snapshot)
        .unwrap();
    let other = app
        .capture_pdf("note-other", 2, &other_draft, &common::origin(), "other")
        .unwrap()
        .capture_id;

    for id in [other, 999] {
        assert_eq!(
            app.pdf_note_body("vault", id, NoteBodyField::Note, 0, 4)
                .unwrap_err()
                .code,
            KnowledgeErrorCode::NotFound,
            "{id}"
        );
    }
    assert_eq!(
        app.pdf_note_body("other-vault", capture_id, NoteBodyField::Note, 0, 4)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    let other_chunk = app
        .pdf_note_body("other-vault", other, NoteBodyField::Note, 0, 4)
        .unwrap();
    assert_eq!(other_chunk.text, "othe");
}

#[test]
fn quote_body_returns_the_stored_source_text() {
    let (mut app, _) = common::imported();
    let single = capture(&mut app, "single-quote", 1, "view");
    let quote = app
        .pdf_note_body(
            "vault",
            single,
            NoteBodyField::Quote,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(quote.text, "hello");
    assert_eq!(quote.total_bytes, 5);
    assert_eq!(quote.next_offset, None);

    let compound = compound_capture(&mut app, "compound-quote", 1, "view");
    let page = app.pdf_notes_page("vault", Some(1), None, 1).unwrap();
    assert_eq!(page.notes[0].capture_id, compound);
    assert!(page.notes[0].compound);
    assert_eq!(page.notes[0].quote_preview, "he\nlo");
    let quote = app
        .pdf_note_body(
            "vault",
            compound,
            NoteBodyField::Quote,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(quote.text, "he\nlo");
    assert_eq!(quote.total_bytes, 5);
}
