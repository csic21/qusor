#[allow(dead_code)]
mod common;
use common::*;
use learning_application::annotation_source::FrozenPdfSource;
#[test]
fn freeze_checks_document_before_any_capture_and_keeps_exact_facts() {
    let (app, draft) = imported();
    assert!(FrozenPdfSource::freeze(&app, 1, "another-vault", draft.clone(), origin()).is_err());
    let mut wrong = draft.clone();
    wrong.exact = "other".into();
    assert!(FrozenPdfSource::freeze(&app, 1, "vault", wrong, origin()).is_err());
    let frozen = FrozenPdfSource::freeze(&app, 1, "vault", draft.clone(), origin()).unwrap();
    assert_eq!(frozen.selection(), &draft);
    assert_eq!(frozen.provenance(), &origin());
    assert_eq!(frozen.document_id(), 1);
    assert_eq!(frozen.vault(), "vault");
    assert!(app.export_all().unwrap().pdf_captures.is_empty());
}
#[test]
fn unknown_commit_can_be_reconciled_without_resubmitting_or_replacing_identity() {
    let (mut app, draft) = imported();
    let frozen = FrozenPdfSource::freeze(&app, 1, "vault", draft, origin()).unwrap();
    assert!(
        frozen
            .receipt(&app, "save", "想法🙂\r\n\0")
            .unwrap()
            .is_none()
    );
    app.store_mut().lose_reply = true;
    assert!(frozen.capture(&mut app, "save", "想法🙂\r\n\0").is_err());
    let receipt = frozen
        .receipt(&app, "save", "想法🙂\r\n\0")
        .unwrap()
        .unwrap();
    assert_eq!(receipt.capture_id, 1);
    assert_eq!(app.export_all().unwrap().pdf_captures.len(), 1);
    assert_eq!(
        frozen.capture(&mut app, "save", "想法🙂\r\n\0").unwrap(),
        receipt
    );
    assert!(frozen.receipt(&app, "save", "changed").is_err());
    let mut other = origin();
    other.installation_id = 2;
    let other =
        FrozenPdfSource::freeze(&app, 1, "vault", frozen.selection().clone(), other).unwrap();
    assert!(other.receipt(&app, "save", "想法🙂\r\n\0").is_err());
    assert!(frozen.receipt(&app, "import", "").is_err());
    assert_eq!(app.export_all().unwrap().commits.len(), 2);
}

#[test]
fn chunked_draft_save_freezes_text_and_reconciles_core_before_unlocking() {
    use learning_application::draft_text::{DraftState, DraftTexts, SaveOutcome};
    let (mut app, source) = imported();
    let source = FrozenPdfSource::freeze(&app, 1, "vault", source, origin()).unwrap();
    let mut drafts = DraftTexts::new();
    let owner = drafts.create_owner().unwrap();
    let id = drafts.create_draft("old").unwrap();
    let text = "想法🙂\r\n\0\n".repeat(100);
    let upload = drafts.begin_replace(&owner, &id, 1, text.len(), 0).unwrap();
    let mut offset = 0;
    while offset < text.len() {
        let mut end = (offset + 512).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        offset = drafts
            .append_chunk(&owner, &upload, offset, &text[offset..end], 1)
            .unwrap();
    }
    let revision = drafts.commit_replace(&owner, &upload, 2).unwrap();
    let frozen = drafts
        .freeze_save(&id, revision, "annotation-save")
        .unwrap();
    assert!(drafts.begin_replace(&owner, &id, revision, 0, 3).is_err());
    app.store_mut().lose_reply = true;
    let (op, note) = drafts.save_text(&frozen).unwrap();
    assert!(source.capture(&mut app, op, note).is_err());
    drafts.resolve_save(&frozen, SaveOutcome::Unknown).unwrap();
    assert_eq!(drafts.state(&id).unwrap(), DraftState::Saving);
    let (op, note) = drafts.save_text(&frozen).unwrap();
    let receipt = source.receipt(&app, op, note).unwrap().unwrap();
    assert_eq!(app.query_pdf(receipt.capture_id).unwrap().user_note, text);
    drafts
        .resolve_save(&frozen, SaveOutcome::Committed)
        .unwrap();
    assert_eq!(drafts.state(&id).unwrap(), DraftState::Saved);
    assert_eq!(app.export_all().unwrap().pdf_captures.len(), 1);
}
