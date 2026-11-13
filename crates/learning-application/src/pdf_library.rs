//! Bounded, read-only PDF document listing for trusted library consumers.
//!
//! This query returns metadata only. It never returns original PDF bytes, CAS
//! blobs, filesystem paths, selections, notes or plugin authority.
use crate::KnowledgeApplication;
use core_api::{
    knowledge::{
        KnowledgeError, KnowledgeErrorCode, PdfDocumentRecord, PdfReceipt, Provenance, Result,
    },
    pdf_contract::{PDF_TEXT_UNIT, PdfDraft},
};
use learning_ports::KnowledgeStore;
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

fn summary(record: &PdfDocumentRecord) -> PdfDocumentSummary {
    PdfDocumentSummary {
        document_id: record.receipt.document_id,
        commit_rev: record.receipt.commit_rev,
        title: record.title.clone(),
        original_sha256: record.receipt.original_sha256.clone(),
        original_bytes: record.snapshot.original_bytes,
        extraction_revision: record.receipt.extraction_revision.clone(),
        page_count: record.snapshot.pages.len(),
    }
}

impl<S: KnowledgeStore> KnowledgeApplication<S> {
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
        let archive = self.export_all()?;
        let mut documents = archive
            .pdf_documents
            .iter()
            .filter(|document| document.vault == vault)
            .collect::<Vec<_>>();
        documents.sort_by(|left, right| {
            right
                .receipt
                .commit_rev
                .cmp(&left.receipt.commit_rev)
                .then_with(|| right.receipt.document_id.cmp(&left.receipt.document_id))
        });
        let start = match after {
            None => 0,
            Some(cursor) => {
                let (commit_rev, document_id) = decode_cursor(cursor)?;
                documents
                    .iter()
                    .position(|document| {
                        document.receipt.commit_rev == commit_rev
                            && document.receipt.document_id == document_id
                    })
                    .ok_or_else(|| invalid("library cursor does not belong to this vault"))?
                    + 1
            }
        };
        let end = start.saturating_add(limit).min(documents.len());
        let next_cursor = if end < documents.len() {
            documents.get(end - 1).map(|document| {
                encode_cursor(document.receipt.commit_rev, document.receipt.document_id)
            })
        } else {
            None
        };
        Ok(PdfDocumentPage {
            documents: documents[start..end]
                .iter()
                .map(|document| summary(document))
                .collect(),
            next_cursor,
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
        let archive = self.export_all()?;
        let document = archive
            .pdf_documents
            .iter()
            .find(|document| document.receipt.document_id == document_id && document.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library document not found")
            })?;
        let page = document
            .snapshot
            .pages
            .get(page_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library page not found")
            })?;
        let text = &page.engine_text;
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
        let archive = self.export_all()?;
        let document = archive
            .pdf_documents
            .iter()
            .find(|document| {
                document.receipt.document_id == range.document_id && document.vault == vault
            })
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
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            page_index: range.page_index,
            page_text_sha256: page.text_sha256.clone(),
            unit: PDF_TEXT_UNIT.into(),
            start: u64::from(range.start),
            end: u64::from(range.end),
            exact,
        };
        self.capture_pdf(operation_id, range.document_id, &draft, origin, user_note)
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
        let archive = self.export_all()?;
        let document = archive
            .pdf_documents
            .iter()
            .find(|document| document.receipt.document_id == document_id && document.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library document not found")
            })?;
        let page = document
            .snapshot
            .pages
            .get(page_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "library page not found")
            })?;
        let scalar_count = page.engine_text.chars().count();
        let mut ranges = BTreeMap::<(u32, u32), i64>::new();
        for capture in archive
            .pdf_captures
            .iter()
            .filter(|capture| capture.receipt.document_id == document_id)
        {
            let drafts = match &capture.compound {
                Some(compound) => compound.segments.iter().collect::<Vec<_>>(),
                None => vec![&capture.draft],
            };
            for draft in drafts {
                if draft.page_index != page_index {
                    continue;
                }
                let start = u32::try_from(draft.start).map_err(|_| {
                    KnowledgeError::new(
                        KnowledgeErrorCode::Integrity,
                        "saved selection start is unsupported",
                    )
                })?;
                let end = u32::try_from(draft.end).map_err(|_| {
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
                        *capture_id = (*capture_id).min(capture.receipt.capture_id);
                    })
                    .or_insert(capture.receipt.capture_id);
            }
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
