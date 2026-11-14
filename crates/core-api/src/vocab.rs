//! Vocabulary study contracts: Core-owned wordbooks, cards, FSRS schedules and
//! append-only review history. No SQL, Host, runtime or UI implementation belongs
//! here; this module only fixes canonical shapes and consistency invariants.
//!
//! Review history is the durable audit record. The current schedule is derived
//! state that must stay consistent with it; export and restore carry both so a
//! reopened Core can continue studying without the plugin that seeded a book.
use crate::error::{
    KnowledgeError, KnowledgeErrorCode, KnowledgeResult as Result, is_bounded_text,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Version of the study archive/envelope itself, independent of Host API and package versions.
pub const STUDY_SCHEMA_VERSION: u32 = 1;
/// Scheduling policy identity recorded with every review; a future algorithm change
/// must introduce a new value instead of silently reinterpreting old history.
pub const STUDY_ALGORITHM: &str = "qusor-fsrs-5-v1";
/// Maximum UTF-8 bytes of a wordbook title.
pub const MAX_WORDBOOK_TITLE_BYTES: usize = 200;
/// Maximum UTF-8 bytes of a wordbook description.
pub const MAX_WORDBOOK_DESCRIPTION_BYTES: usize = 1024;
/// Maximum UTF-8 bytes of one word term.
pub const MAX_TERM_BYTES: usize = 128;
/// Maximum UTF-8 bytes of one word meaning.
pub const MAX_MEANING_BYTES: usize = 512;
/// Maximum UTF-8 bytes of one example sentence.
pub const MAX_EXAMPLE_BYTES: usize = 1024;
/// Maximum UTF-8 bytes of one part-of-speech tag.
pub const MAX_POS_BYTES: usize = 64;
/// Maximum UTF-8 bytes of one phonetic transcription.
pub const MAX_PHONETIC_BYTES: usize = 128;
/// Maximum configurable daily new card limit.
pub const MAX_DAILY_NEW_LIMIT: u32 = 200;
/// Maximum UTF-8 bytes of a vault identifier.
pub const MAX_VAULT_BYTES: usize = 256;
/// Maximum bytes of an idempotency operation id.
pub const MAX_OPERATION_ID_BYTES: usize = 128;
/// Maximum words in one seeded wordbook.
pub const MAX_WORDS_PER_WORDBOOK: usize = 4096;
/// Maximum wordbooks in one archive.
pub const MAX_WORDBOOKS: usize = 256;
/// Maximum cards in one archive.
pub const MAX_CARDS: usize = 65_536;
/// Maximum reviews in one archive.
pub const MAX_REVIEWS: usize = 262_144;
/// Maximum encoded study archive size.
pub const MAX_STUDY_ARCHIVE_BYTES: usize = 16 * 1024 * 1024;
/// Smallest accepted FSRS stability.
pub const MIN_STABILITY: f64 = 0.0001;
/// Largest accepted FSRS stability.
pub const MAX_STABILITY: f64 = 36_500.0;
/// Smallest accepted FSRS difficulty.
pub const MIN_DIFFICULTY: f64 = 1.0;
/// Largest accepted FSRS difficulty.
pub const MAX_DIFFICULTY: f64 = 10.0;
/// Largest accepted timestamp in milliseconds (end of year 9999).
pub const MAX_TIMESTAMP_MS: i64 = 253_402_300_799_999;
/// Largest scheduled interval in minutes (36500 days).
pub const MAX_SCHEDULED_MINUTES: u32 = 52_560_000;
/// Fixed daily introduction budget for the first study slice. A later settings
/// capability may replace it, but progress must stay reproducible until then.
pub const DAILY_NEW_LIMIT: u32 = 10;
/// Maximum cards one study queue page may carry before wire truncation.
pub const MAX_QUEUE_SIZE: usize = 50;

fn invalid<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::invalid_input(message))
}

fn corrupt<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::integrity(message))
}

fn text_ok(value: &str, max: usize, required: bool, allow_newline: bool) -> bool {
    is_bounded_text(value, max, required, allow_newline, false)
}

fn vault_ok(vault: &str) -> bool {
    text_ok(vault, MAX_VAULT_BYTES, true, false)
}

fn timestamp_ok(value: i64) -> bool {
    (1..=MAX_TIMESTAMP_MS).contains(&value)
}

fn stability_ok(value: f64) -> bool {
    value.is_finite() && (MIN_STABILITY..=MAX_STABILITY).contains(&value)
}

fn difficulty_ok(value: f64) -> bool {
    value.is_finite() && (MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&value)
}

fn operation_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OPERATION_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'*')
        })
}

/// One of the four FSRS grades, in the canonical order 1..=4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyRating {
    Again,
    Hard,
    Good,
    Easy,
}

impl StudyRating {
    /// Stable numeric code used by storage and the public wire.
    pub fn code(self) -> u8 {
        match self {
            Self::Again => 1,
            Self::Hard => 2,
            Self::Good => 3,
            Self::Easy => 4,
        }
    }

    /// Reject unknown codes instead of coercing them to a grade.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }
}

/// Where a card currently sits in the study loop. `New` has no schedule row yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyStage {
    New,
    Learning,
    Review,
}

impl StudyStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Learning => "learning",
            Self::Review => "review",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "new" => Some(Self::New),
            "learning" => Some(Self::Learning),
            "review" => Some(Self::Review),
            _ => None,
        }
    }

    fn scheduled(self) -> bool {
        matches!(self, Self::Learning | Self::Review)
    }
}

/// One word the host may seed into a wordbook. Content is Core data once committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWord {
    pub term: String,
    pub meaning: String,
    pub example: String,
    #[serde(default)]
    pub pos: String,
    #[serde(default)]
    pub phonetic: String,
}

/// Idempotent wordbook seed keyed by `(vault, title)` in the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordbookSeed {
    pub title: String,
    pub description: String,
    pub words: Vec<NewWord>,
}

/// A committed wordbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wordbook {
    pub id: i64,
    pub vault: String,
    pub title: String,
    pub description: String,
    pub created_at_ms: i64,
}

/// One committed card. Terms are unique inside a wordbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyCard {
    pub id: i64,
    pub wordbook_id: i64,
    pub position: u32,
    pub term: String,
    pub meaning: String,
    pub example: String,
    #[serde(default)]
    pub pos: String,
    #[serde(default)]
    pub phonetic: String,
}

/// Current scheduling state for one card; `reps` starts at 1 with the first review.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSchedule {
    pub card_id: i64,
    pub due_at_ms: i64,
    pub stage: StudyStage,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: u32,
    pub lapses: u32,
    pub last_review_at_ms: i64,
    pub last_rating: StudyRating,
}

/// Optimistic guard for one transactional review commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewGuard {
    pub card_id: i64,
    /// 0 means "no schedule row exists yet" (a first review).
    pub expected_reps: u32,
    pub expected_last_review_at_ms: Option<i64>,
}

/// Append-only audit entry for one graded review.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewEntry {
    pub id: i64,
    pub card_id: i64,
    pub operation_id: String,
    pub rating: StudyRating,
    pub reviewed_at_ms: i64,
    pub stage_before: StudyStage,
    pub stage_after: StudyStage,
    pub stability: f64,
    pub difficulty: f64,
    pub scheduled_minutes: u32,
    pub due_at_ms: i64,
    pub algorithm: String,
}

/// Complete study data needed for export and empty-Core restore.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyArchive {
    pub schema_version: u32,
    pub wordbooks: Vec<Wordbook>,
    pub cards: Vec<StudyCard>,
    pub schedules: Vec<CardSchedule>,
    pub reviews: Vec<ReviewEntry>,
}

impl StudyArchive {
    /// Encode and verify the archive byte budget.
    pub fn to_json(&self) -> Result<String> {
        validate_archive(self)?;
        serde_json::to_string(self).map_err(|_| {
            KnowledgeError::new(KnowledgeErrorCode::Integrity, "invalid study archive")
        })
    }
}

/// Validate one wordbook seed before a trusted store commits it.
pub fn validate_seed(vault: &str, seed: &WordbookSeed) -> Result<()> {
    if !vault_ok(vault) {
        return invalid("invalid study vault");
    }
    if !text_ok(&seed.title, MAX_WORDBOOK_TITLE_BYTES, true, false) {
        return invalid("invalid wordbook title");
    }
    if !text_ok(
        &seed.description,
        MAX_WORDBOOK_DESCRIPTION_BYTES,
        false,
        true,
    ) {
        return invalid("invalid wordbook description");
    }
    if seed.words.is_empty() || seed.words.len() > MAX_WORDS_PER_WORDBOOK {
        return invalid("wordbook word count is outside the supported range");
    }
    let mut terms = BTreeSet::new();
    for word in &seed.words {
        if !text_ok(&word.term, MAX_TERM_BYTES, true, false) {
            return invalid("invalid word term");
        }
        if !text_ok(&word.meaning, MAX_MEANING_BYTES, true, true) {
            return invalid("invalid word meaning");
        }
        if !text_ok(&word.example, MAX_EXAMPLE_BYTES, false, true) {
            return invalid("invalid word example");
        }
        if !text_ok(&word.pos, MAX_POS_BYTES, false, false) {
            return invalid("invalid word part of speech");
        }
        if !text_ok(&word.phonetic, MAX_PHONETIC_BYTES, false, false) {
            return invalid("invalid word phonetic");
        }
        if !terms.insert(word.term.as_str()) {
            return invalid("duplicate word term in one wordbook");
        }
    }
    Ok(())
}

fn validate_wordbook(book: &Wordbook) -> Result<()> {
    if book.id <= 0 || !vault_ok(&book.vault) {
        return corrupt("invalid stored wordbook identity");
    }
    if !text_ok(&book.title, MAX_WORDBOOK_TITLE_BYTES, true, false)
        || !text_ok(
            &book.description,
            MAX_WORDBOOK_DESCRIPTION_BYTES,
            false,
            true,
        )
        || !timestamp_ok(book.created_at_ms)
    {
        return corrupt("invalid stored wordbook");
    }
    Ok(())
}

fn validate_card(card: &StudyCard) -> Result<()> {
    if card.id <= 0 || card.wordbook_id <= 0 {
        return corrupt("invalid stored card identity");
    }
    if !text_ok(&card.term, MAX_TERM_BYTES, true, false)
        || !text_ok(&card.meaning, MAX_MEANING_BYTES, true, true)
        || !text_ok(&card.example, MAX_EXAMPLE_BYTES, false, true)
        || !text_ok(&card.pos, MAX_POS_BYTES, false, false)
        || !text_ok(&card.phonetic, MAX_PHONETIC_BYTES, false, false)
    {
        return corrupt("invalid stored card text");
    }
    Ok(())
}

pub fn validate_schedule(schedule: &CardSchedule) -> Result<()> {
    if schedule.card_id <= 0
        || !schedule.stage.scheduled()
        || !timestamp_ok(schedule.due_at_ms)
        || !stability_ok(schedule.stability)
        || !difficulty_ok(schedule.difficulty)
        || schedule.reps == 0
        || schedule.lapses > schedule.reps
        || !timestamp_ok(schedule.last_review_at_ms)
    {
        return corrupt("invalid stored card schedule");
    }
    Ok(())
}

pub fn validate_review(review: &ReviewEntry) -> Result<()> {
    if review.card_id <= 0
        || !operation_id_ok(&review.operation_id)
        || !timestamp_ok(review.reviewed_at_ms)
        || !review.stage_after.scheduled()
        || !stability_ok(review.stability)
        || !difficulty_ok(review.difficulty)
        || review.scheduled_minutes == 0
        || review.scheduled_minutes > MAX_SCHEDULED_MINUTES
        || !timestamp_ok(review.due_at_ms)
        || review.algorithm != STUDY_ALGORITHM
    {
        return corrupt("invalid stored review entry");
    }
    Ok(())
}

/// Validate a complete study archive before restore or export.
pub fn validate_archive(archive: &StudyArchive) -> Result<()> {
    if archive.schema_version != STUDY_SCHEMA_VERSION {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::UnsupportedVersion,
            "unsupported study archive schema",
        ));
    }
    if archive.wordbooks.len() > MAX_WORDBOOKS
        || archive.cards.len() > MAX_CARDS
        || archive.reviews.len() > MAX_REVIEWS
    {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "study archive budget exceeded",
        ));
    }
    let mut book_ids = BTreeSet::new();
    for book in &archive.wordbooks {
        validate_wordbook(book)?;
        if !book_ids.insert(book.id) {
            return corrupt("duplicate stored wordbook id");
        }
    }
    let mut card_ids = BTreeSet::new();
    for card in &archive.cards {
        validate_card(card)?;
        if !book_ids.contains(&card.wordbook_id) {
            return corrupt("stored card references an unknown wordbook");
        }
        if !card_ids.insert(card.id) {
            return corrupt("duplicate stored card id");
        }
    }
    let mut scheduled = BTreeSet::new();
    for schedule in &archive.schedules {
        validate_schedule(schedule)?;
        if !card_ids.contains(&schedule.card_id) || !scheduled.insert(schedule.card_id) {
            return corrupt("stored schedule references an unknown or duplicate card");
        }
    }
    let mut reviews_by_card: BTreeMap<i64, Vec<&ReviewEntry>> = BTreeMap::new();
    let mut review_ids = BTreeSet::new();
    let mut operations = BTreeSet::new();
    for review in &archive.reviews {
        validate_review(review)?;
        if review.id <= 0 {
            return corrupt("invalid stored review id");
        }
        if !card_ids.contains(&review.card_id) {
            return corrupt("stored review references an unknown card");
        }
        if !review_ids.insert(review.id) {
            return corrupt("duplicate stored review id");
        }
        if !operations.insert(review.operation_id.as_str()) {
            return corrupt("duplicate stored review operation");
        }
        reviews_by_card
            .entry(review.card_id)
            .or_default()
            .push(review);
    }
    // The current schedule must be the exact projection of the append-only history:
    // one review per rep, lapses counted, and the latest entry's outcome stored.
    for schedule in &archive.schedules {
        let reviews = reviews_by_card.get(&schedule.card_id).ok_or_else(|| {
            KnowledgeError::new(
                KnowledgeErrorCode::Integrity,
                "schedule has no review history",
            )
        })?;
        let Some(latest) = reviews
            .iter()
            .max_by_key(|review| (review.reviewed_at_ms, review.id))
        else {
            return corrupt("schedule has no review history");
        };
        let reps = u32::try_from(reviews.len()).unwrap_or(u32::MAX);
        let lapses = u32::try_from(
            reviews
                .iter()
                .filter(|review| review.rating == StudyRating::Again)
                .count(),
        )
        .unwrap_or(u32::MAX);
        if schedule.reps != reps
            || schedule.lapses != lapses
            || schedule.last_review_at_ms != latest.reviewed_at_ms
            || schedule.last_rating != latest.rating
            || schedule.stage != latest.stage_after
            || schedule.due_at_ms != latest.due_at_ms
        {
            return corrupt("stored schedule does not match its review history");
        }
    }
    // Every reviewed card must still have its schedule row.
    for review in &archive.reviews {
        if !archive
            .schedules
            .iter()
            .any(|schedule| schedule.card_id == review.card_id)
        {
            return corrupt("stored review references a card without a schedule");
        }
    }
    let encoded = serde_json::to_vec(archive)
        .map_err(|_| KnowledgeError::new(KnowledgeErrorCode::Integrity, "invalid study archive"))?;
    if encoded.len() > MAX_STUDY_ARCHIVE_BYTES {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::BudgetExceeded,
            "study archive byte budget exceeded",
        ));
    }
    Ok(())
}
