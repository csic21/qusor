#![cfg(unix)]
//! Real SQLite qualification for vocabulary study data: idempotent seeding,
//! queue ordering, transactional reviews, persistence across reopen, export and
//! empty-Core restore. Scheduling math itself is covered by learning-domain.
use core_api::vocab::{
    CardSchedule, NewWord, ReviewEntry, ReviewGuard, STUDY_ALGORITHM, StudyRating, StudyStage,
    WordbookSeed,
};
use learning_application::study::StudyApplication;
use learning_ports::StudyStore;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

/// 2026-01-01T00:00:00Z; deterministic and far from the 2038 boundary.
const NOW: i64 = 1_767_225_600_000;
const DAY: i64 = 86_400_000;
const HOUR: i64 = 3_600_000;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-study-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn seed() -> WordbookSeed {
    WordbookSeed {
        title: "CET-4 sample".into(),
        description: "Synthetic three-word fixture, not a licensed word list.".into(),
        words: vec![
            NewWord {
                term: "abandon".into(),
                meaning: "放弃".into(),
                example: "Do not abandon the plan.".into(),
            },
            NewWord {
                term: "benefit".into(),
                meaning: "益处".into(),
                example: "Reading has many benefits.".into(),
            },
            NewWord {
                term: "candidate".into(),
                meaning: "候选人".into(),
                example: "The candidate answered well.".into(),
            },
        ],
    }
}

fn open(scratch: &Scratch) -> SqliteStore {
    SqliteStore::open(&scratch.0).unwrap()
}

#[test]
fn seeding_is_idempotent_and_decks_count_new_cards() {
    let scratch = Scratch::new();
    let mut app = StudyApplication::new(open(&scratch));
    let book = app.seed_wordbook("vault", &seed(), NOW).unwrap();
    let repeated = app.seed_wordbook("vault", &seed(), NOW + HOUR).unwrap();
    assert_eq!(book.id, repeated.id);
    assert_eq!(book.created_at_ms, repeated.created_at_ms);

    let decks = app.decks("vault", NOW).unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].wordbook.id, book.id);
    assert_eq!((decks[0].total, decks[0].due, decks[0].new), (3, 0, 3));

    let archive = app.export_study().unwrap();
    assert_eq!(archive.cards.len(), 3);
    assert!(archive.schedules.is_empty());
    assert!(archive.reviews.is_empty());
}

#[test]
fn queue_orders_new_cards_and_reviews_persist_across_reopen() {
    let scratch = Scratch::new();
    let mut app = StudyApplication::new(open(&scratch));
    let book = app.seed_wordbook("vault", &seed(), NOW).unwrap();
    let queue = app.queue("vault", book.id, NOW, NOW - DAY, 6).unwrap();
    assert_eq!(queue.cards.len(), 3);
    assert_eq!(queue.cards[0].card.term, "abandon");
    assert_eq!(queue.cards[0].stage, StudyStage::New);
    assert_eq!(queue.remaining_due, 0);
    assert_eq!(queue.remaining_new, 0);

    let card_id = queue.cards[0].card.id;
    let outcome = app
        .review("vault", card_id, StudyRating::Good, "op-1", NOW, NOW - DAY)
        .unwrap();
    assert_eq!(outcome.stage, StudyStage::Review);
    assert_eq!(outcome.scheduled_minutes, 3 * 24 * 60);
    assert_eq!(outcome.due_at_ms, NOW + 3 * DAY);

    let store = app.into_store();
    drop(store);
    let reopened = open(&scratch);
    let app = StudyApplication::new(reopened);
    let archive = app.export_study().unwrap();
    assert_eq!(archive.reviews.len(), 1);
    assert_eq!(archive.schedules.len(), 1);
    assert_eq!(archive.schedules[0].card_id, card_id);
    assert_eq!(archive.schedules[0].reps, 1);
    assert_eq!(archive.schedules[0].last_rating, StudyRating::Good);
    assert_eq!(archive.reviews[0].stage_before, StudyStage::New);
    assert_eq!(archive.reviews[0].algorithm, STUDY_ALGORITHM);

    let decks = app.decks("vault", NOW).unwrap();
    assert_eq!((decks[0].due, decks[0].new), (0, 2));
}

#[test]
fn review_is_idempotent_and_rejects_a_reused_operation() {
    let scratch = Scratch::new();
    let mut app = StudyApplication::new(open(&scratch));
    let book = app.seed_wordbook("vault", &seed(), NOW).unwrap();
    let card_id = app
        .queue("vault", book.id, NOW, NOW - DAY, 1)
        .unwrap()
        .cards[0]
        .card
        .id;
    let first = app
        .review("vault", card_id, StudyRating::Good, "op-1", NOW, NOW - DAY)
        .unwrap();
    let replay = app
        .review(
            "vault",
            card_id,
            StudyRating::Good,
            "op-1",
            NOW + HOUR,
            NOW - DAY,
        )
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(app.export_study().unwrap().reviews.len(), 1);

    let conflict = app.review(
        "vault",
        card_id,
        StudyRating::Again,
        "op-1",
        NOW + HOUR,
        NOW - DAY,
    );
    assert_eq!(
        conflict.unwrap_err().code,
        core_api::error::KnowledgeErrorCode::Conflict
    );
}

#[test]
fn due_cards_return_after_their_interval() {
    let scratch = Scratch::new();
    let mut app = StudyApplication::new(open(&scratch));
    let book = app.seed_wordbook("vault", &seed(), NOW).unwrap();
    let card_id = app
        .queue("vault", book.id, NOW, NOW - DAY, 1)
        .unwrap()
        .cards[0]
        .card
        .id;
    app.review("vault", card_id, StudyRating::Good, "op-1", NOW, NOW - DAY)
        .unwrap();

    let before = app.queue("vault", book.id, NOW + DAY, NOW, 6).unwrap();
    assert!(before.cards.iter().all(|card| card.card.id != card_id));
    assert_eq!(before.remaining_due, 0);

    let after = app
        .queue("vault", book.id, NOW + 4 * DAY, NOW + 4 * DAY - DAY, 6)
        .unwrap();
    assert_eq!(after.cards[0].card.id, card_id);
    assert_eq!(after.cards[0].stage, StudyStage::Review);
    assert_eq!(after.remaining_due, 0);
    assert_eq!(after.remaining_new, 0);

    let learning = app
        .review(
            "vault",
            card_id,
            StudyRating::Again,
            "op-2",
            NOW + 4 * DAY,
            NOW + 4 * DAY - DAY,
        )
        .unwrap();
    assert_eq!(learning.stage, StudyStage::Review);
    assert!(learning.due_at_ms >= NOW + 5 * DAY);
}

#[test]
fn export_and_restore_round_trip_into_empty_core() {
    let first = Scratch::new();
    let mut app = StudyApplication::new(open(&first));
    app.seed_wordbook("vault", &seed(), NOW).unwrap();
    let book_id = app.decks("vault", NOW).unwrap()[0].wordbook.id;
    let card_id = app
        .queue("vault", book_id, NOW, NOW - DAY, 1)
        .unwrap()
        .cards[0]
        .card
        .id;
    app.review("vault", card_id, StudyRating::Easy, "op-1", NOW, NOW - DAY)
        .unwrap();
    let archive = app.export_study().unwrap();

    let second = Scratch::new();
    let mut restored = StudyApplication::new(open(&second));
    restored.restore_study(&archive).unwrap();
    assert_eq!(restored.export_study().unwrap(), archive);

    let again = restored.restore_study(&archive);
    assert_eq!(
        again.unwrap_err().code,
        core_api::error::KnowledgeErrorCode::Conflict
    );
}

#[test]
fn store_rejects_a_stale_review_guard() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let book = store.ensure_wordbook("vault", &seed(), NOW).unwrap();
    let archive = store.study_snapshot().unwrap();
    assert_eq!(archive.cards.len(), 3);
    let card = archive.cards[0].clone();
    let schedule = CardSchedule {
        card_id: card.id,
        due_at_ms: NOW + 3 * DAY,
        stage: StudyStage::Review,
        stability: 3.173,
        difficulty: 5.0,
        reps: 1,
        lapses: 0,
        last_review_at_ms: NOW,
        last_rating: StudyRating::Good,
    };
    let entry = ReviewEntry {
        id: 0,
        card_id: card.id,
        operation_id: "op-1".into(),
        rating: StudyRating::Good,
        reviewed_at_ms: NOW,
        stage_before: StudyStage::New,
        stage_after: StudyStage::Review,
        stability: 3.173,
        difficulty: 5.0,
        scheduled_minutes: 3 * 24 * 60,
        due_at_ms: NOW + 3 * DAY,
        algorithm: STUDY_ALGORITHM.into(),
    };
    let guard = ReviewGuard {
        card_id: card.id,
        expected_reps: 0,
        expected_last_review_at_ms: None,
    };
    let stored = store.commit_review(&guard, &schedule, &entry).unwrap();
    assert!(stored.id > 0);

    let stale = store.commit_review(
        &guard,
        &CardSchedule {
            due_at_ms: NOW + 2 * DAY,
            ..schedule
        },
        &ReviewEntry {
            operation_id: "op-2".into(),
            rating: StudyRating::Again,
            ..entry
        },
    );
    assert_eq!(
        stale.unwrap_err().code,
        core_api::error::KnowledgeErrorCode::Conflict
    );
    assert_eq!(store.study_snapshot().unwrap().reviews.len(), 1);
    let _ = book;
}
