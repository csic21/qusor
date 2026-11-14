//! Trusted adapter from the public `notes-query-v1` wire to application reads.
//!
//! The Host binds the vault and authorizes `notes.read`; this adapter only maps
//! bounded requests onto `learning-application` and keeps every encoded reply
//! inside the 4 KiB wire budget. It never resolves anchors or writes knowledge.
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode};
use extension_api::{
    ApiError, ErrorCode,
    notes::{
        MAX_NOTE_CHUNK_BYTES, MAX_NOTE_PREVIEW_BYTES, MAX_NOTE_TITLE_BYTES, MIN_NOTE_CHUNK_BYTES,
        NoteBody, NoteField, NoteSummary, NotesOperation, NotesRequest, NotesResponse, NotesResult,
    },
};
use learning_application::{
    KnowledgeApplication,
    pdf_notes::{NoteBodyField, PdfNoteSummary},
};
use learning_ports::KnowledgeQuery;

/// One trusted note reader bound to the same vault as its Host.
pub struct NotesReadBridge<S: KnowledgeQuery> {
    app: KnowledgeApplication<S>,
    vault: String,
}

impl<S: KnowledgeQuery> NotesReadBridge<S> {
    pub fn new(app: KnowledgeApplication<S>, vault: impl Into<String>) -> Self {
        Self {
            app,
            vault: vault.into(),
        }
    }

    pub fn vault(&self) -> &str {
        &self.vault
    }

    /// Produce one bounded reply. A reply that cannot be encoded within the
    /// public budget becomes an explicit `BudgetExceeded` error instead.
    pub fn respond(&self, request: &NotesRequest) -> NotesResponse {
        let request_id = request.request_id.clone();
        let result = match &request.operation {
            NotesOperation::Page { cursor, limit } => self.page(cursor.as_deref(), *limit),
            NotesOperation::Body {
                capture_id,
                field,
                offset,
                max_bytes,
            } => self.body(&request_id, *capture_id, *field, *offset, *max_bytes),
        };
        let response = NotesResponse { request_id, result };
        if response.to_json().is_ok() {
            response
        } else {
            NotesResponse {
                request_id: response.request_id,
                result: NotesResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note response exceeds the wire budget",
                    ),
                },
            }
        }
    }

    fn page(&self, cursor: Option<&str>, limit: u16) -> NotesResult {
        match self
            .app
            .pdf_notes_page(&self.vault, None, cursor, usize::from(limit))
        {
            Ok(page) => NotesResult::Page {
                notes: page.notes.iter().map(summary).collect(),
                next_cursor: page.next_cursor,
            },
            Err(error) => NotesResult::Error {
                error: map_error(&error),
            },
        }
    }

    fn body(
        &self,
        request_id: &str,
        capture_id: i64,
        field: NoteField,
        offset: u32,
        max_bytes: u16,
    ) -> NotesResult {
        let app_field = match field {
            NoteField::Quote => NoteBodyField::Quote,
            NoteField::Note => NoteBodyField::Note,
        };
        let offset_usize = usize::try_from(offset).unwrap_or(0);
        let mut budget = usize::from(max_bytes.min(MAX_NOTE_CHUNK_BYTES));
        loop {
            match self
                .app
                .pdf_note_body(&self.vault, capture_id, app_field, offset_usize, budget)
            {
                Ok(chunk) => {
                    let Ok(total_bytes) = u64::try_from(chunk.total_bytes) else {
                        return NotesResult::Error {
                            error: ApiError::new(
                                ErrorCode::InvalidState,
                                "note body size is unsupported",
                            ),
                        };
                    };
                    let next_offset = match chunk.next_offset {
                        Some(value) => match u32::try_from(value) {
                            Ok(value) => Some(value),
                            Err(_) => {
                                return NotesResult::Error {
                                    error: ApiError::new(
                                        ErrorCode::InvalidState,
                                        "note body offset is unsupported",
                                    ),
                                };
                            }
                        },
                        None => None,
                    };
                    let result = NotesResult::Body(NoteBody {
                        capture_id: chunk.capture_id,
                        commit_rev: chunk.commit_rev,
                        document_id: chunk.document_id,
                        field,
                        offset,
                        total_bytes,
                        text: chunk.text,
                        next_offset,
                    });
                    let probe = NotesResponse {
                        request_id: request_id.to_owned(),
                        result: result.clone(),
                    };
                    if probe.to_json().is_ok() {
                        return result;
                    }
                }
                Err(error) => {
                    return NotesResult::Error {
                        error: map_error(&error),
                    };
                }
            }
            let minimum = usize::from(MIN_NOTE_CHUNK_BYTES);
            if budget <= minimum {
                return NotesResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note chunk cannot fit the wire budget",
                    ),
                };
            }
            budget = (budget / 2).max(minimum);
        }
    }
}

fn summary(note: &PdfNoteSummary) -> NoteSummary {
    let quote_preview = sanitize(&note.quote_preview, MAX_NOTE_PREVIEW_BYTES, "");
    let note_preview = sanitize(&note.note_preview, MAX_NOTE_PREVIEW_BYTES, "");
    let quote_bytes = note.quote_bytes as u64;
    let note_bytes = note.note_bytes as u64;
    NoteSummary {
        capture_id: note.capture_id,
        commit_rev: note.commit_rev,
        document_id: note.document_id,
        document_title: sanitize(&note.document_title, MAX_NOTE_TITLE_BYTES, "?"),
        page_index: note.page_index,
        quote_truncated: quote_bytes > quote_preview.len() as u64,
        quote_preview,
        quote_bytes,
        note_truncated: note_bytes > note_preview.len() as u64,
        note_preview,
        note_bytes,
        compound: note.compound,
    }
}

/// Bound one preview to the wire budget at a UTF-8 boundary. Control
/// characters other than a line break are replaced: JSON escaping would
/// otherwise inflate a preview without showing anything.
fn sanitize(value: &str, limit: usize, empty: &str) -> String {
    let mut out = String::new();
    for character in value.chars() {
        let character = if character.is_control() && character != '\n' {
            ' '
        } else {
            character
        };
        if out.len() + character.len_utf8() > limit {
            break;
        }
        out.push(character);
    }
    if out.is_empty() {
        empty.to_owned()
    } else {
        out
    }
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
    ApiError::new(code, "note read failed")
}
