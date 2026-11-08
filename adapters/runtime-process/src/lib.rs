//! Platform process identity and confirmed cleanup, independent of Host policy.
//!
//! The first implementation supports Linux and the trusted same-executable
//! `--managed-worker <parent-pid> <parent-start> <run-id>` command profile.
//! Other platforms fail explicitly. This adapter provides process control, not Host
//! authorization, an OS sandbox or a complete product supervisor.
pub use extension_runtime_api::{CleanupOutcome, ProcessIdentity};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{cleanup, current, guard_worker, inspect};

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use super::{CleanupOutcome, ProcessIdentity};
    use std::io;

    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process identity profile is implemented only for Linux",
        ))
    }
    pub fn current() -> io::Result<ProcessIdentity> {
        unsupported()
    }
    pub fn inspect(_pid: u32, _run_id: &str) -> io::Result<ProcessIdentity> {
        unsupported()
    }
    pub fn guard_worker(
        _parent_pid: u32,
        _parent_start_time_ticks: u64,
        _run_id: &str,
    ) -> io::Result<()> {
        unsupported()
    }
    pub fn cleanup(_expected: &ProcessIdentity) -> io::Result<CleanupOutcome> {
        unsupported()
    }
}
#[cfg(not(target_os = "linux"))]
pub use unsupported::{cleanup, current, guard_worker, inspect};

mod controller;
mod protocol;
mod transport;
mod worker;
pub use controller::*;
pub use protocol::{ExecutionFailure, FrozenExecution, PROCESS_PROTOCOL, WorkerOutcome};
pub use worker::worker_main;
