//! QUSOR's framework-independent domain boundary.
//!
//! UI, database, network, and model SDK types belong in outer adapters. The
//! current implementation is the pure vocabulary-scheduling kernel: FSRS-5
//! memory mathematics plus this project's learning-step and review policy.

pub mod fsrs;

pub use fsrs::{
    DEFAULT_PARAMS, MemoryState, Policy, Rating, ScheduleInput, Scheduled, SchedulerError, Stage,
    schedule,
};
