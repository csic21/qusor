//! Immutable file CAS and exclusive ownership. Native filesystem details stay here.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::BlobStore;

#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
pub use unsupported::BlobStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobStage {
    AfterWrite,
    BeforePublish,
    AfterPublish,
}
