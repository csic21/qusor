//! Vocabulary study use cases: wordbook seeding, due queue, guided review and
//! study export/restore. Scheduling math lives in `learning-domain`; this module
//! binds split study ports, keeps Core state consistent and never grants Host
//! authority, package identity or UI behavior.
use core_api::{
    knowledge::{KnowledgeError, KnowledgeErrorCode, Result},
    vocab::{
        CardSchedule, DAILY_NEW_LIMIT, MAX_DAILY_NEW_LIMIT, MAX_OPERATION_ID_BYTES, MAX_QUEUE_SIZE,
        MAX_TIMESTAMP_MS, MAX_VAULT_BYTES, ReviewEntry, ReviewGuard, STUDY_ALGORITHM, StudyArchive,
        StudyCard, StudyRating, StudyStage, Wordbook, WordbookSeed, validate_archive,
        validate_seed,
    },
};
use learning_domain::fsrs::{self, MemoryState, Policy, Rating, ScheduleInput, Stage};
use learning_ports::{StudyArchivePort, StudyCommand, StudyQuery};

/// Maximum cards returned in a single `wordbook_cards` page.
pub const MAX_WORDBOOK_CARDS_PAGE: usize = 200;

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn validate_vault(vault: &str) -> Result<()> {
    if vault.is_empty() || vault.len() > MAX_VAULT_BYTES || vault.chars().any(char::is_control) {
        return Err(invalid("invalid study vault"));
    }
    Ok(())
}

fn conflict(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::Conflict, message)
}

fn timestamp_ok(value: i64) -> bool {
    (1..=MAX_TIMESTAMP_MS).contains(&value)
}

fn operation_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OPERATION_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'*')
        })
}

fn stage_of(stage: Stage) -> StudyStage {
    match stage {
        Stage::Learning => StudyStage::Learning,
        Stage::Review => StudyStage::Review,
    }
}

fn rating_of(rating: StudyRating) -> Rating {
    match rating {
        StudyRating::Again => Rating::Again,
        StudyRating::Hard => Rating::Hard,
        StudyRating::Good => Rating::Good,
        StudyRating::Easy => Rating::Easy,
    }
}

/// One wordbook with the counters a study entry needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckSummary {
    pub wordbook: Wordbook,
    pub total: u32,
    pub due: u32,
    pub new: u32,
}

/// One queue entry with its current stage (`New` when no schedule exists yet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyCardView {
    pub card: StudyCard,
    pub stage: StudyStage,
}

/// Bounded queue page plus the remaining counts after this page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyQueue {
    pub cards: Vec<StudyCardView>,
    pub remaining_due: u32,
    pub remaining_new: u32,
}

/// Result of one committed (or replayed) review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewOutcome {
    pub card_id: i64,
    pub stage: StudyStage,
    pub due_at_ms: i64,
    pub scheduled_minutes: u32,
    pub remaining_due: u32,
    pub remaining_new: u32,
}

/// Vocabulary study application over one trusted store.
pub struct StudyApplication<S> {
    store: S,
}

impl<S> StudyApplication<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Release the trusted store, for example to reopen the same Core file.
    pub fn into_store(self) -> S {
        self.store
    }
}

impl<S: StudyCommand> StudyApplication<S> {
    /// Commit a seeded wordbook once per `(vault, title)`. Repeating the same seed
    /// returns the existing wordbook and never duplicates cards.
    pub fn seed_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook> {
        validate_seed(vault, seed)?;
        if !timestamp_ok(now_ms) {
            return Err(invalid("invalid study timestamp"));
        }
        self.store.ensure_wordbook(vault, seed, now_ms)
    }

    /// Delete a wordbook scoped by `vault` along with its cards, schedules, and reviews.
    pub fn delete_wordbook(&mut self, vault: &str, wordbook_id: i64) -> Result<()> {
        validate_vault(vault)?;
        if wordbook_id <= 0 {
            return Err(invalid("invalid wordbook id"));
        }
        self.store.delete_wordbook(vault, wordbook_id)
    }
}

impl<S: StudyQuery> StudyApplication<S> {
    /// Wordbooks in one vault with total/due/new counters for the local day.
    pub fn decks(&self, vault: &str, now_ms: i64) -> Result<Vec<DeckSummary>> {
        if now_ms <= 0 {
            return Err(invalid("invalid study timestamp"));
        }
        let rows = self.store.study_decks(vault, now_ms)?;
        Ok(rows
            .into_iter()
            .map(|row| DeckSummary {
                wordbook: row.wordbook,
                total: row.total,
                due: row.due,
                new: row.new_count,
            })
            .collect())
    }

    /// Return a bounded slice of cards belonging to one wordbook in `vault`, ordered by position.
    pub fn wordbook_cards(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StudyCard>> {
        validate_vault(vault)?;
        if wordbook_id <= 0 {
            return Err(invalid("invalid wordbook id"));
        }
        if limit == 0 || limit > MAX_WORDBOOK_CARDS_PAGE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "wordbook card page limit is outside the supported range",
            ));
        }
        self.store.wordbook_cards(vault, wordbook_id, limit, offset)
    }

    /// One bounded queue page: due reviews first, then new cards while the daily
    /// introduction budget allows. The queue never mutates Core.
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
    ) -> Result<StudyQueue> {
        self.queue_inner(
            vault,
            wordbook_id,
            now_ms,
            day_start_ms,
            limit,
            DAILY_NEW_LIMIT,
        )
    }

    /// One bounded queue page using an explicit daily new-card limit (clamped to `0..=200`).
    pub fn queue_with_limit(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        page_size: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueue> {
        if now_ms <= 0 {
            return Err(invalid("invalid study time range"));
        }
        let day_start_ms = now_ms - now_ms.rem_euclid(86_400_000);
        let clamped_limit = daily_new_limit.min(MAX_DAILY_NEW_LIMIT);
        self.queue_inner(
            vault,
            wordbook_id,
            now_ms,
            day_start_ms,
            page_size,
            clamped_limit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_inner(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueue> {
        if wordbook_id <= 0 {
            return Err(invalid("invalid wordbook id"));
        }
        if now_ms <= 0 || day_start_ms < 0 || day_start_ms > now_ms {
            return Err(invalid("invalid study time range"));
        }
        if limit == 0 || limit > MAX_QUEUE_SIZE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "study queue limit is outside the supported range",
            ));
        }
        let data = self.store.study_queue(
            vault,
            wordbook_id,
            now_ms,
            day_start_ms,
            limit,
            daily_new_limit,
        )?;
        Ok(StudyQueue {
            cards: data
                .cards
                .into_iter()
                .map(|entry| StudyCardView {
                    card: entry.card,
                    stage: entry.stage,
                })
                .collect(),
            remaining_due: data.remaining_due,
            remaining_new: data.remaining_new,
        })
    }
}

impl<S: StudyQuery + StudyCommand> StudyApplication<S> {
    /// Grade one card. The operation id is the idempotency key: repeating the same
    /// card/rating returns the stored outcome; reusing it for another card or
    /// rating is a conflict. A stale schedule guard is also a conflict.
    #[allow(clippy::too_many_arguments)]
    pub fn review(
        &mut self,
        vault: &str,
        card_id: i64,
        rating: StudyRating,
        operation_id: &str,
        now_ms: i64,
        day_start_ms: i64,
    ) -> Result<ReviewOutcome> {
        if card_id <= 0 {
            return Err(invalid("invalid card id"));
        }
        if !operation_id_ok(operation_id) {
            return Err(invalid("invalid review operation id"));
        }
        if now_ms <= 0 || day_start_ms < 0 || day_start_ms > now_ms {
            return Err(invalid("invalid study time range"));
        }
        let ctx = self
            .store
            .card_review_context(vault, card_id, operation_id)?;
        if let Some(existing) = ctx.existing_review {
            if existing.card_id != card_id || existing.rating != rating {
                return Err(conflict(
                    "review operation id is already bound to another grade",
                ));
            }
            let counts = self.store.study_remaining_counts(
                ctx.wordbook_id,
                now_ms,
                day_start_ms,
                DAILY_NEW_LIMIT,
            )?;
            return Ok(ReviewOutcome {
                card_id,
                stage: existing.stage_after,
                due_at_ms: existing.due_at_ms,
                scheduled_minutes: existing.scheduled_minutes,
                remaining_due: counts.0,
                remaining_new: counts.1,
            });
        }
        let previous = ctx.current_schedule.as_ref();
        let input = match previous {
            Some(schedule) => ScheduleInput {
                memory: Some(MemoryState {
                    stability: schedule.stability,
                    difficulty: schedule.difficulty,
                }),
                stage: match schedule.stage {
                    StudyStage::Learning => Stage::Learning,
                    StudyStage::Review => Stage::Review,
                    StudyStage::New => return Err(invalid("stored card stage is invalid")),
                },
                last_review_at_ms: Some(schedule.last_review_at_ms),
            },
            None => ScheduleInput {
                memory: None,
                stage: Stage::Learning,
                last_review_at_ms: None,
            },
        };
        let scheduled = fsrs::schedule(input, rating_of(rating), now_ms, &Policy::default())
            .map_err(|_| invalid("card schedule request was rejected"))?;
        let scheduled_minutes = u32::try_from(scheduled.interval_minutes)
            .map_err(|_| invalid("scheduled interval is outside the supported range"))?;
        if scheduled_minutes == 0 {
            return Err(invalid("scheduled interval is outside the supported range"));
        }
        let stage_after = stage_of(scheduled.stage);
        let reps = previous.map_or(1, |schedule| schedule.reps.saturating_add(1));
        let lapses = previous.map_or(0, |schedule| schedule.lapses)
            + u32::from(rating == StudyRating::Again);
        let schedule = CardSchedule {
            card_id,
            due_at_ms: scheduled.due_at_ms,
            stage: stage_after,
            stability: scheduled.memory.stability,
            difficulty: scheduled.memory.difficulty,
            reps,
            lapses,
            last_review_at_ms: now_ms,
            last_rating: rating,
        };
        let guard = ReviewGuard {
            card_id,
            expected_reps: previous.map_or(0, |schedule| schedule.reps),
            expected_last_review_at_ms: previous.map(|schedule| schedule.last_review_at_ms),
        };
        let entry = ReviewEntry {
            id: 0,
            card_id,
            operation_id: operation_id.to_owned(),
            rating,
            reviewed_at_ms: now_ms,
            stage_before: previous.map_or(StudyStage::New, |schedule| schedule.stage),
            stage_after,
            stability: scheduled.memory.stability,
            difficulty: scheduled.memory.difficulty,
            scheduled_minutes,
            due_at_ms: scheduled.due_at_ms,
            algorithm: STUDY_ALGORITHM.to_owned(),
        };
        self.store.commit_review(&guard, &schedule, &entry)?;
        let counts = self.store.study_remaining_counts(
            ctx.wordbook_id,
            now_ms,
            day_start_ms,
            DAILY_NEW_LIMIT,
        )?;
        Ok(ReviewOutcome {
            card_id,
            stage: stage_after,
            due_at_ms: scheduled.due_at_ms,
            scheduled_minutes,
            remaining_due: counts.0,
            remaining_new: counts.1,
        })
    }
}

impl<S: StudyArchivePort> StudyApplication<S> {
    /// Complete study export; the archive is validated before it leaves Core.
    pub fn export_study(&self) -> Result<StudyArchive> {
        let archive = self.store.study_snapshot()?;
        validate_archive(&archive)?;
        Ok(archive)
    }

    /// Restore study data into an empty study space. Captured knowledge is separate.
    pub fn restore_study(&mut self, archive: &StudyArchive) -> Result<()> {
        validate_archive(archive)?;
        self.store.restore_study(archive)
    }
}
