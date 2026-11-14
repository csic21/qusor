//! Bounded, read-only note queries for trusted workspace consumers.
//!
//! A list response never carries a complete note body: it returns bounded UTF-8
//! previews plus a stable keyset cursor. Full content stays in Core and is read
//! one bounded chunk at a time after the caller has identified one saved
//! capture. Queries never mutate Core, resolve anchors or grant authority.
use crate::KnowledgeApplication;
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode, PdfCaptureRecord, Result};
use learning_ports::{KnowledgeQuery, PdfNoteSummaryRow};

/// Maximum entries in one page. Later entries are reached with `next_cursor`;
/// the page size is not a product limit.
pub const MAX_NOTE_PAGE: usize = 200;
/// Maximum UTF-8 bytes in a quote or note preview.
pub const NOTE_PREVIEW_BYTES: usize = 512;
/// Maximum UTF-8 bytes in one body chunk. Requests below four bytes are
/// rejected so every non-final chunk advances by at least one scalar,
/// including four-byte characters.
pub const MAX_NOTE_CHUNK_BYTES: usize = 16 * 1024;
const MIN_NOTE_CHUNK_BYTES: usize = 4;
const CURSOR_PREFIX: &str = "n1";

/// Which stored field a bounded body chunk reads. Both are immutable Core facts;
/// the quote is the saved source text and the note is the user's own writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteBodyField {
    Quote,
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfNoteSummary {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub operation_id: String,
    pub document_id: i64,
    pub document_title: String,
    pub page_index: u32,
    pub quote_preview: String,
    pub quote_bytes: usize,
    pub quote_truncated: bool,
    pub note_preview: String,
    pub note_bytes: usize,
    pub note_truncated: bool,
    pub compound: bool,
}

/// One newest-first page of note summaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfNotePage {
    pub notes: Vec<PdfNoteSummary>,
    /// Opaque keyset cursor for the next page. `None` means this page is last.
    pub next_cursor: Option<String>,
}

/// One bounded, UTF-8 aligned window of a stored quote or note body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfNoteChunk {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub field: NoteBodyField,
    /// Requested start offset, always on a UTF-8 scalar boundary.
    pub offset: usize,
    /// Complete stored byte length of the requested field.
    pub total_bytes: usize,
    pub text: String,
    /// Start offset of the next chunk. `None` means the field ended.
    pub next_offset: Option<usize>,
}

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn validate_vault(vault: &str) -> Result<()> {
    if vault.is_empty() || vault.len() > 256 {
        return Err(invalid("invalid note vault"));
    }
    Ok(())
}

fn preview(value: &str) -> (String, usize, bool) {
    if value.len() <= NOTE_PREVIEW_BYTES {
        return (value.to_owned(), value.len(), false);
    }
    let mut end = NOTE_PREVIEW_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), value.len(), true)
}

/// Cursors are canonical and opaque: callers must echo the exact string and
/// never construct or compare one. They bind the vault/document scope plus the
/// immutable commit revision and capture id, so a cursor cannot be replayed
/// against another scope and concurrent appends cannot shift a later page.
fn encode_cursor(document_id: Option<i64>, commit_rev: i64, capture_id: i64) -> String {
    let scope = document_id.map_or_else(|| "*".to_owned(), |id| id.to_string());
    format!("{CURSOR_PREFIX}:{scope}:{commit_rev}:{capture_id}")
}

fn canonical_positive(value: &str) -> Result<i64> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| invalid("invalid note cursor"))?;
    if parsed <= 0 || parsed.to_string() != value {
        return Err(invalid("invalid note cursor"));
    }
    Ok(parsed)
}

fn decode_cursor(value: &str) -> Result<(Option<i64>, i64, i64)> {
    let mut parts = value.split(':');
    if parts.next() != Some(CURSOR_PREFIX) {
        return Err(invalid("invalid note cursor"));
    }
    let scope = parts.next().ok_or_else(|| invalid("invalid note cursor"))?;
    let commit = parts.next().ok_or_else(|| invalid("invalid note cursor"))?;
    let capture = parts.next().ok_or_else(|| invalid("invalid note cursor"))?;
    if parts.next().is_some() {
        return Err(invalid("invalid note cursor"));
    }
    let document_id = if scope == "*" {
        None
    } else {
        Some(canonical_positive(scope)?)
    };
    Ok((
        document_id,
        canonical_positive(commit)?,
        canonical_positive(capture)?,
    ))
}

fn quote_text(capture: &PdfCaptureRecord) -> &str {
    match &capture.compound {
        Some(compound) => compound.exact.as_str(),
        None => capture.draft.exact.as_str(),
    }
}

impl<S: KnowledgeQuery> KnowledgeApplication<S> {
    /// Return one newest-first page of note previews in one vault, optionally
    /// one document.
    ///
    /// The query runs against `KnowledgeQuery` without loading full archives or blobs.
    /// It does not resolve anchors, grant authority, or expose the complete note/quote
    /// body. A cursor only continues the exact vault/document scope that produced it.
    pub fn pdf_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<PdfNotePage> {
        validate_vault(vault)?;
        if limit == 0 || limit > MAX_NOTE_PAGE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "note page limit is outside the supported range",
            ));
        }
        let cursor = match after {
            None => None,
            Some(raw) => {
                let (cursor_document, commit_rev, capture_id) = decode_cursor(raw)?;
                if cursor_document != document_id {
                    return Err(invalid("note cursor does not belong to this query scope"));
                }
                Some((commit_rev, capture_id))
            }
        };
        let page = self
            .store
            .pdf_notes_page(vault, document_id, cursor, limit)?;
        let notes = page
            .notes
            .into_iter()
            .map(summary)
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = page
            .next_cursor_key
            .map(|(rev, cap_id)| encode_cursor(document_id, rev, cap_id));
        Ok(PdfNotePage { notes, next_cursor })
    }

    /// Read one bounded window of a stored quote or note body.
    ///
    /// `offset` must be a UTF-8 boundary and is normally the previous
    /// `next_offset`. The returned text never ends inside a scalar; because
    /// `max_bytes` is at least four bytes, every non-final chunk makes progress.
    /// Missing captures and captures in another vault are both `NotFound`; they
    /// are not distinguished to avoid leaking cross-vault existence.
    pub fn pdf_note_body(
        &self,
        vault: &str,
        capture_id: i64,
        field: NoteBodyField,
        offset: usize,
        max_bytes: usize,
    ) -> Result<PdfNoteChunk> {
        validate_vault(vault)?;
        if !(MIN_NOTE_CHUNK_BYTES..=MAX_NOTE_CHUNK_BYTES).contains(&max_bytes) {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "note chunk size is outside the supported range",
            ));
        }
        let found = self
            .store
            .pdf_capture_with_vault(capture_id)?
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "note capture not found")
            })?;
        if found.document_vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "note capture not found",
            ));
        }
        let capture = &found.record;
        let text = match field {
            NoteBodyField::Quote => quote_text(capture),
            NoteBodyField::Note => capture.user_note.as_str(),
        };
        let total_bytes = text.len();
        if offset > total_bytes {
            return Err(invalid("note body offset is outside the stored text"));
        }
        if !text.is_char_boundary(offset) {
            return Err(invalid("note body offset is not a UTF-8 boundary"));
        }
        let mut end = offset.saturating_add(max_bytes).min(total_bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let next_offset = if end < total_bytes { Some(end) } else { None };
        Ok(PdfNoteChunk {
            capture_id: capture.receipt.capture_id,
            commit_rev: capture.receipt.commit_rev,
            document_id: capture.receipt.document_id,
            field,
            offset,
            total_bytes,
            text: text[offset..end].to_owned(),
            next_offset,
        })
    }
}

fn summary(row: PdfNoteSummaryRow) -> Result<PdfNoteSummary> {
    let capture = &row.record;
    let (quote, page_index, compound) = match &capture.compound {
        Some(compound) => (
            compound.exact.as_str(),
            compound
                .segments
                .first()
                .map(|segment| segment.page_index)
                .ok_or_else(|| {
                    KnowledgeError::new(
                        KnowledgeErrorCode::Integrity,
                        "compound note has no source segment",
                    )
                })?,
            true,
        ),
        None => (
            capture.draft.exact.as_str(),
            capture.draft.page_index,
            false,
        ),
    };
    let (quote_preview, quote_bytes, quote_truncated) = preview(quote);
    let (note_preview, note_bytes, note_truncated) = preview(&capture.user_note);
    Ok(PdfNoteSummary {
        capture_id: capture.receipt.capture_id,
        commit_rev: capture.receipt.commit_rev,
        operation_id: capture.receipt.operation_id.clone(),
        document_id: capture.receipt.document_id,
        document_title: row.document_title,
        page_index,
        quote_preview,
        quote_bytes,
        quote_truncated,
        note_preview,
        note_bytes,
        note_truncated,
        compound,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_stop_at_a_utf8_boundary() {
        let (text, bytes, truncated) = preview(&"你".repeat(300));
        assert_eq!(bytes, 900);
        assert!(truncated);
        assert_eq!(text.len(), 510);
        assert_eq!(text.chars().count(), 170);
    }

    #[test]
    fn short_content_is_not_rewritten() {
        let (text, bytes, truncated) = preview("原文\u{0}与想法 😀");
        assert_eq!(text, "原文\u{0}与想法 😀");
        assert_eq!(bytes, text.len());
        assert!(!truncated);
    }

    #[test]
    fn cursors_are_canonical_and_reject_forgeries() {
        assert_eq!(encode_cursor(None, 7, 3), "n1:*:7:3");
        assert_eq!(encode_cursor(Some(2), 7, 3), "n1:2:7:3");
        assert_eq!(decode_cursor("n1:*:7:3").unwrap(), (None, 7, 3));
        assert_eq!(decode_cursor("n1:2:7:3").unwrap(), (Some(2), 7, 3));
        for cursor in [
            "",
            "n1:7:3",
            "n1:*:7",
            "n1:*:7:3:1",
            "n2:*:7:3",
            "n1:*:0:3",
            "n1:*:7:0",
            "n1:*:07:3",
            "n1:*:+7:3",
            "n1:*:7:03",
            "n1:*: 7:3",
            "n1:*:-7:3",
            "n1::7:3",
            "n1:0:7:3",
            "n1:02:7:3",
            "n1:2x:7:3",
        ] {
            assert_eq!(
                decode_cursor(cursor).unwrap_err().code,
                KnowledgeErrorCode::InvalidInput,
                "{cursor}"
            );
        }
    }

    #[test]
    fn chunk_bounds_never_split_a_scalar_and_always_advance() {
        for text in ["a", "你", "😀", "a😀你b", &"\u{0}".repeat(9)] {
            for max_bytes in [MIN_NOTE_CHUNK_BYTES, 5, 8, MAX_NOTE_CHUNK_BYTES] {
                let mut offset: usize = 0;
                let mut rebuilt = String::new();
                loop {
                    let mut end = offset.saturating_add(max_bytes).min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    assert!(end > offset || offset == text.len());
                    rebuilt.push_str(&text[offset..end]);
                    if end == text.len() {
                        break;
                    }
                    offset = end;
                }
                assert_eq!(rebuilt, text);
            }
        }
    }
}
