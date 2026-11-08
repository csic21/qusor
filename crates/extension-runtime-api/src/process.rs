//! Persistable process observations, never permission to signal or execute.
//!
//! The platform adapter must revalidate these untrusted records against a pinned
//! kernel process object. A matching numeric PID alone is not authority.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub boot_id: String,
    pub start_time_ticks: u64,
    pub uid: u32,
    pub exe_device: u64,
    pub exe_inode: u64,
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupOutcome {
    /// No process currently has this numeric PID.
    Absent,
    /// The pinned kernel object has exited. It may still need reaping by its parent.
    Terminated,
    /// A live process exists, but it is not this recorded managed worker. No signal sent.
    Mismatch,
    /// A signal was sent to the verified object, but termination was not confirmed.
    Uncertain,
}
