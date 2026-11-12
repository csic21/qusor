//! Bounded, read-only PDF document listing for trusted library consumers.
//!
//! This query returns metadata only. It never returns original PDF bytes, CAS
//! blobs, filesystem paths, selections, notes or plugin authority.
use crate::KnowledgeApplication;
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode, PdfDocumentRecord, Result};
use learning_ports::KnowledgeStore;

/// Maximum entries in one Library page. Later entries use `next_cursor`.
pub const MAX_LIBRARY_PAGE: usize = 200;
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
}
