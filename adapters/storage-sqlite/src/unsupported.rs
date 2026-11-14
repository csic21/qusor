use core_api::{
    contract::CaptureDraft,
    epub_contract::EpubSnapshot,
    knowledge::{
        ArchiveV2, CaptureRecord, CommitEntry, EpubDocumentReceipt, EpubReceipt, EpubTocRecord,
        KnowledgeError, KnowledgeErrorCode, PdfCaptureRecord, PdfDocumentReceipt,
        PdfDocumentRecord, PdfReceipt, Provenance, Receipt, Result,
    },
    pdf_compound::PdfCompoundDraft,
    pdf_contract::{PdfDraft, PdfSnapshot},
    vocab::{
        CardSchedule, ReviewEntry, ReviewGuard, StudyArchive, StudyCard, Wordbook, WordbookSeed,
    },
};
#[allow(deprecated)]
use learning_ports::{CommitPlan, KnowledgeStore, StorageStage, StudyStore};
use learning_ports::{
    EpubCaptureWithVault, EpubChapterLookup, EpubDocumentsPageData, EpubNotesPageData,
    EpubSavedSelectionsLookup, KnowledgeArchive, KnowledgeCommand, KnowledgeQuery, OperationLookup,
    PdfCaptureWithVault, PdfDocumentsPageData, PdfNotesPageData, PdfPageTextLookup,
    PdfSavedSelectionsLookup, StudyArchivePort, StudyCommand, StudyDeckCounts, StudyQuery,
    StudyQueueData, StudyReviewContext,
};
use std::{path::Path, sync::Arc};

/// Non-Unix placeholder core handle returning explicit [`KnowledgeErrorCode::Unavailable`].
pub struct SqliteCore;

/// Non-Unix placeholder store returning explicit [`KnowledgeErrorCode::Unavailable`].
pub struct SqliteStore {
    core: SqliteCore,
}

fn unavailable<T>() -> Result<T> {
    Err(KnowledgeError::new(
        KnowledgeErrorCode::Unavailable,
        "private durable filesystem adapter is not qualified on this platform [category=unsupported platform=non-unix]",
    ))
}

impl SqliteCore {
    pub fn blob_read_count(&self) -> u64 {
        0
    }

    pub fn reset_blob_read_count(&self) {}
}

impl SqliteStore {
    pub fn open(_: impl AsRef<Path>) -> Result<Self> {
        unavailable()
    }

    pub fn with_observer(
        _: impl AsRef<Path>,
        _: Arc<dyn Fn(StorageStage) + Send + Sync>,
    ) -> Result<Self> {
        unavailable()
    }

    pub fn set_observer(&mut self, _: Arc<dyn Fn(StorageStage) + Send + Sync>) {}

    pub fn core(&self) -> &SqliteCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut SqliteCore {
        &mut self.core
    }

    pub fn blob_read_count(&self) -> u64 {
        0
    }

    pub fn reset_blob_read_count(&self) {}
}

impl KnowledgeQuery for SqliteCore {
    fn text_capture(&self, _: i64) -> Result<Option<CaptureRecord>> {
        unavailable()
    }
    fn text_receipt_by_operation(&self, _: &str) -> Result<Option<Receipt>> {
        unavailable()
    }
    fn commit_by_operation(&self, _: &str) -> Result<Option<CommitEntry>> {
        unavailable()
    }
    fn pdf_document(&self, _: i64) -> Result<Option<PdfDocumentRecord>> {
        unavailable()
    }
    fn pdf_document_receipt_by_operation(&self, _: &str) -> Result<Option<PdfDocumentReceipt>> {
        unavailable()
    }
    fn pdf_capture(&self, _: i64) -> Result<Option<PdfCaptureRecord>> {
        unavailable()
    }
    fn pdf_capture_with_vault(&self, _: i64) -> Result<Option<PdfCaptureWithVault>> {
        unavailable()
    }
    fn pdf_capture_by_operation(&self, _: &str) -> Result<OperationLookup<PdfCaptureWithVault>> {
        unavailable()
    }
    fn pdf_documents_page(
        &self,
        _: &str,
        _: Option<(i64, i64)>,
        _: usize,
    ) -> Result<PdfDocumentsPageData> {
        unavailable()
    }
    fn pdf_page_text(&self, _: &str, _: i64, _: u32) -> Result<PdfPageTextLookup> {
        unavailable()
    }
    fn pdf_saved_selections(&self, _: &str, _: i64, _: u32) -> Result<PdfSavedSelectionsLookup> {
        unavailable()
    }
    fn pdf_notes_page(
        &self,
        _: &str,
        _: Option<i64>,
        _: Option<(i64, i64)>,
        _: usize,
    ) -> Result<PdfNotesPageData> {
        unavailable()
    }
    fn read_original_pdf(&self, _: i64) -> Result<Vec<u8>> {
        unavailable()
    }
    fn epub_documents_page(
        &self,
        _: &str,
        _: Option<(i64, i64)>,
        _: usize,
    ) -> Result<EpubDocumentsPageData> {
        unavailable()
    }
    fn epub_chapter_text(&self, _: &str, _: i64, _: u32) -> Result<EpubChapterLookup> {
        unavailable()
    }
    fn epub_toc(&self, _: &str, _: i64) -> Result<Option<Vec<EpubTocRecord>>> {
        unavailable()
    }
    fn epub_saved_selections(&self, _: &str, _: i64, _: u32) -> Result<EpubSavedSelectionsLookup> {
        unavailable()
    }
    fn epub_notes_page(
        &self,
        _: &str,
        _: Option<i64>,
        _: Option<(i64, i64)>,
        _: usize,
    ) -> Result<EpubNotesPageData> {
        unavailable()
    }
    fn epub_capture_with_vault(&self, _: i64) -> Result<Option<EpubCaptureWithVault>> {
        unavailable()
    }
}

impl KnowledgeCommand for SqliteCore {
    fn commit_text_capture(
        &mut self,
        _: &str,
        _: &str,
        _: &CaptureDraft,
        _: &Provenance,
        _: &str,
    ) -> Result<Receipt> {
        unavailable()
    }
    fn import_pdf_document(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
        _: &[u8],
        _: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt> {
        unavailable()
    }
    fn capture_pdf(
        &mut self,
        _: &str,
        _: i64,
        _: &PdfDraft,
        _: &Provenance,
        _: &str,
    ) -> Result<PdfReceipt> {
        unavailable()
    }
    fn capture_pdf_compound(
        &mut self,
        _: &str,
        _: i64,
        _: &PdfCompoundDraft,
        _: &Provenance,
        _: &str,
    ) -> Result<PdfReceipt> {
        unavailable()
    }
    fn import_epub(&mut self, _: &str, _: &str, _: &EpubSnapshot) -> Result<EpubDocumentReceipt> {
        unavailable()
    }
    fn capture_epub_selection(
        &mut self,
        _: &str,
        _: &str,
        _: i64,
        _: u32,
        _: u64,
        _: u64,
        _: &Provenance,
        _: &str,
    ) -> Result<EpubReceipt> {
        unavailable()
    }
    fn update_pdf_capture_note(&mut self, _: &str, _: i64, _: &str) -> Result<()> {
        unavailable()
    }
    fn delete_pdf_capture(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn delete_pdf_document(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn update_epub_capture_note(&mut self, _: &str, _: i64, _: &str) -> Result<()> {
        unavailable()
    }
    fn delete_epub_capture(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn delete_epub_document(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
}

impl KnowledgeArchive for SqliteCore {
    fn is_empty(&self) -> Result<bool> {
        unavailable()
    }
    fn snapshot(&self) -> Result<ArchiveV2> {
        unavailable()
    }
    fn restore(&mut self, _: &ArchiveV2) -> Result<()> {
        unavailable()
    }
}

impl StudyQuery for SqliteCore {
    fn study_decks(&self, _: &str, _: i64) -> Result<Vec<StudyDeckCounts>> {
        unavailable()
    }
    fn wordbook_cards(&self, _: &str, _: i64, _: usize, _: usize) -> Result<Vec<StudyCard>> {
        unavailable()
    }
    fn study_queue(
        &self,
        _: &str,
        _: i64,
        _: i64,
        _: i64,
        _: usize,
        _: u32,
    ) -> Result<StudyQueueData> {
        unavailable()
    }
    fn card_review_context(&self, _: &str, _: i64, _: &str) -> Result<StudyReviewContext> {
        unavailable()
    }
    fn study_remaining_counts(&self, _: i64, _: i64, _: i64, _: u32) -> Result<(u32, u32)> {
        unavailable()
    }
}

impl StudyCommand for SqliteCore {
    fn ensure_wordbook(&mut self, _: &str, _: &WordbookSeed, _: i64) -> Result<Wordbook> {
        unavailable()
    }
    fn commit_review(
        &mut self,
        _: &ReviewGuard,
        _: &CardSchedule,
        _: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        unavailable()
    }
    fn delete_wordbook(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
}

impl StudyArchivePort for SqliteCore {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        unavailable()
    }
    fn restore_study(&mut self, _: &StudyArchive) -> Result<()> {
        unavailable()
    }
}

#[allow(deprecated)]
impl KnowledgeStore for SqliteStore {
    fn snapshot(&self) -> Result<ArchiveV2> {
        unavailable()
    }
    fn commit(&mut self, _: CommitPlan) -> Result<()> {
        unavailable()
    }
    fn restore(&mut self, _: &ArchiveV2) -> Result<()> {
        unavailable()
    }
    fn update_pdf_capture_note(&mut self, _: &str, _: i64, _: &str) -> Result<()> {
        unavailable()
    }
    fn delete_pdf_capture(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn delete_pdf_document(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn update_epub_capture_note(&mut self, _: &str, _: i64, _: &str) -> Result<()> {
        unavailable()
    }
    fn delete_epub_capture(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn delete_epub_document(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn query_port(&self) -> Option<&dyn KnowledgeQuery> {
        Some(&self.core)
    }
    fn command_port(&mut self) -> Option<&mut dyn KnowledgeCommand> {
        Some(&mut self.core)
    }
    fn archive_port(&self) -> Option<&dyn KnowledgeArchive> {
        Some(&self.core)
    }
    fn archive_port_mut(&mut self) -> Option<&mut dyn KnowledgeArchive> {
        Some(&mut self.core)
    }
}

#[allow(deprecated)]
impl StudyStore for SqliteStore {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        unavailable()
    }
    fn ensure_wordbook(&mut self, _: &str, _: &WordbookSeed, _: i64) -> Result<Wordbook> {
        unavailable()
    }
    fn commit_review(
        &mut self,
        _: &ReviewGuard,
        _: &CardSchedule,
        _: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        unavailable()
    }
    fn restore_study(&mut self, _: &StudyArchive) -> Result<()> {
        unavailable()
    }
    fn delete_wordbook(&mut self, _: &str, _: i64) -> Result<()> {
        unavailable()
    }
    fn study_query_port(&self) -> Option<&dyn StudyQuery> {
        Some(&self.core)
    }
    fn study_command_port(&mut self) -> Option<&mut dyn StudyCommand> {
        Some(&mut self.core)
    }
    fn study_archive_port(&self) -> Option<&dyn StudyArchivePort> {
        Some(&self.core)
    }
    fn study_archive_port_mut(&mut self) -> Option<&mut dyn StudyArchivePort> {
        Some(&mut self.core)
    }
}
