//! Whole-transaction knowledge persistence boundary. No SQL or platform objects cross it.
use core_api::knowledge::{ArchiveV2, Result};

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
