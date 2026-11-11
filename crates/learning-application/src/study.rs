//! Vocabulary study use cases: wordbook seeding, due queue, guided review and
//! study export/restore. Scheduling math lives in `learning-domain`; this module
//! binds a trusted `StudyStore`, keeps Core state consistent and never grants
//! Host authority, package identity or UI behavior.
use core_api::{
    knowledge::{KnowledgeError, KnowledgeErrorCode, Result},
    vocab::{
        CardSchedule, DAILY_NEW_LIMIT, MAX_OPERATION_ID_BYTES, MAX_QUEUE_SIZE, MAX_TIMESTAMP_MS,
        ReviewEntry, ReviewGuard, STUDY_ALGORITHM, StudyArchive, StudyCard, StudyRating,
        StudyStage, Wordbook, WordbookSeed, validate_archive, validate_seed,
    },
};
use learning_domain::fsrs::{self, MemoryState, Policy, Rating, ScheduleInput, Stage};
use learning_ports::StudyStore;
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn not_found(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::NotFound, message)
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
pub struct StudyApplication<S: StudyStore> {
    store: S,
}

impl<S: StudyStore> StudyApplication<S> {
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

    /// Wordbooks in one vault with total/due/new counters for the local day.
    pub fn decks(&self, vault: &str, now_ms: i64) -> Result<Vec<DeckSummary>> {
        if now_ms <= 0 {
            return Err(invalid("invalid study timestamp"));
        }
        let archive = self.store.study_snapshot()?;
        let mut cards: BTreeMap<i64, Vec<&StudyCard>> = BTreeMap::new();
        for card in &archive.cards {
            cards.entry(card.wordbook_id).or_default().push(card);
        }
        let mut schedules = BTreeMap::new();
        for schedule in &archive.schedules {
            schedules.insert(schedule.card_id, schedule);
        }
        let mut decks = Vec::new();
        for book in archive.wordbooks.iter().filter(|book| book.vault == vault) {
            let book_cards = cards.get(&book.id).map(Vec::as_slice).unwrap_or(&[]);
            let mut due = 0u32;
            let mut new = 0u32;
            for card in book_cards {
                match schedules.get(&card.id) {
                    Some(schedule) if schedule.due_at_ms <= now_ms => due += 1,
                    Some(_) => {}
                    None => new += 1,
                }
            }
            decks.push(DeckSummary {
                wordbook: book.clone(),
                total: u32::try_from(book_cards.len()).unwrap_or(u32::MAX),
                due,
                new,
            });
        }
        decks.sort_by_key(|deck| deck.wordbook.id);
        Ok(decks)
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
        let archive = self.store.study_snapshot()?;
        let book = archive
            .wordbooks
            .iter()
            .find(|book| book.id == wordbook_id && book.vault == vault)
            .ok_or_else(|| not_found("wordbook not found in this vault"))?;
        let schedules: BTreeMap<i64, &CardSchedule> = archive
            .schedules
            .iter()
            .map(|schedule| (schedule.card_id, schedule))
            .collect();
        let book_cards: Vec<&StudyCard> = archive
            .cards
            .iter()
            .filter(|card| card.wordbook_id == book.id)
            .collect();
        let mut due: Vec<&StudyCard> = book_cards
            .iter()
            .copied()
            .filter(|card| {
                schedules
                    .get(&card.id)
                    .is_some_and(|schedule| schedule.due_at_ms <= now_ms)
            })
            .collect();
        due.sort_by_key(|card| {
            (
                schedules.get(&card.id).map(|s| s.due_at_ms),
                card.position,
                card.id,
            )
        });
        let mut new_cards: Vec<&StudyCard> = book_cards
            .iter()
            .copied()
            .filter(|card| !schedules.contains_key(&card.id))
            .collect();
        new_cards.sort_by_key(|card| (card.position, card.id));

        let card_ids: BTreeSet<i64> = book_cards.iter().map(|card| card.id).collect();
        let introduced_today = archive
            .reviews
            .iter()
            .filter(|review| {
                card_ids.contains(&review.card_id)
                    && review.stage_before == StudyStage::New
                    && review.reviewed_at_ms >= day_start_ms
                    && review.reviewed_at_ms <= now_ms
            })
            .count() as u32;
        let new_allowance = DAILY_NEW_LIMIT.saturating_sub(introduced_today);
        let take_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(new_cards.len())
            .min(limit.saturating_sub(due.len().min(limit)));
        let due_take = limit.min(due.len());
        let mut cards = Vec::with_capacity(due_take + take_new);
        for card in &due[..due_take] {
            let stage = schedules
                .get(&card.id)
                .map_or(StudyStage::New, |schedule| schedule.stage);
            cards.push(StudyCardView {
                card: (*card).clone(),
                stage,
            });
        }
        for card in &new_cards[..take_new] {
            cards.push(StudyCardView {
                card: (*card).clone(),
                stage: StudyStage::New,
            });
        }
        // Counts describe what is left after this page, matching the review outcome.
        let remaining_due = due.len().saturating_sub(due_take);
        let remaining_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(new_cards.len())
            .saturating_sub(take_new);
        Ok(StudyQueue {
            cards,
            remaining_due: u32::try_from(remaining_due).unwrap_or(u32::MAX),
            remaining_new: u32::try_from(remaining_new).unwrap_or(u32::MAX),
        })
    }

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
        let archive = self.store.study_snapshot()?;
        let card = archive
            .cards
            .iter()
            .find(|card| card.id == card_id)
            .ok_or_else(|| not_found("card not found"))?;
        let book = archive
            .wordbooks
            .iter()
            .find(|book| book.id == card.wordbook_id && book.vault == vault)
            .ok_or_else(|| not_found("card not found in this vault"))?;
        if let Some(existing) = archive
            .reviews
            .iter()
            .find(|review| review.operation_id == operation_id)
        {
            if existing.card_id != card_id || existing.rating != rating {
                return Err(conflict(
                    "review operation id is already bound to another grade",
                ));
            }
            let counts = self.remaining_counts(&archive, book.id, now_ms, day_start_ms)?;
            return Ok(ReviewOutcome {
                card_id,
                stage: existing.stage_after,
                due_at_ms: existing.due_at_ms,
                scheduled_minutes: existing.scheduled_minutes,
                remaining_due: counts.0,
                remaining_new: counts.1,
            });
        }
        let previous = archive
            .schedules
            .iter()
            .find(|schedule| schedule.card_id == card_id);
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
        let after = self.store.study_snapshot()?;
        let counts = self.remaining_counts(&after, book.id, now_ms, day_start_ms)?;
        Ok(ReviewOutcome {
            card_id,
            stage: stage_after,
            due_at_ms: scheduled.due_at_ms,
            scheduled_minutes,
            remaining_due: counts.0,
            remaining_new: counts.1,
        })
    }

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

    fn remaining_counts(
        &self,
        archive: &StudyArchive,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
    ) -> Result<(u32, u32)> {
        let card_ids: BTreeSet<i64> = archive
            .cards
            .iter()
            .filter(|card| card.wordbook_id == wordbook_id)
            .map(|card| card.id)
            .collect();
        let due = archive
            .schedules
            .iter()
            .filter(|schedule| card_ids.contains(&schedule.card_id) && schedule.due_at_ms <= now_ms)
            .count();
        let introduced_today = archive
            .reviews
            .iter()
            .filter(|review| {
                card_ids.contains(&review.card_id)
                    && review.stage_before == StudyStage::New
                    && review.reviewed_at_ms >= day_start_ms
                    && review.reviewed_at_ms <= now_ms
            })
            .count();
        let new_cards = archive
            .cards
            .iter()
            .filter(|card| card_ids.contains(&card.id))
            .filter(|card| {
                !archive
                    .schedules
                    .iter()
                    .any(|schedule| schedule.card_id == card.id)
            })
            .count();
        let allowance = DAILY_NEW_LIMIT.saturating_sub(introduced_today as u32);
        Ok((
            u32::try_from(due).unwrap_or(u32::MAX),
            allowance.min(u32::try_from(new_cards).unwrap_or(u32::MAX)),
        ))
    }
}
