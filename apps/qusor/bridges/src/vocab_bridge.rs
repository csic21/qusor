//! Trusted adapter from the public `vocab-study-v1` wire to vocabulary use cases.
//!
//! The Host binds the vault and authorizes `vocab.read`/`vocab.review`; this
//! adapter only maps bounded requests onto `learning-application` and keeps every
//! encoded reply inside the 4 KiB wire budget. It never grants authority and never
//! bypasses the scheduling policy in `learning-domain`.
use core_api::{
    knowledge::{KnowledgeError, KnowledgeErrorCode},
    vocab::{StudyRating, StudyStage},
};
use extension_api::{
    ApiError, ErrorCode,
    vocab::{
        MAX_MEANING_BYTES, MAX_TERM_BYTES, MAX_TITLE_BYTES, MAX_VOCAB_DECKS, MAX_VOCAB_PAGE,
        VocabCardView, VocabDeck, VocabOperation, VocabRating, VocabRequest, VocabResponse,
        VocabResult, VocabReviewOutcome, VocabStage,
    },
};
use learning_application::study::StudyApplication;
use learning_ports::StudyStore;

/// One trusted vocabulary backend bound to the same vault as its Host.
pub struct VocabBridge<S: StudyStore> {
    app: StudyApplication<S>,
    vault: String,
    now_ms: i64,
    day_start_ms: i64,
}

impl<S: StudyStore> VocabBridge<S> {
    pub fn new(
        app: StudyApplication<S>,
        vault: impl Into<String>,
        now_ms: i64,
        day_start_ms: i64,
    ) -> Self {
        Self {
            app,
            vault: vault.into(),
            now_ms,
            day_start_ms,
        }
    }

    pub fn vault(&self) -> &str {
        &self.vault
    }

    /// Trusted callers set the clock per request; the bridge itself never reads
    /// the wall clock, so qualification runs stay deterministic.
    pub fn set_clock(&mut self, now_ms: i64, day_start_ms: i64) {
        self.now_ms = now_ms;
        self.day_start_ms = day_start_ms;
    }

    /// Read-only access for the trusted qualification harness.
    pub fn app(&self) -> &StudyApplication<S> {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut StudyApplication<S> {
        &mut self.app
    }

    /// Produce one bounded reply. A reply that cannot be encoded within the
    /// public budget becomes an explicit `BudgetExceeded` error instead.
    pub fn respond(&mut self, request: &VocabRequest) -> VocabResponse {
        let request_id = request.request_id.clone();
        let result = match &request.operation {
            VocabOperation::Decks {} => self.decks(),
            VocabOperation::Queue { wordbook_id, limit } => self.queue(*wordbook_id, *limit),
            VocabOperation::Review { card_id, rating } => {
                self.review(&request_id, *card_id, *rating)
            }
        };
        let response = VocabResponse { request_id, result };
        if response.to_json().is_ok() {
            response
        } else {
            VocabResponse {
                request_id: response.request_id,
                result: VocabResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "vocab response exceeds the wire budget",
                    ),
                },
            }
        }
    }

    fn decks(&self) -> VocabResult {
        match self.app.decks(&self.vault, self.now_ms) {
            Ok(decks) => VocabResult::Decks {
                decks: decks
                    .iter()
                    .take(MAX_VOCAB_DECKS)
                    .map(|deck| VocabDeck {
                        wordbook_id: deck.wordbook.id,
                        title: bounded(&deck.wordbook.title, MAX_TITLE_BYTES, false),
                        total: deck.total,
                        due: deck.due,
                        new: deck.new,
                    })
                    .collect(),
            },
            Err(error) => VocabResult::Error {
                error: map_error(&error),
            },
        }
    }

    fn queue(&self, wordbook_id: i64, limit: u16) -> VocabResult {
        match self.app.queue(
            &self.vault,
            wordbook_id,
            self.now_ms,
            self.day_start_ms,
            usize::from(limit.min(MAX_VOCAB_PAGE)),
        ) {
            Ok(queue) => VocabResult::Queue {
                cards: queue.cards.iter().map(card_view).collect(),
                remaining_due: queue.remaining_due,
                remaining_new: queue.remaining_new,
            },
            Err(error) => VocabResult::Error {
                error: map_error(&error),
            },
        }
    }

    fn review(&mut self, request_id: &str, card_id: i64, rating: VocabRating) -> VocabResult {
        match self.app.review(
            &self.vault,
            card_id,
            study_rating(rating),
            request_id,
            self.now_ms,
            self.day_start_ms,
        ) {
            Ok(outcome) => VocabResult::Review(VocabReviewOutcome {
                card_id: outcome.card_id,
                stage: stage(outcome.stage),
                due_at_ms: outcome.due_at_ms,
                scheduled_minutes: outcome.scheduled_minutes,
                remaining_due: outcome.remaining_due,
                remaining_new: outcome.remaining_new,
            }),
            Err(error) => VocabResult::Error {
                error: map_error(&error),
            },
        }
    }
}

fn card_view(card: &learning_application::study::StudyCardView) -> VocabCardView {
    VocabCardView {
        card_id: card.card.id,
        term: bounded(&card.card.term, MAX_TERM_BYTES, false),
        meaning: bounded(&card.card.meaning, MAX_MEANING_BYTES, true),
        example: bounded(
            &card.card.example,
            extension_api::vocab::MAX_EXAMPLE_BYTES,
            true,
        ),
        stage: stage(card.stage),
    }
}

fn study_rating(rating: VocabRating) -> StudyRating {
    match rating {
        VocabRating::Again => StudyRating::Again,
        VocabRating::Hard => StudyRating::Hard,
        VocabRating::Good => StudyRating::Good,
        VocabRating::Easy => StudyRating::Easy,
    }
}

fn stage(stage: StudyStage) -> VocabStage {
    match stage {
        StudyStage::New => VocabStage::New,
        StudyStage::Learning => VocabStage::Learning,
        StudyStage::Review => VocabStage::Review,
    }
}

/// Keep the reply inside the public byte budget and replace unsupported control
/// characters; truncation never changes the stored Core text.
fn bounded(value: &str, max: usize, allow_newline: bool) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let ch = if ch.is_control() && !(allow_newline && ch == '\n') {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > max {
            break;
        }
        out.push(ch);
    }
    out
}

fn map_error(error: &KnowledgeError) -> ApiError {
    let code = match error.code {
        KnowledgeErrorCode::InvalidInput => ErrorCode::InvalidRequest,
        KnowledgeErrorCode::BudgetExceeded => ErrorCode::BudgetExceeded,
        KnowledgeErrorCode::NotFound => ErrorCode::NotFound,
        KnowledgeErrorCode::UnsupportedVersion => ErrorCode::UnsupportedVersion,
        KnowledgeErrorCode::Unavailable => ErrorCode::CapabilityUnavailable,
        _ => ErrorCode::InvalidState,
    };
    ApiError::new(code, "vocabulary study request failed")
}
