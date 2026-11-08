//! Immutable file CAS and exclusive ownership. Native filesystem details stay here.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::BlobStore;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobStage {
    AfterWrite,
    BeforePublish,
    AfterPublish,
}
