//! Bounded EPUB import, library listing, chapter text reading, TOC queries,
//! selection captures, and note management for trusted consumers.
use crate::{
    KnowledgeApplication,
    pdf_library::{MAX_LIBRARY_PAGE, MAX_SAVED_SELECTIONS, MIN_LIBRARY_TEXT_CHUNK},
    pdf_notes::{MAX_NOTE_CHUNK_BYTES, MAX_NOTE_PAGE, NOTE_PREVIEW_BYTES, NoteBodyField},
};
use core_api::{
    epub_contract::{EpubSnapshot, EpubTocEntry, MAX_EPUB_CHAPTER_BYTES},
    knowledge::{
        EpubDocumentReceipt, EpubReceipt, KnowledgeError, KnowledgeErrorCode, Provenance, Result,
        native_import, validate_note_input,
    },
};
use learning_ports::{
    EpubChapterLookup, EpubDocumentSummaryRow, EpubNoteSummaryRow, EpubSavedSelectionsLookup,
    KnowledgeCommand, KnowledgeQuery,
};
use std::collections::BTreeMap;

/// Maximum UTF-8 bytes returned by one `epub_chapter_text` chunk request.
pub const MAX_EPUB_TEXT_CHUNK: usize = 64 * 1024;
const MIN_NOTE_CHUNK_BYTES: usize = 4;
const DOC_CURSOR_PREFIX: &str = "e1";
const NOTE_CURSOR_PREFIX: &str = "en1";

/// Read-only metadata summary of one imported EPUB document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubDocumentSummary {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub author: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub chapter_count: usize,
}

/// One newest-first page of EPUB document metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubDocumentPage {
    pub documents: Vec<EpubDocumentSummary>,
    pub next_cursor: Option<String>,
}

/// One bounded, UTF-8 aligned chunk of an EPUB chapter's stored text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubChapterTextChunk {
    pub document_id: i64,
    pub chapter_index: u32,
    pub title: String,
    pub href: String,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
    pub next_offset: Option<usize>,
}

/// Scalar range inside one EPUB chapter to capture into Core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpubSelectionRange {
    pub document_id: i64,
    pub chapter_index: u32,
    pub start: u64,
    pub end: u64,
}

/// Saved scalar range projection for one EPUB chapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpubSavedSelection {
    pub capture_id: i64,
    pub start: u64,
    pub end: u64,
}

/// Bounded preview summary of one saved EPUB capture/note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubNoteSummary {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub operation_id: String,
    pub document_id: i64,
    pub document_title: String,
    pub chapter_index: u32,
    pub quote_preview: String,
    pub quote_bytes: usize,
    pub quote_truncated: bool,
    pub note_preview: String,
    pub note_bytes: usize,
    pub note_truncated: bool,
}

/// One newest-first page of EPUB note previews.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubNotePage {
    pub notes: Vec<EpubNoteSummary>,
    pub next_cursor: Option<String>,
}

/// One bounded, UTF-8 aligned window of a stored EPUB quote or note body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubNoteChunk {
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub field: NoteBodyField,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
    pub next_offset: Option<usize>,
}

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn validate_vault(vault: &str) -> Result<()> {
    if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
        return Err(invalid("invalid EPUB vault"));
    }
    Ok(())
}

fn canonical_positive(value: &str, err_msg: &'static str) -> Result<i64> {
    let parsed = value.parse::<i64>().map_err(|_| invalid(err_msg))?;
    if parsed <= 0 || parsed.to_string() != value {
        return Err(invalid(err_msg));
    }
    Ok(parsed)
}

fn encode_doc_cursor(commit_rev: i64, document_id: i64) -> String {
    format!("{DOC_CURSOR_PREFIX}:{commit_rev}:{document_id}")
}

fn decode_doc_cursor(value: &str) -> Result<(i64, i64)> {
    let mut parts = value.split(':');
    if parts.next() != Some(DOC_CURSOR_PREFIX) {
        return Err(invalid("invalid EPUB library cursor"));
    }
    let commit = canonical_positive(
        parts
            .next()
            .ok_or_else(|| invalid("invalid EPUB library cursor"))?,
        "invalid EPUB library cursor",
    )?;
    let document = canonical_positive(
        parts
            .next()
            .ok_or_else(|| invalid("invalid EPUB library cursor"))?,
        "invalid EPUB library cursor",
    )?;
    if parts.next().is_some() {
        return Err(invalid("invalid EPUB library cursor"));
    }
    Ok((commit, document))
}

fn encode_note_cursor(document_id: Option<i64>, commit_rev: i64, capture_id: i64) -> String {
    let scope = document_id.map_or_else(|| "*".to_owned(), |id| id.to_string());
    format!("{NOTE_CURSOR_PREFIX}:{scope}:{commit_rev}:{capture_id}")
}

fn decode_note_cursor(value: &str) -> Result<(Option<i64>, i64, i64)> {
    let mut parts = value.split(':');
    if parts.next() != Some(NOTE_CURSOR_PREFIX) {
        return Err(invalid("invalid EPUB note cursor"));
    }
    let scope = parts
        .next()
        .ok_or_else(|| invalid("invalid EPUB note cursor"))?;
    let commit = parts
        .next()
        .ok_or_else(|| invalid("invalid EPUB note cursor"))?;
    let capture = parts
        .next()
        .ok_or_else(|| invalid("invalid EPUB note cursor"))?;
    if parts.next().is_some() {
        return Err(invalid("invalid EPUB note cursor"));
    }
    let document_id = if scope == "*" {
        None
    } else {
        Some(canonical_positive(scope, "invalid EPUB note cursor")?)
    };
    Ok((
        document_id,
        canonical_positive(commit, "invalid EPUB note cursor")?,
        canonical_positive(capture, "invalid EPUB note cursor")?,
    ))
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

fn doc_summary(row: EpubDocumentSummaryRow) -> EpubDocumentSummary {
    EpubDocumentSummary {
        document_id: row.document_id,
        commit_rev: row.commit_rev,
        title: row.title,
        author: row.author,
        original_sha256: row.original_sha256,
        original_bytes: row.original_bytes,
        extraction_revision: row.extraction_revision,
        chapter_count: row.chapter_count,
    }
}

fn note_summary(row: EpubNoteSummaryRow) -> EpubNoteSummary {
    let (quote_preview, quote_bytes, quote_truncated) = preview(&row.record.draft.exact);
    let (note_preview, note_bytes, note_truncated) = preview(&row.record.user_note);
    EpubNoteSummary {
        capture_id: row.record.receipt.capture_id,
        commit_rev: row.record.receipt.commit_rev,
        operation_id: row.record.receipt.operation_id,
        document_id: row.record.receipt.document_id,
        document_title: row.document_title,
        chapter_index: row.record.draft.chapter_index,
        quote_preview,
        quote_bytes,
        quote_truncated,
        note_preview,
        note_bytes,
        note_truncated,
    }
}

impl<S: KnowledgeQuery> KnowledgeApplication<S> {
    /// Return one newest-first page of EPUB metadata in `vault`.
    pub fn epub_documents_page(
        &self,
        vault: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<EpubDocumentPage> {
        validate_vault(vault)?;
        if limit == 0 || limit > MAX_LIBRARY_PAGE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "EPUB library page limit is outside the supported range",
            ));
        }
        let cursor = after.map(decode_doc_cursor).transpose()?;
        let page = self.store.epub_documents_page(vault, cursor, limit)?;
        Ok(EpubDocumentPage {
            documents: page.documents.into_iter().map(doc_summary).collect(),
            next_cursor: page
                .next_cursor_key
                .map(|(rev, doc_id)| encode_doc_cursor(rev, doc_id)),
        })
    }

    /// Read one bounded UTF-8 chunk of an EPUB chapter's stored text.
    pub fn epub_chapter_text(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
        offset: usize,
        max_bytes: usize,
    ) -> Result<EpubChapterTextChunk> {
        validate_vault(vault)?;
        if !(MIN_LIBRARY_TEXT_CHUNK..=MAX_EPUB_TEXT_CHUNK).contains(&max_bytes) {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "EPUB text chunk size is outside the supported range",
            ));
        }
        if offset > MAX_EPUB_CHAPTER_BYTES {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "EPUB text offset is outside the supported range",
            ));
        }
        let (title, href, text) =
            match self
                .store
                .epub_chapter_text(vault, document_id, chapter_index)?
            {
                EpubChapterLookup::DocumentMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "EPUB document not found",
                    ));
                }
                EpubChapterLookup::ChapterMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "EPUB chapter not found",
                    ));
                }
                EpubChapterLookup::Found { title, href, text } => (title, href, text),
            };
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err(invalid("EPUB text offset is not a UTF-8 boundary"));
        }
        let mut end = offset.saturating_add(max_bytes).min(text.len());
        while end > offset && !text.is_char_boundary(end) {
            end -= 1;
        }
        let next_offset = (end < text.len()).then_some(end);
        Ok(EpubChapterTextChunk {
            document_id,
            chapter_index,
            title,
            href,
            offset,
            total_bytes: text.len(),
            text: text[offset..end].to_owned(),
            next_offset,
        })
    }

    /// Return the stored table of contents for one EPUB document in `vault`.
    pub fn epub_toc(&self, vault: &str, document_id: i64) -> Result<Vec<EpubTocEntry>> {
        validate_vault(vault)?;
        let toc = self.store.epub_toc(vault, document_id)?.ok_or_else(|| {
            KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB document not found")
        })?;
        Ok(toc
            .into_iter()
            .map(|entry| EpubTocEntry {
                title: entry.title,
                chapter_index: entry.chapter_index,
                depth: entry.depth,
            })
            .collect())
    }

    /// Return deduplicated saved scalar ranges for one EPUB chapter.
    pub fn epub_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<Vec<EpubSavedSelection>> {
        validate_vault(vault)?;
        let (scalar_count, raw_ranges) =
            match self
                .store
                .epub_saved_selections(vault, document_id, chapter_index)?
            {
                EpubSavedSelectionsLookup::DocumentMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "EPUB document not found",
                    ));
                }
                EpubSavedSelectionsLookup::ChapterMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "EPUB chapter not found",
                    ));
                }
                EpubSavedSelectionsLookup::Found {
                    chapter_scalar_count,
                    ranges,
                } => (chapter_scalar_count, ranges),
            };
        let mut ranges = BTreeMap::<(u64, u64), i64>::new();
        for raw in raw_ranges {
            let (start, end) = (raw.start, raw.end);
            if start >= end || end > scalar_count {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "saved EPUB selection range is invalid",
                ));
            }
            ranges
                .entry((start, end))
                .and_modify(|capture_id| {
                    *capture_id = (*capture_id).min(raw.capture_id);
                })
                .or_insert(raw.capture_id);
        }
        if ranges.len() > MAX_SAVED_SELECTIONS {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "saved selection chapter exceeds its range budget",
            ));
        }
        Ok(ranges
            .into_iter()
            .map(|((start, end), capture_id)| EpubSavedSelection {
                capture_id,
                start,
                end,
            })
            .collect())
    }

    /// Return one newest-first page of EPUB note previews in `vault`, optionally filtered by `document_id`.
    pub fn epub_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<EpubNotePage> {
        validate_vault(vault)?;
        if limit == 0 || limit > MAX_NOTE_PAGE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "EPUB note page limit is outside the supported range",
            ));
        }
        let cursor = match after {
            None => None,
            Some(raw) => {
                let (cursor_document, commit_rev, capture_id) = decode_note_cursor(raw)?;
                if cursor_document != document_id {
                    return Err(invalid(
                        "EPUB note cursor does not belong to this query scope",
                    ));
                }
                Some((commit_rev, capture_id))
            }
        };
        let page = self
            .store
            .epub_notes_page(vault, document_id, cursor, limit)?;
        Ok(EpubNotePage {
            notes: page.notes.into_iter().map(note_summary).collect(),
            next_cursor: page
                .next_cursor_key
                .map(|(rev, cap_id)| encode_note_cursor(document_id, rev, cap_id)),
        })
    }

    /// Read one bounded window of a stored EPUB quote or note body.
    pub fn epub_note_body(
        &self,
        vault: &str,
        capture_id: i64,
        field: NoteBodyField,
        offset: usize,
        max_bytes: usize,
    ) -> Result<EpubNoteChunk> {
        validate_vault(vault)?;
        if !(MIN_NOTE_CHUNK_BYTES..=MAX_NOTE_CHUNK_BYTES).contains(&max_bytes) {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "EPUB note chunk size is outside the supported range",
            ));
        }
        let found = self
            .store
            .epub_capture_with_vault(capture_id)?
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB note capture not found")
            })?;
        if found.document_vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB note capture not found",
            ));
        }
        let capture = &found.record;
        let text = match field {
            NoteBodyField::Quote => capture.draft.exact.as_str(),
            NoteBodyField::Note => capture.user_note.as_str(),
        };
        let total_bytes = text.len();
        if offset > total_bytes {
            return Err(invalid("EPUB note body offset is outside the stored text"));
        }
        if !text.is_char_boundary(offset) {
            return Err(invalid("EPUB note body offset is not a UTF-8 boundary"));
        }
        let mut end = offset.saturating_add(max_bytes).min(total_bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let next_offset = if end < total_bytes { Some(end) } else { None };
        Ok(EpubNoteChunk {
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

impl<S: KnowledgeCommand> KnowledgeApplication<S> {
    /// Atomically import an extracted EPUB snapshot into Core, keyed idempotently by `op`.
    pub fn import_epub(
        &mut self,
        op: &str,
        vault: &str,
        snapshot: &EpubSnapshot,
    ) -> Result<EpubDocumentReceipt> {
        native_import(op, vault, &snapshot.title)?;
        snapshot.validate()?;
        if snapshot.vault != vault {
            return Err(invalid("EPUB snapshot vault does not match target vault"));
        }
        self.store.import_epub(op, vault, snapshot)
    }

    /// Capture one scalar range from an EPUB chapter into Core.
    pub fn capture_epub_selection(
        &mut self,
        operation_id: &str,
        vault: &str,
        range: EpubSelectionRange,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<EpubReceipt> {
        validate_vault(vault)?;
        if range.start >= range.end {
            return Err(invalid("EPUB selection range is empty or inverted"));
        }
        self.store.capture_epub_selection(
            operation_id,
            vault,
            range.document_id,
            range.chapter_index,
            range.start,
            range.end,
            origin,
            user_note,
        )
    }

    /// Update the user note on an EPUB capture scoped by `vault`.
    pub fn update_epub_capture_note(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> Result<()> {
        validate_vault(vault)?;
        validate_note_input(note)?;
        self.store.update_epub_capture_note(vault, capture_id, note)
    }

    /// Delete an EPUB capture scoped by `vault`.
    pub fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        validate_vault(vault)?;
        self.store.delete_epub_capture(vault, capture_id)
    }

    /// Delete an EPUB document and its chapters, TOC, and captures scoped by `vault`.
    pub fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        validate_vault(vault)?;
        self.store.delete_epub_document(vault, document_id)
    }
}
