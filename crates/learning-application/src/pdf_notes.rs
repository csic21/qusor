//! Bounded, read-only note index for trusted workspace consumers.
//!
//! A list response never carries a complete note body. Full content stays in
//! Core and is fetched only after the caller has identified one saved capture.
use crate::KnowledgeApplication;
use core_api::knowledge::{
    KnowledgeError, KnowledgeErrorCode, PdfCaptureRecord, PdfDocumentRecord, Result,
};
use learning_ports::KnowledgeStore;
use std::collections::BTreeMap;

pub const MAX_NOTE_QUERY: usize = 200;
pub const NOTE_PREVIEW_BYTES: usize = 512;

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

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
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

impl<S: KnowledgeStore> KnowledgeApplication<S> {
    /// Return newest-first note previews in one vault, optionally one document.
    ///
    /// The query reads one validated Core snapshot. It does not resolve anchors,
    /// grant authority, or expose the complete note/quote body.
    pub fn pdf_notes(
        &self,
        vault: &str,
        document_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<PdfNoteSummary>> {
        if vault.is_empty() || vault.len() > 256 {
            return Err(invalid("invalid note vault"));
        }
        if limit == 0 || limit > MAX_NOTE_QUERY {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "note list limit is outside the supported range",
            ));
        }
        let archive = self.export_all()?;
        let documents: BTreeMap<i64, &PdfDocumentRecord> = archive
            .pdf_documents
            .iter()
            .map(|document| (document.receipt.document_id, document))
            .collect();
        let mut captures = archive
            .pdf_captures
            .iter()
            .filter(|capture| {
                document_id.is_none_or(|id| capture.receipt.document_id == id)
                    && documents
                        .get(&capture.receipt.document_id)
                        .is_some_and(|document| document.vault == vault)
            })
            .collect::<Vec<_>>();
        captures.sort_by(|left, right| {
            right
                .receipt
                .commit_rev
                .cmp(&left.receipt.commit_rev)
                .then_with(|| right.receipt.capture_id.cmp(&left.receipt.capture_id))
        });
        captures
            .into_iter()
            .take(limit)
            .map(|capture| summary(&documents, capture))
            .collect()
    }
}

fn summary(
    documents: &BTreeMap<i64, &PdfDocumentRecord>,
    capture: &PdfCaptureRecord,
) -> Result<PdfNoteSummary> {
    let document = documents.get(&capture.receipt.document_id).ok_or_else(|| {
        KnowledgeError::new(
            KnowledgeErrorCode::Integrity,
            "note document is missing from Core",
        )
    })?;
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
        document_title: document.title.clone(),
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
}
