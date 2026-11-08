mod common;
use common::*;
use learning_application::{annotation_draft::AnnotationDrafts, draft_text::*, provider_events::*};
fn fixture() -> (AnnotationDrafts, DraftId) {
    let (app, selection) = imported();
    let mut drafts = AnnotationDrafts::new();
    let id = drafts
        .create_pdf(&app, 1, "vault", selection, origin(), 1)
        .unwrap();
    (drafts, id)
}
fn request(
    cursor: EventCursor,
    expected: u64,
    effect: Option<DraftEffect>,
    checkpoint: Vec<u8>,
    complete: bool,
) -> AdvanceEvent {
    AdvanceEvent {
        cursor,
        expected_checkpoint_revision: expected,
        effect,
        checkpoint,
        complete,
    }
}
fn begin(
    events: &mut ProviderEvents,
    drafts: &mut AnnotationDrafts,
    mount: &EventMount,
    cursor: EventCursor,
    total: usize,
    now: u64,
) -> AdvanceReceipt {
    events
        .advance(
            drafts,
            mount,
            request(
                cursor,
                1,
                Some(DraftEffect::BeginReplace {
                    expected_revision: cursor.base_revision,
                    total_bytes: total,
                }),
                vec![1],
                false,
            ),
            now,
        )
        .unwrap()
}
#[test]
fn one_mib_chunked_effects_lost_replies_checkpoint_and_core_receipt() {
    let (mut app, selection) = imported();
    let mut drafts = AnnotationDrafts::new();
    let id = drafts
        .create_pdf(&app, 1, "vault", selection, origin(), 9)
        .unwrap();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let mut input = "文🙂\0\r\n".repeat(MAX_TEXT_BYTES / 10);
    input.extend(std::iter::repeat_n('x', MAX_TEXT_BYTES - input.len()));
    assert_eq!(input.len(), MAX_TEXT_BYTES);
    let event = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            input.clone(),
            0,
        )
        .unwrap();
    assert_eq!(event.checkpoint_revision, 1);
    let cursor = event.cursor;
    let mut reconstructed = String::new();
    let mut offset = 0;
    while offset < input.len() {
        let c = events
            .read_input_chunk(&mut drafts, &mount, cursor, offset, 512, 1)
            .unwrap();
        assert!(!c.text.is_empty());
        offset += c.text.len();
        reconstructed.push_str(&c.text);
    }
    assert_eq!(input, reconstructed);
    let first = request(
        cursor,
        1,
        Some(DraftEffect::BeginReplace {
            expected_revision: 1,
            total_bytes: input.len(),
        }),
        vec![1],
        false,
    );
    let started = events
        .advance(&mut drafts, &mount, first.clone(), 1)
        .unwrap();
    assert_eq!(
        started,
        events.advance(&mut drafts, &mount, first, 1).unwrap()
    );
    let mut checkpoint = started.checkpoint_revision;
    offset = 0;
    while offset < input.len() {
        let mut end = (offset + 512).min(input.len());
        while !input.is_char_boundary(end) {
            end -= 1;
        }
        let req = request(
            cursor,
            checkpoint,
            Some(DraftEffect::AppendChunk {
                offset,
                text: input[offset..end].into(),
            }),
            (end as u64).to_be_bytes().to_vec(),
            false,
        );
        let receipt = events.advance(&mut drafts, &mount, req.clone(), 1).unwrap();
        assert_eq!(
            receipt,
            events.advance(&mut drafts, &mount, req, 1).unwrap()
        );
        checkpoint = receipt.checkpoint_revision;
        offset = end;
    }
    assert_eq!(drafts.read_chunk(&id, None, 0, 512).unwrap().revision, 1);
    let final_checkpoint = vec![255; 8192];
    let commit = request(
        cursor,
        checkpoint,
        Some(DraftEffect::CommitReplace),
        final_checkpoint.clone(),
        true,
    );
    let done = events
        .advance(&mut drafts, &mount, commit.clone(), 1)
        .unwrap();
    assert_eq!(done.state, EventState::Completed);
    assert_eq!(
        done.last_effect,
        Some(EffectOutcome::Replaced { revision: 2 })
    );
    assert_eq!(
        done,
        events
            .advance(&mut drafts, &mount, commit.clone(), 2)
            .unwrap()
    );
    let frozen = drafts.freeze_save(&id, 2, "provider-event-save").unwrap();
    let receipt = drafts.commit(&mut app, &frozen).unwrap();
    assert_eq!(app.query_pdf(receipt.capture_id).unwrap().user_note, input);
    assert_eq!(
        done,
        events.advance(&mut drafts, &mount, commit, 10000).unwrap()
    );
    let mut bytes = vec![];
    for offset in (0..8192).step_by(2048) {
        let part = events
            .read_checkpoint_chunk(
                &mut drafts,
                &mount,
                cursor,
                done.checkpoint_revision,
                offset,
                2048,
                10000,
            )
            .unwrap();
        bytes.extend(part.bytes);
    }
    assert_eq!(bytes, final_checkpoint);
    events.close(&mut drafts, &mount).unwrap();
    assert_eq!(events.reserved_input_bytes(), 0);
    assert_eq!(drafts.reserved_text_bytes(), MAX_TEXT_BYTES);
}
#[test]
fn checkpoint_failures_and_completion_checks_precede_effects() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let cursor = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "abc".into(),
            0,
        )
        .unwrap()
        .cursor;
    for req in [
        request(
            cursor,
            1,
            Some(DraftEffect::BeginReplace {
                expected_revision: 1,
                total_bytes: 3,
            }),
            vec![0; 8193],
            false,
        ),
        request(
            cursor,
            1,
            Some(DraftEffect::BeginReplace {
                expected_revision: 1,
                total_bytes: 3,
            }),
            vec![],
            true,
        ),
    ] {
        assert!(events.advance(&mut drafts, &mount, req, 0).is_err());
        assert_eq!(drafts.reserved_text_bytes(), 0);
        assert_eq!(
            events
                .snapshot(&mut drafts, &mount, cursor, 0)
                .unwrap()
                .checkpoint_revision,
            1
        );
    }
    let started = begin(&mut events, &mut drafts, &mount, cursor, 3, 0);
    let bad = request(
        cursor,
        started.checkpoint_revision,
        Some(DraftEffect::AppendChunk {
            offset: 0,
            text: "abc".into(),
        }),
        vec![],
        true,
    );
    assert!(events.advance(&mut drafts, &mount, bad, 0).is_err());
    let append = request(
        cursor,
        2,
        Some(DraftEffect::AppendChunk {
            offset: 0,
            text: "abc".into(),
        }),
        vec![2],
        false,
    );
    let accepted = events
        .advance(&mut drafts, &mount, append.clone(), 0)
        .unwrap();
    let mut conflict = append.clone();
    conflict.checkpoint = vec![9];
    assert!(events.advance(&mut drafts, &mount, conflict, 0).is_err());
    assert_eq!(
        accepted,
        events.advance(&mut drafts, &mount, append, 0).unwrap()
    );
    assert!(
        events
            .advance(
                &mut drafts,
                &mount,
                request(
                    cursor,
                    accepted.checkpoint_revision,
                    Some(DraftEffect::CommitReplace),
                    vec![],
                    false
                ),
                0
            )
            .is_err()
    );
    assert_eq!(drafts.read_chunk(&id, None, 0, 512).unwrap().revision, 1);
    assert!(
        events
            .advance(
                &mut drafts,
                &mount,
                request(cursor, u64::MAX, None, vec![], false),
                0
            )
            .is_err()
    );
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, cursor, 0)
            .unwrap()
            .checkpoint_revision,
        accepted.checkpoint_revision
    );
}
#[test]
fn new_event_cancel_and_close_preserve_draft_text_and_reject_old_handles() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let old = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "old".into(),
            0,
        )
        .unwrap()
        .cursor;
    begin(&mut events, &mut drafts, &mount, old, 1024, 0);
    assert_eq!(drafts.reserved_text_bytes(), 1024);
    let new = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "new full input".into(),
            1,
        )
        .unwrap();
    assert_ne!(new.cursor.sequence, old.sequence);
    assert_eq!(drafts.reserved_text_bytes(), 0);
    assert!(
        events
            .read_input_chunk(&mut drafts, &mount, old, 0, 512, 1)
            .is_err()
    );
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                2,
                EventAction::InputChanged,
                "wrong revision".into(),
                1
            )
            .is_err()
    );
    begin(&mut events, &mut drafts, &mount, new.cursor, 512, 1);
    let cancelled = events
        .cancel_event(&mut drafts, &mount, new.cursor, 2, 1)
        .unwrap();
    assert_eq!(cancelled.state, EventState::Cancelled);
    assert_eq!(drafts.reserved_text_bytes(), 0);
    assert_eq!(
        cancelled,
        events
            .cancel_event(&mut drafts, &mount, new.cursor, 2, 1)
            .unwrap()
    );
    assert!(
        events
            .cancel_event(
                &mut drafts,
                &mount,
                new.cursor,
                cancelled.checkpoint_revision,
                1
            )
            .is_err()
    );
    assert!(
        events
            .read_checkpoint_chunk(
                &mut drafts,
                &mount,
                new.cursor,
                cancelled.checkpoint_revision,
                0,
                2048,
                1
            )
            .is_err()
    );
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &mount, new.cursor, 0, 512, 1)
            .unwrap()
            .text,
        "new full input"
    );
    events.close(&mut drafts, &mount).unwrap();
    assert_eq!(events.reserved_input_bytes(), 0);
    assert!(events.snapshot(&mut drafts, &mount, new.cursor, 1).is_err());
    let successor = events.register(&mut drafts, id.clone()).unwrap();
    assert!(events.snapshot(&mut drafts, &mount, new.cursor, 1).is_err());
    events
        .publish(
            &mut drafts,
            &successor,
            1,
            EventAction::InputChanged,
            "retained draft".into(),
            2,
        )
        .unwrap();
    assert_eq!(drafts.read_chunk(&id, None, 0, 512).unwrap().text, "");
}
#[test]
fn scopes_input_budget_and_move_metadata_do_not_transform_text() {
    let (app, selection) = imported();
    let mut drafts = AnnotationDrafts::new();
    let mut events = ProviderEvents::new();
    let mut mounts = vec![];
    let mut ids = vec![];
    for _ in 0..8 {
        let id = drafts
            .create_pdf(&app, 1, "vault", selection.clone(), origin(), 1)
            .unwrap();
        let mount = events.register(&mut drafts, id.clone()).unwrap();
        events
            .publish(
                &mut drafts,
                &mount,
                1,
                EventAction::InputChanged,
                "x".repeat(MAX_TEXT_BYTES),
                0,
            )
            .unwrap();
        ids.push(id);
        mounts.push(mount);
    }
    assert_eq!(events.reserved_input_bytes(), 8 * MAX_TEXT_BYTES);
    assert!(events.register(&mut drafts, ids[0].clone()).is_err());
    assert!(
        events
            .publish(
                &mut drafts,
                &mounts[0],
                1,
                EventAction::InputChanged,
                "x".repeat(MAX_TEXT_BYTES + 1),
                0
            )
            .is_err()
    );
    assert_eq!(events.reserved_input_bytes(), 8 * MAX_TEXT_BYTES);
    assert!(
        events
            .publish(
                &mut drafts,
                &mounts[0],
                1,
                EventAction::MoveParagraph { from: 0, to: 1 },
                "unexpected input".into(),
                0
            )
            .is_err()
    );
    assert!(
        events
            .publish(
                &mut drafts,
                &mounts[0],
                1,
                EventAction::MoveParagraph { from: 0, to: 1 },
                String::new(),
                0
            )
            .is_err()
    );
    let previous = EventCursor {
        sequence: 1,
        base_revision: 1,
    };
    events
        .cancel_event(&mut drafts, &mounts[0], previous, 1, 0)
        .unwrap();
    let moved = events
        .publish(
            &mut drafts,
            &mounts[0],
            1,
            EventAction::MoveParagraph { from: 0, to: 1 },
            String::new(),
            0,
        )
        .unwrap();
    assert_eq!(moved.action, EventAction::MoveParagraph { from: 0, to: 1 });
    assert_eq!(drafts.read_chunk(&ids[0], None, 0, 512).unwrap().text, "");
    let (mut other, other_id) = fixture();
    assert!(
        events
            .publish(
                &mut other,
                &mounts[0],
                1,
                EventAction::InputChanged,
                "foreign".into(),
                0
            )
            .is_err()
    );
    let mut foreign_events = ProviderEvents::new();
    let foreign_mount = foreign_events.register(&mut other, other_id).unwrap();
    assert!(
        events
            .snapshot(&mut drafts, &foreign_mount, moved.cursor, 0)
            .is_err()
    );
}
#[test]
fn expiry_read_activity_wrong_cas_cross_owner_clock_and_utf8_boundary() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let cursor = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "🙂文".into(),
            0,
        )
        .unwrap()
        .cursor;
    begin(&mut events, &mut drafts, &mount, cursor, 100, 0);
    assert!(
        events
            .read_input_chunk(&mut drafts, &mount, cursor, 0, 1, 19)
            .is_err()
    );
    events
        .read_input_chunk(&mut drafts, &mount, cursor, 0, 512, 20)
        .unwrap();
    events.expire(&mut drafts, 49).unwrap();
    assert_eq!(drafts.reserved_text_bytes(), 100);
    events.expire(&mut drafts, 50).unwrap();
    assert_eq!(drafts.reserved_text_bytes(), 0);
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, cursor, 40)
            .unwrap()
            .state,
        EventState::Expired
    );
    assert!(
        events
            .read_checkpoint_chunk(&mut drafts, &mount, cursor, 2, 0, 2048, 50)
            .is_err()
    );
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &mount, cursor, 0, 512, 100)
            .unwrap()
            .text,
        "🙂文"
    );
    let next = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "new".into(),
            100,
        )
        .unwrap();
    begin(&mut events, &mut drafts, &mount, next.cursor, 100, 100);
    assert!(
        events
            .advance(
                &mut drafts,
                &mount,
                request(next.cursor, 99, None, vec![], false),
                129
            )
            .is_err()
    );
    events.expire(&mut drafts, 130).unwrap();
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, next.cursor, 130)
            .unwrap()
            .state,
        EventState::Expired
    );
    let next = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "clock".into(),
            200,
        )
        .unwrap();
    begin(&mut events, &mut drafts, &mount, next.cursor, 100, 200);
    let external_owner = drafts.create_owner().unwrap();
    let (app, selection) = imported();
    let other = drafts
        .create_pdf(&app, 1, "vault", selection, origin(), 1)
        .unwrap();
    drafts
        .begin_replace(&external_owner, &other, 1, 1, 240)
        .unwrap();
    events.expire(&mut drafts, 201).unwrap();
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, next.cursor, 202)
            .unwrap()
            .state,
        EventState::Expired
    );
    assert_eq!(drafts.reserved_text_bytes(), 1);
}
#[test]
fn removed_draft_rejects_all_event_access_without_poisoning_other_mounts() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let cursor = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            String::new(),
            0,
        )
        .unwrap()
        .cursor;
    let done = request(cursor, 1, None, vec![1], true);
    events
        .advance(&mut drafts, &mount, done.clone(), 0)
        .unwrap();
    let (app, selection) = imported();
    let second = drafts
        .create_pdf(&app, 1, "vault", selection, origin(), 1)
        .unwrap();
    let second_mount = events.register(&mut drafts, second).unwrap();
    let second_cursor = events
        .publish(
            &mut drafts,
            &second_mount,
            1,
            EventAction::InputChanged,
            "other".into(),
            0,
        )
        .unwrap()
        .cursor;
    drafts.remove(&id).unwrap();
    assert!(events.snapshot(&mut drafts, &mount, cursor, 0).is_err());
    assert!(
        events
            .read_input_chunk(&mut drafts, &mount, cursor, 0, 512, 0)
            .is_err()
    );
    assert!(
        events
            .read_checkpoint_chunk(&mut drafts, &mount, cursor, 2, 0, 2048, 0)
            .is_err()
    );
    assert!(events.advance(&mut drafts, &mount, done, 0).is_err());
    assert!(
        events
            .cancel_event(&mut drafts, &mount, cursor, 2, 0)
            .is_err()
    );
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &second_mount, second_cursor, 0, 512, 0)
            .unwrap()
            .text,
        "other"
    );
    events.close(&mut drafts, &mount).unwrap();
}

#[test]
fn partial_row_input_only_coalesces_the_same_row_and_base() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id).unwrap();
    let first = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 0 },
            "first".into(),
            0,
        )
        .unwrap();
    begin(&mut events, &mut drafts, &mount, first.cursor, 5, 0);
    let latest = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 0 },
            "latest🙂\0".into(),
            1,
        )
        .unwrap();
    assert_eq!(drafts.reserved_text_bytes(), 0);
    assert!(
        events
            .snapshot(&mut drafts, &mount, first.cursor, 1)
            .is_err()
    );
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                1,
                EventAction::ReplaceParagraph { index: 1 },
                "another row".into(),
                2
            )
            .is_err()
    );
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                2,
                EventAction::ReplaceParagraph { index: 0 },
                "stale source".into(),
                2
            )
            .is_err()
    );
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                1,
                EventAction::ReplaceParagraph { index: u32::MAX },
                "x".into(),
                2
            )
            .is_err()
    );
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &mount, latest.cursor, 0, 512, 2)
            .unwrap()
            .text,
        "latest🙂\0"
    );
    let whole = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::InputChanged,
            "complete note".into(),
            3,
        )
        .unwrap();
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                1,
                EventAction::ReplaceParagraph { index: 0 },
                "partial".into(),
                4
            )
            .is_err()
    );
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &mount, whole.cursor, 0, 512, 4)
            .unwrap()
            .text,
        "complete note"
    );
    events
        .cancel_event(&mut drafts, &mount, whole.cursor, 1, 5)
        .unwrap();
    let row = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 0 },
            String::new(),
            6,
        )
        .unwrap();
    assert_eq!(row.input_bytes, 0);
}

#[test]
fn rejected_partial_publish_preserves_live_upload_checkpoint_clock_and_state_transitions() {
    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let first = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 0 },
            "abc".into(),
            0,
        )
        .unwrap();
    begin(&mut events, &mut drafts, &mount, first.cursor, 3, 0);
    let append = request(
        first.cursor,
        2,
        Some(DraftEffect::AppendChunk {
            offset: 0,
            text: "a".into(),
        }),
        vec![9, 0, 255],
        false,
    );
    let acknowledged = events
        .advance(&mut drafts, &mount, append.clone(), 0)
        .unwrap();
    assert_eq!(acknowledged.checkpoint_revision, 3);
    for (base, action, input) in [
        (
            1,
            EventAction::ReplaceParagraph { index: 1 },
            "different row".into(),
        ),
        (
            2,
            EventAction::ReplaceParagraph { index: 0 },
            "different base".into(),
        ),
        (
            1,
            EventAction::ReplaceParagraph { index: u32::MAX },
            "overflow row".into(),
        ),
        (
            1,
            EventAction::ReplaceParagraph { index: 0 },
            "x".repeat(MAX_TEXT_BYTES + 1),
        ),
    ] {
        // A rejected request must not even advance the shared logical clock:
        // otherwise the following old-time metadata read would expire this upload.
        assert!(
            events
                .publish(&mut drafts, &mount, base, action, input, 100)
                .is_err()
        );
        let still_live = events
            .snapshot(&mut drafts, &mount, first.cursor, 0)
            .unwrap();
        assert_eq!(still_live.state, EventState::Active);
        assert_eq!(
            still_live.checkpoint_revision,
            acknowledged.checkpoint_revision
        );
        assert_eq!(still_live.last_effect, acknowledged.last_effect);
        assert_eq!(drafts.reserved_text_bytes(), 3);
        assert_eq!(
            events
                .read_input_chunk(&mut drafts, &mount, first.cursor, 0, 512, 0)
                .unwrap()
                .text,
            "abc"
        );
        assert_eq!(
            events
                .read_checkpoint_chunk(&mut drafts, &mount, first.cursor, 3, 0, 2048, 0)
                .unwrap()
                .bytes,
            vec![9, 0, 255]
        );
        assert_eq!(
            events
                .advance(&mut drafts, &mount, append.clone(), 0)
                .unwrap(),
            acknowledged
        );
    }
    let appended = events
        .advance(
            &mut drafts,
            &mount,
            request(
                first.cursor,
                3,
                Some(DraftEffect::AppendChunk {
                    offset: 1,
                    text: "bc".into(),
                }),
                vec![7],
                false,
            ),
            0,
        )
        .unwrap();
    let completed = events
        .advance(
            &mut drafts,
            &mount,
            request(
                first.cursor,
                appended.checkpoint_revision,
                Some(DraftEffect::CommitReplace),
                vec![8],
                true,
            ),
            0,
        )
        .unwrap();
    assert_eq!(completed.state, EventState::Completed);
    assert_eq!(
        completed.last_effect,
        Some(EffectOutcome::Replaced { revision: 2 })
    );
    assert_eq!(drafts.read_chunk(&id, Some(2), 0, 512).unwrap().text, "abc");
    assert!(
        events
            .publish(
                &mut drafts,
                &mount,
                1,
                EventAction::ReplaceParagraph { index: 1 },
                "stale".into(),
                100
            )
            .is_err()
    );
    let retained = events
        .snapshot(&mut drafts, &mount, first.cursor, 0)
        .unwrap();
    assert_eq!(retained.state, EventState::Completed);
    assert_eq!(retained.checkpoint_revision, completed.checkpoint_revision);
    assert_eq!(
        events
            .read_checkpoint_chunk(
                &mut drafts,
                &mount,
                first.cursor,
                completed.checkpoint_revision,
                0,
                2048,
                0
            )
            .unwrap()
            .bytes,
        vec![8]
    );
    let next = events
        .publish(
            &mut drafts,
            &mount,
            2,
            EventAction::ReplaceParagraph { index: 1 },
            "new revision".into(),
            0,
        )
        .unwrap();
    assert_eq!(next.cursor.base_revision, 2);
    assert_eq!(next.cursor.sequence, first.cursor.sequence + 1);
    events.expire(&mut drafts, 30).unwrap();
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, next.cursor, 30)
            .unwrap()
            .state,
        EventState::Expired
    );

    let (mut drafts, id) = fixture();
    let mut events = ProviderEvents::new();
    let mount = events.register(&mut drafts, id.clone()).unwrap();
    let old = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 4 },
            "old row".into(),
            0,
        )
        .unwrap();
    begin(&mut events, &mut drafts, &mount, old.cursor, 8, 0);
    events.expire(&mut drafts, 30).unwrap();
    assert_eq!(drafts.reserved_text_bytes(), 0);
    for (base, index) in [(1, 5), (2, 4)] {
        assert!(
            events
                .publish(
                    &mut drafts,
                    &mount,
                    base,
                    EventAction::ReplaceParagraph { index },
                    "must not replace expired input".into(),
                    100
                )
                .is_err()
        );
        assert_eq!(
            events
                .snapshot(&mut drafts, &mount, old.cursor, 30)
                .unwrap()
                .state,
            EventState::Expired
        );
        assert_eq!(
            events
                .read_input_chunk(&mut drafts, &mount, old.cursor, 0, 512, 30)
                .unwrap()
                .text,
            "old row"
        );
    }
    let resumed = events
        .publish(
            &mut drafts,
            &mount,
            1,
            EventAction::ReplaceParagraph { index: 4 },
            "replacement".into(),
            31,
        )
        .unwrap();
    assert_eq!(resumed.state, EventState::Active);
    assert_eq!(resumed.cursor.sequence, old.cursor.sequence + 1);
    assert_eq!(resumed.checkpoint_revision, 1);
    assert_eq!(
        events
            .read_input_chunk(&mut drafts, &mount, resumed.cursor, 0, 512, 31)
            .unwrap()
            .text,
        "replacement"
    );
    assert_eq!(drafts.read_chunk(&id, Some(1), 0, 512).unwrap().text, "");
    events.expire(&mut drafts, 61).unwrap();
    assert_eq!(
        events
            .snapshot(&mut drafts, &mount, resumed.cursor, 61)
            .unwrap()
            .state,
        EventState::Expired
    );
}
