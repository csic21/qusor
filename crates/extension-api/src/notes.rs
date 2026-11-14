//! Versioned bounded note read wire for ordinary plugins.
//!
//! A guest asks for one page of note previews or one bounded body chunk. The
//! Host binds the vault from the session's exact `notes.read` grant and a
//! trusted backend reads Core; no database, anchor resolution or authority is
//! carried here. Preview and chunk bounds keep every response inside the
//! public 4 KiB wire budget.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_4, MAX_REQUEST_BYTES, decode_json, encode_json,
    validate_opaque_id,
};
use serde::{Deserialize, Serialize};

/// Version of the note read wire itself, independent from Host API and package versions.
pub const NOTES_PROTOCOL: u16 = 1;
/// Maximum summaries in one page. This is a wire budget, not a product limit.
pub const MAX_NOTE_PAGE: u16 = 6;
/// Maximum UTF-8 bytes of a quote or note preview in a summary.
pub const MAX_NOTE_PREVIEW_BYTES: usize = 96;
/// Maximum UTF-8 bytes of a document title in a summary.
pub const MAX_NOTE_TITLE_BYTES: usize = 96;
/// Smallest body chunk. Four bytes guarantee progress over any UTF-8 scalar.
pub const MIN_NOTE_CHUNK_BYTES: u16 = 4;
/// Maximum UTF-8 bytes of one body chunk.
pub const MAX_NOTE_CHUNK_BYTES: u16 = 2048;
/// Maximum body start offset; the application model is far smaller.
pub const MAX_NOTE_OFFSET: u32 = 16 * 1024 * 1024;
/// Maximum opaque cursor bytes; its format belongs to the trusted backend.
pub const MAX_NOTE_CURSOR_BYTES: usize = 128;

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn protocol(notes_protocol: u16, api_version: ApiVersion) -> Result<(), ApiError> {
    if notes_protocol != NOTES_PROTOCOL
        || !crate::is_supported_api(api_version)
        || api_version < HOST_API_V0_4
    {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "notes require protocol1 and minimum Host API0.4",
        ));
    }
    Ok(())
}

/// Opaque cursor from the trusted backend. Only bounded printable ASCII passes;
/// canonical format and scope validation stay with the backend.
fn cursor(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > MAX_NOTE_CURSOR_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'*')
        })
    {
        return Err(invalid("invalid note cursor"));
    }
    Ok(())
}

fn positive(value: i64, field: &'static str) -> Result<(), ApiError> {
    if value <= 0 {
        return Err(invalid(field));
    }
    Ok(())
}

/// Which immutable Core field a body chunk reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteField {
    Quote,
    Note,
}

/// One note read request from a guest; the Host supplies the vault scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotesRequest {
    pub notes_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub operation: NotesOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotesOperation {
    /// Newest-first page inside the Host-bound scope.
    Page { cursor: Option<String>, limit: u16 },
    /// One bounded UTF-8 aligned body chunk of one saved capture.
    Body {
        capture_id: i64,
        field: NoteField,
        offset: u32,
        max_bytes: u16,
    },
}

impl NotesRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        protocol(self.notes_protocol, self.api_version)?;
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.operation {
            NotesOperation::Page {
                cursor: after,
                limit,
            } => {
                if *limit == 0 || *limit > MAX_NOTE_PAGE {
                    return Err(ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note page limit is outside the supported range",
                    ));
                }
                if let Some(after) = after {
                    cursor(after)?;
                }
            }
            NotesOperation::Body {
                capture_id,
                offset,
                max_bytes,
                ..
            } => {
                positive(*capture_id, "invalid note capture id")?;
                if *offset > MAX_NOTE_OFFSET {
                    return Err(invalid("note body offset is outside the supported range"));
                }
                if !(MIN_NOTE_CHUNK_BYTES..=MAX_NOTE_CHUNK_BYTES).contains(max_bytes) {
                    return Err(ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note chunk size is outside the supported range",
                    ));
                }
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

/// One bounded preview summary. Previews never replace the stored content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteSummary {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub document_title: String,
    pub page_index: u32,
    pub quote_preview: String,
    pub quote_bytes: u64,
    pub quote_truncated: bool,
    pub note_preview: String,
    pub note_bytes: u64,
    pub note_truncated: bool,
    pub compound: bool,
}

impl NoteSummary {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.capture_id, "invalid note capture id")?;
        positive(self.commit_rev, "invalid note commit revision")?;
        positive(self.document_id, "invalid note document id")?;
        if self.document_title.is_empty()
            || self.document_title.len() > MAX_NOTE_TITLE_BYTES
            || self.document_title.chars().any(char::is_control)
        {
            return Err(invalid("invalid note document title"));
        }
        for (preview, bytes, truncated) in [
            (&self.quote_preview, self.quote_bytes, self.quote_truncated),
            (&self.note_preview, self.note_bytes, self.note_truncated),
        ] {
            if preview.len() > MAX_NOTE_PREVIEW_BYTES {
                return Err(invalid("note preview exceeds its budget"));
            }
            if preview.chars().any(|ch| ch.is_control() && ch != '\n') {
                return Err(invalid(
                    "note preview carries unsupported control characters",
                ));
            }
            let expected =
                u64::try_from(preview.len()).map_err(|_| invalid("invalid note bytes"))?;
            if truncated {
                if bytes <= expected {
                    return Err(invalid("truncated note bytes are inconsistent"));
                }
            } else if bytes != expected {
                return Err(invalid("complete note bytes are inconsistent"));
            }
        }
        Ok(())
    }
}

/// One bounded body chunk of a stored quote or note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteBody {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub field: NoteField,
    pub offset: u32,
    pub total_bytes: u64,
    pub text: String,
    pub next_offset: Option<u32>,
}

impl NoteBody {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.capture_id, "invalid note capture id")?;
        positive(self.commit_rev, "invalid note commit revision")?;
        positive(self.document_id, "invalid note document id")?;
        if self.offset > MAX_NOTE_OFFSET {
            return Err(invalid("note body offset is outside the supported range"));
        }
        if self.text.len() > usize::from(MAX_NOTE_CHUNK_BYTES) {
            return Err(ApiError::new(
                ErrorCode::BudgetExceeded,
                "note body chunk exceeds its budget",
            ));
        }
        let start = u64::from(self.offset);
        let end = start
            .checked_add(u64::try_from(self.text.len()).map_err(|_| invalid("invalid note body"))?)
            .ok_or_else(|| invalid("invalid note body"))?;
        if end > self.total_bytes {
            return Err(invalid("note body chunk exceeds the stored text"));
        }
        match self.next_offset {
            Some(next) => {
                if u64::from(next) != end || end >= self.total_bytes {
                    return Err(invalid("note body next offset is inconsistent"));
                }
            }
            None => {
                if end != self.total_bytes {
                    return Err(invalid("note body end is inconsistent"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotesResult {
    Page {
        notes: Vec<NoteSummary>,
        next_cursor: Option<String>,
    },
    Body(NoteBody),
    Error {
        error: ApiError,
    },
}

/// One bounded reply; the adapter must also correlate `request_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotesResponse {
    pub request_id: String,
    pub result: NotesResult,
}

impl NotesResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.result {
            NotesResult::Page { notes, next_cursor } => {
                if notes.len() > usize::from(MAX_NOTE_PAGE) {
                    return Err(ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note page exceeds its budget",
                    ));
                }
                for note in notes {
                    note.validate()?;
                }
                if let Some(next) = next_cursor {
                    cursor(next)?;
                }
            }
            NotesResult::Body(body) => body.validate()?,
            NotesResult::Error { error } => {
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
