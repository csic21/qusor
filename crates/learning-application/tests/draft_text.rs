use core_api::knowledge::KnowledgeErrorCode;
use learning_application::draft_text::*;

#[test]
fn utf8_revision_and_atomic_upload() {
    let mut e = DraftTexts::new();
    let o = e.create_owner().unwrap();
    let d = e.create_draft("中😀\r\n\0").unwrap();
    assert!(e.read_chunk(&d, None, 1, 512).is_err());
    assert!(e.read_chunk(&d, None, 0, 2).is_err());
    assert_eq!(e.read_chunk(&d, None, 0, 3).unwrap().text, "中");
    let u = e.begin_replace(&o, &d, 1, 7, 0).unwrap();
    e.append_chunk(&o, &u, 0, "😀", 1).unwrap();
    assert!(e.commit_replace(&o, &u, 1).is_err());
    assert!(e.append_chunk(&o, &u, 0, "中", 1).is_err());
    assert_eq!(e.read_chunk(&d, None, 0, 512).unwrap().text, "中😀\r\n\0");
    e.append_chunk(&o, &u, 4, "中", 2).unwrap();
    assert_eq!(e.commit_replace(&o, &u, 2).unwrap(), 2);
    assert!(e.commit_replace(&o, &u, 2).is_err());
    assert!(e.read_chunk(&d, Some(1), 0, 512).is_err());
    assert_eq!(e.read_chunk(&d, Some(2), 0, 512).unwrap().text, "😀中");
    assert_eq!(e.read_chunk(&d, Some(2), 7, 1).unwrap().text, "");
}
#[test]
fn owner_timeout_and_foreign_identity() {
    let mut e = DraftTexts::new();
    let o = e.create_owner().unwrap();
    let b = e.create_owner().unwrap();
    let d = e.create_draft("").unwrap();
    let u = e.begin_replace(&o, &d, 1, 1, 10).unwrap();
    assert!(e.append_chunk(&b, &u, 0, "a", 11).is_err());
    assert!(e.begin_replace(&b, &d, 1, 0, 11).is_err());
    assert!(e.append_chunk(&o, &u, 0, "a", 40).is_err());
    let u = e.begin_replace(&o, &d, 1, 1, 40).unwrap();
    e.revoke_owner(&o).unwrap();
    assert!(e.append_chunk(&o, &u, 0, "a", 40).is_err());
    let u = e.begin_replace(&b, &d, 1, 0, 40).unwrap();
    assert_eq!(e.commit_replace(&b, &u, 40).unwrap(), 2);
    let mut foreign = DraftTexts::new();
    let fd = foreign.create_draft("").unwrap();
    assert!(e.read_chunk(&fd, None, 0, 512).is_err());
    assert!(foreign.read_chunk(&d, None, 0, 512).is_err());
}
#[test]
fn budgets_and_full_megabyte_roundtrip() {
    let mut e = DraftTexts::new();
    let o = e.create_owner().unwrap();
    let d = e.create_draft("old").unwrap();
    assert_eq!(
        e.begin_replace(&o, &d, 1, MAX_TEXT_BYTES + 1, 0)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::BudgetExceeded
    );
    let u = e.begin_replace(&o, &d, 1, MAX_TEXT_BYTES, 0).unwrap();
    assert!(e.append_chunk(&o, &u, 0, &"a".repeat(513), 0).is_err());
    let chunk = "\0".repeat(512);
    for offset in (0..MAX_TEXT_BYTES).step_by(512) {
        e.append_chunk(&o, &u, offset, &chunk, 0).unwrap();
    }
    e.commit_replace(&o, &u, 0).unwrap();
    for offset in (0..MAX_TEXT_BYTES).step_by(512) {
        assert_eq!(e.read_chunk(&d, Some(2), offset, 512).unwrap().text, chunk);
    }
    for _ in 1..MAX_DRAFTS {
        e.create_draft(&"b".repeat(MAX_TEXT_BYTES)).unwrap();
    }
    assert!(e.create_draft("").is_err());
    assert!(e.reserved_bytes() <= MAX_SESSION_BYTES);
}
#[test]
fn save_unknown_locked_and_exact_retry() {
    let mut e = DraftTexts::new();
    let o = e.create_owner().unwrap();
    let d = e.create_draft("想法").unwrap();
    let s = e.freeze_save(&d, 1, "save-1").unwrap();
    assert_eq!(e.state(&d).unwrap(), DraftState::Saving);
    e.resolve_save(&s, SaveOutcome::Unknown).unwrap();
    assert!(e.begin_replace(&o, &d, 1, 0, 0).is_err());
    assert!(e.remove_draft(&d).is_err());
    assert!(e.freeze_save(&d, 1, "save-2").is_err());
    let retry = e.freeze_save(&d, 1, "save-1").unwrap();
    assert_eq!(e.save_text(&retry).unwrap(), ("save-1", "想法"));
    e.resolve_save(&retry, SaveOutcome::DefinitelyNotCommitted)
        .unwrap();
    assert!(e.save_text(&s).is_err());
    let s = e.freeze_save(&d, 1, "save-2").unwrap();
    e.resolve_save(&s, SaveOutcome::Committed).unwrap();
    e.resolve_save(&s, SaveOutcome::Committed).unwrap();
    assert_eq!(e.state(&d).unwrap(), DraftState::Saved);
    assert!(
        e.resolve_save(&s, SaveOutcome::DefinitelyNotCommitted)
            .is_err()
    );
    assert!(e.begin_replace(&o, &d, 1, 0, 0).is_err());
}
#[test]
fn cancellation_and_monotonic_clock() {
    let mut e = DraftTexts::new();
    let o = e.create_owner().unwrap();
    let d = e.create_draft("x").unwrap();
    let u = e.begin_replace(&o, &d, 1, 1, 100).unwrap();
    e.append_chunk(&o, &u, 0, "y", 90).unwrap();
    assert!(e.freeze_save(&d, 1, "save").is_err());
    e.expire_uploads(130);
    assert!(e.commit_replace(&o, &u, 1).is_err());
    assert_eq!(e.read_chunk(&d, None, 0, 512).unwrap().text, "x");
    let u = e.begin_replace(&o, &d, 1, 1, 130).unwrap();
    e.cancel_replace(&o, &u).unwrap();
    assert!(e.append_chunk(&o, &u, 0, "z", 130).is_err());
}
