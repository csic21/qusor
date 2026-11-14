//! SQLite/CAS implementation of the split knowledge and study persistence ports.
pub use learning_ports::StorageStage;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{SqliteCore, SqliteStore};
#[cfg(not(unix))]
mod unsupported;
#[cfg(not(unix))]
pub use unsupported::{SqliteCore, SqliteStore};
