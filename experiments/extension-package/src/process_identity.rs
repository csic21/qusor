//! Compatibility forwarding to the production platform implementation.
//! Persistent records are observations; the adapter pins and revalidates the
//! exact native process before cleanup. No second implementation lives here.
pub use runtime_process::{
    CleanupOutcome, ProcessIdentity, cleanup, current, guard_worker, inspect,
};
