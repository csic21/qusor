#[allow(dead_code)]
mod common;

use core_api::knowledge::KnowledgeErrorCode;
use learning_application::pdf_library::{
    MAX_LIBRARY_PAGE, MAX_LIBRARY_TEXT_CHUNK, MAX_LIBRARY_TEXT_OFFSET, PdfSavedSelection,
    PdfSelectionRange,
};

fn app() -> learning_application::KnowledgeApplication<common::Memory> {
    let (original, snapshot, _) = common::pdf_fixture();
    let mut app = learning_application::KnowledgeApplication::new(common::Memory::empty());
    app.import_document("first", "vault", "First", &original, &snapshot)
        .unwrap();
    app.import_document("second", "vault", "Second", &original, &snapshot)
        .unwrap();
    app.import_document("other", "other-vault", "Other", &original, &snapshot)
        .unwrap();
    app
}

#[test]
fn lists_newest_first_and_isolates_vault() {
    let app = app();
    let page = app.pdf_documents_page("vault", None, 20).unwrap();
    assert_eq!(
        page.documents
            .iter()
            .map(|document| document.title.as_str())
            .collect::<Vec<_>>(),
        ["Second", "First"]
    );
    assert!(page.next_cursor.is_none());
    assert_eq!(page.documents[0].page_count, 1);
    assert_eq!(
        page.documents[0].original_bytes,
        common::pdf_fixture().0.len() as u64
    );
    assert!(
        app.pdf_documents_page("empty", None, 20)
            .unwrap()
            .documents
            .is_empty()
    );
}

#[test]
fn keyset_cursor_is_bounded_and_scope_checked() {
    let app = app();
    let first = app.pdf_documents_page("vault", None, 1).unwrap();
    assert_eq!(first.documents[0].title, "Second");
    let cursor = first.next_cursor.unwrap();
    let second = app.pdf_documents_page("vault", Some(&cursor), 1).unwrap();
    assert_eq!(second.documents[0].title, "First");
    assert!(second.next_cursor.is_none());
    assert!(
        app.pdf_documents_page("other-vault", Some(&cursor), 1)
            .is_err()
    );
    assert!(app.pdf_documents_page("vault", Some("bad"), 1).is_err());
}

#[test]
fn query_limits_and_vault_are_validated() {
    let app = app();
    for limit in [0, MAX_LIBRARY_PAGE + 1] {
        assert_eq!(
            app.pdf_documents_page("vault", None, limit)
                .unwrap_err()
                .code,
            KnowledgeErrorCode::BudgetExceeded
        );
    }
    assert_eq!(
        app.pdf_documents_page("", None, 1).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );
    assert_eq!(
        app.pdf_documents_page(&"v".repeat(257), None, 1)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::InvalidInput
    );
}

#[test]
fn page_text_is_bounded_and_keeps_the_core_source() {
    let app = app();
    let first = app.pdf_page_text("vault", 2, 0, 0, 2048).unwrap();
    assert_eq!(first.document_id, 2);
    assert_eq!(first.page_index, 0);
    assert_eq!(first.offset, 0);
    assert_eq!(first.text, "hello");
    assert_eq!(first.total_bytes, 5);
    assert_eq!(first.next_offset, None);

    let chunk = app
        .pdf_page_text("vault", 2, 0, 0, MAX_LIBRARY_TEXT_CHUNK)
        .unwrap();
    assert_eq!(chunk.text, "hello");
    assert!(app.pdf_page_text("other-vault", 2, 0, 0, 4).is_err());
    assert!(
        app.pdf_page_text("vault", 2, 0, 6, 4)
            .unwrap_err()
            .message
            .contains("UTF-8")
    );
    assert!(
        app.pdf_page_text("vault", 2, 0, MAX_LIBRARY_TEXT_OFFSET + 1, 4)
            .is_err()
    );
}

#[test]
fn a_text_selection_can_be_captured_into_core() {
    let mut app = app();
    let receipt = app
        .capture_pdf_selection(
            "library-capture",
            "vault",
            PdfSelectionRange {
                document_id: 1,
                page_index: 0,
                start: 0,
                end: 5,
            },
            &common::origin(),
            "remember this",
        )
        .unwrap();
    assert_eq!(receipt.document_id, 1);
    assert_eq!(receipt.capture_id, 1);
    let capture = app.query_pdf(receipt.capture_id).unwrap();
    assert_eq!(capture.draft.exact, "hello");
    assert_eq!(capture.user_note, "remember this");
    assert!(
        app.capture_pdf_selection(
            "library-capture",
            "vault",
            PdfSelectionRange {
                document_id: 1,
                page_index: 0,
                start: 0,
                end: 4,
            },
            &common::origin(),
            "",
        )
        .is_err()
    );
}

#[test]
fn saved_selection_ranges_are_read_back_for_the_exact_page() {
    let mut app = app();
    let receipt = app
        .capture_pdf_selection(
            "library-saved-range",
            "vault",
            PdfSelectionRange {
                document_id: 1,
                page_index: 0,
                start: 1,
                end: 4,
            },
            &common::origin(),
            "",
        )
        .unwrap();
    assert_eq!(
        app.pdf_saved_selections("vault", 1, 0).unwrap(),
        vec![learning_application::pdf_library::PdfSavedSelection {
            capture_id: receipt.capture_id,
            start: 1,
            end: 4,
        }]
    );
    assert!(
        app.pdf_saved_selections("other-vault", 1, 0)
            .unwrap_err()
            .code
            == KnowledgeErrorCode::NotFound
    );
    assert!(
        app.pdf_saved_selections("vault", 1, 1).unwrap_err().code == KnowledgeErrorCode::NotFound
    );
}

#[test]
fn saved_ranges_deduplicate_and_split_compound_captures() {
    let mut app = app();
    let first = app
        .capture_pdf_selection(
            "saved-repeat-1",
            "vault",
            PdfSelectionRange {
                document_id: 1,
                page_index: 0,
                start: 1,
                end: 4,
            },
            &common::origin(),
            "",
        )
        .unwrap();
    app.capture_pdf_selection(
        "saved-repeat-2",
        "vault",
        PdfSelectionRange {
            document_id: 1,
            page_index: 0,
            start: 1,
            end: 4,
        },
        &common::origin(),
        "separate intent",
    )
    .unwrap();

    use core_api::pdf_compound::{PDF_COMPOUND_PROTOCOL_VERSION, PDF_SELECTION_PROFILE};
    let (_, _, base) = common::pdf_fixture();
    let mut first_segment = base.clone();
    first_segment.start = 0;
    first_segment.end = 2;
    first_segment.exact = "he".into();
    let mut second_segment = base.clone();
    second_segment.start = 3;
    second_segment.end = 5;
    second_segment.exact = "lo".into();
    let compound = app
        .capture_pdf_compound(
            "saved-compound",
            1,
            &core_api::pdf_compound::PdfCompoundDraft {
                protocol_version: PDF_COMPOUND_PROTOCOL_VERSION,
                original_sha256: base.original_sha256,
                extraction_revision: base.extraction_revision,
                selection_profile: PDF_SELECTION_PROFILE.into(),
                separator: "\n".into(),
                segments: vec![first_segment, second_segment],
                exact: "he\nlo".into(),
            },
            &common::origin(),
            "",
        )
        .unwrap();

    assert_eq!(
        app.pdf_saved_selections("vault", 1, 0).unwrap(),
        vec![
            PdfSavedSelection {
                capture_id: compound.capture_id,
                start: 0,
                end: 2,
            },
            PdfSavedSelection {
                capture_id: first.capture_id,
                start: 1,
                end: 4,
            },
            PdfSavedSelection {
                capture_id: compound.capture_id,
                start: 3,
                end: 5,
            },
        ]
    );
}
