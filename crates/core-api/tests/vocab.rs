use core_api::{
    error::KnowledgeErrorCode,
    vocab::{
        CardSchedule, NewWord, ReviewEntry, STUDY_ALGORITHM, STUDY_SCHEMA_VERSION, StudyArchive,
        StudyCard, StudyRating, StudyStage, Wordbook, WordbookSeed, validate_archive,
        validate_seed,
    },
};

const NOW: i64 = 1_767_225_600_000;
const DAY: i64 = 86_400_000;

fn seed() -> WordbookSeed {
    WordbookSeed {
        title: "sample".into(),
        description: "synthetic".into(),
        words: vec![NewWord {
            term: "abandon".into(),
            meaning: "放弃".into(),
            example: "Do not abandon it.".into(),
            pos: "v.".into(),
            phonetic: "/əˈbændən/".into(),
        }],
    }
}

fn archive() -> StudyArchive {
    StudyArchive {
        schema_version: STUDY_SCHEMA_VERSION,
        wordbooks: vec![Wordbook {
            id: 1,
            vault: "vault".into(),
            title: "sample".into(),
            description: "synthetic".into(),
            created_at_ms: NOW,
        }],
        cards: vec![StudyCard {
            id: 1,
            wordbook_id: 1,
            position: 0,
            term: "abandon".into(),
            meaning: "放弃".into(),
            example: "Do not abandon it.".into(),
            pos: "v.".into(),
            phonetic: "/əˈbændən/".into(),
        }],
        schedules: vec![CardSchedule {
            card_id: 1,
            due_at_ms: NOW + 3 * DAY,
            stage: StudyStage::Review,
            stability: 3.173,
            difficulty: 5.0,
            reps: 1,
            lapses: 0,
            last_review_at_ms: NOW,
            last_rating: StudyRating::Good,
        }],
        reviews: vec![ReviewEntry {
            id: 1,
            card_id: 1,
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
        }],
    }
}

#[test]
fn a_consistent_archive_round_trips() {
    let archive = archive();
    assert!(validate_archive(&archive).is_ok());
    let json = archive.to_json().unwrap();
    let decoded: StudyArchive = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, archive);
    assert_eq!(validate_seed("vault", &seed()), Ok(()));
}

#[test]
fn seeds_reject_bounds_duplicates_and_empty_content() {
    let mut empty = seed();
    empty.words.clear();
    assert_eq!(
        validate_seed("vault", &empty).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut duplicate = seed();
    duplicate.words.push(duplicate.words[0].clone());
    assert_eq!(
        validate_seed("vault", &duplicate).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut long_title = seed();
    long_title.title = "t".repeat(201);
    assert_eq!(
        validate_seed("vault", &long_title).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut control = seed();
    control.words[0].term = "bad\nterm".into();
    assert_eq!(
        validate_seed("vault", &control).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    assert_eq!(
        validate_seed("", &seed()).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );
}

#[test]
fn archives_reject_inconsistent_identity_and_history() {
    let mut duplicate_book = archive();
    duplicate_book
        .wordbooks
        .push(duplicate_book.wordbooks[0].clone());
    assert_eq!(
        validate_archive(&duplicate_book).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut unknown_book = archive();
    unknown_book.cards[0].wordbook_id = 9;
    assert_eq!(
        validate_archive(&unknown_book).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut duplicate_card = archive();
    duplicate_card.cards.push(duplicate_card.cards[0].clone());
    assert_eq!(
        validate_archive(&duplicate_card).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut missing_schedule = archive();
    missing_schedule.schedules.clear();
    assert_eq!(
        validate_archive(&missing_schedule).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut missing_history = archive();
    missing_history.reviews.clear();
    assert_eq!(
        validate_archive(&missing_history).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut mismatched = archive();
    mismatched.schedules[0].due_at_ms = NOW + 9 * DAY;
    assert_eq!(
        validate_archive(&mismatched).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut stale_reps = archive();
    stale_reps.schedules[0].reps = 2;
    assert_eq!(
        validate_archive(&stale_reps).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut duplicate_operation = archive();
    duplicate_operation.reviews.push(ReviewEntry {
        id: 2,
        ..duplicate_operation.reviews[0].clone()
    });
    assert_eq!(
        validate_archive(&duplicate_operation).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut zero_id = archive();
    zero_id.reviews[0].id = 0;
    assert_eq!(
        validate_archive(&zero_id).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut unknown_algorithm = archive();
    unknown_algorithm.reviews[0].algorithm = "other".into();
    assert_eq!(
        validate_archive(&unknown_algorithm).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );

    let mut wrong_schema = archive();
    wrong_schema.schema_version = 2;
    assert_eq!(
        validate_archive(&wrong_schema).unwrap_err().code,
        KnowledgeErrorCode::UnsupportedVersion
    );
}

#[test]
fn pos_and_phonetic_validate_bounds_and_deserialize_legacy_json() {
    use core_api::vocab::{MAX_PHONETIC_BYTES, MAX_POS_BYTES};

    let legacy_seed_json = r#"{
        "title": "legacy",
        "description": "no pos or phonetic",
        "words": [
            { "term": "alpha", "meaning": "第一个", "example": "Alpha test." }
        ]
    }"#;
    let legacy_seed: WordbookSeed = serde_json::from_str(legacy_seed_json).unwrap();
    assert_eq!(legacy_seed.words[0].pos, "");
    assert_eq!(legacy_seed.words[0].phonetic, "");
    assert!(validate_seed("vault", &legacy_seed).is_ok());

    let legacy_card_json = r#"{
        "id": 1,
        "wordbook_id": 1,
        "position": 0,
        "term": "alpha",
        "meaning": "第一个",
        "example": ""
    }"#;
    let legacy_card: StudyCard = serde_json::from_str(legacy_card_json).unwrap();
    assert_eq!(legacy_card.pos, "");
    assert_eq!(legacy_card.phonetic, "");

    let mut bad_pos = seed();
    bad_pos.words[0].pos = "n".repeat(MAX_POS_BYTES + 1);
    assert_eq!(
        validate_seed("vault", &bad_pos).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut bad_pos_ctrl = seed();
    bad_pos_ctrl.words[0].pos = "n.\n".into();
    assert_eq!(
        validate_seed("vault", &bad_pos_ctrl).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut bad_phonetic = seed();
    bad_phonetic.words[0].phonetic = "a".repeat(MAX_PHONETIC_BYTES + 1);
    assert_eq!(
        validate_seed("vault", &bad_phonetic).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut bad_phonetic_ctrl = seed();
    bad_phonetic_ctrl.words[0].phonetic = "/a\t/".into();
    assert_eq!(
        validate_seed("vault", &bad_phonetic_ctrl).unwrap_err().code,
        KnowledgeErrorCode::InvalidInput
    );

    let mut bad_card_archive = archive();
    bad_card_archive.cards[0].pos = "x".repeat(MAX_POS_BYTES + 1);
    assert_eq!(
        validate_archive(&bad_card_archive).unwrap_err().code,
        KnowledgeErrorCode::Integrity
    );
}
