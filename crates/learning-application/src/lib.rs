//! Application use cases independent of SQL, filesystems, Host, UI, and runtime.
//!
//! Organized by domain boundary:
//! - [`knowledge`]: durable knowledge capture, library/note queries, and archive/restore
//! - [`session`]: ephemeral interactive draft, provider event, and projection state machines
//! - [`study`]: FSRS-5 vocabulary study use cases
pub mod knowledge;
pub mod session;
pub mod study;

// Root-level compatibility re-exports (ADR 0054).
pub use knowledge::{
    KnowledgeApplication, annotation_source, archive, epub_library, pdf_library, pdf_notes,
};
pub use session::{annotation_draft, draft_text, provider_events, provider_projections};

use core_api::knowledge::{KnowledgeError, Result};

pub(crate) fn fail<T>(message: &str) -> Result<T> {
    Err(KnowledgeError::conflict(message))
}
