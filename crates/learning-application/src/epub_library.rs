//! Bounded EPUB import, library listing, chapter text reading, TOC queries,
//! selection captures, and note management for trusted consumers.
use crate::{
    KnowledgeApplication, ensure_new_operation, fail,
    pdf_library::{MAX_LIBRARY_PAGE, MAX_SAVED_SELECTIONS, MIN_LIBRARY_TEXT_CHUNK},
    pdf_notes::{MAX_NOTE_CHUNK_BYTES, MAX_NOTE_PAGE, NOTE_PREVIEW_BYTES, NoteBodyField},
};
use core_api::{
    contract::sha256,
    epub_contract::{
        EPUB_PROTOCOL_VERSION, EPUB_TEXT_UNIT, EpubDraft, EpubSnapshot, EpubTocEntry,
        MAX_EPUB_CHAPTER_BYTES, resolve_epub,
    },
    knowledge::{
        EpubCaptureRecord, EpubChapterRecord, EpubDocumentReceipt, EpubDocumentRecord, EpubReceipt,
        EpubTocRecord, KnowledgeError, KnowledgeErrorCode, Provenance, Result,
        archive_schema_version, commit_for_epub_capture, commit_for_epub_document,
        epub_capture_input, epub_document_input, native_import, validate_archive, validate_native,
        validate_note_input,
    },
};
use learning_ports::{CommitPlan, KnowledgeStore};
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

fn doc_summary(record: &EpubDocumentRecord) -> EpubDocumentSummary {
    EpubDocumentSummary {
        document_id: record.receipt.document_id,
        commit_rev: record.receipt.commit_rev,
        title: record.title.clone(),
        author: record.author.clone(),
        original_sha256: record.receipt.original_sha256.clone(),
        original_bytes: record.original_bytes,
        extraction_revision: record.receipt.extraction_revision.clone(),
        chapter_count: record.chapters.len(),
    }
}

impl<S: KnowledgeStore> KnowledgeApplication<S> {
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
        let mut archive = self.export_all()?;
        let max_rev = archive
            .commits
            .iter()
            .map(|c| c.commit_rev)
            .max()
            .unwrap_or(0);
        let id = archive
            .epub_documents
            .iter()
            .map(|d| d.receipt.document_id)
            .max()
            .unwrap_or(0)
            + 1;
        let receipt = EpubDocumentReceipt {
            operation_id: op.into(),
            document_id: id,
            commit_rev: max_rev + 1,
            original_sha256: snapshot.original_sha256.clone(),
            extraction_revision: snapshot.extraction_revision.clone(),
        };
        let record = EpubDocumentRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            title: snapshot.title.clone(),
            author: snapshot.author.clone(),
            original_bytes: snapshot.original_bytes,
            chapters: snapshot
                .chapters
                .iter()
                .map(|c| EpubChapterRecord {
                    chapter_index: c.chapter_index,
                    title: c.title.clone(),
                    href: c.href.clone(),
                    text: c.text.clone(),
                })
                .collect(),
            toc: snapshot
                .toc
                .iter()
                .map(|t| EpubTocRecord {
                    title: t.title.clone(),
                    chapter_index: t.chapter_index,
                    depth: t.depth,
                })
                .collect(),
        };
        let expected_input_hash = epub_document_input(&record)?;
        if let Some(prior) = archive.commits.iter().find(|old| old.operation_id == op) {
            if prior.kind != "epub_import" || prior.input_hash != expected_input_hash {
                return fail("operation id payload conflict");
            }
            return Ok(serde_json::from_str(&prior.receipt)?);
        }
        ensure_new_operation(&archive, op)?;
        let commit = commit_for_epub_document(&record)?;
        archive.epub_documents.push(record);
        archive.commits.push(commit);
        archive.schema_version = archive_schema_version(&archive);
        validate_archive(&archive)?;
        self.store.commit(CommitPlan {
            expected_revision: max_rev,
            next: archive,
        })?;
        Ok(receipt)
    }

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
        let archive = self.export_all()?;
        let mut documents: Vec<&EpubDocumentRecord> = archive
            .epub_documents
            .iter()
            .filter(|doc| doc.vault == vault)
            .collect();
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
                let (commit_rev, document_id) = decode_doc_cursor(cursor)?;
                documents
                    .iter()
                    .position(|doc| {
                        doc.receipt.commit_rev == commit_rev
                            && doc.receipt.document_id == document_id
                    })
                    .ok_or_else(|| invalid("EPUB library cursor does not belong to this vault"))?
                    + 1
            }
        };
        let end = start.saturating_add(limit).min(documents.len());
        let next_cursor = if end < documents.len() {
            documents
                .get(end - 1)
                .map(|doc| encode_doc_cursor(doc.receipt.commit_rev, doc.receipt.document_id))
        } else {
            None
        };
        Ok(EpubDocumentPage {
            documents: documents[start..end]
                .iter()
                .map(|doc| doc_summary(doc))
                .collect(),
            next_cursor,
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
        let archive = self.export_all()?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB document not found")
            })?;
        let chapter = document
            .chapters
            .get(chapter_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB chapter not found")
            })?;
        let text = &chapter.text;
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
            title: chapter.title.clone(),
            href: chapter.href.clone(),
            offset,
            total_bytes: text.len(),
            text: text[offset..end].to_owned(),
            next_offset,
        })
    }

    /// Return the stored table of contents for one EPUB document in `vault`.
    pub fn epub_toc(&self, vault: &str, document_id: i64) -> Result<Vec<EpubTocEntry>> {
        validate_vault(vault)?;
        let archive = self.export_all()?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB document not found")
            })?;
        Ok(document
            .toc
            .iter()
            .map(|entry| EpubTocEntry {
                title: entry.title.clone(),
                chapter_index: entry.chapter_index,
                depth: entry.depth,
            })
            .collect())
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
        let mut archive = self.export_all()?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == range.document_id && doc.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB document not found")
            })?
            .clone();
        validate_native(operation_id, &document.vault, origin, user_note)?;
        let chapter = document
            .chapters
            .get(range.chapter_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB chapter not found")
            })?;
        let scalar_count = chapter.text.chars().count() as u64;
        if range.end > scalar_count {
            return Err(invalid("EPUB selection exceeds the stored chapter"));
        }
        let exact = chapter
            .text
            .chars()
            .skip(range.start as usize)
            .take((range.end - range.start) as usize)
            .collect::<String>();
        let draft = EpubDraft {
            protocol_version: EPUB_PROTOCOL_VERSION,
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            chapter_index: range.chapter_index,
            chapter_text_sha256: sha256(chapter.text.as_bytes()),
            unit: EPUB_TEXT_UNIT.into(),
            start: range.start,
            end: range.end,
            exact,
        };
        resolve_epub(&document.to_snapshot(), &draft)?;
        let max_rev = archive
            .commits
            .iter()
            .map(|c| c.commit_rev)
            .max()
            .unwrap_or(0);
        let capture_id = archive
            .epub_captures
            .iter()
            .map(|c| c.receipt.capture_id)
            .max()
            .unwrap_or(0)
            + 1;
        let receipt = EpubReceipt {
            operation_id: operation_id.into(),
            capture_id,
            document_id: range.document_id,
            commit_rev: max_rev + 1,
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            draft_hash: draft.draft_hash()?,
        };
        let record = EpubCaptureRecord {
            receipt: receipt.clone(),
            draft,
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let expected_input_hash = epub_capture_input(&record, &document)?;
        if let Some(prior) = archive
            .commits
            .iter()
            .find(|old| old.operation_id == operation_id)
        {
            if prior.kind != "epub_capture" || prior.input_hash != expected_input_hash {
                return fail("operation id payload conflict");
            }
            return Ok(serde_json::from_str(&prior.receipt)?);
        }
        ensure_new_operation(&archive, operation_id)?;
        let commit = commit_for_epub_capture(&record, &document)?;
        archive.epub_captures.push(record);
        archive.commits.push(commit);
        archive.schema_version = archive_schema_version(&archive);
        validate_archive(&archive)?;
        self.store.commit(CommitPlan {
            expected_revision: max_rev,
            next: archive,
        })?;
        Ok(receipt)
    }

    /// Return deduplicated saved scalar ranges for one EPUB chapter.
    pub fn epub_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<Vec<EpubSavedSelection>> {
        validate_vault(vault)?;
        let archive = self.export_all()?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB document not found")
            })?;
        let chapter = document
            .chapters
            .get(chapter_index as usize)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB chapter not found")
            })?;
        let scalar_count = chapter.text.chars().count() as u64;
        let mut ranges = BTreeMap::<(u64, u64), i64>::new();
        for capture in archive.epub_captures.iter().filter(|capture| {
            capture.receipt.document_id == document_id
                && capture.draft.chapter_index == chapter_index
        }) {
            let (start, end) = (capture.draft.start, capture.draft.end);
            if start >= end || end > scalar_count {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "saved EPUB selection range is invalid",
                ));
            }
            ranges
                .entry((start, end))
                .and_modify(|capture_id| {
                    *capture_id = (*capture_id).min(capture.receipt.capture_id);
                })
                .or_insert(capture.receipt.capture_id);
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
        let archive = self.export_all()?;
        let documents: BTreeMap<i64, &EpubDocumentRecord> = archive
            .epub_documents
            .iter()
            .map(|doc| (doc.receipt.document_id, doc))
            .collect();
        let mut captures: Vec<&EpubCaptureRecord> = archive
            .epub_captures
            .iter()
            .filter(|capture| {
                document_id.is_none_or(|id| capture.receipt.document_id == id)
                    && documents
                        .get(&capture.receipt.document_id)
                        .is_some_and(|doc| doc.vault == vault)
            })
            .collect();
        captures.sort_by(|left, right| {
            right
                .receipt
                .commit_rev
                .cmp(&left.receipt.commit_rev)
                .then_with(|| right.receipt.capture_id.cmp(&left.receipt.capture_id))
        });
        let start = match after {
            None => 0,
            Some(cursor) => {
                let (cursor_document, commit_rev, capture_id) = decode_note_cursor(cursor)?;
                if cursor_document != document_id {
                    return Err(invalid(
                        "EPUB note cursor does not belong to this query scope",
                    ));
                }
                let position = captures
                    .iter()
                    .position(|capture| {
                        capture.receipt.commit_rev == commit_rev
                            && capture.receipt.capture_id == capture_id
                    })
                    .ok_or_else(|| {
                        invalid("EPUB note cursor does not belong to this query scope")
                    })?;
                position + 1
            }
        };
        let end = start.saturating_add(limit).min(captures.len());
        let notes = captures[start..end]
            .iter()
            .map(|capture| {
                let doc = documents.get(&capture.receipt.document_id).ok_or_else(|| {
                    KnowledgeError::new(
                        KnowledgeErrorCode::Integrity,
                        "EPUB note document is missing from Core",
                    )
                })?;
                let (quote_preview, quote_bytes, quote_truncated) = preview(&capture.draft.exact);
                let (note_preview, note_bytes, note_truncated) = preview(&capture.user_note);
                Ok(EpubNoteSummary {
                    capture_id: capture.receipt.capture_id,
                    commit_rev: capture.receipt.commit_rev,
                    operation_id: capture.receipt.operation_id.clone(),
                    document_id: capture.receipt.document_id,
                    document_title: doc.title.clone(),
                    chapter_index: capture.draft.chapter_index,
                    quote_preview,
                    quote_bytes,
                    quote_truncated,
                    note_preview,
                    note_bytes,
                    note_truncated,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = if end < captures.len() {
            captures.get(end - 1).map(|capture| {
                encode_note_cursor(
                    document_id,
                    capture.receipt.commit_rev,
                    capture.receipt.capture_id,
                )
            })
        } else {
            None
        };
        Ok(EpubNotePage { notes, next_cursor })
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
        let archive = self.export_all()?;
        let capture = archive
            .epub_captures
            .iter()
            .find(|c| c.receipt.capture_id == capture_id)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "EPUB note capture not found")
            })?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == capture.receipt.document_id)
            .ok_or_else(|| {
                KnowledgeError::new(
                    KnowledgeErrorCode::Integrity,
                    "EPUB note document is missing from Core",
                )
            })?;
        if document.vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB note capture not found",
            ));
        }
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
