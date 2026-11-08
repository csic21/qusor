mod common;
use common::*;
use learning_application::{
    annotation_draft::AnnotationDrafts, draft_text::*, provider_events as edit,
    provider_projections::*,
};

fn fixture(text: &str) -> (AnnotationDrafts, DraftId) {
    let (app, selection) = imported();
    let mut drafts = AnnotationDrafts::new();
    let id = drafts
        .create_pdf(&app, 1, "vault", selection, origin(), 1)
        .unwrap();
    if !text.is_empty() {
        replace(&mut drafts, &id, text);
    }
    (drafts, id)
}
fn replace(drafts: &mut AnnotationDrafts, id: &DraftId, text: &str) -> u64 {
    let revision = drafts.read_chunk(id, None, 0, 512).unwrap().revision;
    let owner = drafts.create_owner().unwrap();
    let upload = drafts
        .begin_replace(&owner, id, revision, text.len(), 0)
        .unwrap();
    for (offset, chunk) in text.as_bytes().chunks(512).enumerate() {
        // Test fixtures are short or ASCII; production scalar chunking is tested elsewhere.
        drafts
            .append_chunk(
                &owner,
                &upload,
                offset * 512,
                std::str::from_utf8(chunk).unwrap(),
                0,
            )
            .unwrap();
    }
    let revision = drafts.commit_replace(&owner, &upload, 0).unwrap();
    drafts.revoke_owner(&owner).unwrap();
    revision
}
fn step(
    cas: u64,
    checkpoint: Vec<u8>,
    window: Option<ProjectionWindow>,
    complete: bool,
) -> AdvanceProjection {
    AdvanceProjection {
        expected_checkpoint_revision: cas,
        checkpoint,
        window,
        complete,
    }
}
fn row(ordinal: u32, start: usize, end: usize, complete: bool) -> RowSpan {
    RowSpan {
        ordinal,
        start,
        end,
        complete,
    }
}
fn window(rows: Vec<RowSpan>, reached_eof: bool) -> ProjectionWindow {
    ProjectionWindow { rows, reached_eof }
}

#[test]
fn progressive_utf8_window_and_atomic_lost_reply_replay() {
    let (drafts, id) = fixture("a\r\n中🙂\0\n");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id.clone()).unwrap();
    let query = projections.start(&drafts, &mount, 2, 0, 2, 0).unwrap();
    let first = step(
        1,
        vec![0, 9, 255],
        Some(window(vec![row(0, 0, 1, true), row(1, 3, 6, false)], false)),
        false,
    );
    let receipt = projections
        .advance(&drafts, &query, first.clone(), 0)
        .unwrap();
    assert_eq!(receipt.checkpoint_revision, 2);
    assert!(!receipt.window_ready);
    // The first reply is deliberately ignored; retrying must not apply twice.
    assert_eq!(
        projections
            .advance(&drafts, &query, first.clone(), 1)
            .unwrap(),
        receipt
    );
    let mut changed = first;
    changed.checkpoint.push(1);
    assert!(projections.advance(&drafts, &query, changed, 100).is_err());
    assert_eq!(projections.metadata(&drafts, &query, 1).unwrap(), receipt);
    assert_eq!(
        projections
            .read_checkpoint(&drafts, &query, 2, 1, 2048, 1)
            .unwrap()
            .bytes,
        vec![9, 255]
    );
    assert!(
        projections
            .advance(&drafts, &query, step(2, vec![], None, true), 100)
            .is_err()
    );
    assert_eq!(projections.metadata(&drafts, &query, 1).unwrap(), receipt);
    let completed_request = step(
        2,
        vec![4],
        Some(window(vec![row(0, 0, 1, true), row(1, 3, 11, true)], true)),
        true,
    );
    let completed = projections
        .advance(&drafts, &query, completed_request.clone(), 2)
        .unwrap();
    assert_eq!(completed.state, ProjectionState::Completed);
    assert!(completed.window_ready);
    assert_eq!(
        projections
            .advance(&drafts, &query, completed_request, 100)
            .unwrap(),
        completed
    );
    assert_eq!(
        projections
            .read_checkpoint(&drafts, &query, 3, 0, 2048, 100)
            .unwrap()
            .bytes,
        vec![4]
    );
    assert!(
        projections
            .advance(&drafts, &query, step(3, vec![], None, false), 100)
            .is_err()
    );
    assert_eq!(
        drafts.read_chunk(&id, Some(2), 0, 512).unwrap().text,
        "a\r\n中🙂\0\n"
    );
}

#[test]
fn malformed_spans_and_regressing_windows_leave_cas_and_clock_intact() {
    let (drafts, id) = fixture("a\r\n中🙂\0\n");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id).unwrap();
    let query = projections.start(&drafts, &mount, 2, 0, 2, 0).unwrap();
    let initial = projections
        .advance(
            &drafts,
            &query,
            step(
                1,
                vec![3],
                Some(window(vec![row(0, 0, 1, true), row(1, 3, 6, false)], false)),
                false,
            ),
            0,
        )
        .unwrap();
    for bad in [
        window(vec![row(0, 0, 1, true), row(1, 3, 7, true)], true), // emoji byte interior
        window(vec![row(0, 0, 1, true), row(2, 3, 11, true)], true), // ordinal gap
        window(vec![row(0, 0, 4, true), row(1, 3, 11, true)], true), // overlap
        window(vec![row(0, 0, 1, false), row(1, 3, 11, true)], false),
        window(vec![row(0, 0, 1, true), row(1, 3, 11, false)], true),
        window(vec![row(0, 0, 1, true)], false), // drops already published row
        window(vec![row(0, 0, 0, true), row(1, 3, 11, true)], false), // changes complete prefix
        window(vec![row(0, 0, 1, true), row(1, 3, 3, false)], false), // shrinks partial
        window(vec![row(0, 0, 1, true), row(1, 6, 11, true)], false), // shifts partial start
    ] {
        assert!(
            projections
                .advance(&drafts, &query, step(2, vec![8], Some(bad), false), 100)
                .is_err()
        );
        assert_eq!(projections.metadata(&drafts, &query, 0).unwrap(), initial);
    }
    assert!(
        projections
            .advance(
                &drafts,
                &query,
                step(2, vec![0; MAX_PROJECTION_CHECKPOINT_BYTES + 1], None, false),
                100
            )
            .is_err()
    );
    assert!(
        projections
            .read_checkpoint(&drafts, &query, 1, 0, 1, 100)
            .is_err()
    );
    assert!(
        projections
            .read_checkpoint(&drafts, &query, 2, 0, 2049, 100)
            .is_err()
    );
    assert_eq!(projections.metadata(&drafts, &query, 0).unwrap(), initial);
}

#[test]
fn empty_source_eof_and_mount_budgets_are_explicit() {
    let (drafts, id) = fixture("");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id.clone()).unwrap();
    let single = projections.start(&drafts, &mount, 1, 0, 1, 0).unwrap();
    assert!(
        projections
            .advance(
                &drafts,
                &single,
                step(
                    1,
                    vec![],
                    Some(window(vec![row(0, 0, 0, true)], false)),
                    false
                ),
                0
            )
            .is_err()
    );
    assert_eq!(
        projections
            .metadata(&drafts, &single, 0)
            .unwrap()
            .checkpoint_revision,
        1
    );
    let query = projections.start(&drafts, &mount, 1, 0, 8, 0).unwrap();
    for bad in [
        window(vec![], true),
        window(vec![row(0, 0, 0, false)], false),
    ] {
        assert!(
            projections
                .advance(&drafts, &query, step(1, vec![], Some(bad), true), 0)
                .is_err()
        );
    }
    let done = projections
        .advance(
            &drafts,
            &query,
            step(
                1,
                vec![],
                Some(window(vec![row(0, 0, 0, true)], true)),
                true,
            ),
            0,
        )
        .unwrap();
    assert!(done.window_ready);
    for (first, max_rows) in [(0, 0), (0, 9), (u32::MAX, 1)] {
        assert!(
            projections
                .start(&drafts, &mount, 1, first, max_rows, 100)
                .is_err()
        );
        assert_eq!(projections.metadata(&drafts, &query, 0).unwrap(), done);
    }
    let beyond = projections.start(&drafts, &mount, 1, 1, 8, 0).unwrap();
    let empty = projections
        .advance(
            &drafts,
            &beyond,
            step(1, vec![], Some(window(vec![], true)), true),
            0,
        )
        .unwrap();
    assert!(empty.window_ready);
    for _ in 1..MAX_DRAFTS {
        projections.register(&drafts, id.clone()).unwrap();
    }
    assert!(projections.register(&drafts, id.clone()).is_err());
    projections.close(&mount).unwrap();
    let successor = projections.register(&drafts, id).unwrap();
    assert!(projections.metadata(&drafts, &beyond, 0).is_err());
    assert!(projections.close(&mount).is_err());
    projections.close(&successor).unwrap();
}

#[test]
fn query_scope_revision_deletion_cancel_and_independent_timeout() {
    let (mut drafts, id) = fixture("abc");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id.clone()).unwrap();
    let old = projections.start(&drafts, &mount, 2, 0, 1, 0).unwrap();
    let next = projections.start(&drafts, &mount, 2, 0, 1, 0).unwrap();
    assert!(projections.metadata(&drafts, &old, 100).is_err());
    let mut other = ProviderProjections::new();
    assert!(other.metadata(&drafts, &next, 0).is_err());
    let (foreign_drafts, _) = fixture("abc");
    assert!(projections.metadata(&foreign_drafts, &next, 100).is_err());
    assert_eq!(
        projections.metadata(&drafts, &next, 0).unwrap().state,
        ProjectionState::Active
    );
    let cancelled = projections.cancel(&drafts, &next, 1, 0).unwrap();
    assert_eq!(cancelled.state, ProjectionState::Cancelled);
    assert_eq!(cancelled.checkpoint_revision, 2);
    assert_eq!(projections.cancel(&drafts, &next, 1, 0).unwrap(), cancelled);
    assert!(
        projections
            .read_checkpoint(&drafts, &next, 2, 0, 1, 0)
            .is_err()
    );
    let timed = projections.start(&drafts, &mount, 2, 0, 1, 0).unwrap();
    projections
        .advance(&drafts, &timed, step(1, vec![0; 8192], None, false), 0)
        .unwrap();
    let mut bytes = Vec::new();
    for offset in (0..8192).step_by(2048) {
        bytes.extend(
            projections
                .read_checkpoint(&drafts, &timed, 2, offset, 2048, 0)
                .unwrap()
                .bytes,
        );
    }
    assert_eq!(bytes, vec![0; 8192]);
    projections.expire(&drafts, 30).unwrap();
    assert_eq!(
        projections.metadata(&drafts, &timed, 0).unwrap().state,
        ProjectionState::Expired
    );
    assert!(
        projections
            .read_checkpoint(&drafts, &timed, 2, 0, 1, 30)
            .is_err()
    );
    let changed = projections.start(&drafts, &mount, 2, 0, 1, 30).unwrap();
    replace(&mut drafts, &id, "new");
    assert!(projections.metadata(&drafts, &changed, 30).is_err());
    let removed = projections.start(&drafts, &mount, 3, 0, 1, 30).unwrap();
    drafts.remove(&id).unwrap();
    assert!(projections.metadata(&drafts, &removed, 30).is_err());
    projections.close(&mount).unwrap();
}

#[test]
fn viewport_changes_and_expiry_do_not_touch_pending_edit_upload_or_checkpoint() {
    let (mut drafts, id) = fixture("old");
    let mut edits = edit::ProviderEvents::new();
    let edit_mount = edits.register(&mut drafts, id.clone()).unwrap();
    let event = edits
        .publish(
            &mut drafts,
            &edit_mount,
            2,
            edit::EventAction::ReplaceParagraph { index: 0 },
            "pending🙂\0".into(),
            0,
        )
        .unwrap();
    edits
        .advance(
            &mut drafts,
            &edit_mount,
            edit::AdvanceEvent {
                cursor: event.cursor,
                expected_checkpoint_revision: 1,
                effect: Some(edit::DraftEffect::BeginReplace {
                    expected_revision: 2,
                    total_bytes: "pending🙂\0".len(),
                }),
                checkpoint: vec![4, 0, 255],
                complete: false,
            },
            0,
        )
        .unwrap();
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id.clone()).unwrap();
    let editing_reserved = drafts.reserved_text_bytes();
    assert_eq!(editing_reserved, "old".len() + "pending🙂\0".len());
    let first = projections.start(&drafts, &mount, 2, 0, 8, 0).unwrap();
    projections
        .advance(
            &drafts,
            &first,
            step(
                1,
                vec![9],
                Some(window(vec![row(0, 0, 3, true)], true)),
                true,
            ),
            0,
        )
        .unwrap();
    let second = projections.start(&drafts, &mount, 2, 1, 8, 0).unwrap();
    projections.cancel(&drafts, &second, 1, 0).unwrap();
    let third = projections.start(&drafts, &mount, 2, 0, 8, 0).unwrap();
    projections.expire(&drafts, 100).unwrap();
    assert_eq!(
        projections.metadata(&drafts, &third, 0).unwrap().state,
        ProjectionState::Expired
    );
    assert_eq!(drafts.reserved_text_bytes(), editing_reserved);
    let retained = edits
        .snapshot(&mut drafts, &edit_mount, event.cursor, 0)
        .unwrap();
    assert_eq!(retained.state, edit::EventState::Active);
    assert_eq!(retained.checkpoint_revision, 2);
    assert_eq!(
        edits
            .read_input_chunk(&mut drafts, &edit_mount, event.cursor, 0, 512, 0)
            .unwrap()
            .text,
        "pending🙂\0"
    );
    assert_eq!(
        edits
            .read_checkpoint_chunk(&mut drafts, &edit_mount, event.cursor, 2, 0, 2048, 0)
            .unwrap()
            .bytes,
        vec![4, 0, 255]
    );
    let appended = edits
        .advance(
            &mut drafts,
            &edit_mount,
            edit::AdvanceEvent {
                cursor: event.cursor,
                expected_checkpoint_revision: 2,
                effect: Some(edit::DraftEffect::AppendChunk {
                    offset: 0,
                    text: "pending🙂\0".into(),
                }),
                checkpoint: vec![5],
                complete: false,
            },
            0,
        )
        .unwrap();
    edits
        .advance(
            &mut drafts,
            &edit_mount,
            edit::AdvanceEvent {
                cursor: event.cursor,
                expected_checkpoint_revision: appended.checkpoint_revision,
                effect: Some(edit::DraftEffect::CommitReplace),
                checkpoint: vec![],
                complete: true,
            },
            0,
        )
        .unwrap();
    assert_eq!(
        drafts.read_chunk(&id, Some(3), 0, 512).unwrap().text,
        "pending🙂\0"
    );
}

#[test]
fn one_mib_long_row_remains_a_bounded_incomplete_view_until_confirmed() {
    let source = "x".repeat(MAX_TEXT_BYTES);
    let (drafts, id) = fixture(&source);
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id.clone()).unwrap();
    let query = projections.start(&drafts, &mount, 2, 0, 1, 0).unwrap();
    let partial = projections
        .advance(
            &drafts,
            &query,
            step(
                1,
                vec![0; 128],
                Some(window(vec![row(0, 0, 128, false)], false)),
                false,
            ),
            0,
        )
        .unwrap();
    assert_eq!(partial.context.total_bytes, MAX_TEXT_BYTES);
    assert!(!partial.window_ready);
    assert!(
        projections
            .advance(&drafts, &query, step(2, vec![], None, true), 0)
            .is_err()
    );
    let completed = projections
        .advance(
            &drafts,
            &query,
            step(
                2,
                vec![1],
                Some(window(vec![row(0, 0, MAX_TEXT_BYTES, true)], true)),
                true,
            ),
            0,
        )
        .unwrap();
    assert!(completed.window_ready);
    assert_eq!(completed.window.unwrap().rows.len(), 1);
    assert_eq!(
        drafts
            .read_chunk(&id, Some(2), MAX_TEXT_BYTES - 512, 512)
            .unwrap()
            .text,
        "x".repeat(512)
    );
}

#[test]
fn a_full_requested_window_can_finish_before_eof_and_published_eof_never_grows() {
    let (drafts, id) = fixture("a\nb\nc\nd\ne\nf\ng\nh\ni\n");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id).unwrap();
    let query = projections.start(&drafts, &mount, 2, 0, 8, 0).unwrap();
    let rows = (0..8)
        .map(|i| row(i, i as usize * 2, i as usize * 2 + 1, true))
        .collect();
    let done = projections
        .advance(
            &drafts,
            &query,
            step(1, vec![], Some(window(rows, false)), true),
            0,
        )
        .unwrap();
    assert_eq!(done.state, ProjectionState::Completed);
    assert!(done.window_ready);
    assert!(!done.window.unwrap().reached_eof);

    let (drafts, id) = fixture("a\nb");
    let mut projections = ProviderProjections::new();
    let mount = projections.register(&drafts, id).unwrap();
    let query = projections.start(&drafts, &mount, 2, 0, 8, 0).unwrap();
    let rows = vec![row(0, 0, 1, true), row(1, 2, 3, true)];
    let ready = projections
        .advance(
            &drafts,
            &query,
            step(1, vec![9], Some(window(rows.clone(), true)), false),
            0,
        )
        .unwrap();
    let mut grown = rows.clone();
    grown.push(row(2, 3, 3, true));
    for next in [window(rows, false), window(grown, true)] {
        assert!(
            projections
                .advance(&drafts, &query, step(2, vec![1], Some(next), false), 100)
                .is_err()
        );
        assert_eq!(projections.metadata(&drafts, &query, 0).unwrap(), ready);
    }
    let done = projections
        .advance(&drafts, &query, step(2, vec![], None, true), 0)
        .unwrap();
    assert_eq!(done.state, ProjectionState::Completed);
}
