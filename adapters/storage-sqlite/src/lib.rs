//! SQLite/CAS implementation of the whole knowledge transaction port.
pub use learning_ports::StorageStage;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::SqliteStore;
#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
pub use unsupported::SqliteStore;
