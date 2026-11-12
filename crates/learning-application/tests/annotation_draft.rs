#[allow(dead_code)]
mod common;
use common::*;
use learning_application::annotation_draft::AnnotationDrafts;
#[test]
fn source_binding_unknown_save_and_foreign_handles() {
    use learning_application::draft_text::DraftState;
    let (mut app, selection) = imported();
    let mut a = AnnotationDrafts::new();
    let mut b = AnnotationDrafts::new();
    let id = a
        .create_pdf(&app, 1, "vault", selection.clone(), origin(), 7)
        .unwrap();
    let other = b
        .create_pdf(&app, 1, "vault", selection, origin(), 8)
        .unwrap();
    assert!(a.source(&other).is_err());
    assert_eq!(a.source(&id).unwrap().1, 7);
    let owner = a.create_owner().unwrap();
    let u = a
        .begin_replace(&owner, &id, 1, "想法🙂\0".len(), 0)
        .unwrap();
    a.append_chunk(&owner, &u, 0, "想法🙂\0", 0).unwrap();
    let rev = a.commit_replace(&owner, &u, 0).unwrap();
    let frozen = a.freeze_save(&id, rev, "save-bound").unwrap();
    assert!(b.commit(&mut app, &frozen).is_err());
    assert!(a.reconcile(&app, &frozen).unwrap().is_none());
    assert_eq!(a.state(&id).unwrap(), DraftState::Saving);
    assert!(a.remove(&id).is_err());
    app.store_mut().lose_reply = true;
    assert!(a.commit(&mut app, &frozen).is_err());
    assert_eq!(a.state(&id).unwrap(), DraftState::Saving);
    let receipt = a.reconcile(&app, &frozen).unwrap().unwrap();
    assert_eq!(a.state(&id).unwrap(), DraftState::Saved);
    assert_eq!(
        app.query_pdf(receipt.capture_id).unwrap().user_note,
        "想法🙂\0"
    );
    assert_eq!(app.export_all().unwrap().pdf_captures.len(), 1);
    a.remove(&id).unwrap();
    assert!(a.commit(&mut app, &frozen).is_err());
    assert_eq!(a.source_bytes(), 0);
    assert_eq!(a.reserved_text_bytes(), 0);
}
#[test]
fn invalid_sources_and_count_leave_no_partial_draft() {
    let (app, selection) = imported();
    let mut a = AnnotationDrafts::new();
    assert!(
        a.create_pdf(&app, 1, "vault", selection.clone(), origin(), 0)
            .is_err()
    );
    assert!(
        a.create_pdf(&app, 1, "wrong", selection.clone(), origin(), 1)
            .is_err()
    );
    assert_eq!(a.source_bytes(), 0);
    for _ in 0..8 {
        a.create_pdf(&app, 1, "vault", selection.clone(), origin(), 1)
            .unwrap();
    }
    assert!(
        a.create_pdf(&app, 1, "vault", selection, origin(), 1)
            .is_err()
    );
}

#[test]
fn two_documents_keep_source_text_and_generation_paired() {
    let (mut app, selection) = imported();
    let (original, snapshot, _) = pdf_fixture();
    let second = app
        .import_document("import-second", "vault", "Second", &original, &snapshot)
        .unwrap();
    let mut drafts = AnnotationDrafts::new();
    let a = drafts
        .create_pdf(&app, 1, "vault", selection.clone(), origin(), 11)
        .unwrap();
    let mut second_origin = origin();
    second_origin.installation_id = 2;
    let b = drafts
        .create_pdf(
            &app,
            second.document_id,
            "vault",
            selection,
            second_origin.clone(),
            22,
        )
        .unwrap();
    for (id, text, operation, generation) in [
        (&a, "first note", "save-first", 11),
        (&b, "第二想法", "save-second", 22),
    ] {
        let owner = drafts.create_owner().unwrap();
        let upload = drafts.begin_replace(&owner, id, 1, text.len(), 0).unwrap();
        drafts.append_chunk(&owner, &upload, 0, text, 0).unwrap();
        drafts.commit_replace(&owner, &upload, 0).unwrap();
        let frozen = drafts.freeze_save(id, 2, operation).unwrap();
        assert_eq!(drafts.save_source(&frozen).unwrap().1, generation);
        let receipt = drafts.commit(&mut app, &frozen).unwrap();
        let record = app.query_pdf(receipt.capture_id).unwrap();
        assert_eq!(record.user_note, text);
        assert_eq!(
            receipt.document_id,
            if generation == 11 {
                1
            } else {
                second.document_id
            }
        );
        assert_eq!(
            record.provenance,
            if generation == 11 {
                origin()
            } else {
                second_origin.clone()
            }
        );
    }
    assert_eq!(app.export_all().unwrap().pdf_captures.len(), 2);
}
