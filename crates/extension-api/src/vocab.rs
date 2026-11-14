//! Versioned bounded vocabulary study wire for ordinary plugins.
//!
//! A guest lists wordbooks, asks for one bounded study queue page or submits one
//! graded review. The Host binds the vault from the session's exact
//! `vocab.read`/`vocab.review` grant and a trusted backend reads or updates Core;
//! no database, package identity or authority is carried here. Every reply must
//! fit the public 4 KiB wire budget.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_5, MAX_REQUEST_BYTES, decode_json, encode_json,
    validate_opaque_id,
};
use serde::{Deserialize, Serialize};

/// Version of the vocabulary study wire itself, independent from Host API and package versions.
pub const VOCAB_PROTOCOL: u16 = 1;
/// Maximum cards in one queue page. This is a wire budget, not a product limit.
pub const MAX_VOCAB_PAGE: u16 = 6;
/// Maximum wordbooks in one deck reply.
pub const MAX_VOCAB_DECKS: usize = 6;
/// Maximum UTF-8 bytes of one word term.
pub const MAX_TERM_BYTES: usize = 64;
/// Maximum UTF-8 bytes of one word meaning.
pub const MAX_MEANING_BYTES: usize = 96;
/// Maximum UTF-8 bytes of one example sentence.
pub const MAX_EXAMPLE_BYTES: usize = 160;
/// Maximum UTF-8 bytes of one wordbook title.
pub const MAX_TITLE_BYTES: usize = 96;

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn budget(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::BudgetExceeded, message)
}

fn protocol(vocab_protocol: u16, api_version: ApiVersion) -> Result<(), ApiError> {
    if vocab_protocol != VOCAB_PROTOCOL
        || !crate::is_supported_api(api_version)
        || api_version < HOST_API_V0_5
    {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "vocab requires protocol1 and minimum Host API0.5",
        ));
    }
    Ok(())
}

fn positive(value: i64, field: &'static str) -> Result<(), ApiError> {
    if value <= 0 {
        return Err(invalid(field));
    }
    Ok(())
}

fn text(
    value: &str,
    limit: usize,
    required: bool,
    allow_newline: bool,
    field: &'static str,
) -> Result<(), ApiError> {
    if (required && value.is_empty())
        || value.len() > limit
        || value
            .chars()
            .any(|ch| ch.is_control() && !(allow_newline && ch == '\n'))
    {
        return Err(invalid(field));
    }
    Ok(())
}

/// One of the four FSRS grades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabRating {
    Again,
    Hard,
    Good,
    Easy,
}

/// Where a card currently sits in the study loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabStage {
    New,
    Learning,
    Review,
}

/// One vocabulary study request from a guest; the Host supplies the vault scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabRequest {
    pub vocab_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub operation: VocabOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VocabOperation {
    /// Wordbooks and counters inside the Host-bound vault.
    Decks {},
    /// Due reviews first, then new cards while the daily budget allows.
    Queue { wordbook_id: i64, limit: u16 },
    /// One graded review; `request_id` is also the idempotency key.
    Review { card_id: i64, rating: VocabRating },
}

impl VocabRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        protocol(self.vocab_protocol, self.api_version)?;
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.operation {
            VocabOperation::Decks {} => {}
            VocabOperation::Queue { wordbook_id, limit } => {
                positive(*wordbook_id, "invalid wordbook id")?;
                if *limit == 0 || *limit > MAX_VOCAB_PAGE {
                    return Err(budget("study queue limit is outside the supported range"));
                }
            }
            VocabOperation::Review { card_id, .. } => {
                positive(*card_id, "invalid card id")?;
            }
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let request: Self = decode_json(json, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        request.validate()?;
        Ok(request)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}

/// One wordbook summary with bounded counters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabDeck {
    pub wordbook_id: i64,
    pub title: String,
    pub total: u32,
    pub due: u32,
    pub new: u32,
}

impl VocabDeck {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.wordbook_id, "invalid wordbook id")?;
        text(
            &self.title,
            MAX_TITLE_BYTES,
            true,
            false,
            "invalid wordbook title",
        )?;
        if self.due > self.total || self.new > self.total {
            return Err(invalid("wordbook counters are inconsistent"));
        }
        Ok(())
    }
}

/// One queue card with the text a study surface needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabCardView {
    pub card_id: i64,
    pub term: String,
    pub meaning: String,
    pub example: String,
    pub stage: VocabStage,
}

impl VocabCardView {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.card_id, "invalid card id")?;
        text(&self.term, MAX_TERM_BYTES, true, false, "invalid word term")?;
        text(
            &self.meaning,
            MAX_MEANING_BYTES,
            true,
            true,
            "invalid word meaning",
        )?;
        text(
            &self.example,
            MAX_EXAMPLE_BYTES,
            false,
            true,
            "invalid word example",
        )?;
        Ok(())
    }
}

/// The committed result of one graded review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabReviewOutcome {
    pub card_id: i64,
    pub stage: VocabStage,
    pub due_at_ms: i64,
    pub scheduled_minutes: u32,
    pub remaining_due: u32,
    pub remaining_new: u32,
}

impl VocabReviewOutcome {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.card_id, "invalid card id")?;
        if self.stage == VocabStage::New {
            return Err(invalid("review outcome cannot remain new"));
        }
        positive(self.due_at_ms, "invalid review due time")?;
        if self.scheduled_minutes == 0 {
            return Err(invalid("invalid scheduled interval"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VocabResult {
    Decks {
        decks: Vec<VocabDeck>,
    },
    Queue {
        cards: Vec<VocabCardView>,
        remaining_due: u32,
        remaining_new: u32,
    },
    Review(VocabReviewOutcome),
    Error {
        error: ApiError,
    },
}

/// One bounded reply; the adapter must also correlate `request_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabResponse {
    pub request_id: String,
    pub result: VocabResult,
}

impl VocabResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.result {
            VocabResult::Decks { decks } => {
                if decks.len() > MAX_VOCAB_DECKS {
                    return Err(budget("wordbook reply exceeds its budget"));
                }
                for deck in decks {
                    deck.validate()?;
                }
            }
            VocabResult::Queue { cards, .. } => {
                if cards.len() > usize::from(MAX_VOCAB_PAGE) {
                    return Err(budget("study queue page exceeds its budget"));
                }
                for card in cards {
                    card.validate()?;
                }
            }
            VocabResult::Review(outcome) => outcome.validate()?,
            VocabResult::Error { error } => {
                error.validate()?;
            }
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let response: Self = decode_json(json, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        response.validate()?;
        Ok(response)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}
