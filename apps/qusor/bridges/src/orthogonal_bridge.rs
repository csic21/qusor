//! Trusted adapter from the orthogonal `core.knowledge` wire (`Host API 0.8+`)
//! to `learning-application` queries and split command ports.
//!
//! The Host binds the vault, authorizes `core.knowledge.read` /
//! `core.knowledge.write` / `core.capture.propose`, and supplies authenticated
//! [`Provenance`]. This adapter never trusts request-supplied provenance,
//! never resolves anchors, never bypasses `&self.vault`, and keeps every reply
//! inside `CoreKnowledgeResponse::validate` and the 16 KiB wire budget.

use core_api::{
    error::{KnowledgeError, KnowledgeErrorCode},
    knowledge::Provenance,
};
use extension_api::{
    ApiError, ErrorCode,
    notes::NoteField,
    orthogonal::{
        CoreKnowledgeOperation, CoreKnowledgeRequest, CoreKnowledgeResponse, CoreKnowledgeResult,
        DocumentFormat, KnowledgeDocumentSummary, KnowledgeNoteBody, KnowledgeNoteSummary,
        KnowledgePdfMetadata, KnowledgeSavedRange, KnowledgeTocEntry, MAX_KNOWLEDGE_CHUNK_BYTES,
        MAX_KNOWLEDGE_CURSOR_BYTES, MAX_KNOWLEDGE_PAGE, MAX_KNOWLEDGE_PREVIEW_BYTES,
        MAX_KNOWLEDGE_TITLE_BYTES, MAX_KNOWLEDGE_TOC_DEPTH, MAX_KNOWLEDGE_TOC_ENTRIES,
        MIN_KNOWLEDGE_CHUNK_BYTES,
    },
};
use extension_host::OrthogonalAuthorizedContext;
use learning_application::{
    KnowledgeApplication,
    epub_library::{EpubNoteSummary, EpubSelectionRange},
    pdf_notes::{NoteBodyField, PdfNoteSummary},
};
use learning_ports::{KnowledgeCommand, KnowledgeQuery};

const DOC_MIXED_CURSOR_PREFIX: &str = "m1";
const NOTE_MIXED_CURSOR_PREFIX: &str = "mn1";

/// One trusted orthogonal Core Knowledge adapter bound to the same vault as its Host.
pub struct OrthogonalKnowledgeBridge<S: KnowledgeQuery + KnowledgeCommand> {
    app: KnowledgeApplication<S>,
    vault: String,
}

impl<S: KnowledgeQuery + KnowledgeCommand> OrthogonalKnowledgeBridge<S> {
    /// Construct a new orthogonal bridge bound to `vault`.
    pub fn new(app: KnowledgeApplication<S>, vault: impl Into<String>) -> Self {
        Self {
            app,
            vault: vault.into(),
        }
    }

    /// The Host-bound vault identifier enforced on every query and mutation.
    pub fn vault(&self) -> &str {
        &self.vault
    }

    /// Execute one validated [`CoreKnowledgeRequest`] using the Host's
    /// [`OrthogonalAuthorizedContext`] as the authoritative [`Provenance`].
    pub fn respond_authorized(
        &mut self,
        request: &CoreKnowledgeRequest,
        ctx: &OrthogonalAuthorizedContext,
    ) -> CoreKnowledgeResponse {
        let provenance = Provenance {
            package_id: ctx.package_id.clone(),
            publisher_id: ctx.publisher_id.clone(),
            package_digest: ctx.package_digest.clone(),
            installation_id: i64::try_from(ctx.installation_id.sequence()).unwrap_or(i64::MAX),
        };
        self.respond(request, &provenance)
    }

    /// Execute one validated [`CoreKnowledgeRequest`] using Host-authenticated `provenance`
    /// and enforce `CoreKnowledgeResponse::validate` + wire budget limits.
    pub fn respond(
        &mut self,
        request: &CoreKnowledgeRequest,
        provenance: &Provenance,
    ) -> CoreKnowledgeResponse {
        let request_id = request.request_id.clone();
        if let Err(error) = request.validate() {
            return CoreKnowledgeResponse {
                request_id,
                result: CoreKnowledgeResult::Error { error },
            };
        }

        let result = match &request.operation {
            CoreKnowledgeOperation::DocumentsPage {
                format,
                cursor,
                limit,
            } => self.documents_page(*format, cursor.as_deref(), *limit),
            CoreKnowledgeOperation::DocumentToc {
                format,
                document_id,
            } => self.document_toc(*format, *document_id),
            CoreKnowledgeOperation::DocumentChunk {
                format,
                document_id,
                section_index,
                offset,
                max_bytes,
            } => self.document_chunk(*format, *document_id, *section_index, *offset, *max_bytes),
            CoreKnowledgeOperation::SavedSelections {
                format,
                document_id,
                section_index,
            } => self.saved_selections(*format, *document_id, *section_index),
            CoreKnowledgeOperation::NotesPage {
                format,
                document_id,
                cursor,
                limit,
            } => self.notes_page(*format, *document_id, cursor.as_deref(), *limit),
            CoreKnowledgeOperation::NoteBody {
                format,
                capture_id,
                field,
                offset,
                max_bytes,
            } => self.note_body(
                &request_id,
                *format,
                *capture_id,
                *field,
                *offset,
                *max_bytes,
            ),
            CoreKnowledgeOperation::PdfMetadata { document_id } => self.pdf_metadata(*document_id),
            CoreKnowledgeOperation::UpdateCaptureNote {
                format,
                capture_id,
                note,
            } => self.update_capture_note(*format, *capture_id, note),
            CoreKnowledgeOperation::DeleteCapture { format, capture_id } => {
                self.delete_capture(*format, *capture_id)
            }
            CoreKnowledgeOperation::DeleteDocument {
                format,
                document_id,
            } => self.delete_document(*format, *document_id),
            CoreKnowledgeOperation::CaptureEpubSelection {
                operation_id,
                document_id,
                chapter_index,
                start,
                end,
                quote: _,
                note,
            } => self.capture_epub_selection(
                operation_id,
                *document_id,
                *chapter_index,
                *start,
                *end,
                provenance,
                note,
            ),
        };

        let response = CoreKnowledgeResponse { request_id, result };
        match response.to_json() {
            Ok(_) => response,
            Err(err) => CoreKnowledgeResponse {
                request_id: response.request_id,
                result: CoreKnowledgeResult::Error {
                    error: if err.code == ErrorCode::BudgetExceeded {
                        ApiError::new(
                            ErrorCode::BudgetExceeded,
                            "orthogonal knowledge response exceeds the wire budget",
                        )
                    } else {
                        err
                    },
                },
            },
        }
    }

    fn documents_page(
        &self,
        format: Option<DocumentFormat>,
        cursor: Option<&str>,
        limit: u16,
    ) -> CoreKnowledgeResult {
        if limit == 0 || limit > MAX_KNOWLEDGE_PAGE {
            return CoreKnowledgeResult::Error {
                error: ApiError::new(
                    ErrorCode::BudgetExceeded,
                    "documents page limit exceeds budget",
                ),
            };
        }
        let page_limit = usize::from(limit);
        match format {
            Some(DocumentFormat::Pdf) => {
                match self.app.pdf_documents_page(&self.vault, cursor, page_limit) {
                    Ok(page) => CoreKnowledgeResult::DocumentsPage {
                        documents: page
                            .documents
                            .iter()
                            .map(|doc| KnowledgeDocumentSummary {
                                document_id: doc.document_id,
                                format: DocumentFormat::Pdf,
                                commit_rev: doc.commit_rev,
                                title: sanitize_singleline(
                                    &doc.title,
                                    MAX_KNOWLEDGE_TITLE_BYTES,
                                    "?",
                                ),
                                section_count: u32::try_from(doc.page_count).unwrap_or(u32::MAX),
                            })
                            .collect(),
                        next_cursor: page.next_cursor,
                    },
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            Some(DocumentFormat::Epub) => {
                match self
                    .app
                    .epub_documents_page(&self.vault, cursor, page_limit)
                {
                    Ok(page) => CoreKnowledgeResult::DocumentsPage {
                        documents: page
                            .documents
                            .iter()
                            .map(|doc| KnowledgeDocumentSummary {
                                document_id: doc.document_id,
                                format: DocumentFormat::Epub,
                                commit_rev: doc.commit_rev,
                                title: sanitize_singleline(
                                    &doc.title,
                                    MAX_KNOWLEDGE_TITLE_BYTES,
                                    "?",
                                ),
                                section_count: u32::try_from(doc.chapter_count).unwrap_or(u32::MAX),
                            })
                            .collect(),
                        next_cursor: page.next_cursor,
                    },
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            None => self.mixed_documents_page(cursor, page_limit),
        }
    }

    fn mixed_documents_page(&self, cursor: Option<&str>, limit: usize) -> CoreKnowledgeResult {
        let (pdf_state, epub_state) = match cursor {
            None => (StreamCursor::Start, StreamCursor::Start),
            Some(raw) => match decode_mixed_doc_cursor(raw) {
                Ok(parsed) => parsed,
                Err(error) => return CoreKnowledgeResult::Error { error },
            },
        };

        let (pdf_items, pdf_has_more) = match pdf_state {
            StreamCursor::Done => (Vec::new(), false),
            StreamCursor::Start => match self.app.pdf_documents_page(&self.vault, None, limit) {
                Ok(page) => (page.documents, page.next_cursor.is_some()),
                Err(error) => {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
            },
            StreamCursor::At(rev, id) => {
                let raw_cursor = format!("l1:{rev}:{id}");
                match self
                    .app
                    .pdf_documents_page(&self.vault, Some(&raw_cursor), limit)
                {
                    Ok(page) => (page.documents, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
        };

        let (epub_items, epub_has_more) = match epub_state {
            StreamCursor::Done => (Vec::new(), false),
            StreamCursor::Start => match self.app.epub_documents_page(&self.vault, None, limit) {
                Ok(page) => (page.documents, page.next_cursor.is_some()),
                Err(error) => {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
            },
            StreamCursor::At(rev, id) => {
                let raw_cursor = format!("e1:{rev}:{id}");
                match self
                    .app
                    .epub_documents_page(&self.vault, Some(&raw_cursor), limit)
                {
                    Ok(page) => (page.documents, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
        };

        let pdf_fetched = pdf_items.len();
        let epub_fetched = epub_items.len();

        let mut merged: Vec<KnowledgeDocumentSummary> =
            Vec::with_capacity(pdf_fetched + epub_fetched);
        for doc in pdf_items {
            merged.push(KnowledgeDocumentSummary {
                document_id: doc.document_id,
                format: DocumentFormat::Pdf,
                commit_rev: doc.commit_rev,
                title: sanitize_singleline(&doc.title, MAX_KNOWLEDGE_TITLE_BYTES, "?"),
                section_count: u32::try_from(doc.page_count).unwrap_or(u32::MAX),
            });
        }
        for doc in epub_items {
            merged.push(KnowledgeDocumentSummary {
                document_id: doc.document_id,
                format: DocumentFormat::Epub,
                commit_rev: doc.commit_rev,
                title: sanitize_singleline(&doc.title, MAX_KNOWLEDGE_TITLE_BYTES, "?"),
                section_count: u32::try_from(doc.chapter_count).unwrap_or(u32::MAX),
            });
        }

        // Order newest-first by commit revision, then deterministic format/id tie-breakers.
        merged.sort_by(|a, b| {
            b.commit_rev
                .cmp(&a.commit_rev)
                .then_with(|| format_rank(a.format).cmp(&format_rank(b.format)))
                .then_with(|| b.document_id.cmp(&a.document_id))
        });
        merged.truncate(limit);

        let last_pdf = merged
            .iter()
            .filter(|item| item.format == DocumentFormat::Pdf)
            .collect::<Vec<_>>();
        let next_pdf = advance_stream_cursor(
            pdf_state,
            pdf_fetched,
            pdf_has_more,
            last_pdf.len(),
            last_pdf.last().map(|d| (d.commit_rev, d.document_id)),
        );

        let last_epub = merged
            .iter()
            .filter(|item| item.format == DocumentFormat::Epub)
            .collect::<Vec<_>>();
        let next_epub = advance_stream_cursor(
            epub_state,
            epub_fetched,
            epub_has_more,
            last_epub.len(),
            last_epub.last().map(|d| (d.commit_rev, d.document_id)),
        );

        let next_cursor = if next_pdf == StreamCursor::Done && next_epub == StreamCursor::Done {
            None
        } else {
            Some(encode_mixed_doc_cursor(next_pdf, next_epub))
        };

        CoreKnowledgeResult::DocumentsPage {
            documents: merged,
            next_cursor,
        }
    }

    fn document_toc(&self, format: DocumentFormat, document_id: i64) -> CoreKnowledgeResult {
        match format {
            DocumentFormat::Epub => match self.app.epub_toc(&self.vault, document_id) {
                Ok(entries) => {
                    if entries.len() > MAX_KNOWLEDGE_TOC_ENTRIES {
                        return CoreKnowledgeResult::Error {
                            error: ApiError::new(
                                ErrorCode::BudgetExceeded,
                                "document TOC exceeds entry budget",
                            ),
                        };
                    }
                    CoreKnowledgeResult::DocumentToc {
                        format,
                        document_id,
                        entries: entries
                            .into_iter()
                            .map(|entry| KnowledgeTocEntry {
                                title: sanitize_singleline(
                                    &entry.title,
                                    MAX_KNOWLEDGE_TITLE_BYTES,
                                    "?",
                                ),
                                section_index: entry.chapter_index,
                                depth: entry.depth.min(MAX_KNOWLEDGE_TOC_DEPTH),
                            })
                            .collect(),
                    }
                }
                Err(error) => CoreKnowledgeResult::Error {
                    error: map_error(&error),
                },
            },
            DocumentFormat::Pdf => match self.app.query_document(document_id) {
                Ok(doc) if doc.vault == self.vault => CoreKnowledgeResult::DocumentToc {
                    format,
                    document_id,
                    entries: Vec::new(),
                },
                Ok(_) => CoreKnowledgeResult::Error {
                    error: ApiError::new(ErrorCode::NotFound, "PDF document not found"),
                },
                Err(error) => CoreKnowledgeResult::Error {
                    error: map_error(&error),
                },
            },
        }
    }

    fn document_chunk(
        &self,
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> CoreKnowledgeResult {
        let offset_usize = usize::try_from(offset).unwrap_or(usize::MAX);
        let max_usize = usize::from(max_bytes.min(MAX_KNOWLEDGE_CHUNK_BYTES));
        match format {
            DocumentFormat::Pdf => {
                match self.app.pdf_page_text(
                    &self.vault,
                    document_id,
                    section_index,
                    offset_usize,
                    max_usize,
                ) {
                    Ok(chunk) => build_chunk_result(
                        format,
                        chunk.document_id,
                        chunk.page_index,
                        chunk.offset,
                        chunk.total_bytes,
                        chunk.text,
                        chunk.next_offset,
                    ),
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            DocumentFormat::Epub => {
                match self.app.epub_chapter_text(
                    &self.vault,
                    document_id,
                    section_index,
                    offset_usize,
                    max_usize,
                ) {
                    Ok(chunk) => build_chunk_result(
                        format,
                        chunk.document_id,
                        chunk.chapter_index,
                        chunk.offset,
                        chunk.total_bytes,
                        chunk.text,
                        chunk.next_offset,
                    ),
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
        }
    }

    fn saved_selections(
        &self,
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
    ) -> CoreKnowledgeResult {
        match format {
            DocumentFormat::Pdf => {
                match self
                    .app
                    .pdf_saved_selections(&self.vault, document_id, section_index)
                {
                    Ok(ranges) => CoreKnowledgeResult::SavedSelections {
                        format,
                        document_id,
                        section_index,
                        ranges: ranges
                            .into_iter()
                            .map(|range| KnowledgeSavedRange {
                                capture_id: range.capture_id,
                                start: range.start,
                                end: range.end,
                            })
                            .collect(),
                    },
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            DocumentFormat::Epub => {
                match self
                    .app
                    .epub_saved_selections(&self.vault, document_id, section_index)
                {
                    Ok(ranges) => {
                        let mut mapped = Vec::with_capacity(ranges.len());
                        for range in ranges {
                            let Ok(start) = u32::try_from(range.start) else {
                                return CoreKnowledgeResult::Error {
                                    error: ApiError::new(
                                        ErrorCode::BudgetExceeded,
                                        "saved selection range exceeds u32 bounds",
                                    ),
                                };
                            };
                            let Ok(end) = u32::try_from(range.end) else {
                                return CoreKnowledgeResult::Error {
                                    error: ApiError::new(
                                        ErrorCode::BudgetExceeded,
                                        "saved selection range exceeds u32 bounds",
                                    ),
                                };
                            };
                            mapped.push(KnowledgeSavedRange {
                                capture_id: range.capture_id,
                                start,
                                end,
                            });
                        }
                        CoreKnowledgeResult::SavedSelections {
                            format,
                            document_id,
                            section_index,
                            ranges: mapped,
                        }
                    }
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
        }
    }

    fn notes_page(
        &self,
        format: Option<DocumentFormat>,
        document_id: Option<i64>,
        cursor: Option<&str>,
        limit: u16,
    ) -> CoreKnowledgeResult {
        if limit == 0 || limit > MAX_KNOWLEDGE_PAGE {
            return CoreKnowledgeResult::Error {
                error: ApiError::new(ErrorCode::BudgetExceeded, "notes page limit exceeds budget"),
            };
        }
        let page_limit = usize::from(limit);
        match format {
            Some(DocumentFormat::Pdf) => {
                match self
                    .app
                    .pdf_notes_page(&self.vault, document_id, cursor, page_limit)
                {
                    Ok(page) => CoreKnowledgeResult::NotesPage {
                        notes: page
                            .notes
                            .iter()
                            .map(|note| pdf_note_summary(note))
                            .collect(),
                        next_cursor: page.next_cursor,
                    },
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            Some(DocumentFormat::Epub) => {
                match self
                    .app
                    .epub_notes_page(&self.vault, document_id, cursor, page_limit)
                {
                    Ok(page) => CoreKnowledgeResult::NotesPage {
                        notes: page
                            .notes
                            .iter()
                            .map(|note| epub_note_summary(note))
                            .collect(),
                        next_cursor: page.next_cursor,
                    },
                    Err(error) => CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    },
                }
            }
            None => self.mixed_notes_page(document_id, cursor, page_limit),
        }
    }

    fn mixed_notes_page(
        &self,
        document_id: Option<i64>,
        cursor: Option<&str>,
        limit: usize,
    ) -> CoreKnowledgeResult {
        let (pdf_state, epub_state) = match cursor {
            None => (StreamCursor::Start, StreamCursor::Start),
            Some(raw) => match decode_mixed_note_cursor(raw, document_id) {
                Ok(parsed) => parsed,
                Err(error) => return CoreKnowledgeResult::Error { error },
            },
        };

        let scope = document_id.map_or_else(|| "*".to_owned(), |id| id.to_string());

        let (pdf_items, pdf_has_more) = match pdf_state {
            StreamCursor::Done => (Vec::new(), false),
            StreamCursor::Start => {
                match self
                    .app
                    .pdf_notes_page(&self.vault, document_id, None, limit)
                {
                    Ok(page) => (page.notes, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
            StreamCursor::At(rev, id) => {
                let raw_cursor = format!("n1:{scope}:{rev}:{id}");
                match self
                    .app
                    .pdf_notes_page(&self.vault, document_id, Some(&raw_cursor), limit)
                {
                    Ok(page) => (page.notes, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
        };

        let (epub_items, epub_has_more) = match epub_state {
            StreamCursor::Done => (Vec::new(), false),
            StreamCursor::Start => {
                match self
                    .app
                    .epub_notes_page(&self.vault, document_id, None, limit)
                {
                    Ok(page) => (page.notes, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
            StreamCursor::At(rev, id) => {
                let raw_cursor = format!("en1:{scope}:{rev}:{id}");
                match self
                    .app
                    .epub_notes_page(&self.vault, document_id, Some(&raw_cursor), limit)
                {
                    Ok(page) => (page.notes, page.next_cursor.is_some()),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                }
            }
        };

        let pdf_fetched = pdf_items.len();
        let epub_fetched = epub_items.len();

        let mut merged: Vec<KnowledgeNoteSummary> = Vec::with_capacity(pdf_fetched + epub_fetched);
        for note in &pdf_items {
            merged.push(pdf_note_summary(note));
        }
        for note in &epub_items {
            merged.push(epub_note_summary(note));
        }

        merged.sort_by(|a, b| {
            b.commit_rev
                .cmp(&a.commit_rev)
                .then_with(|| format_rank(a.format).cmp(&format_rank(b.format)))
                .then_with(|| b.capture_id.cmp(&a.capture_id))
        });
        merged.truncate(limit);

        let returned_pdfs: Vec<&KnowledgeNoteSummary> = merged
            .iter()
            .filter(|item| item.format == DocumentFormat::Pdf)
            .collect();
        let next_pdf = advance_stream_cursor(
            pdf_state,
            pdf_fetched,
            pdf_has_more,
            returned_pdfs.len(),
            returned_pdfs.last().map(|n| (n.commit_rev, n.capture_id)),
        );

        let returned_epubs: Vec<&KnowledgeNoteSummary> = merged
            .iter()
            .filter(|item| item.format == DocumentFormat::Epub)
            .collect();
        let next_epub = advance_stream_cursor(
            epub_state,
            epub_fetched,
            epub_has_more,
            returned_epubs.len(),
            returned_epubs.last().map(|n| (n.commit_rev, n.capture_id)),
        );

        let next_cursor = if next_pdf == StreamCursor::Done && next_epub == StreamCursor::Done {
            None
        } else {
            Some(encode_mixed_note_cursor(document_id, next_pdf, next_epub))
        };

        CoreKnowledgeResult::NotesPage {
            notes: merged,
            next_cursor,
        }
    }

    fn note_body(
        &self,
        request_id: &str,
        format: DocumentFormat,
        capture_id: i64,
        field: NoteField,
        offset: u32,
        max_bytes: u16,
    ) -> CoreKnowledgeResult {
        let app_field = match field {
            NoteField::Quote => NoteBodyField::Quote,
            NoteField::Note => NoteBodyField::Note,
        };
        let offset_usize = usize::try_from(offset).unwrap_or(usize::MAX);
        let minimum = usize::from(MIN_KNOWLEDGE_CHUNK_BYTES);
        let mut budget = usize::from(max_bytes.min(MAX_KNOWLEDGE_CHUNK_BYTES)).max(minimum);
        loop {
            let result = match format {
                DocumentFormat::Pdf => match self.app.pdf_note_body(
                    &self.vault,
                    capture_id,
                    app_field,
                    offset_usize,
                    budget,
                ) {
                    Ok(chunk) => build_note_body_result(
                        format,
                        chunk.capture_id,
                        chunk.commit_rev,
                        chunk.document_id,
                        field,
                        chunk.offset,
                        chunk.total_bytes,
                        chunk.text,
                        chunk.next_offset,
                    ),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                },
                DocumentFormat::Epub => match self.app.epub_note_body(
                    &self.vault,
                    capture_id,
                    app_field,
                    offset_usize,
                    budget,
                ) {
                    Ok(chunk) => build_note_body_result(
                        format,
                        chunk.capture_id,
                        chunk.commit_rev,
                        chunk.document_id,
                        field,
                        chunk.offset,
                        chunk.total_bytes,
                        chunk.text,
                        chunk.next_offset,
                    ),
                    Err(error) => {
                        return CoreKnowledgeResult::Error {
                            error: map_error(&error),
                        };
                    }
                },
            };

            let probe = CoreKnowledgeResponse {
                request_id: request_id.to_owned(),
                result,
            };
            if probe.to_json().is_ok() {
                return probe.result;
            }
            if budget <= minimum {
                return CoreKnowledgeResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note body chunk exceeds the wire budget",
                    ),
                };
            }
            budget = (budget / 2).max(minimum);
        }
    }

    fn pdf_metadata(&self, document_id: i64) -> CoreKnowledgeResult {
        match self.app.query_document(document_id) {
            Ok(doc) if doc.vault == self.vault => {
                CoreKnowledgeResult::PdfMetadata(KnowledgePdfMetadata {
                    document_id: doc.receipt.document_id,
                    commit_rev: doc.receipt.commit_rev,
                    title: sanitize_singleline(&doc.title, MAX_KNOWLEDGE_TITLE_BYTES, "?"),
                    original_sha256: doc.receipt.original_sha256,
                    original_bytes: doc.snapshot.original_bytes,
                    extraction_revision: doc.receipt.extraction_revision,
                    page_count: u32::try_from(doc.snapshot.pages.len()).unwrap_or(u32::MAX),
                })
            }
            Ok(_) => CoreKnowledgeResult::Error {
                error: ApiError::new(ErrorCode::NotFound, "PDF document not found"),
            },
            Err(error) => CoreKnowledgeResult::Error {
                error: map_error(&error),
            },
        }
    }

    fn update_capture_note(
        &mut self,
        format: DocumentFormat,
        capture_id: i64,
        note: &str,
    ) -> CoreKnowledgeResult {
        let min_chunk = usize::from(MIN_KNOWLEDGE_CHUNK_BYTES);
        match format {
            DocumentFormat::Pdf => {
                if let Err(error) = self
                    .app
                    .update_pdf_capture_note(&self.vault, capture_id, note)
                {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                let commit_rev = self
                    .app
                    .pdf_note_body(&self.vault, capture_id, NoteBodyField::Note, 0, min_chunk)
                    .map(|chunk| chunk.commit_rev)
                    .unwrap_or(1)
                    .max(1);
                CoreKnowledgeResult::NoteUpdated {
                    format,
                    capture_id,
                    commit_rev,
                }
            }
            DocumentFormat::Epub => {
                if let Err(error) = self
                    .app
                    .update_epub_capture_note(&self.vault, capture_id, note)
                {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                let commit_rev = self
                    .app
                    .epub_note_body(&self.vault, capture_id, NoteBodyField::Note, 0, min_chunk)
                    .map(|chunk| chunk.commit_rev)
                    .unwrap_or(1)
                    .max(1);
                CoreKnowledgeResult::NoteUpdated {
                    format,
                    capture_id,
                    commit_rev,
                }
            }
        }
    }

    fn delete_capture(&mut self, format: DocumentFormat, capture_id: i64) -> CoreKnowledgeResult {
        let min_chunk = usize::from(MIN_KNOWLEDGE_CHUNK_BYTES);
        match format {
            DocumentFormat::Pdf => {
                let pre_rev = self
                    .app
                    .pdf_note_body(&self.vault, capture_id, NoteBodyField::Note, 0, min_chunk)
                    .ok()
                    .map(|chunk| chunk.commit_rev);
                if let Err(error) = self.app.delete_pdf_capture(&self.vault, capture_id) {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                CoreKnowledgeResult::CaptureDeleted {
                    format,
                    capture_id,
                    commit_rev: pre_rev.unwrap_or(1).max(1),
                }
            }
            DocumentFormat::Epub => {
                let pre_rev = self
                    .app
                    .epub_note_body(&self.vault, capture_id, NoteBodyField::Note, 0, min_chunk)
                    .ok()
                    .map(|chunk| chunk.commit_rev);
                if let Err(error) = self.app.delete_epub_capture(&self.vault, capture_id) {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                CoreKnowledgeResult::CaptureDeleted {
                    format,
                    capture_id,
                    commit_rev: pre_rev.unwrap_or(1).max(1),
                }
            }
        }
    }

    fn delete_document(&mut self, format: DocumentFormat, document_id: i64) -> CoreKnowledgeResult {
        match format {
            DocumentFormat::Pdf => {
                let pre_rev = self
                    .app
                    .query_document(document_id)
                    .ok()
                    .filter(|doc| doc.vault == self.vault)
                    .map(|doc| doc.receipt.commit_rev);
                if let Err(error) = self.app.delete_pdf_document(&self.vault, document_id) {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                CoreKnowledgeResult::DocumentDeleted {
                    format,
                    document_id,
                    commit_rev: pre_rev.unwrap_or(1).max(1),
                }
            }
            DocumentFormat::Epub => {
                let pre_rev = self
                    .app
                    .epub_documents_page(&self.vault, None, usize::from(MAX_KNOWLEDGE_PAGE))
                    .ok()
                    .and_then(|page| {
                        page.documents
                            .into_iter()
                            .find(|doc| doc.document_id == document_id)
                            .map(|doc| doc.commit_rev)
                    });
                if let Err(error) = self.app.delete_epub_document(&self.vault, document_id) {
                    return CoreKnowledgeResult::Error {
                        error: map_error(&error),
                    };
                }
                CoreKnowledgeResult::DocumentDeleted {
                    format,
                    document_id,
                    commit_rev: pre_rev.unwrap_or(1).max(1),
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_epub_selection(
        &mut self,
        operation_id: &str,
        document_id: i64,
        chapter_index: u32,
        start: u32,
        end: u32,
        provenance: &Provenance,
        note: &str,
    ) -> CoreKnowledgeResult {
        let selection = EpubSelectionRange {
            document_id,
            chapter_index,
            start: u64::from(start),
            end: u64::from(end),
        };
        match self.app.capture_epub_selection(
            operation_id,
            &self.vault,
            selection,
            provenance,
            note,
        ) {
            Ok(receipt) => CoreKnowledgeResult::CaptureCreated {
                format: DocumentFormat::Epub,
                capture_id: receipt.capture_id,
                commit_rev: receipt.commit_rev,
            },
            Err(error) => CoreKnowledgeResult::Error {
                error: map_error(&error),
            },
        }
    }
}

fn build_chunk_result(
    format: DocumentFormat,
    document_id: i64,
    section_index: u32,
    offset: usize,
    total_bytes: usize,
    text: String,
    next_offset: Option<usize>,
) -> CoreKnowledgeResult {
    let Ok(offset_u32) = u32::try_from(offset) else {
        return CoreKnowledgeResult::Error {
            error: ApiError::new(
                ErrorCode::BudgetExceeded,
                "document chunk offset exceeds u32 bounds",
            ),
        };
    };
    let Ok(total_u64) = u64::try_from(total_bytes) else {
        return CoreKnowledgeResult::Error {
            error: ApiError::new(
                ErrorCode::BudgetExceeded,
                "document section exceeds u64 byte bounds",
            ),
        };
    };
    let next_u32 = match next_offset {
        None => None,
        Some(value) => match u32::try_from(value) {
            Ok(converted) => Some(converted),
            Err(_) => {
                return CoreKnowledgeResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "document next offset exceeds u32 bounds",
                    ),
                };
            }
        },
    };
    CoreKnowledgeResult::DocumentChunk {
        format,
        document_id,
        section_index,
        offset: offset_u32,
        total_bytes: total_u64,
        text,
        next_offset: next_u32,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_note_body_result(
    format: DocumentFormat,
    capture_id: i64,
    commit_rev: i64,
    document_id: i64,
    field: NoteField,
    offset: usize,
    total_bytes: usize,
    text: String,
    next_offset: Option<usize>,
) -> CoreKnowledgeResult {
    let Ok(offset_u32) = u32::try_from(offset) else {
        return CoreKnowledgeResult::Error {
            error: ApiError::new(
                ErrorCode::BudgetExceeded,
                "note body offset exceeds u32 bounds",
            ),
        };
    };
    let Ok(total_u64) = u64::try_from(total_bytes) else {
        return CoreKnowledgeResult::Error {
            error: ApiError::new(
                ErrorCode::BudgetExceeded,
                "note body exceeds u64 byte bounds",
            ),
        };
    };
    let next_u32 = match next_offset {
        None => None,
        Some(value) => match u32::try_from(value) {
            Ok(converted) => Some(converted),
            Err(_) => {
                return CoreKnowledgeResult::Error {
                    error: ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "note body next offset exceeds u32 bounds",
                    ),
                };
            }
        },
    };
    CoreKnowledgeResult::NoteBody(KnowledgeNoteBody {
        format,
        capture_id,
        commit_rev,
        document_id,
        field,
        offset: offset_u32,
        total_bytes: total_u64,
        text,
        next_offset: next_u32,
    })
}

fn pdf_note_summary(note: &PdfNoteSummary) -> KnowledgeNoteSummary {
    let (quote_preview, quote_bytes, quote_truncated) =
        bounded_preview(&note.quote_preview, note.quote_bytes, note.quote_truncated);
    let (note_preview, note_bytes, note_truncated) =
        bounded_preview(&note.note_preview, note.note_bytes, note.note_truncated);
    KnowledgeNoteSummary {
        format: DocumentFormat::Pdf,
        capture_id: note.capture_id,
        commit_rev: note.commit_rev,
        document_id: note.document_id,
        document_title: sanitize_singleline(&note.document_title, MAX_KNOWLEDGE_TITLE_BYTES, "?"),
        section_index: note.page_index,
        quote_preview,
        quote_bytes,
        quote_truncated,
        note_preview,
        note_bytes,
        note_truncated,
    }
}

fn epub_note_summary(note: &EpubNoteSummary) -> KnowledgeNoteSummary {
    let (quote_preview, quote_bytes, quote_truncated) =
        bounded_preview(&note.quote_preview, note.quote_bytes, note.quote_truncated);
    let (note_preview, note_bytes, note_truncated) =
        bounded_preview(&note.note_preview, note.note_bytes, note.note_truncated);
    KnowledgeNoteSummary {
        format: DocumentFormat::Epub,
        capture_id: note.capture_id,
        commit_rev: note.commit_rev,
        document_id: note.document_id,
        document_title: sanitize_singleline(&note.document_title, MAX_KNOWLEDGE_TITLE_BYTES, "?"),
        section_index: note.chapter_index,
        quote_preview,
        quote_bytes,
        quote_truncated,
        note_preview,
        note_bytes,
        note_truncated,
    }
}

/// Sanitize and bound a note/quote preview so `bytes` and `truncated` always
/// satisfy `KnowledgeNoteSummary::validate()` (even if multi-byte control chars
/// are normalized to spaces).
fn bounded_preview(raw: &str, raw_total_bytes: usize, raw_truncated: bool) -> (String, u64, bool) {
    let preview = sanitize_multiline(raw, MAX_KNOWLEDGE_PREVIEW_BYTES, "");
    let preview_len = preview.len();
    let truncated = raw_truncated || raw_total_bytes > preview_len;
    let bytes = if truncated {
        u64::try_from(raw_total_bytes.max(preview_len.saturating_add(1))).unwrap_or(u64::MAX)
    } else {
        preview_len as u64
    };
    (preview, bytes, truncated)
}

fn sanitize_singleline(value: &str, limit: usize, empty: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if out.len() + ch.len_utf8() > limit {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        empty.to_owned()
    } else {
        out
    }
}

fn sanitize_multiline(value: &str, limit: usize, empty: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let ch = if ch.is_control() && ch != '\n' {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > limit {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        empty.to_owned()
    } else {
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamCursor {
    Start,
    At(i64, i64),
    Done,
}

impl StreamCursor {
    fn encode(self) -> String {
        match self {
            Self::Start => "start".to_owned(),
            Self::At(rev, id) => format!("{rev}_{id}"),
            Self::Done => "done".to_owned(),
        }
    }

    fn decode(raw: &str) -> Result<Self, ApiError> {
        if raw == "start" {
            return Ok(Self::Start);
        }
        if raw == "done" {
            return Ok(Self::Done);
        }
        let Some((rev_str, id_str)) = raw.split_once('_') else {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "invalid orthogonal stream cursor segment",
            ));
        };
        let Ok(rev) = rev_str.parse::<i64>() else {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "invalid orthogonal stream cursor revision",
            ));
        };
        let Ok(id) = id_str.parse::<i64>() else {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "invalid orthogonal stream cursor identifier",
            ));
        };
        if rev <= 0 || id <= 0 {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "orthogonal stream cursor values must be positive",
            ));
        }
        Ok(Self::At(rev, id))
    }
}

fn advance_stream_cursor(
    prev: StreamCursor,
    fetched: usize,
    has_more: bool,
    consumed: usize,
    last_consumed: Option<(i64, i64)>,
) -> StreamCursor {
    if prev == StreamCursor::Done {
        return StreamCursor::Done;
    }
    if consumed == 0 {
        if fetched == 0 && !has_more {
            StreamCursor::Done
        } else {
            prev
        }
    } else if consumed < fetched || has_more {
        match last_consumed {
            Some((rev, id)) => StreamCursor::At(rev, id),
            None => StreamCursor::Done,
        }
    } else {
        StreamCursor::Done
    }
}

fn format_rank(format: DocumentFormat) -> u8 {
    match format {
        DocumentFormat::Epub => 0,
        DocumentFormat::Pdf => 1,
    }
}

fn encode_mixed_doc_cursor(pdf: StreamCursor, epub: StreamCursor) -> String {
    let encoded = format!(
        "{DOC_MIXED_CURSOR_PREFIX}.{}.{}",
        pdf.encode(),
        epub.encode()
    );
    debug_assert!(encoded.len() <= MAX_KNOWLEDGE_CURSOR_BYTES);
    encoded
}

fn decode_mixed_doc_cursor(raw: &str) -> Result<(StreamCursor, StreamCursor), ApiError> {
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 3 || parts[0] != DOC_MIXED_CURSOR_PREFIX {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "invalid mixed documents cursor",
        ));
    }
    Ok((
        StreamCursor::decode(parts[1])?,
        StreamCursor::decode(parts[2])?,
    ))
}

fn encode_mixed_note_cursor(
    document_id: Option<i64>,
    pdf: StreamCursor,
    epub: StreamCursor,
) -> String {
    let scope = document_id.map_or_else(|| "*".to_owned(), |id| id.to_string());
    let encoded = format!(
        "{NOTE_MIXED_CURSOR_PREFIX}.{scope}.{}.{}",
        pdf.encode(),
        epub.encode()
    );
    debug_assert!(encoded.len() <= MAX_KNOWLEDGE_CURSOR_BYTES);
    encoded
}

fn decode_mixed_note_cursor(
    raw: &str,
    expected_document_id: Option<i64>,
) -> Result<(StreamCursor, StreamCursor), ApiError> {
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 4 || parts[0] != NOTE_MIXED_CURSOR_PREFIX {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "invalid mixed notes cursor",
        ));
    }
    let expected_scope = expected_document_id.map_or_else(|| "*".to_owned(), |id| id.to_string());
    if parts[1] != expected_scope {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "mixed notes cursor scope mismatch",
        ));
    }
    Ok((
        StreamCursor::decode(parts[2])?,
        StreamCursor::decode(parts[3])?,
    ))
}

fn map_error(error: &KnowledgeError) -> ApiError {
    let code = match error.code {
        KnowledgeErrorCode::InvalidInput => ErrorCode::InvalidRequest,
        KnowledgeErrorCode::BudgetExceeded => ErrorCode::BudgetExceeded,
        KnowledgeErrorCode::NotFound => ErrorCode::NotFound,
        KnowledgeErrorCode::UnsupportedVersion => ErrorCode::UnsupportedVersion,
        KnowledgeErrorCode::Unavailable => ErrorCode::CapabilityUnavailable,
        KnowledgeErrorCode::Conflict
        | KnowledgeErrorCode::Integrity
        | KnowledgeErrorCode::Storage => ErrorCode::InvalidState,
    };
    ApiError::new(code, "core knowledge operation failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_api::{
        contract::{CaptureDraft, sha256},
        epub_contract::{EPUB_PROTOCOL_VERSION, EPUB_TEXT_UNIT, EpubDraft, EpubSnapshot},
        knowledge::{
            CaptureRecord, CommitEntry, EpubCaptureRecord, EpubDocumentReceipt, EpubReceipt,
            EpubTocRecord, PdfCaptureRecord, PdfDocumentReceipt, PdfDocumentRecord, PdfReceipt,
            Receipt, Result as CoreResult,
        },
        pdf_compound::PdfCompoundDraft,
        pdf_contract::{
            ExtractionIdentity, PDF_PROTOCOL_VERSION, PdfDraft, PdfPage, PdfRect, PdfSnapshot,
        },
    };
    use extension_api::{
        HOST_API_V0_8,
        orthogonal::{CORE_KNOWLEDGE_PROTOCOL, MAX_KNOWLEDGE_RESPONSE_BYTES},
    };
    use learning_ports::{
        EpubCaptureWithVault, EpubChapterLookup, EpubDocumentSummaryRow, EpubDocumentsPageData,
        EpubNoteSummaryRow, EpubNotesPageData, EpubSavedSelectionRaw, EpubSavedSelectionsLookup,
        OperationLookup, PdfCaptureWithVault, PdfDocumentSummaryRow, PdfDocumentsPageData,
        PdfNoteSummaryRow, PdfNotesPageData, PdfPageTextLookup, PdfSavedSelectionRaw,
        PdfSavedSelectionsLookup,
    };
    use std::collections::BTreeMap;

    #[derive(Clone)]
    struct StoredEpubDoc {
        vault: String,
        title: String,
        author: String,
        commit_rev: i64,
        original_sha256: String,
        original_bytes: u64,
        extraction_revision: String,
        toc: Vec<EpubTocRecord>,
        chapters: Vec<(String, String, String)>,
    }

    #[derive(Default)]
    struct MockKnowledgeStore {
        next_rev: i64,
        next_id: i64,
        pdf_docs: BTreeMap<i64, PdfDocumentRecord>,
        pdf_captures: BTreeMap<i64, PdfCaptureRecord>,
        epub_docs: BTreeMap<i64, StoredEpubDoc>,
        epub_captures: BTreeMap<i64, EpubCaptureRecord>,
        epub_capture_vaults: BTreeMap<i64, String>,
    }

    impl MockKnowledgeStore {
        fn seeded() -> Self {
            let mut s = Self {
                next_rev: 20,
                next_id: 200,
                ..Default::default()
            };
            s.insert_pdf_doc(1, "v1", 2, "PDF Early", &["Pdf p0", "Pdf p1"]);
            s.insert_pdf_doc(2, "v1", 6, "PDF Latest", &["Hello world from PDF"]);
            s.insert_pdf_doc(3, "other-vault", 9, "Foreign PDF", &["secret"]);

            s.insert_epub_doc(
                10,
                "v1",
                4,
                "EPUB Mid",
                vec![
                    EpubTocRecord {
                        title: "Chapter Zero".to_owned(),
                        chapter_index: 0,
                        depth: 0,
                    },
                    EpubTocRecord {
                        title: "Chapter One".to_owned(),
                        chapter_index: 1,
                        depth: 1,
                    },
                ],
                vec![
                    (
                        "Chapter Zero".to_owned(),
                        "ch0.xhtml".to_owned(),
                        "Alpha Beta Gamma Delta Epsilon".to_owned(),
                    ),
                    (
                        "Chapter One".to_owned(),
                        "ch1.xhtml".to_owned(),
                        "Second chapter text".to_owned(),
                    ),
                ],
            );
            s.insert_epub_doc(
                11,
                "v1",
                5,
                "EPUB Newer",
                vec![EpubTocRecord {
                    title: "Intro".to_owned(),
                    chapter_index: 0,
                    depth: 0,
                }],
                vec![(
                    "Intro".to_owned(),
                    "intro.xhtml".to_owned(),
                    "Some EPUB text".to_owned(),
                )],
            );

            s.insert_pdf_capture(101, 2, 10, 0, 0, 5, "Hello", "pdf note 1");
            s.insert_pdf_capture(102, 1, 14, 1, 0, 6, "Pdf p1", "pdf note 2");
            s.insert_pdf_capture(103, 3, 18, 0, 0, 6, "secret", "foreign note");

            s.insert_epub_capture(151, "v1", 10, 12, 0, 6, 16, "Beta Gamma", "epub note 1");
            s.insert_epub_capture(152, "v1", 11, 16, 0, 0, 4, "Some", &"z".repeat(160));
            s
        }

        fn insert_pdf_doc(
            &mut self,
            id: i64,
            vault: &str,
            commit_rev: i64,
            title: &str,
            pages: &[&str],
        ) {
            let pdf_pages = pages
                .iter()
                .enumerate()
                .map(|(idx, text)| PdfPage {
                    page_index: idx as u32,
                    engine_text: (*text).to_owned(),
                    text_sha256: sha256(text.as_bytes()),
                    rotation_degrees: 0,
                    media_box: PdfRect {
                        left: 0.0,
                        bottom: 0.0,
                        right: 612.0,
                        top: 792.0,
                    },
                    crop_box: PdfRect {
                        left: 0.0,
                        bottom: 0.0,
                        right: 612.0,
                        top: 792.0,
                    },
                    characters: Vec::new(),
                })
                .collect();
            self.pdf_docs.insert(
                id,
                PdfDocumentRecord {
                    receipt: PdfDocumentReceipt {
                        operation_id: format!("op-pdf-doc-{id}"),
                        document_id: id,
                        commit_rev,
                        original_sha256: sha256(title.as_bytes()),
                        extraction_revision: sha256(b"pdfium-test"),
                    },
                    vault: vault.to_owned(),
                    title: title.to_owned(),
                    snapshot: PdfSnapshot {
                        protocol_version: PDF_PROTOCOL_VERSION,
                        original_sha256: sha256(title.as_bytes()),
                        original_bytes: 1024,
                        extraction: ExtractionIdentity {
                            provider: "pdfium".to_owned(),
                            algorithm: "test".to_owned(),
                            wrapper: "test".to_owned(),
                            abi: "1".to_owned(),
                            library_sha256: sha256(b"lib"),
                            mapping_check: "ok".to_owned(),
                            normalization: "nfc".to_owned(),
                        },
                        engine_report_sha256: sha256(b"report"),
                        pages: pdf_pages,
                    },
                },
            );
        }

        #[allow(clippy::too_many_arguments)]
        fn insert_epub_doc(
            &mut self,
            id: i64,
            vault: &str,
            commit_rev: i64,
            title: &str,
            toc: Vec<EpubTocRecord>,
            chapters: Vec<(String, String, String)>,
        ) {
            self.epub_docs.insert(
                id,
                StoredEpubDoc {
                    vault: vault.to_owned(),
                    title: title.to_owned(),
                    author: "Tester".to_owned(),
                    commit_rev,
                    original_sha256: sha256(title.as_bytes()),
                    original_bytes: 2048,
                    extraction_revision: "epub-test".to_owned(),
                    toc,
                    chapters,
                },
            );
        }

        #[allow(clippy::too_many_arguments)]
        fn insert_pdf_capture(
            &mut self,
            capture_id: i64,
            document_id: i64,
            commit_rev: i64,
            page_index: u32,
            start: u64,
            end: u64,
            exact: &str,
            note: &str,
        ) {
            self.pdf_captures.insert(
                capture_id,
                PdfCaptureRecord {
                    receipt: PdfReceipt {
                        operation_id: format!("op-pdf-cap-{capture_id}"),
                        capture_id,
                        document_id,
                        commit_rev,
                        original_sha256: sha256(b"pdf"),
                        extraction_revision: sha256(b"pdfium-test"),
                        draft_hash: sha256(exact.as_bytes()),
                    },
                    draft: PdfDraft {
                        protocol_version: PDF_PROTOCOL_VERSION,
                        original_sha256: sha256(b"pdf"),
                        extraction_revision: sha256(b"pdfium-test"),
                        page_index,
                        page_text_sha256: sha256(exact.as_bytes()),
                        unit: "unicode_scalar".to_owned(),
                        start,
                        end,
                        exact: exact.to_owned(),
                    },
                    compound: None,
                    provenance: test_provenance(),
                    user_note: note.to_owned(),
                },
            );
        }

        #[allow(clippy::too_many_arguments)]
        fn insert_epub_capture(
            &mut self,
            capture_id: i64,
            vault: &str,
            document_id: i64,
            commit_rev: i64,
            chapter_index: u32,
            start: u64,
            end: u64,
            exact: &str,
            note: &str,
        ) {
            self.epub_capture_vaults
                .insert(capture_id, vault.to_owned());
            self.epub_captures.insert(
                capture_id,
                EpubCaptureRecord {
                    receipt: EpubReceipt {
                        operation_id: format!("op-epub-cap-{capture_id}"),
                        capture_id,
                        document_id,
                        commit_rev,
                        original_sha256: sha256(b"epub"),
                        extraction_revision: sha256(b"epub-test"),
                        draft_hash: sha256(exact.as_bytes()),
                    },
                    draft: EpubDraft {
                        protocol_version: EPUB_PROTOCOL_VERSION,
                        original_sha256: sha256(b"epub"),
                        extraction_revision: sha256(b"epub-test"),
                        chapter_index,
                        chapter_text_sha256: sha256(exact.as_bytes()),
                        unit: EPUB_TEXT_UNIT.to_owned(),
                        start,
                        end,
                        exact: exact.to_owned(),
                    },
                    provenance: test_provenance(),
                    user_note: note.to_owned(),
                },
            );
        }

        fn next_rev_and_id(&mut self) -> (i64, i64) {
            self.next_rev += 1;
            self.next_id += 1;
            (self.next_rev, self.next_id)
        }
    }

    impl KnowledgeQuery for MockKnowledgeStore {
        fn text_capture(&self, _id: i64) -> CoreResult<Option<CaptureRecord>> {
            Ok(None)
        }
        fn text_receipt_by_operation(&self, _op: &str) -> CoreResult<Option<Receipt>> {
            Ok(None)
        }
        fn commit_by_operation(&self, _op: &str) -> CoreResult<Option<CommitEntry>> {
            Ok(None)
        }
        fn pdf_document(&self, id: i64) -> CoreResult<Option<PdfDocumentRecord>> {
            Ok(self.pdf_docs.get(&id).cloned())
        }
        fn pdf_document_receipt_by_operation(
            &self,
            _op: &str,
        ) -> CoreResult<Option<PdfDocumentReceipt>> {
            Ok(None)
        }
        fn pdf_capture(&self, id: i64) -> CoreResult<Option<PdfCaptureRecord>> {
            Ok(self.pdf_captures.get(&id).cloned())
        }
        fn pdf_capture_with_vault(&self, id: i64) -> CoreResult<Option<PdfCaptureWithVault>> {
            Ok(self.pdf_captures.get(&id).and_then(|record| {
                self.pdf_docs
                    .get(&record.receipt.document_id)
                    .map(|doc| PdfCaptureWithVault {
                        record: record.clone(),
                        document_vault: doc.vault.clone(),
                    })
            }))
        }
        fn pdf_capture_by_operation(
            &self,
            _op: &str,
        ) -> CoreResult<OperationLookup<PdfCaptureWithVault>> {
            Ok(OperationLookup::Absent)
        }
        fn pdf_documents_page(
            &self,
            vault: &str,
            after: Option<(i64, i64)>,
            limit: usize,
        ) -> CoreResult<PdfDocumentsPageData> {
            let mut rows: Vec<PdfDocumentSummaryRow> = self
                .pdf_docs
                .values()
                .filter(|doc| doc.vault == vault)
                .filter(|doc| match after {
                    None => true,
                    Some((rev, id)) => {
                        (doc.receipt.commit_rev, doc.receipt.document_id) < (rev, id)
                    }
                })
                .map(|doc| PdfDocumentSummaryRow {
                    document_id: doc.receipt.document_id,
                    commit_rev: doc.receipt.commit_rev,
                    title: doc.title.clone(),
                    original_sha256: doc.receipt.original_sha256.clone(),
                    original_bytes: doc.snapshot.original_bytes,
                    extraction_revision: doc.receipt.extraction_revision.clone(),
                    page_count: doc.snapshot.pages.len(),
                })
                .collect();
            rows.sort_by(|a, b| (b.commit_rev, b.document_id).cmp(&(a.commit_rev, a.document_id)));
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let next_cursor_key = if has_more {
                rows.last().map(|r| (r.commit_rev, r.document_id))
            } else {
                None
            };
            Ok(PdfDocumentsPageData {
                documents: rows,
                next_cursor_key,
            })
        }
        fn pdf_page_text(
            &self,
            vault: &str,
            document_id: i64,
            page_index: u32,
        ) -> CoreResult<PdfPageTextLookup> {
            let Some(doc) = self.pdf_docs.get(&document_id).filter(|d| d.vault == vault) else {
                return Ok(PdfPageTextLookup::DocumentMissing);
            };
            match doc.snapshot.pages.get(page_index as usize) {
                Some(page) => Ok(PdfPageTextLookup::Found {
                    engine_text: page.engine_text.clone(),
                }),
                None => Ok(PdfPageTextLookup::PageMissing),
            }
        }
        fn pdf_saved_selections(
            &self,
            vault: &str,
            document_id: i64,
            page_index: u32,
        ) -> CoreResult<PdfSavedSelectionsLookup> {
            let Some(doc) = self.pdf_docs.get(&document_id).filter(|d| d.vault == vault) else {
                return Ok(PdfSavedSelectionsLookup::DocumentMissing);
            };
            let Some(page) = doc.snapshot.pages.get(page_index as usize) else {
                return Ok(PdfSavedSelectionsLookup::PageMissing);
            };
            let ranges = self
                .pdf_captures
                .values()
                .filter(|c| {
                    c.receipt.document_id == document_id && c.draft.page_index == page_index
                })
                .map(|c| PdfSavedSelectionRaw {
                    capture_id: c.receipt.capture_id,
                    start: c.draft.start,
                    end: c.draft.end,
                })
                .collect();
            Ok(PdfSavedSelectionsLookup::Found {
                page_scalar_count: page.engine_text.chars().count(),
                ranges,
            })
        }
        fn pdf_notes_page(
            &self,
            vault: &str,
            document_id: Option<i64>,
            after: Option<(i64, i64)>,
            limit: usize,
        ) -> CoreResult<PdfNotesPageData> {
            let mut rows: Vec<PdfNoteSummaryRow> = self
                .pdf_captures
                .values()
                .filter_map(|cap| {
                    let doc = self.pdf_docs.get(&cap.receipt.document_id)?;
                    if doc.vault != vault {
                        return None;
                    }
                    if let Some(target_doc) = document_id
                        && cap.receipt.document_id != target_doc
                    {
                        return None;
                    }
                    if let Some((rev, id)) = after
                        && (cap.receipt.commit_rev, cap.receipt.capture_id) >= (rev, id)
                    {
                        return None;
                    }
                    Some(PdfNoteSummaryRow {
                        record: cap.clone(),
                        document_title: doc.title.clone(),
                    })
                })
                .collect();
            rows.sort_by(|a, b| {
                (b.record.receipt.commit_rev, b.record.receipt.capture_id)
                    .cmp(&(a.record.receipt.commit_rev, a.record.receipt.capture_id))
            });
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let next_cursor_key = if has_more {
                rows.last()
                    .map(|r| (r.record.receipt.commit_rev, r.record.receipt.capture_id))
            } else {
                None
            };
            Ok(PdfNotesPageData {
                notes: rows,
                next_cursor_key,
            })
        }
        fn read_original_pdf(&self, _id: i64) -> CoreResult<Vec<u8>> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn epub_documents_page(
            &self,
            vault: &str,
            after: Option<(i64, i64)>,
            limit: usize,
        ) -> CoreResult<EpubDocumentsPageData> {
            let mut rows: Vec<EpubDocumentSummaryRow> = self
                .epub_docs
                .iter()
                .filter(|(_, doc)| doc.vault == vault)
                .filter(|(id, doc)| match after {
                    None => true,
                    Some((rev, cursor_id)) => (doc.commit_rev, **id) < (rev, cursor_id),
                })
                .map(|(id, doc)| EpubDocumentSummaryRow {
                    document_id: *id,
                    commit_rev: doc.commit_rev,
                    title: doc.title.clone(),
                    author: doc.author.clone(),
                    original_sha256: doc.original_sha256.clone(),
                    original_bytes: doc.original_bytes,
                    extraction_revision: doc.extraction_revision.clone(),
                    chapter_count: doc.chapters.len(),
                })
                .collect();
            rows.sort_by(|a, b| (b.commit_rev, b.document_id).cmp(&(a.commit_rev, a.document_id)));
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let next_cursor_key = if has_more {
                rows.last().map(|r| (r.commit_rev, r.document_id))
            } else {
                None
            };
            Ok(EpubDocumentsPageData {
                documents: rows,
                next_cursor_key,
            })
        }
        fn epub_chapter_text(
            &self,
            vault: &str,
            document_id: i64,
            chapter_index: u32,
        ) -> CoreResult<EpubChapterLookup> {
            let Some(doc) = self
                .epub_docs
                .get(&document_id)
                .filter(|d| d.vault == vault)
            else {
                return Ok(EpubChapterLookup::DocumentMissing);
            };
            match doc.chapters.get(chapter_index as usize) {
                Some((title, href, text)) => Ok(EpubChapterLookup::Found {
                    title: title.clone(),
                    href: href.clone(),
                    text: text.clone(),
                }),
                None => Ok(EpubChapterLookup::ChapterMissing),
            }
        }
        fn epub_toc(
            &self,
            vault: &str,
            document_id: i64,
        ) -> CoreResult<Option<Vec<EpubTocRecord>>> {
            Ok(self
                .epub_docs
                .get(&document_id)
                .filter(|d| d.vault == vault)
                .map(|d| d.toc.clone()))
        }
        fn epub_saved_selections(
            &self,
            vault: &str,
            document_id: i64,
            chapter_index: u32,
        ) -> CoreResult<EpubSavedSelectionsLookup> {
            let Some(doc) = self
                .epub_docs
                .get(&document_id)
                .filter(|d| d.vault == vault)
            else {
                return Ok(EpubSavedSelectionsLookup::DocumentMissing);
            };
            let Some((_, _, text)) = doc.chapters.get(chapter_index as usize) else {
                return Ok(EpubSavedSelectionsLookup::ChapterMissing);
            };
            let ranges = self
                .epub_captures
                .values()
                .filter(|c| {
                    c.receipt.document_id == document_id && c.draft.chapter_index == chapter_index
                })
                .map(|c| EpubSavedSelectionRaw {
                    capture_id: c.receipt.capture_id,
                    start: c.draft.start,
                    end: c.draft.end,
                })
                .collect();
            Ok(EpubSavedSelectionsLookup::Found {
                chapter_scalar_count: text.chars().count() as u64,
                ranges,
            })
        }
        fn epub_notes_page(
            &self,
            vault: &str,
            document_id: Option<i64>,
            after: Option<(i64, i64)>,
            limit: usize,
        ) -> CoreResult<EpubNotesPageData> {
            let mut rows: Vec<EpubNoteSummaryRow> = self
                .epub_captures
                .values()
                .filter_map(|cap| {
                    let doc = self.epub_docs.get(&cap.receipt.document_id)?;
                    if doc.vault != vault {
                        return None;
                    }
                    if let Some(target_doc) = document_id
                        && cap.receipt.document_id != target_doc
                    {
                        return None;
                    }
                    if let Some((rev, id)) = after
                        && (cap.receipt.commit_rev, cap.receipt.capture_id) >= (rev, id)
                    {
                        return None;
                    }
                    Some(EpubNoteSummaryRow {
                        record: cap.clone(),
                        document_title: doc.title.clone(),
                    })
                })
                .collect();
            rows.sort_by(|a, b| {
                (b.record.receipt.commit_rev, b.record.receipt.capture_id)
                    .cmp(&(a.record.receipt.commit_rev, a.record.receipt.capture_id))
            });
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let next_cursor_key = if has_more {
                rows.last()
                    .map(|r| (r.record.receipt.commit_rev, r.record.receipt.capture_id))
            } else {
                None
            };
            Ok(EpubNotesPageData {
                notes: rows,
                next_cursor_key,
            })
        }
        fn epub_capture_with_vault(&self, id: i64) -> CoreResult<Option<EpubCaptureWithVault>> {
            Ok(self.epub_captures.get(&id).and_then(|record| {
                self.epub_capture_vaults
                    .get(&id)
                    .map(|vault| EpubCaptureWithVault {
                        record: record.clone(),
                        document_vault: vault.clone(),
                    })
            }))
        }
    }

    impl KnowledgeCommand for MockKnowledgeStore {
        fn commit_text_capture(
            &mut self,
            _op: &str,
            _vault: &str,
            _draft: &CaptureDraft,
            _origin: &Provenance,
            _user_note: &str,
        ) -> CoreResult<Receipt> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn import_pdf_document(
            &mut self,
            _op: &str,
            _vault: &str,
            _title: &str,
            _original: &[u8],
            _snapshot: &PdfSnapshot,
        ) -> CoreResult<PdfDocumentReceipt> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn capture_pdf(
            &mut self,
            _op: &str,
            _document_id: i64,
            _draft: &PdfDraft,
            _origin: &Provenance,
            _user_note: &str,
        ) -> CoreResult<PdfReceipt> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn capture_pdf_compound(
            &mut self,
            _op: &str,
            _document_id: i64,
            _draft: &PdfCompoundDraft,
            _origin: &Provenance,
            _user_note: &str,
        ) -> CoreResult<PdfReceipt> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn import_epub(
            &mut self,
            _op: &str,
            _vault: &str,
            _snapshot: &EpubSnapshot,
        ) -> CoreResult<EpubDocumentReceipt> {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "unused",
            ))
        }
        fn capture_epub_selection(
            &mut self,
            op: &str,
            vault: &str,
            document_id: i64,
            chapter_index: u32,
            start: u64,
            end: u64,
            origin: &Provenance,
            user_note: &str,
        ) -> CoreResult<EpubReceipt> {
            let Some(doc) = self
                .epub_docs
                .get(&document_id)
                .filter(|d| d.vault == vault)
            else {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "EPUB document not found",
                ));
            };
            let Some((_, _, text)) = doc.chapters.get(chapter_index as usize) else {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "EPUB chapter not found",
                ));
            };
            let exact: String = text
                .chars()
                .skip(start as usize)
                .take((end - start) as usize)
                .collect();
            let orig_sha = doc.original_sha256.clone();
            let ext_rev = doc.extraction_revision.clone();
            let ch_sha = sha256(text.as_bytes());
            let (commit_rev, capture_id) = self.next_rev_and_id();
            let receipt = EpubReceipt {
                operation_id: op.to_owned(),
                capture_id,
                document_id,
                commit_rev,
                original_sha256: orig_sha.clone(),
                extraction_revision: ext_rev.clone(),
                draft_hash: sha256(exact.as_bytes()),
            };
            self.epub_capture_vaults
                .insert(capture_id, vault.to_owned());
            self.epub_captures.insert(
                capture_id,
                EpubCaptureRecord {
                    receipt: receipt.clone(),
                    draft: EpubDraft {
                        protocol_version: EPUB_PROTOCOL_VERSION,
                        original_sha256: orig_sha,
                        extraction_revision: ext_rev,
                        chapter_index,
                        chapter_text_sha256: ch_sha,
                        unit: EPUB_TEXT_UNIT.to_owned(),
                        start,
                        end,
                        exact,
                    },
                    provenance: origin.clone(),
                    user_note: user_note.to_owned(),
                },
            );
            Ok(receipt)
        }
        fn update_pdf_capture_note(
            &mut self,
            vault: &str,
            capture_id: i64,
            note: &str,
        ) -> CoreResult<()> {
            let doc_vault = self
                .pdf_captures
                .get(&capture_id)
                .and_then(|c| self.pdf_docs.get(&c.receipt.document_id))
                .map(|d| d.vault.as_str());
            if doc_vault != Some(vault) {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "PDF capture not found",
                ));
            }
            let (rev, _) = self.next_rev_and_id();
            let cap = self.pdf_captures.get_mut(&capture_id).expect("exists");
            cap.user_note = note.to_owned();
            cap.receipt.commit_rev = rev;
            Ok(())
        }
        fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> CoreResult<()> {
            let doc_vault = self
                .pdf_captures
                .get(&capture_id)
                .and_then(|c| self.pdf_docs.get(&c.receipt.document_id))
                .map(|d| d.vault.as_str());
            if doc_vault != Some(vault) {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "PDF capture not found",
                ));
            }
            self.pdf_captures.remove(&capture_id);
            Ok(())
        }
        fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> CoreResult<()> {
            if self.pdf_docs.get(&document_id).map(|d| d.vault.as_str()) != Some(vault) {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "PDF document not found",
                ));
            }
            self.pdf_docs.remove(&document_id);
            self.pdf_captures
                .retain(|_, c| c.receipt.document_id != document_id);
            Ok(())
        }
        fn update_epub_capture_note(
            &mut self,
            vault: &str,
            capture_id: i64,
            note: &str,
        ) -> CoreResult<()> {
            if self
                .epub_capture_vaults
                .get(&capture_id)
                .map(String::as_str)
                != Some(vault)
            {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "EPUB capture not found",
                ));
            }
            let (rev, _) = self.next_rev_and_id();
            let cap = self.epub_captures.get_mut(&capture_id).expect("exists");
            cap.user_note = note.to_owned();
            cap.receipt.commit_rev = rev;
            Ok(())
        }
        fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> CoreResult<()> {
            if self
                .epub_capture_vaults
                .get(&capture_id)
                .map(String::as_str)
                != Some(vault)
            {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "EPUB capture not found",
                ));
            }
            self.epub_captures.remove(&capture_id);
            self.epub_capture_vaults.remove(&capture_id);
            Ok(())
        }
        fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> CoreResult<()> {
            if self.epub_docs.get(&document_id).map(|d| d.vault.as_str()) != Some(vault) {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::NotFound,
                    "EPUB document not found",
                ));
            }
            self.epub_docs.remove(&document_id);
            self.epub_captures
                .retain(|_, c| c.receipt.document_id != document_id);
            Ok(())
        }
    }

    fn test_provenance() -> Provenance {
        Provenance {
            package_id: "org.qusor.reader".to_owned(),
            publisher_id: "pub-qusor".to_owned(),
            package_digest: "abc123".to_owned(),
            installation_id: 7,
        }
    }

    fn req(id: &str, operation: CoreKnowledgeOperation) -> CoreKnowledgeRequest {
        CoreKnowledgeRequest {
            protocol_version: CORE_KNOWLEDGE_PROTOCOL,
            api_version: HOST_API_V0_8,
            request_id: id.to_owned(),
            operation,
        }
    }

    fn call(
        bridge: &mut OrthogonalKnowledgeBridge<MockKnowledgeStore>,
        id: &str,
        operation: CoreKnowledgeOperation,
    ) -> CoreKnowledgeResponse {
        let resp = bridge.respond(&req(id, operation), &test_provenance());
        let json = resp
            .to_json()
            .expect("every reply must stay inside wire budget");
        assert!(json.len() <= MAX_KNOWLEDGE_RESPONSE_BYTES);
        resp.validate().expect("reply must validate");
        resp
    }

    #[test]
    fn mixed_documents_page_paginates_newest_first_without_cross_vault_leak() {
        let mut bridge = OrthogonalKnowledgeBridge::new(
            KnowledgeApplication::new(MockKnowledgeStore::seeded()),
            "v1",
        );

        let page1 = call(
            &mut bridge,
            "req-docs-1",
            CoreKnowledgeOperation::DocumentsPage {
                format: None,
                cursor: None,
                limit: 2,
            },
        );
        let (docs1, cursor1) = match page1.result {
            CoreKnowledgeResult::DocumentsPage {
                documents,
                next_cursor,
            } => (documents, next_cursor),
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(docs1.len(), 2);
        assert_eq!(docs1[0].document_id, 2); // PDF rev 6
        assert_eq!(docs1[0].format, DocumentFormat::Pdf);
        assert_eq!(docs1[1].document_id, 11); // EPUB rev 5
        assert_eq!(docs1[1].format, DocumentFormat::Epub);
        let cursor1 = cursor1.expect("more documents remain");

        let page2 = call(
            &mut bridge,
            "req-docs-2",
            CoreKnowledgeOperation::DocumentsPage {
                format: None,
                cursor: Some(cursor1),
                limit: 2,
            },
        );
        let (docs2, cursor2) = match page2.result {
            CoreKnowledgeResult::DocumentsPage {
                documents,
                next_cursor,
            } => (documents, next_cursor),
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(docs2.len(), 2);
        assert_eq!(docs2[0].document_id, 10); // EPUB rev 4
        assert_eq!(docs2[1].document_id, 1); // PDF rev 2
        assert!(cursor2.is_none());
        assert!(
            docs1.iter().chain(docs2.iter()).all(|d| d.document_id != 3),
            "other-vault PDF must never appear"
        );
    }

    #[test]
    fn mixed_notes_page_paginates_and_rejects_mismatched_scope_or_forged_cursor() {
        let mut bridge = OrthogonalKnowledgeBridge::new(
            KnowledgeApplication::new(MockKnowledgeStore::seeded()),
            "v1",
        );

        let page1 = call(
            &mut bridge,
            "req-notes-1",
            CoreKnowledgeOperation::NotesPage {
                format: None,
                document_id: None,
                cursor: None,
                limit: 2,
            },
        );
        let (notes1, cursor1) = match page1.result {
            CoreKnowledgeResult::NotesPage { notes, next_cursor } => (notes, next_cursor),
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(notes1.len(), 2);
        assert_eq!(notes1[0].capture_id, 152); // EPUB rev 16 (preview truncated)
        assert!(notes1[0].note_truncated);
        assert!(notes1[0].note_bytes > notes1[0].note_preview.len() as u64);
        assert_eq!(notes1[1].capture_id, 102); // PDF rev 14
        let cursor1 = cursor1.expect("next cursor");

        // Mismatched document_id scope with wildcard cursor must be rejected.
        let wrong_scope = call(
            &mut bridge,
            "req-notes-wrong-scope",
            CoreKnowledgeOperation::NotesPage {
                format: None,
                document_id: Some(10),
                cursor: Some(cursor1.clone()),
                limit: 2,
            },
        );
        assert!(matches!(
            wrong_scope.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::InvalidRequest,
                    ..
                }
            }
        ));

        let forged = call(
            &mut bridge,
            "req-notes-forged",
            CoreKnowledgeOperation::NotesPage {
                format: None,
                document_id: None,
                cursor: Some("mn1.*.bad.start".to_owned()),
                limit: 2,
            },
        );
        assert!(matches!(
            forged.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::InvalidRequest,
                    ..
                }
            }
        ));

        let page2 = call(
            &mut bridge,
            "req-notes-2",
            CoreKnowledgeOperation::NotesPage {
                format: None,
                document_id: None,
                cursor: Some(cursor1),
                limit: 2,
            },
        );
        let (notes2, cursor2) = match page2.result {
            CoreKnowledgeResult::NotesPage { notes, next_cursor } => (notes, next_cursor),
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(notes2.len(), 2);
        assert_eq!(notes2[0].capture_id, 151); // EPUB rev 12
        assert_eq!(notes2[1].capture_id, 101); // PDF rev 10
        assert!(cursor2.is_none());
    }

    #[test]
    fn toc_chunks_saved_selections_and_metadata_enforce_vault_and_budgets() {
        let mut store = MockKnowledgeStore::seeded();
        let big_toc: Vec<EpubTocRecord> = (0..(MAX_KNOWLEDGE_TOC_ENTRIES + 1))
            .map(|idx| EpubTocRecord {
                title: format!("Sec {idx}"),
                chapter_index: 0,
                depth: 0,
            })
            .collect();
        store.insert_epub_doc(
            12,
            "v1",
            7,
            "Huge TOC",
            big_toc,
            vec![("C0".to_owned(), "c0.xhtml".to_owned(), "text".to_owned())],
        );

        let mut bridge = OrthogonalKnowledgeBridge::new(KnowledgeApplication::new(store), "v1");

        let toc_ok = call(
            &mut bridge,
            "req-toc-epub",
            CoreKnowledgeOperation::DocumentToc {
                format: DocumentFormat::Epub,
                document_id: 10,
            },
        );
        assert!(matches!(
            toc_ok.result,
            CoreKnowledgeResult::DocumentToc { ref entries, .. } if entries.len() == 2
        ));

        let toc_pdf = call(
            &mut bridge,
            "req-toc-pdf",
            CoreKnowledgeOperation::DocumentToc {
                format: DocumentFormat::Pdf,
                document_id: 2,
            },
        );
        assert!(matches!(
            toc_pdf.result,
            CoreKnowledgeResult::DocumentToc { ref entries, .. } if entries.is_empty()
        ));

        let toc_foreign_pdf = call(
            &mut bridge,
            "req-toc-pdf-foreign",
            CoreKnowledgeOperation::DocumentToc {
                format: DocumentFormat::Pdf,
                document_id: 3,
            },
        );
        assert!(matches!(
            toc_foreign_pdf.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::NotFound,
                    ..
                }
            }
        ));

        let toc_overflow = call(
            &mut bridge,
            "req-toc-overflow",
            CoreKnowledgeOperation::DocumentToc {
                format: DocumentFormat::Epub,
                document_id: 12,
            },
        );
        assert!(matches!(
            toc_overflow.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::BudgetExceeded,
                    ..
                }
            }
        ));

        let chunk = call(
            &mut bridge,
            "req-chunk",
            CoreKnowledgeOperation::DocumentChunk {
                format: DocumentFormat::Epub,
                document_id: 10,
                section_index: 0,
                offset: 0,
                max_bytes: 10,
            },
        );
        assert!(matches!(
            chunk.result,
            CoreKnowledgeResult::DocumentChunk {
                next_offset: Some(10),
                ..
            }
        ));

        let selections = call(
            &mut bridge,
            "req-sel",
            CoreKnowledgeOperation::SavedSelections {
                format: DocumentFormat::Epub,
                document_id: 10,
                section_index: 0,
            },
        );
        assert!(matches!(
            selections.result,
            CoreKnowledgeResult::SavedSelections { ref ranges, .. } if ranges.len() == 1 && ranges[0].capture_id == 151
        ));

        let note_body = call(
            &mut bridge,
            "req-note-body",
            CoreKnowledgeOperation::NoteBody {
                format: DocumentFormat::Epub,
                capture_id: 152,
                field: NoteField::Note,
                offset: 0,
                max_bytes: 64,
            },
        );
        assert!(matches!(
            note_body.result,
            CoreKnowledgeResult::NoteBody(KnowledgeNoteBody {
                total_bytes: 160,
                next_offset: Some(64),
                ..
            })
        ));

        let meta_ok = call(
            &mut bridge,
            "req-meta-ok",
            CoreKnowledgeOperation::PdfMetadata { document_id: 2 },
        );
        assert!(matches!(
            meta_ok.result,
            CoreKnowledgeResult::PdfMetadata(KnowledgePdfMetadata {
                document_id: 2,
                page_count: 1,
                ..
            })
        ));

        let meta_foreign = call(
            &mut bridge,
            "req-meta-foreign",
            CoreKnowledgeOperation::PdfMetadata { document_id: 3 },
        );
        assert!(matches!(
            meta_foreign.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::NotFound,
                    ..
                }
            }
        ));
    }

    #[test]
    fn mutations_and_capture_enforce_vault_isolation_and_host_provenance() {
        let mut bridge = OrthogonalKnowledgeBridge::new(
            KnowledgeApplication::new(MockKnowledgeStore::seeded()),
            "v1",
        );

        let updated = call(
            &mut bridge,
            "req-upd",
            CoreKnowledgeOperation::UpdateCaptureNote {
                format: DocumentFormat::Pdf,
                capture_id: 101,
                note: "revised note".to_owned(),
            },
        );
        assert!(matches!(
            updated.result,
            CoreKnowledgeResult::NoteUpdated {
                format: DocumentFormat::Pdf,
                capture_id: 101,
                commit_rev: 21,
            }
        ));

        let cross_vault_update = call(
            &mut bridge,
            "req-upd-foreign",
            CoreKnowledgeOperation::UpdateCaptureNote {
                format: DocumentFormat::Pdf,
                capture_id: 103,
                note: "hijack".to_owned(),
            },
        );
        assert!(matches!(
            cross_vault_update.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::NotFound,
                    ..
                }
            }
        ));

        let cross_vault_delete_doc = call(
            &mut bridge,
            "req-del-foreign-doc",
            CoreKnowledgeOperation::DeleteDocument {
                format: DocumentFormat::Pdf,
                document_id: 3,
            },
        );
        assert!(matches!(
            cross_vault_delete_doc.result,
            CoreKnowledgeResult::Error {
                error: ApiError {
                    code: ErrorCode::NotFound,
                    ..
                }
            }
        ));

        // Capture EPUB selection using Host Provenance even when request quote differs.
        let host_prov = Provenance {
            package_id: "org.qusor.trusted-host-pkg".to_owned(),
            publisher_id: "pub-host".to_owned(),
            package_digest: "deadbeef".to_owned(),
            installation_id: 42,
        };
        let capture_req = req(
            "req-cap-epub",
            CoreKnowledgeOperation::CaptureEpubSelection {
                operation_id: "op-cap-new-1".to_owned(),
                document_id: 10,
                chapter_index: 0,
                start: 0,
                end: 5,
                quote: "spoofed-quote".to_owned(),
                note: "host note".to_owned(),
            },
        );
        let capture_resp = bridge.respond(&capture_req, &host_prov);
        capture_resp.validate().expect("capture response valid");
        let created_id = match capture_resp.result {
            CoreKnowledgeResult::CaptureCreated {
                format: DocumentFormat::Epub,
                capture_id,
                ..
            } => capture_id,
            other => panic!("unexpected capture result: {other:?}"),
        };

        // Stored quote must come from canonical chapter scalars, ignoring spoofed request quote.
        let quote_check = call(
            &mut bridge,
            "req-cap-quote",
            CoreKnowledgeOperation::NoteBody {
                format: DocumentFormat::Epub,
                capture_id: created_id,
                field: NoteField::Quote,
                offset: 0,
                max_bytes: 32,
            },
        );
        assert!(matches!(
            quote_check.result,
            CoreKnowledgeResult::NoteBody(KnowledgeNoteBody { ref text, .. }) if text == "Alpha"
        ));

        let del_cap = call(
            &mut bridge,
            "req-del-cap",
            CoreKnowledgeOperation::DeleteCapture {
                format: DocumentFormat::Epub,
                capture_id: created_id,
            },
        );
        assert!(matches!(
            del_cap.result,
            CoreKnowledgeResult::CaptureDeleted {
                format: DocumentFormat::Epub,
                capture_id,
                ..
            } if capture_id == created_id
        ));

        let del_doc = call(
            &mut bridge,
            "req-del-doc",
            CoreKnowledgeOperation::DeleteDocument {
                format: DocumentFormat::Epub,
                document_id: 11,
            },
        );
        assert!(matches!(
            del_doc.result,
            CoreKnowledgeResult::DocumentDeleted {
                format: DocumentFormat::Epub,
                document_id: 11,
                ..
            }
        ));
    }
}
