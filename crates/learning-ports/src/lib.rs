//! Whole-transaction knowledge persistence boundary. No SQL or platform objects cross it.
//!
//! Storage capabilities are split into read-only query ports (`KnowledgeQuery`, `StudyQuery`),
//! incremental transactional command ports (`KnowledgeCommand`, `StudyCommand`), and whole-store
//! archive/restore ports (`KnowledgeArchive`, `StudyArchivePort`). Legacy `KnowledgeStore` and
//! `StudyStore` traits remain temporarily for source compatibility via blanket delegation.
use core_api::{
    contract::{CaptureDraft, sha256},
    epub_contract::{EPUB_PROTOCOL_VERSION, EPUB_TEXT_UNIT, EpubDraft, EpubSnapshot, resolve_epub},
    error::{KnowledgeError, KnowledgeErrorCode},
    knowledge::{
        ArchiveV2, BinaryBlob, CaptureRecord, CommitEntry, EpubCaptureRecord, EpubDocumentReceipt,
        EpubDocumentRecord, EpubReceipt, EpubTocRecord, ExportEntry, MAX_ORIGINAL,
        PdfCaptureRecord, PdfDocumentReceipt, PdfDocumentRecord, PdfReceipt, Provenance, Receipt,
        Result, archive_schema_version, capture_input, commit_for_epub_capture,
        commit_for_epub_document, commit_for_pdf, document_input, epub_capture_input,
        epub_document_input, event, input_hash, native_import, operation_for_capture,
        operation_for_document, text_commit, validate_archive, validate_native,
    },
    pdf_compound::PdfCompoundDraft,
    pdf_contract::{PdfDraft, PdfSnapshot, resolve_pdf},
    vocab::{
        CardSchedule, DAILY_NEW_LIMIT, ReviewEntry, ReviewGuard, StudyArchive, StudyCard,
        StudyStage, Wordbook, WordbookSeed,
    },
};
use std::collections::{BTreeMap, BTreeSet};

fn unavailable<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::new(
        KnowledgeErrorCode::Unavailable,
        message,
    ))
}

fn conflict<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Conflict, message))
}

fn not_found(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::NotFound, message)
}

fn invalid(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::InvalidInput, message)
}

fn integrity(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::Integrity, message)
}

/// A single already-validated application transition. Adapters must still verify it.
#[derive(Clone, Debug)]
pub struct CommitPlan {
    pub expected_revision: i64,
    pub next: ArchiveV2,
}

/// Row projection returned for one PDF document in bounded library listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfDocumentSummaryRow {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub page_count: usize,
}

/// Bounded page of PDF document metadata plus the keyset cursor of the last returned item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfDocumentsPageData {
    pub documents: Vec<PdfDocumentSummaryRow>,
    pub next_cursor_key: Option<(i64, i64)>,
}

/// Result of looking up one PDF page's stored text layer within a vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdfPageTextLookup {
    DocumentMissing,
    PageMissing,
    Found { engine_text: String },
}

/// Unvalidated raw scalar selection stored on a PDF capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfSavedSelectionRaw {
    pub capture_id: i64,
    pub start: u64,
    pub end: u64,
}

/// Result of querying saved selection ranges for one PDF page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdfSavedSelectionsLookup {
    DocumentMissing,
    PageMissing,
    Found {
        page_scalar_count: usize,
        ranges: Vec<PdfSavedSelectionRaw>,
    },
}

/// PDF capture paired with its owning document title for bounded note list previews.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfNoteSummaryRow {
    pub record: PdfCaptureRecord,
    pub document_title: String,
}

/// Bounded page of PDF note rows plus the keyset cursor of the last returned item.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfNotesPageData {
    pub notes: Vec<PdfNoteSummaryRow>,
    pub next_cursor_key: Option<(i64, i64)>,
}

/// PDF capture paired with the vault of its owning document.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfCaptureWithVault {
    pub record: PdfCaptureRecord,
    pub document_vault: String,
}

/// Outcome of reconciling an idempotent operation id against stored facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationLookup<T> {
    Match(T),
    OtherOperationKind,
    Absent,
}

/// Row projection returned for one EPUB document in bounded library listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubDocumentSummaryRow {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub author: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub chapter_count: usize,
}

/// Bounded page of EPUB document metadata plus the keyset cursor of the last returned item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubDocumentsPageData {
    pub documents: Vec<EpubDocumentSummaryRow>,
    pub next_cursor_key: Option<(i64, i64)>,
}

/// Result of looking up one EPUB chapter within a vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EpubChapterLookup {
    DocumentMissing,
    ChapterMissing,
    Found {
        title: String,
        href: String,
        text: String,
    },
}

/// Unvalidated raw scalar selection stored on an EPUB capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubSavedSelectionRaw {
    pub capture_id: i64,
    pub start: u64,
    pub end: u64,
}

/// Result of querying saved selection ranges for one EPUB chapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EpubSavedSelectionsLookup {
    DocumentMissing,
    ChapterMissing,
    Found {
        chapter_scalar_count: u64,
        ranges: Vec<EpubSavedSelectionRaw>,
    },
}

/// EPUB capture paired with its owning document title for bounded note list previews.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubNoteSummaryRow {
    pub record: EpubCaptureRecord,
    pub document_title: String,
}

/// Bounded page of EPUB note rows plus the keyset cursor of the last returned item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubNotesPageData {
    pub notes: Vec<EpubNoteSummaryRow>,
    pub next_cursor_key: Option<(i64, i64)>,
}

/// EPUB capture paired with the vault of its owning document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubCaptureWithVault {
    pub record: EpubCaptureRecord,
    pub document_vault: String,
}

/// Wordbook metadata plus due/new/total card counts computed for `now_ms`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyDeckCounts {
    pub wordbook: Wordbook,
    pub total: u32,
    pub due: u32,
    pub new_count: u32,
}

/// Single due or new entry returned inside a bounded study queue page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyQueueEntry {
    pub card: StudyCard,
    pub stage: StudyStage,
}

/// Bounded queue page plus remaining due/new counts after the returned slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudyQueueData {
    pub cards: Vec<StudyQueueEntry>,
    pub remaining_due: u32,
    pub remaining_new: u32,
}

/// Point lookup context needed to schedule or idempotently replay a single card review.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyReviewContext {
    pub card: StudyCard,
    pub wordbook_id: i64,
    pub existing_review: Option<ReviewEntry>,
    pub current_schedule: Option<CardSchedule>,
}

/// Read-only point and bounded keyset queries for knowledge persistence.
/// Implementations must not load unrelated rows or original document blobs for normal queries.
pub trait KnowledgeQuery {
    fn text_capture(&self, id: i64) -> Result<Option<CaptureRecord>>;
    fn text_receipt_by_operation(&self, op: &str) -> Result<Option<Receipt>>;
    fn commit_by_operation(&self, op: &str) -> Result<Option<CommitEntry>>;
    fn pdf_document(&self, id: i64) -> Result<Option<PdfDocumentRecord>>;
    fn pdf_document_receipt_by_operation(&self, op: &str) -> Result<Option<PdfDocumentReceipt>>;
    fn pdf_capture(&self, id: i64) -> Result<Option<PdfCaptureRecord>>;
    fn pdf_capture_with_vault(&self, id: i64) -> Result<Option<PdfCaptureWithVault>>;
    fn pdf_capture_by_operation(&self, op: &str) -> Result<OperationLookup<PdfCaptureWithVault>>;
    fn pdf_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfDocumentsPageData>;
    fn pdf_page_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfPageTextLookup>;
    fn pdf_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfSavedSelectionsLookup>;
    fn pdf_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfNotesPageData>;
    fn read_original_pdf(&self, id: i64) -> Result<Vec<u8>>;
    fn epub_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubDocumentsPageData>;
    fn epub_chapter_text(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubChapterLookup>;
    fn epub_toc(&self, vault: &str, document_id: i64) -> Result<Option<Vec<EpubTocRecord>>>;
    fn epub_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubSavedSelectionsLookup>;
    fn epub_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubNotesPageData>;
    fn epub_capture_with_vault(&self, id: i64) -> Result<Option<EpubCaptureWithVault>>;
}

/// Incremental atomic knowledge mutations preserving single-writer, blob integrity,
/// receipt, audit, outbox, index job and global revision invariants.
pub trait KnowledgeCommand {
    fn commit_text_capture(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<Receipt>;
    fn import_pdf_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt>;
    fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt>;
    fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt>;
    fn import_epub(
        &mut self,
        op: &str,
        vault: &str,
        snapshot: &EpubSnapshot,
    ) -> Result<EpubDocumentReceipt>;
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<EpubReceipt>;
    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()>;
    fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()>;
    fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()>;
    fn update_epub_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()>;
    fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()>;
    fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()>;
}

/// Whole-repository archive export and empty-store restore port.
pub trait KnowledgeArchive {
    fn is_empty(&self) -> Result<bool>;
    fn snapshot(&self) -> Result<ArchiveV2>;
    fn restore(&mut self, archive: &ArchiveV2) -> Result<()>;
}

/// Read-only bounded queries for vocabulary study state.
pub trait StudyQuery {
    fn study_decks(&self, vault: &str, now_ms: i64) -> Result<Vec<StudyDeckCounts>>;
    fn wordbook_cards(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StudyCard>>;
    fn study_queue(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueueData>;
    fn card_review_context(
        &self,
        vault: &str,
        card_id: i64,
        operation_id: &str,
    ) -> Result<StudyReviewContext>;
    fn study_remaining_counts(
        &self,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        daily_new_limit: u32,
    ) -> Result<(u32, u32)>;
}

/// Atomic study command port for wordbook seeding, review commits and deletions.
pub trait StudyCommand {
    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook>;
    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry>;
    fn delete_wordbook(&mut self, vault: &str, wordbook_id: i64) -> Result<()>;
}

/// Whole-study archive export and empty-store restore port.
pub trait StudyArchivePort {
    fn study_snapshot(&self) -> Result<StudyArchive>;
    fn restore_study(&mut self, archive: &StudyArchive) -> Result<()>;
}

/// Legacy whole-snapshot knowledge persistence boundary.
///
/// New code should depend on [`KnowledgeQuery`], [`KnowledgeCommand`], and [`KnowledgeArchive`]
/// instead so normal reads and writes do not load full-database snapshots.
#[deprecated(
    note = "use split KnowledgeQuery / KnowledgeCommand / KnowledgeArchive ports instead of whole-snapshot KnowledgeStore"
)]
pub trait KnowledgeStore {
    fn snapshot(&self) -> Result<ArchiveV2>;
    fn commit(&mut self, plan: CommitPlan) -> Result<()>;
    fn restore(&mut self, archive: &ArchiveV2) -> Result<()>;
    /// Update the user note of a PDF capture scoped by `vault` and record an audit row.
    fn update_pdf_capture_note(
        &mut self,
        _vault: &str,
        _capture_id: i64,
        _note: &str,
    ) -> Result<()> {
        unavailable("PDF note updates are not supported by this store")
    }
    /// Delete a PDF capture scoped by `vault` and record an audit row.
    fn delete_pdf_capture(&mut self, _vault: &str, _capture_id: i64) -> Result<()> {
        unavailable("PDF capture deletion is not supported by this store")
    }
    /// Delete a PDF document and its dependent captures scoped by `vault` and record an audit row.
    fn delete_pdf_document(&mut self, _vault: &str, _document_id: i64) -> Result<()> {
        unavailable("PDF document deletion is not supported by this store")
    }
    /// Update the user note of an EPUB capture scoped by `vault` and record an audit row.
    fn update_epub_capture_note(
        &mut self,
        _vault: &str,
        _capture_id: i64,
        _note: &str,
    ) -> Result<()> {
        unavailable("EPUB note updates are not supported by this store")
    }
    /// Delete an EPUB capture scoped by `vault` and record an audit row.
    fn delete_epub_capture(&mut self, _vault: &str, _capture_id: i64) -> Result<()> {
        unavailable("EPUB capture deletion is not supported by this store")
    }
    /// Delete an EPUB document and its chapters/TOC/captures scoped by `vault` and record an audit row.
    fn delete_epub_document(&mut self, _vault: &str, _document_id: i64) -> Result<()> {
        unavailable("EPUB document deletion is not supported by this store")
    }
    #[doc(hidden)]
    fn query_port(&self) -> Option<&dyn KnowledgeQuery> {
        None
    }
    #[doc(hidden)]
    fn command_port(&mut self) -> Option<&mut dyn KnowledgeCommand> {
        None
    }
    #[doc(hidden)]
    fn archive_port(&self) -> Option<&dyn KnowledgeArchive> {
        None
    }
    #[doc(hidden)]
    fn archive_port_mut(&mut self) -> Option<&mut dyn KnowledgeArchive> {
        None
    }
}

/// Legacy vocabulary study persistence boundary.
///
/// New code should depend on [`StudyQuery`], [`StudyCommand`], and [`StudyArchivePort`]
/// instead so normal reads and reviews do not load the full study archive.
#[deprecated(
    note = "use split StudyQuery / StudyCommand / StudyArchivePort ports instead of full-snapshot StudyStore"
)]
pub trait StudyStore {
    /// Complete study data for export and diagnostics.
    fn study_snapshot(&self) -> Result<StudyArchive>;
    /// Idempotently commit one seeded wordbook keyed by `(vault, title)`.
    /// An existing wordbook with the same title is returned unchanged.
    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook>;
    /// Commit one graded review: verify the guard, append the review and replace
    /// the card schedule in one transaction. Returns the stored entry with its
    /// assigned id. A repeated operation id returns the stored entry instead of
    /// scheduling the card twice.
    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry>;
    /// Restore study data into an empty study space without touching captured knowledge.
    fn restore_study(&mut self, archive: &StudyArchive) -> Result<()>;
    /// Delete a wordbook scoped by `vault` along with its cards, schedules, and reviews.
    fn delete_wordbook(&mut self, _vault: &str, _wordbook_id: i64) -> Result<()> {
        unavailable("wordbook deletion is not supported by this store")
    }
    #[doc(hidden)]
    fn study_query_port(&self) -> Option<&dyn StudyQuery> {
        None
    }
    #[doc(hidden)]
    fn study_command_port(&mut self) -> Option<&mut dyn StudyCommand> {
        None
    }
    #[doc(hidden)]
    fn study_archive_port(&self) -> Option<&dyn StudyArchivePort> {
        None
    }
    #[doc(hidden)]
    fn study_archive_port_mut(&mut self) -> Option<&mut dyn StudyArchivePort> {
        None
    }
}

fn ensure_new_operation(archive: &ArchiveV2, op: &str) -> Result<()> {
    if archive.commits.iter().any(|c| c.operation_id == op) {
        return conflict("operation id already belongs to a different payload or kind");
    }
    Ok(())
}

#[allow(deprecated)]
fn validated_archive<T: KnowledgeStore + ?Sized>(store: &T) -> Result<ArchiveV2> {
    let archive = store.snapshot()?;
    validate_archive(&archive)?;
    Ok(archive)
}

#[allow(deprecated)]
impl<T: KnowledgeStore + ?Sized> KnowledgeArchive for T {
    fn is_empty(&self) -> Result<bool> {
        if let Some(port) = self.archive_port() {
            return port.is_empty();
        }
        Ok(validated_archive(self)?.commits.is_empty())
    }

    fn snapshot(&self) -> Result<ArchiveV2> {
        if let Some(port) = self.archive_port() {
            return port.snapshot();
        }
        KnowledgeStore::snapshot(self)
    }

    fn restore(&mut self, archive: &ArchiveV2) -> Result<()> {
        if let Some(port) = self.archive_port_mut() {
            return port.restore(archive);
        }
        KnowledgeStore::restore(self, archive)
    }
}

#[allow(deprecated)]
impl<T: KnowledgeStore + ?Sized> KnowledgeQuery for T {
    fn text_capture(&self, id: i64) -> Result<Option<CaptureRecord>> {
        if let Some(port) = self.query_port() {
            return port.text_capture(id);
        }
        Ok(validated_archive(self)?
            .text_entries
            .into_iter()
            .map(|e| e.record)
            .find(|r| r.receipt.capture_id == id))
    }

    fn text_receipt_by_operation(&self, op: &str) -> Result<Option<Receipt>> {
        if let Some(port) = self.query_port() {
            return port.text_receipt_by_operation(op);
        }
        Ok(validated_archive(self)?
            .text_entries
            .into_iter()
            .find(|e| e.record.receipt.operation_id == op)
            .map(|e| e.record.receipt))
    }

    fn commit_by_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        if let Some(port) = self.query_port() {
            return port.commit_by_operation(op);
        }
        Ok(validated_archive(self)?
            .commits
            .into_iter()
            .find(|c| c.operation_id == op))
    }

    fn pdf_document(&self, id: i64) -> Result<Option<PdfDocumentRecord>> {
        if let Some(port) = self.query_port() {
            return port.pdf_document(id);
        }
        Ok(validated_archive(self)?
            .pdf_documents
            .into_iter()
            .find(|d| d.receipt.document_id == id))
    }

    fn pdf_document_receipt_by_operation(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        if let Some(port) = self.query_port() {
            return port.pdf_document_receipt_by_operation(op);
        }
        Ok(validated_archive(self)?
            .pdf_documents
            .into_iter()
            .find(|d| d.receipt.operation_id == op)
            .map(|d| d.receipt))
    }

    fn pdf_capture(&self, id: i64) -> Result<Option<PdfCaptureRecord>> {
        if let Some(port) = self.query_port() {
            return port.pdf_capture(id);
        }
        Ok(validated_archive(self)?
            .pdf_captures
            .into_iter()
            .find(|c| c.receipt.capture_id == id))
    }

    fn pdf_capture_with_vault(&self, id: i64) -> Result<Option<PdfCaptureWithVault>> {
        if let Some(port) = self.query_port() {
            return port.pdf_capture_with_vault(id);
        }
        let archive = validated_archive(self)?;
        let Some(capture) = archive
            .pdf_captures
            .into_iter()
            .find(|c| c.receipt.capture_id == id)
        else {
            return Ok(None);
        };
        let document = archive
            .pdf_documents
            .into_iter()
            .find(|d| d.receipt.document_id == capture.receipt.document_id)
            .ok_or_else(|| integrity("note document is missing from Core"))?;
        Ok(Some(PdfCaptureWithVault {
            record: capture,
            document_vault: document.vault,
        }))
    }

    fn pdf_capture_by_operation(&self, op: &str) -> Result<OperationLookup<PdfCaptureWithVault>> {
        if let Some(port) = self.query_port() {
            return port.pdf_capture_by_operation(op);
        }
        let archive = validated_archive(self)?;
        if let Some(record) = archive
            .pdf_captures
            .into_iter()
            .find(|r| r.receipt.operation_id == op)
        {
            let document = archive
                .pdf_documents
                .into_iter()
                .find(|d| d.receipt.document_id == record.receipt.document_id)
                .ok_or_else(|| integrity("note document is missing from Core"))?;
            return Ok(OperationLookup::Match(PdfCaptureWithVault {
                record,
                document_vault: document.vault,
            }));
        }
        if archive.commits.iter().any(|c| c.operation_id == op) {
            return Ok(OperationLookup::OtherOperationKind);
        }
        Ok(OperationLookup::Absent)
    }

    fn pdf_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfDocumentsPageData> {
        if let Some(port) = self.query_port() {
            return port.pdf_documents_page(vault, after, limit);
        }
        let archive = validated_archive(self)?;
        let mut documents = archive
            .pdf_documents
            .iter()
            .filter(|doc| doc.vault == vault)
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
            Some((commit_rev, document_id)) => {
                documents
                    .iter()
                    .position(|doc| {
                        doc.receipt.commit_rev == commit_rev
                            && doc.receipt.document_id == document_id
                    })
                    .ok_or_else(|| invalid("library cursor does not belong to this vault"))?
                    + 1
            }
        };
        let end = start.saturating_add(limit).min(documents.len());
        let next_cursor_key = if end < documents.len() {
            documents
                .get(end - 1)
                .map(|doc| (doc.receipt.commit_rev, doc.receipt.document_id))
        } else {
            None
        };
        Ok(PdfDocumentsPageData {
            documents: documents[start..end]
                .iter()
                .map(|doc| PdfDocumentSummaryRow {
                    document_id: doc.receipt.document_id,
                    commit_rev: doc.receipt.commit_rev,
                    title: doc.title.clone(),
                    original_sha256: doc.receipt.original_sha256.clone(),
                    original_bytes: doc.snapshot.original_bytes,
                    extraction_revision: doc.receipt.extraction_revision.clone(),
                    page_count: doc.snapshot.pages.len(),
                })
                .collect(),
            next_cursor_key,
        })
    }

    fn pdf_page_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfPageTextLookup> {
        if let Some(port) = self.query_port() {
            return port.pdf_page_text(vault, document_id, page_index);
        }
        let archive = validated_archive(self)?;
        let Some(document) = archive
            .pdf_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
        else {
            return Ok(PdfPageTextLookup::DocumentMissing);
        };
        let Some(page) = document.snapshot.pages.get(page_index as usize) else {
            return Ok(PdfPageTextLookup::PageMissing);
        };
        Ok(PdfPageTextLookup::Found {
            engine_text: page.engine_text.clone(),
        })
    }

    fn pdf_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfSavedSelectionsLookup> {
        if let Some(port) = self.query_port() {
            return port.pdf_saved_selections(vault, document_id, page_index);
        }
        let archive = validated_archive(self)?;
        let Some(document) = archive
            .pdf_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
        else {
            return Ok(PdfSavedSelectionsLookup::DocumentMissing);
        };
        let Some(page) = document.snapshot.pages.get(page_index as usize) else {
            return Ok(PdfSavedSelectionsLookup::PageMissing);
        };
        let mut ranges = Vec::new();
        for capture in archive
            .pdf_captures
            .iter()
            .filter(|capture| capture.receipt.document_id == document_id)
        {
            let drafts: Vec<&PdfDraft> = match &capture.compound {
                Some(compound) => compound.segments.iter().collect(),
                None => vec![&capture.draft],
            };
            for draft in drafts {
                if draft.page_index == page_index {
                    ranges.push(PdfSavedSelectionRaw {
                        capture_id: capture.receipt.capture_id,
                        start: draft.start,
                        end: draft.end,
                    });
                }
            }
        }
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
    ) -> Result<PdfNotesPageData> {
        if let Some(port) = self.query_port() {
            return port.pdf_notes_page(vault, document_id, after, limit);
        }
        let archive = validated_archive(self)?;
        let documents: BTreeMap<i64, &PdfDocumentRecord> = archive
            .pdf_documents
            .iter()
            .map(|doc| (doc.receipt.document_id, doc))
            .collect();
        let mut captures = archive
            .pdf_captures
            .iter()
            .filter(|capture| {
                document_id.is_none_or(|id| capture.receipt.document_id == id)
                    && documents
                        .get(&capture.receipt.document_id)
                        .is_some_and(|doc| doc.vault == vault)
            })
            .collect::<Vec<_>>();
        captures.sort_by(|left, right| {
            right
                .receipt
                .commit_rev
                .cmp(&left.receipt.commit_rev)
                .then_with(|| right.receipt.capture_id.cmp(&left.receipt.capture_id))
        });
        let start = match after {
            None => 0,
            Some((commit_rev, capture_id)) => {
                captures
                    .iter()
                    .position(|capture| {
                        capture.receipt.commit_rev == commit_rev
                            && capture.receipt.capture_id == capture_id
                    })
                    .ok_or_else(|| invalid("note cursor does not belong to this query scope"))?
                    + 1
            }
        };
        let end = start.saturating_add(limit).min(captures.len());
        let notes = captures[start..end]
            .iter()
            .map(|capture| {
                let doc = documents
                    .get(&capture.receipt.document_id)
                    .ok_or_else(|| integrity("note document is missing from Core"))?;
                Ok(PdfNoteSummaryRow {
                    record: (*capture).clone(),
                    document_title: doc.title.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let next_cursor_key = if end < captures.len() {
            captures
                .get(end - 1)
                .map(|capture| (capture.receipt.commit_rev, capture.receipt.capture_id))
        } else {
            None
        };
        Ok(PdfNotesPageData {
            notes,
            next_cursor_key,
        })
    }

    fn read_original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        if let Some(port) = self.query_port() {
            return port.read_original_pdf(id);
        }
        let archive = validated_archive(self)?;
        let doc = archive
            .pdf_documents
            .iter()
            .find(|d| d.receipt.document_id == id)
            .ok_or_else(|| not_found("PDF document not found"))?;
        archive
            .blobs
            .into_iter()
            .find(|b| b.hash == doc.receipt.original_sha256)
            .ok_or("PDF original missing")?
            .decode()
    }

    fn epub_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubDocumentsPageData> {
        if let Some(port) = self.query_port() {
            return port.epub_documents_page(vault, after, limit);
        }
        let archive = validated_archive(self)?;
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
            Some((commit_rev, document_id)) => {
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
        let next_cursor_key = if end < documents.len() {
            documents
                .get(end - 1)
                .map(|doc| (doc.receipt.commit_rev, doc.receipt.document_id))
        } else {
            None
        };
        Ok(EpubDocumentsPageData {
            documents: documents[start..end]
                .iter()
                .map(|doc| EpubDocumentSummaryRow {
                    document_id: doc.receipt.document_id,
                    commit_rev: doc.receipt.commit_rev,
                    title: doc.title.clone(),
                    author: doc.author.clone(),
                    original_sha256: doc.receipt.original_sha256.clone(),
                    original_bytes: doc.original_bytes,
                    extraction_revision: doc.receipt.extraction_revision.clone(),
                    chapter_count: doc.chapters.len(),
                })
                .collect(),
            next_cursor_key,
        })
    }

    fn epub_chapter_text(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubChapterLookup> {
        if let Some(port) = self.query_port() {
            return port.epub_chapter_text(vault, document_id, chapter_index);
        }
        let archive = validated_archive(self)?;
        let Some(document) = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
        else {
            return Ok(EpubChapterLookup::DocumentMissing);
        };
        let Some(chapter) = document.chapters.get(chapter_index as usize) else {
            return Ok(EpubChapterLookup::ChapterMissing);
        };
        Ok(EpubChapterLookup::Found {
            title: chapter.title.clone(),
            href: chapter.href.clone(),
            text: chapter.text.clone(),
        })
    }

    fn epub_toc(&self, vault: &str, document_id: i64) -> Result<Option<Vec<EpubTocRecord>>> {
        if let Some(port) = self.query_port() {
            return port.epub_toc(vault, document_id);
        }
        let archive = validated_archive(self)?;
        Ok(archive
            .epub_documents
            .into_iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
            .map(|doc| doc.toc))
    }

    fn epub_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubSavedSelectionsLookup> {
        if let Some(port) = self.query_port() {
            return port.epub_saved_selections(vault, document_id, chapter_index);
        }
        let archive = validated_archive(self)?;
        let Some(document) = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
        else {
            return Ok(EpubSavedSelectionsLookup::DocumentMissing);
        };
        let Some(chapter) = document.chapters.get(chapter_index as usize) else {
            return Ok(EpubSavedSelectionsLookup::ChapterMissing);
        };
        let ranges = archive
            .epub_captures
            .iter()
            .filter(|capture| {
                capture.receipt.document_id == document_id
                    && capture.draft.chapter_index == chapter_index
            })
            .map(|capture| EpubSavedSelectionRaw {
                capture_id: capture.receipt.capture_id,
                start: capture.draft.start,
                end: capture.draft.end,
            })
            .collect();
        Ok(EpubSavedSelectionsLookup::Found {
            chapter_scalar_count: chapter.text.chars().count() as u64,
            ranges,
        })
    }

    fn epub_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubNotesPageData> {
        if let Some(port) = self.query_port() {
            return port.epub_notes_page(vault, document_id, after, limit);
        }
        let archive = validated_archive(self)?;
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
            Some((commit_rev, capture_id)) => {
                captures
                    .iter()
                    .position(|capture| {
                        capture.receipt.commit_rev == commit_rev
                            && capture.receipt.capture_id == capture_id
                    })
                    .ok_or_else(|| {
                        invalid("EPUB note cursor does not belong to this query scope")
                    })?
                    + 1
            }
        };
        let end = start.saturating_add(limit).min(captures.len());
        let notes = captures[start..end]
            .iter()
            .map(|capture| {
                let doc = documents
                    .get(&capture.receipt.document_id)
                    .ok_or_else(|| integrity("EPUB note document is missing from Core"))?;
                Ok(EpubNoteSummaryRow {
                    record: (*capture).clone(),
                    document_title: doc.title.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let next_cursor_key = if end < captures.len() {
            captures
                .get(end - 1)
                .map(|capture| (capture.receipt.commit_rev, capture.receipt.capture_id))
        } else {
            None
        };
        Ok(EpubNotesPageData {
            notes,
            next_cursor_key,
        })
    }

    fn epub_capture_with_vault(&self, id: i64) -> Result<Option<EpubCaptureWithVault>> {
        if let Some(port) = self.query_port() {
            return port.epub_capture_with_vault(id);
        }
        let archive = validated_archive(self)?;
        let Some(capture) = archive
            .epub_captures
            .into_iter()
            .find(|c| c.receipt.capture_id == id)
        else {
            return Ok(None);
        };
        let document = archive
            .epub_documents
            .into_iter()
            .find(|doc| doc.receipt.document_id == capture.receipt.document_id)
            .ok_or_else(|| integrity("EPUB note document is missing from Core"))?;
        Ok(Some(EpubCaptureWithVault {
            record: capture,
            document_vault: document.vault,
        }))
    }
}

#[allow(deprecated)]
impl<T: KnowledgeStore + ?Sized> KnowledgeCommand for T {
    fn commit_text_capture(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<Receipt> {
        if let Some(port) = self.command_port() {
            return port.commit_text_capture(op, vault, draft, origin, user_note);
        }
        draft.validate()?;
        validate_native(op, vault, origin, user_note)?;
        let initial = validated_archive(self)?;
        let max_rev = initial
            .commits
            .iter()
            .map(|c| c.commit_rev)
            .max()
            .unwrap_or(0);
        let id = initial.text_entries.len() as i64 + 1;
        let receipt = Receipt {
            operation_id: op.into(),
            capture_id: id,
            source_id: id,
            document_version_id: id,
            commit_rev: max_rev + 1,
            snapshot_hash: draft.snapshot_hash(),
            draft_hash: draft.draft_hash()?,
        };
        let record = CaptureRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            draft: draft.clone(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let hash = input_hash(&record)?;
        if let Some(prior_entry) = initial
            .text_entries
            .iter()
            .find(|e| e.record.receipt.operation_id == op)
        {
            if prior_entry.input_hash != hash {
                return conflict("operation id payload conflict");
            }
            return Ok(prior_entry.record.receipt.clone());
        }
        ensure_new_operation(&initial, op)?;
        let ev = event(&record)?;
        let entry = ExportEntry {
            record,
            input_hash: hash,
            operation: ev.clone(),
            outbox: ev.clone(),
            index_job: ev,
        };
        let mut projected = initial;
        projected.commits.push(text_commit(&entry)?);
        projected.text_entries.push(entry);
        if !projected
            .blobs
            .iter()
            .any(|b| b.hash == receipt.snapshot_hash)
        {
            projected
                .blobs
                .push(BinaryBlob::new(draft.snapshot_text.as_bytes()));
        }
        projected.schema_version = archive_schema_version(&projected);
        validate_archive(&projected)?;
        self.commit(CommitPlan {
            expected_revision: max_rev,
            next: projected,
        })?;
        Ok(receipt)
    }

    fn import_pdf_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt> {
        if let Some(port) = self.command_port() {
            return port.import_pdf_document(op, vault, title, original, snapshot);
        }
        native_import(op, vault, title)?;
        if original.len() > MAX_ORIGINAL {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "PDF original budget exceeded",
            ));
        }
        snapshot.validate_original(original)?;
        let mut archive = validated_archive(self)?;
        let max_rev = archive
            .commits
            .iter()
            .map(|c| c.commit_rev)
            .max()
            .unwrap_or(0);
        let id = archive
            .pdf_documents
            .iter()
            .map(|d| d.receipt.document_id)
            .max()
            .unwrap_or(0)
            + 1;
        let receipt = PdfDocumentReceipt {
            operation_id: op.into(),
            document_id: id,
            commit_rev: max_rev + 1,
            original_sha256: sha256(original),
            extraction_revision: snapshot.extraction_revision()?,
        };
        let record = PdfDocumentRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            title: title.into(),
            snapshot: snapshot.clone(),
        };
        let expected_input = document_input(&record)?;
        if let Some(prior_doc) = archive
            .pdf_documents
            .iter()
            .find(|d| d.receipt.operation_id == op)
        {
            let prior_op = archive
                .pdf_operations
                .iter()
                .find(|old| old.operation_id == op);
            if prior_op
                .is_none_or(|old| old.kind != "pdf_import" || old.input_hash != expected_input)
            {
                return conflict("operation id payload conflict");
            }
            return Ok(prior_doc.receipt.clone());
        }
        ensure_new_operation(&archive, op)?;
        let operation = operation_for_document(&record)?;
        archive.pdf_documents.push(record);
        archive.pdf_operations.push(operation.clone());
        archive
            .commits
            .push(commit_for_pdf(&operation, receipt.commit_rev));
        if !archive
            .blobs
            .iter()
            .any(|blob| blob.hash == receipt.original_sha256)
        {
            archive.blobs.push(BinaryBlob::new(original));
        }
        archive.schema_version = archive_schema_version(&archive);
        validate_archive(&archive)?;
        self.commit(CommitPlan {
            expected_revision: max_rev,
            next: archive,
        })?;
        Ok(receipt)
    }

    fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        if let Some(port) = self.command_port() {
            return port.capture_pdf(op, document_id, draft, origin, user_note);
        }
        capture_pdf_legacy(self, op, document_id, Some(draft), None, origin, user_note)
    }

    fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        if let Some(port) = self.command_port() {
            return port.capture_pdf_compound(op, document_id, draft, origin, user_note);
        }
        capture_pdf_legacy(self, op, document_id, None, Some(draft), origin, user_note)
    }

    fn import_epub(
        &mut self,
        op: &str,
        vault: &str,
        snapshot: &EpubSnapshot,
    ) -> Result<EpubDocumentReceipt> {
        if let Some(port) = self.command_port() {
            return port.import_epub(op, vault, snapshot);
        }
        native_import(op, vault, &snapshot.title)?;
        snapshot.validate()?;
        if snapshot.vault != vault {
            return Err(invalid("EPUB snapshot vault does not match target vault"));
        }
        let mut archive = validated_archive(self)?;
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
                .map(|c| core_api::knowledge::EpubChapterRecord {
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
        if let Some(prior_doc) = archive
            .epub_documents
            .iter()
            .find(|d| d.receipt.operation_id == op)
        {
            let prior = archive.commits.iter().find(|old| old.operation_id == op);
            if prior.is_none_or(|c| c.kind != "epub_import" || c.input_hash != expected_input_hash)
            {
                return conflict("operation id payload conflict");
            }
            return Ok(prior_doc.receipt.clone());
        }
        ensure_new_operation(&archive, op)?;
        let commit = commit_for_epub_document(&record)?;
        archive.epub_documents.push(record);
        archive.commits.push(commit);
        archive.schema_version = archive_schema_version(&archive);
        validate_archive(&archive)?;
        self.commit(CommitPlan {
            expected_revision: max_rev,
            next: archive,
        })?;
        Ok(receipt)
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
    ) -> Result<EpubReceipt> {
        if let Some(port) = self.command_port() {
            return port.capture_epub_selection(
                op,
                vault,
                document_id,
                chapter_index,
                start,
                end,
                origin,
                user_note,
            );
        }
        if start >= end {
            return Err(invalid("EPUB selection range is empty or inverted"));
        }
        let mut archive = validated_archive(self)?;
        let document = archive
            .epub_documents
            .iter()
            .find(|doc| doc.receipt.document_id == document_id && doc.vault == vault)
            .ok_or_else(|| not_found("EPUB document not found"))?
            .clone();
        validate_native(op, &document.vault, origin, user_note)?;
        let chapter = document
            .chapters
            .get(chapter_index as usize)
            .ok_or_else(|| not_found("EPUB chapter not found"))?;
        let scalar_count = chapter.text.chars().count() as u64;
        if end > scalar_count {
            return Err(invalid("EPUB selection exceeds the stored chapter"));
        }
        let exact = chapter
            .text
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect::<String>();
        let draft = EpubDraft {
            protocol_version: EPUB_PROTOCOL_VERSION,
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            chapter_index,
            chapter_text_sha256: sha256(chapter.text.as_bytes()),
            unit: EPUB_TEXT_UNIT.into(),
            start,
            end,
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
            operation_id: op.into(),
            capture_id,
            document_id,
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
        if let Some(prior_cap) = archive
            .epub_captures
            .iter()
            .find(|c| c.receipt.operation_id == op)
        {
            let prior = archive.commits.iter().find(|old| old.operation_id == op);
            if prior.is_none_or(|c| c.kind != "epub_capture" || c.input_hash != expected_input_hash)
            {
                return conflict("operation id payload conflict");
            }
            return Ok(prior_cap.receipt.clone());
        }
        ensure_new_operation(&archive, op)?;
        let commit = commit_for_epub_capture(&record, &document)?;
        archive.epub_captures.push(record);
        archive.commits.push(commit);
        archive.schema_version = archive_schema_version(&archive);
        validate_archive(&archive)?;
        self.commit(CommitPlan {
            expected_revision: max_rev,
            next: archive,
        })?;
        Ok(receipt)
    }

    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.update_pdf_capture_note(vault, capture_id, note);
        }
        KnowledgeStore::update_pdf_capture_note(self, vault, capture_id, note)
    }

    fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.delete_pdf_capture(vault, capture_id);
        }
        KnowledgeStore::delete_pdf_capture(self, vault, capture_id)
    }

    fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.delete_pdf_document(vault, document_id);
        }
        KnowledgeStore::delete_pdf_document(self, vault, document_id)
    }

    fn update_epub_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.update_epub_capture_note(vault, capture_id, note);
        }
        KnowledgeStore::update_epub_capture_note(self, vault, capture_id, note)
    }

    fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.delete_epub_capture(vault, capture_id);
        }
        KnowledgeStore::delete_epub_capture(self, vault, capture_id)
    }

    fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        if let Some(port) = self.command_port() {
            return port.delete_epub_document(vault, document_id);
        }
        KnowledgeStore::delete_epub_document(self, vault, document_id)
    }
}

#[allow(deprecated)]
fn capture_pdf_legacy<T: KnowledgeStore + ?Sized>(
    store: &mut T,
    op: &str,
    document_id: i64,
    single: Option<&PdfDraft>,
    compound: Option<&PdfCompoundDraft>,
    origin: &Provenance,
    user_note: &str,
) -> Result<PdfReceipt> {
    let mut archive = validated_archive(store)?;
    let document = archive
        .pdf_documents
        .iter()
        .find(|d| d.receipt.document_id == document_id)
        .ok_or_else(|| not_found("PDF document not found"))?
        .clone();
    validate_native(op, &document.vault, origin, user_note)?;
    let (draft, draft_hash) = match (single, compound) {
        (Some(d), None) => {
            resolve_pdf(&document.snapshot, d)?;
            (d, d.draft_hash()?)
        }
        (None, Some(c)) => {
            c.resolve(&document.snapshot)?;
            let first = c.segments.first().ok_or("compound PDF has no segments")?;
            (first, c.draft_hash()?)
        }
        _ => return Err(invalid("invalid PDF capture payload")),
    };
    let max_rev = archive
        .commits
        .iter()
        .map(|c| c.commit_rev)
        .max()
        .unwrap_or(0);
    let next_capture_id = archive
        .pdf_captures
        .iter()
        .map(|c| c.receipt.capture_id)
        .max()
        .unwrap_or(0)
        + 1;
    let receipt = PdfReceipt {
        operation_id: op.into(),
        capture_id: next_capture_id,
        document_id,
        commit_rev: max_rev + 1,
        original_sha256: document.receipt.original_sha256.clone(),
        extraction_revision: document.receipt.extraction_revision.clone(),
        draft_hash,
    };
    let record = PdfCaptureRecord {
        receipt: receipt.clone(),
        draft: draft.clone(),
        compound: compound.cloned(),
        provenance: origin.clone(),
        user_note: user_note.into(),
    };
    let expected_input = capture_input(&record, &document)?;
    if let Some(prior_cap) = archive
        .pdf_captures
        .iter()
        .find(|c| c.receipt.operation_id == op)
    {
        let prior = archive
            .pdf_operations
            .iter()
            .find(|old| old.operation_id == op);
        if prior.is_none_or(|old| old.kind != "pdf_capture" || old.input_hash != expected_input) {
            return conflict("operation id payload conflict");
        }
        return Ok(prior_cap.receipt.clone());
    }
    ensure_new_operation(&archive, op)?;
    let operation = operation_for_capture(&record, &document)?;
    archive.pdf_captures.push(record);
    archive.pdf_operations.push(operation.clone());
    archive
        .commits
        .push(commit_for_pdf(&operation, receipt.commit_rev));
    archive.schema_version = archive_schema_version(&archive);
    validate_archive(&archive)?;
    store.commit(CommitPlan {
        expected_revision: max_rev,
        next: archive,
    })?;
    Ok(receipt)
}

#[allow(deprecated)]
impl<T: StudyStore + ?Sized> StudyArchivePort for T {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        if let Some(port) = self.study_archive_port() {
            return port.study_snapshot();
        }
        StudyStore::study_snapshot(self)
    }

    fn restore_study(&mut self, archive: &StudyArchive) -> Result<()> {
        if let Some(port) = self.study_archive_port_mut() {
            return port.restore_study(archive);
        }
        StudyStore::restore_study(self, archive)
    }
}

#[allow(deprecated)]
impl<T: StudyStore + ?Sized> StudyCommand for T {
    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook> {
        if let Some(port) = self.study_command_port() {
            return port.ensure_wordbook(vault, seed, now_ms);
        }
        StudyStore::ensure_wordbook(self, vault, seed, now_ms)
    }

    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        if let Some(port) = self.study_command_port() {
            return port.commit_review(guard, schedule, review);
        }
        StudyStore::commit_review(self, guard, schedule, review)
    }

    fn delete_wordbook(&mut self, vault: &str, wordbook_id: i64) -> Result<()> {
        if let Some(port) = self.study_command_port() {
            return port.delete_wordbook(vault, wordbook_id);
        }
        StudyStore::delete_wordbook(self, vault, wordbook_id)
    }
}

#[allow(deprecated)]
impl<T: StudyStore + ?Sized> StudyQuery for T {
    fn study_decks(&self, vault: &str, now_ms: i64) -> Result<Vec<StudyDeckCounts>> {
        if let Some(port) = self.study_query_port() {
            return port.study_decks(vault, now_ms);
        }
        let archive = StudyStore::study_snapshot(self)?;
        let mut cards: BTreeMap<i64, Vec<&StudyCard>> = BTreeMap::new();
        for card in &archive.cards {
            cards.entry(card.wordbook_id).or_default().push(card);
        }
        let mut schedules = BTreeMap::new();
        for schedule in &archive.schedules {
            schedules.insert(schedule.card_id, schedule);
        }
        let mut decks = Vec::new();
        for book in archive.wordbooks.iter().filter(|book| book.vault == vault) {
            let book_cards = cards.get(&book.id).map(Vec::as_slice).unwrap_or(&[]);
            let mut due = 0u32;
            let mut new_count = 0u32;
            for card in book_cards {
                match schedules.get(&card.id) {
                    Some(schedule) if schedule.due_at_ms <= now_ms => due += 1,
                    Some(_) => {}
                    None => new_count += 1,
                }
            }
            decks.push(StudyDeckCounts {
                wordbook: book.clone(),
                total: u32::try_from(book_cards.len()).unwrap_or(u32::MAX),
                due,
                new_count,
            });
        }
        decks.sort_by_key(|deck| deck.wordbook.id);
        Ok(decks)
    }

    fn wordbook_cards(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StudyCard>> {
        if let Some(port) = self.study_query_port() {
            return port.wordbook_cards(vault, wordbook_id, limit, offset);
        }
        let archive = StudyStore::study_snapshot(self)?;
        if !archive
            .wordbooks
            .iter()
            .any(|book| book.id == wordbook_id && book.vault == vault)
        {
            return Err(not_found("wordbook not found in this vault"));
        }
        let mut cards: Vec<StudyCard> = archive
            .cards
            .into_iter()
            .filter(|card| card.wordbook_id == wordbook_id)
            .collect();
        cards.sort_by_key(|card| (card.position, card.id));
        Ok(cards.into_iter().skip(offset).take(limit).collect())
    }

    fn study_queue(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueueData> {
        if let Some(port) = self.study_query_port() {
            return port.study_queue(
                vault,
                wordbook_id,
                now_ms,
                day_start_ms,
                limit,
                daily_new_limit,
            );
        }
        let archive = StudyStore::study_snapshot(self)?;
        let book = archive
            .wordbooks
            .iter()
            .find(|book| book.id == wordbook_id && book.vault == vault)
            .ok_or_else(|| not_found("wordbook not found in this vault"))?;
        let schedules: BTreeMap<i64, &CardSchedule> = archive
            .schedules
            .iter()
            .map(|schedule| (schedule.card_id, schedule))
            .collect();
        let book_cards: Vec<&StudyCard> = archive
            .cards
            .iter()
            .filter(|card| card.wordbook_id == book.id)
            .collect();
        let mut due: Vec<&StudyCard> = book_cards
            .iter()
            .copied()
            .filter(|card| {
                schedules
                    .get(&card.id)
                    .is_some_and(|schedule| schedule.due_at_ms <= now_ms)
            })
            .collect();
        due.sort_by_key(|card| {
            (
                schedules.get(&card.id).map(|s| s.due_at_ms),
                card.position,
                card.id,
            )
        });
        let mut new_cards: Vec<&StudyCard> = book_cards
            .iter()
            .copied()
            .filter(|card| !schedules.contains_key(&card.id))
            .collect();
        new_cards.sort_by_key(|card| (card.position, card.id));

        let card_ids: BTreeSet<i64> = book_cards.iter().map(|card| card.id).collect();
        let introduced_today = archive
            .reviews
            .iter()
            .filter(|review| {
                card_ids.contains(&review.card_id)
                    && review.stage_before == StudyStage::New
                    && review.reviewed_at_ms >= day_start_ms
                    && review.reviewed_at_ms <= now_ms
            })
            .count() as u32;
        let new_allowance = daily_new_limit.saturating_sub(introduced_today);
        let take_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(new_cards.len())
            .min(limit.saturating_sub(due.len().min(limit)));
        let due_take = limit.min(due.len());
        let mut cards = Vec::with_capacity(due_take + take_new);
        for card in &due[..due_take] {
            let stage = schedules
                .get(&card.id)
                .map_or(StudyStage::New, |schedule| schedule.stage);
            cards.push(StudyQueueEntry {
                card: (*card).clone(),
                stage,
            });
        }
        for card in &new_cards[..take_new] {
            cards.push(StudyQueueEntry {
                card: (*card).clone(),
                stage: StudyStage::New,
            });
        }
        let remaining_due = due.len().saturating_sub(due_take);
        let remaining_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(new_cards.len())
            .saturating_sub(take_new);
        Ok(StudyQueueData {
            cards,
            remaining_due: u32::try_from(remaining_due).unwrap_or(u32::MAX),
            remaining_new: u32::try_from(remaining_new).unwrap_or(u32::MAX),
        })
    }

    fn card_review_context(
        &self,
        vault: &str,
        card_id: i64,
        operation_id: &str,
    ) -> Result<StudyReviewContext> {
        if let Some(port) = self.study_query_port() {
            return port.card_review_context(vault, card_id, operation_id);
        }
        let archive = StudyStore::study_snapshot(self)?;
        let card = archive
            .cards
            .iter()
            .find(|card| card.id == card_id)
            .ok_or_else(|| not_found("card not found"))?
            .clone();
        let book = archive
            .wordbooks
            .iter()
            .find(|book| book.id == card.wordbook_id && book.vault == vault)
            .ok_or_else(|| not_found("card not found in this vault"))?;
        let existing_review = archive
            .reviews
            .iter()
            .find(|review| review.operation_id == operation_id)
            .cloned();
        let current_schedule = archive
            .schedules
            .iter()
            .find(|schedule| schedule.card_id == card_id)
            .cloned();
        Ok(StudyReviewContext {
            card,
            wordbook_id: book.id,
            existing_review,
            current_schedule,
        })
    }

    fn study_remaining_counts(
        &self,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        daily_new_limit: u32,
    ) -> Result<(u32, u32)> {
        if let Some(port) = self.study_query_port() {
            return port.study_remaining_counts(wordbook_id, now_ms, day_start_ms, daily_new_limit);
        }
        let archive = StudyStore::study_snapshot(self)?;
        let card_ids: BTreeSet<i64> = archive
            .cards
            .iter()
            .filter(|card| card.wordbook_id == wordbook_id)
            .map(|card| card.id)
            .collect();
        let due = archive
            .schedules
            .iter()
            .filter(|schedule| card_ids.contains(&schedule.card_id) && schedule.due_at_ms <= now_ms)
            .count();
        let introduced_today = archive
            .reviews
            .iter()
            .filter(|review| {
                card_ids.contains(&review.card_id)
                    && review.stage_before == StudyStage::New
                    && review.reviewed_at_ms >= day_start_ms
                    && review.reviewed_at_ms <= now_ms
            })
            .count();
        let new_cards = archive
            .cards
            .iter()
            .filter(|card| card_ids.contains(&card.id))
            .filter(|card| {
                !archive
                    .schedules
                    .iter()
                    .any(|schedule| schedule.card_id == card.id)
            })
            .count();
        let allowance = daily_new_limit
            .min(DAILY_NEW_LIMIT)
            .saturating_sub(introduced_today as u32);
        Ok((
            u32::try_from(due).unwrap_or(u32::MAX),
            allowance.min(u32::try_from(new_cards).unwrap_or(u32::MAX)),
        ))
    }
}

/// Durability observations for diagnostics and external crash qualification.
/// Implementations report stages; they never terminate the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageStage {
    AfterBlobWrite,
    BeforeBlobPublish,
    AfterBlobPublish,
    BeforeTransaction,
    AfterEntities,
    BeforeCommit,
    AfterCommit,
    BeforeMigrationCommit,
    AfterMigrationCommit,
}
