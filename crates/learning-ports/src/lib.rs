//! Whole-transaction knowledge persistence boundary. No SQL or platform objects cross it.
use core_api::{
    error::{KnowledgeError, KnowledgeErrorCode},
    knowledge::{ArchiveV2, Result},
    vocab::{CardSchedule, ReviewEntry, ReviewGuard, StudyArchive, Wordbook, WordbookSeed},
};

fn unavailable<T>(message: &'static str) -> Result<T> {
    Err(KnowledgeError::new(
        KnowledgeErrorCode::Unavailable,
        message,
    ))
}

/// A single already-validated application transition. Adapters must still verify it.
#[derive(Clone, Debug)]
pub struct CommitPlan {
    pub expected_revision: i64,
    pub next: ArchiveV2,
}

/// A consistent complete knowledge snapshot and atomic write boundary.
/// Implementations serialize writers, verify immutable blob integrity, and commit
/// entities, receipt, audit, outbox, index job and global revision together.
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
}

/// Atomic vocabulary study persistence. Review history is append-only and the
/// current schedule row changes in the same transaction as its review entry.
///
/// Implementations bind one trusted Core/vault, verify relationships, enforce the
/// optimistic review guard and compute ids inside the transaction. No Host
/// authority, package identity, scheduling algorithm or UI type crosses this port.
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
