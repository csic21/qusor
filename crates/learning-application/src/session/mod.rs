//! Interactive, session-only draft editing, provider event, and projection
//! state machines, strictly separate from durable Core persistence.
pub mod annotation_draft;
pub mod draft_text;
pub mod provider_events;
pub mod provider_projections;
