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
                pos: "v.".into(),
                phonetic: "/əˈbændən/".into(),
                meaning: "放弃".into(),
                example: "Do not abandon the plan.".into(),
            },
            NewWord {
                term: "benefit".into(),
                pos: "n.".into(),
                phonetic: "/ˈbenɪfɪt/".into(),
                meaning: "益处".into(),
                example: "Reading has many benefits.".into(),
            },
            NewWord {
                term: "candidate".into(),
                pos: "n.".into(),
                phonetic: "/ˈkændɪdət/".into(),
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

#[test]
fn pos_and_phonetic_roundtrip_across_reopen_and_restore() {
    let dir = Scratch::new();
    let store = SqliteStore::open(&dir.0).unwrap();
    let mut app = StudyApplication::new(store);
    let book = app.seed_wordbook("main", &seed(), NOW).unwrap();
    let cards = app.wordbook_cards("main", book.id, 10, 0).unwrap();
    assert_eq!(cards.len(), 3);
    assert_eq!(cards[0].pos, "v.");
    assert_eq!(cards[0].phonetic, "/əˈbændən/");
    assert_eq!(cards[1].pos, "n.");
    assert_eq!(cards[1].phonetic, "/ˈbenɪfɪt/");
    drop(app);

    let reopened = StudyApplication::new(SqliteStore::open(&dir.0).unwrap());
    let reopened_cards = reopened.wordbook_cards("main", book.id, 10, 0).unwrap();
    assert_eq!(reopened_cards, cards);

    let archive = reopened.into_store().study_snapshot().unwrap();
    let dir2 = Scratch::new();
    let mut restored_store = SqliteStore::open(&dir2.0).unwrap();
    restored_store.restore_study(&archive).unwrap();
    let restored_app = StudyApplication::new(restored_store);
    let restored_cards = restored_app.wordbook_cards("main", book.id, 10, 0).unwrap();
    assert_eq!(restored_cards, cards);
}

#[test]
fn delete_wordbook_cascades_and_rejects_wrong_or_missing_vault() {
    let dir = Scratch::new();
    let mut app = StudyApplication::new(SqliteStore::open(&dir.0).unwrap());
    let main_book = app.seed_wordbook("main", &seed(), NOW).unwrap();
    let other_book = app.seed_wordbook("other", &seed(), NOW).unwrap();

    // Review one card in main_book so schedule + review rows exist and must cascade.
    let q = app.queue("main", main_book.id, NOW, NOW - DAY, 10).unwrap();
    app.review(
        "main",
        q.cards[0].card.id,
        StudyRating::Good,
        "op-del-1",
        NOW + HOUR,
        NOW - DAY,
    )
    .unwrap();

    let err_wrong_vault = app.delete_wordbook("other", main_book.id).unwrap_err();
    assert_eq!(
        err_wrong_vault.code,
        core_api::error::KnowledgeErrorCode::NotFound
    );
    let err_missing = app.delete_wordbook("main", 9999).unwrap_err();
    assert_eq!(
        err_missing.code,
        core_api::error::KnowledgeErrorCode::NotFound
    );

    app.delete_wordbook("main", main_book.id).unwrap();
    assert!(app.decks("main", NOW).unwrap().is_empty());

    let snapshot = app.into_store().study_snapshot().unwrap();
    assert_eq!(snapshot.wordbooks, vec![other_book.clone()]);
    assert_eq!(snapshot.cards.len(), 3);
    assert!(
        snapshot
            .cards
            .iter()
            .all(|c| c.wordbook_id == other_book.id)
    );
    assert!(snapshot.schedules.is_empty());
    assert!(snapshot.reviews.is_empty());

    // Reopen and confirm deletion persisted.
    let reopened = StudyApplication::new(SqliteStore::open(&dir.0).unwrap());
    assert_eq!(reopened.decks("other", NOW).unwrap().len(), 1);
    assert!(reopened.decks("main", NOW).unwrap().is_empty());
}

#[test]
fn wordbook_cards_paginates_and_checks_bounds_and_vault() {
    let dir = Scratch::new();
    let mut app = StudyApplication::new(SqliteStore::open(&dir.0).unwrap());
    let book = app.seed_wordbook("main", &seed(), NOW).unwrap();

    let page1 = app.wordbook_cards("main", book.id, 2, 0).unwrap();
    let page2 = app.wordbook_cards("main", book.id, 2, 2).unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].term, "abandon");
    assert_eq!(page1[1].term, "benefit");
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].term, "candidate");

    assert_eq!(
        app.wordbook_cards("other", book.id, 2, 0).unwrap_err().code,
        core_api::error::KnowledgeErrorCode::NotFound
    );
    assert_eq!(
        app.wordbook_cards("main", book.id, 0, 0).unwrap_err().code,
        core_api::error::KnowledgeErrorCode::BudgetExceeded
    );
    assert_eq!(
        app.wordbook_cards("main", book.id, 201, 0)
            .unwrap_err()
            .code,
        core_api::error::KnowledgeErrorCode::BudgetExceeded
    );
}

#[test]
fn queue_with_limit_clamps_daily_new_limit_and_queue_keeps_default() {
    let dir = Scratch::new();
    let mut app = StudyApplication::new(SqliteStore::open(&dir.0).unwrap());
    let words: Vec<NewWord> = (0..25)
        .map(|i| NewWord {
            term: format!("word-{i:02}"),
            pos: "n.".into(),
            phonetic: format!("/w{i}/"),
            meaning: format!("释义-{i}"),
            example: format!("Example sentence {i}."),
        })
        .collect();
    let book = app
        .seed_wordbook(
            "main",
            &WordbookSeed {
                title: "25-card deck".into(),
                description: "Synthetic test deck".into(),
                words,
            },
            NOW,
        )
        .unwrap();

    let zero_q = app.queue_with_limit("main", book.id, NOW, 50, 0).unwrap();
    assert_eq!(zero_q.remaining_new, 0);
    assert!(zero_q.cards.is_empty());

    let five_q = app.queue_with_limit("main", book.id, NOW, 50, 5).unwrap();
    assert_eq!(five_q.remaining_new, 0);
    assert_eq!(five_q.cards.len(), 5);

    // Default `queue` remains pinned to DAILY_NEW_LIMIT (20).
    let default_q = app.queue("main", book.id, NOW, NOW, 50).unwrap();
    assert_eq!(default_q.remaining_new, 0);
    assert_eq!(
        default_q.cards.len(),
        core_api::vocab::DAILY_NEW_LIMIT as usize
    );

    // Values above MAX_DAILY_NEW_LIMIT (200) clamp down to 200; seed 210 words to observe clamping.
    let big_words: Vec<NewWord> = (0..210)
        .map(|i| NewWord {
            term: format!("big-{i:03}"),
            pos: String::new(),
            phonetic: String::new(),
            meaning: format!("m-{i}"),
            example: String::new(),
        })
        .collect();
    let big_book = app
        .seed_wordbook(
            "main",
            &WordbookSeed {
                title: "210-card deck".into(),
                description: "Clamp verification".into(),
                words: big_words,
            },
            NOW,
        )
        .unwrap();
    let clamped_q = app
        .queue_with_limit("main", big_book.id, NOW, 50, 999)
        .unwrap();
    assert_eq!(clamped_q.remaining_new, 150);
    assert_eq!(clamped_q.cards.len(), 50);
}

#[test]
fn schema_v4_migrates_cleanly_to_v5_preserving_cards_and_adding_tables() {
    let dir = Scratch::new();
    let db_path = dir.0.join("core.sqlite");
    {
        let db = rusqlite::Connection::open(&db_path).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;
            BEGIN;
            CREATE TABLE source(id INTEGER PRIMARY KEY, descriptor TEXT NOT NULL);
            CREATE TABLE document_version(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),snapshot_hash TEXT NOT NULL);
            CREATE TABLE capture(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),version_id INTEGER NOT NULL REFERENCES document_version(id),record TEXT NOT NULL,anchor TEXT NOT NULL);
            CREATE TABLE operation(id TEXT PRIMARY KEY,capture_id INTEGER UNIQUE NOT NULL REFERENCES capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE receipt(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE outbox(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE index_job(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE commit_log(commit_rev INTEGER PRIMARY KEY,operation_id TEXT UNIQUE NOT NULL,kind TEXT NOT NULL CHECK(kind IN('text','pdf_import','pdf_capture')),entity_id INTEGER NOT NULL,input_hash TEXT NOT NULL,receipt TEXT NOT NULL);
            CREATE TABLE pdf_document(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,original_hash TEXT NOT NULL,original_bytes INTEGER NOT NULL,extraction_revision TEXT NOT NULL,record TEXT NOT NULL);
            CREATE TABLE pdf_extraction(document_id INTEGER PRIMARY KEY REFERENCES pdf_document(id),snapshot TEXT NOT NULL);
            CREATE TABLE pdf_capture(id INTEGER PRIMARY KEY,document_id INTEGER NOT NULL REFERENCES pdf_document(id),record TEXT NOT NULL,anchor TEXT NOT NULL);
            CREATE TABLE pdf_operation(operation_id TEXT PRIMARY KEY REFERENCES commit_log(operation_id),kind TEXT NOT NULL,document_id INTEGER NOT NULL REFERENCES pdf_document(id),capture_id INTEGER UNIQUE REFERENCES pdf_capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE pdf_receipt(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE pdf_outbox(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE pdf_index_job(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE vocab_wordbook(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,description TEXT NOT NULL,created_at_ms INTEGER NOT NULL,UNIQUE(vault,title));
            CREATE TABLE vocab_card(id INTEGER PRIMARY KEY,wordbook_id INTEGER NOT NULL REFERENCES vocab_wordbook(id),position INTEGER NOT NULL,term TEXT NOT NULL,meaning TEXT NOT NULL,example TEXT NOT NULL,UNIQUE(wordbook_id,position));
            CREATE TABLE vocab_schedule(card_id INTEGER PRIMARY KEY REFERENCES vocab_card(id),due_at_ms INTEGER NOT NULL,stage TEXT NOT NULL CHECK(stage IN('learning','review')),stability REAL NOT NULL,difficulty REAL NOT NULL,reps INTEGER NOT NULL CHECK(reps > 0),lapses INTEGER NOT NULL CHECK(lapses >= 0 AND lapses <= reps),last_review_at_ms INTEGER NOT NULL,last_rating INTEGER NOT NULL CHECK(last_rating BETWEEN 1 AND 4));
            CREATE TABLE vocab_review(id INTEGER PRIMARY KEY,card_id INTEGER NOT NULL REFERENCES vocab_card(id),operation_id TEXT NOT NULL UNIQUE,rating INTEGER NOT NULL CHECK(rating BETWEEN 1 AND 4),reviewed_at_ms INTEGER NOT NULL,stage_before TEXT NOT NULL,stage_after TEXT NOT NULL CHECK(stage_after IN('learning','review')),stability REAL NOT NULL,difficulty REAL NOT NULL,scheduled_minutes INTEGER NOT NULL CHECK(scheduled_minutes > 0),due_at_ms INTEGER NOT NULL,algorithm TEXT NOT NULL);
            INSERT INTO vocab_wordbook(id,vault,title,description,created_at_ms) VALUES(1,'main','Legacy Deck','Before v5',1767225600000);
            INSERT INTO vocab_card(id,wordbook_id,position,term,meaning,example) VALUES(10,1,0,'legacy','遗留','Legacy example.');
            PRAGMA user_version=4;
            COMMIT;",
        )
        .unwrap();
    }

    let store = SqliteStore::open(&dir.0).unwrap();
    let app = StudyApplication::new(store);
    let cards = app.wordbook_cards("main", 1, 10, 0).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].term, "legacy");
    assert_eq!(cards[0].pos, "");
    assert_eq!(cards[0].phonetic, "");

    let db = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 5);
    let v5_tables: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('epub_documents','epub_chapters','epub_toc','epub_captures','knowledge_audit')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v5_tables, 5);
}
