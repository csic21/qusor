//! Bounded, read-only PDF document listing for trusted library consumers.
//!
//! This query returns metadata only. It never returns original PDF bytes, CAS
//! blobs, filesystem paths, selections, notes or plugin authority.
use crate::knowledge::KnowledgeApplication;
use core_api::{
    knowledge::{KnowledgeError, KnowledgeErrorCode, PdfReceipt, Provenance, Result},
    pdf_contract::{PDF_TEXT_UNIT, PdfDraft},
};
use learning_ports::{
    KnowledgeCommand, KnowledgeQuery, PdfDocumentSummaryRow, PdfPageTextLookup,
    PdfSavedSelectionsLookup,
};
use std::collections::BTreeMap;

/// Maximum entries in one Library page. Later entries use `next_cursor`.
pub const MAX_LIBRARY_PAGE: usize = 200;
pub const MAX_LIBRARY_TEXT_CHUNK: usize = 2048;
pub const MIN_LIBRARY_TEXT_CHUNK: usize = 4;
pub const MAX_LIBRARY_TEXT_OFFSET: usize = 16 * 1024 * 1024;
const CURSOR_PREFIX: &str = "l1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfDocumentSummary {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub page_count: usize,
}

/// One newest-first page of PDF document metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfDocumentPage {
    pub documents: Vec<PdfDocumentSummary>,
    /// Opaque keyset cursor for the next page. `None` means this page is last.
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfPageTextChunk {
    pub document_id: i64,
    pub page_index: u32,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
    pub next_offset: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfSelectionRange {
    pub document_id: i64,
    pub page_index: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfSavedSelection {
    pub capture_id: i64,
    pub start: u32,
    pub end: u32,
}

pub const MAX_SAVED_SELECTIONS: usize = 64;

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn validate_vault(vault: &str) -> Result<()> {
    if vault.is_empty() || vault.len() > 256 {
        return Err(invalid("invalid library vault"));
    }
    Ok(())
}

fn encode_cursor(commit_rev: i64, document_id: i64) -> String {
    format!("{CURSOR_PREFIX}:{commit_rev}:{document_id}")
}

fn canonical_positive(value: &str) -> Result<i64> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| invalid("invalid library cursor"))?;
    if parsed <= 0 || parsed.to_string() != value {
        return Err(invalid("invalid library cursor"));
    }
    Ok(parsed)
}

fn decode_cursor(value: &str) -> Result<(i64, i64)> {
    let mut parts = value.split(':');
    if parts.next() != Some(CURSOR_PREFIX) {
        return Err(invalid("invalid library cursor"));
    }
    let commit = canonical_positive(
        parts
            .next()
            .ok_or_else(|| invalid("invalid library cursor"))?,
    )?;
    let document = canonical_positive(
        parts
            .next()
            .ok_or_else(|| invalid("invalid library cursor"))?,
    )?;
    if parts.next().is_some() {
        return Err(invalid("invalid library cursor"));
    }
    Ok((commit, document))
}

fn summary(row: PdfDocumentSummaryRow) -> PdfDocumentSummary {
    PdfDocumentSummary {
        document_id: row.document_id,
        commit_rev: row.commit_rev,
        title: row.title,
        original_sha256: row.original_sha256,
        original_bytes: row.original_bytes,
        extraction_revision: row.extraction_revision,
        page_count: row.page_count,
    }
}

impl<S: KnowledgeQuery> KnowledgeApplication<S> {
    /// Return one newest-first page of PDF metadata in one vault.
    ///
    /// The cursor binds the vault implicitly: a cursor from another vault cannot
    /// find its document in the filtered sequence and is rejected.
    pub fn pdf_documents_page(
        &self,
        vault: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<PdfDocumentPage> {
        validate_vault(vault)?;
        if limit == 0 || limit > MAX_LIBRARY_PAGE {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "library page limit is outside the supported range",
            ));
        }
        let cursor = after.map(decode_cursor).transpose()?;
        let page = self.store.pdf_documents_page(vault, cursor, limit)?;
        Ok(PdfDocumentPage {
            documents: page.documents.into_iter().map(summary).collect(),
            next_cursor: page
                .next_cursor_key
                .map(|(rev, doc_id)| encode_cursor(rev, doc_id)),
        })
    }

    /// Read one bounded UTF-8 chunk of a PDF page's stored text layer.
    ///
    /// This is Core text, not a renderer. It is available for text-layer PDFs
    /// even when a native page renderer is not mounted; image-only pages remain
    /// an explicit empty-text case.
    pub fn pdf_page_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
        offset: usize,
        max_bytes: usize,
    ) -> Result<PdfPageTextChunk> {
        validate_vault(vault)?;
        if !(MIN_LIBRARY_TEXT_CHUNK..=MAX_LIBRARY_TEXT_CHUNK).contains(&max_bytes) {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "library text chunk size is outside the supported range",
            ));
        }
        if offset > MAX_LIBRARY_TEXT_OFFSET {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "library text offset is outside the supported range",
            ));
        }
        let text = match self.store.pdf_page_text(vault, document_id, page_index)? {
            PdfPageTextLookup::DocumentMissing => {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "library document not found",
                ));
            }
            PdfPageTextLookup::PageMissing => {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "library page not found",
                ));
            }
            PdfPageTextLookup::Found { engine_text } => engine_text,
        };
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "library text offset is not a UTF-8 boundary",
            ));
        }
        let mut end = (offset + max_bytes).min(text.len());
        while end > offset && !text.is_char_boundary(end) {
            end -= 1;
        }
        let next_offset = (end < text.len()).then_some(end);
        Ok(PdfPageTextChunk {
            document_id,
            page_index,
            offset,
            total_bytes: text.len(),
            text: text[offset..end].to_owned(),
            next_offset,
        })
    }

    /// Return saved scalar ranges for one PDF page.
    ///
    /// Multiple captures with the same exact range collapse to one visual range;
    /// compound captures contribute each segment that belongs to this page.
    /// This is a read-only projection and performs no text search or remapping.
    pub fn pdf_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<Vec<PdfSavedSelection>> {
        validate_vault(vault)?;
        let (scalar_count, raw_ranges) =
            match self
                .store
                .pdf_saved_selections(vault, document_id, page_index)?
            {
                PdfSavedSelectionsLookup::DocumentMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "library document not found",
                    ));
                }
                PdfSavedSelectionsLookup::PageMissing => {
                    return Err(KnowledgeError::new(
                        KnowledgeErrorCode::NotFound,
                        "library page not found",
                    ));
                }
                PdfSavedSelectionsLookup::Found {
                    page_scalar_count,
                    ranges,
                } => (page_scalar_count, ranges),
            };
        let mut ranges = BTreeMap::<(u32, u32), i64>::new();
        for raw in raw_ranges {
            let start = u32::try_from(raw.start).map_err(|_| {
                KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "saved selection start is unsupported",
                )
            })?;
            let end = u32::try_from(raw.end).map_err(|_| {
                KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "saved selection end is unsupported",
                )
            })?;
            if start >= end || end as usize > scalar_count {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "saved selection range is invalid",
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
                "saved selection page exceeds its range budget",
            ));
        }
        Ok(ranges
            .into_iter()
            .map(|((start, end), capture_id)| PdfSavedSelection {
                capture_id,
                start,
                end,
            })
            .collect())
    }
}

impl<S: KnowledgeQuery + KnowledgeCommand> KnowledgeApplication<S> {
    /// Capture one scalar range from a PDF text layer into Core.
    ///
    /// The range is checked against the stored page before a draft is formed;
    /// the normal application capture transaction owns idempotency, source
    /// provenance and immutable output.
    pub fn capture_pdf_selection(
        &mut self,
        operation_id: &str,
        vault: &str,
        range: PdfSelectionRange,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        validate_vault(vault)?;
        if range.start >= range.end {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "library selection range is empty or inverted",
            ));
        }
        let document = self
            .store
            .pdf_document(range.document_id)?
            .filter(|doc| doc.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library document not found")
            })?;
        let page = document
            .snapshot
            .pages
            .get(range.page_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library page not found")
            })?;
        let scalar_count = page.engine_text.chars().count();
        if u64::from(range.end) > scalar_count as u64 {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "library selection exceeds the stored page",
            ));
        }
        let exact = page
            .engine_text
            .chars()
            .skip(range.start as usize)
            .take((range.end - range.start) as usize)
            .collect::<String>();
        let draft = PdfDraft {
            protocol_version: 1,
            original_sha256: document.receipt.original_sha256,
            extraction_revision: document.receipt.extraction_revision,
            page_index: range.page_index,
            page_text_sha256: page.text_sha256.clone(),
            unit: PDF_TEXT_UNIT.into(),
            start: u64::from(range.start),
            end: u64::from(range.end),
            exact,
        };
        self.capture_pdf(operation_id, range.document_id, &draft, origin, user_note)
    }
}
