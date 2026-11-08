use core_api::knowledge::*;
use learning_ports::{CommitPlan, KnowledgeStore, StorageStage};
use std::{path::Path, sync::Arc};
pub struct SqliteStore;
fn unavailable<T>() -> Result<T> {
    Err(KnowledgeError::new(
        KnowledgeErrorCode::Unavailable,
        "private durable filesystem adapter is not qualified on this platform",
    ))
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
}
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
}
