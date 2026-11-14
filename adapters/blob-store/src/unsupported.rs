use super::BlobStage;
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode, Result};
use std::path::Path;

fn unavailable<T>() -> Result<T> {
    Err(KnowledgeError::new(
        KnowledgeErrorCode::Unavailable,
        "immutable blob store is not qualified on this platform [category=unsupported platform=non-unix]",
    ))
}

pub struct BlobStore;

impl BlobStore {
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        unavailable()
    }

    pub fn read(&self, _hash: &str) -> Result<Vec<u8>> {
        unavailable()
    }

    pub fn publish(&self, _bytes: &[u8], _observe: &dyn Fn(BlobStage)) -> Result<()> {
        unavailable()
    }

    pub fn read_count(&self) -> u64 {
        0
    }

    pub fn reset_read_count(&self) {}
}
