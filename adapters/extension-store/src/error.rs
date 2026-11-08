//! Stable error categories; callers never infer control flow from diagnostics.
use extension_package::package::PackageError;
use std::{error::Error, fmt, io};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreErrorCode {
    InvalidInput,
    Conflict,
    Corrupt,
    Busy,
    Io,
    Package,
    External,
    Unsupported,
}
#[derive(Debug)]
pub enum StoreError {
    InvalidInput(String),
    Conflict(String),
    Corrupt(String),
    Busy(String),
    Io(io::Error),
    Database(rusqlite::Error),
    Package(PackageError),
    External(String),
    Encoding(serde_json::Error),
    Protocol(extension_api::ApiError),
    Unsupported(String),
}
impl StoreError {
    pub fn code(&self) -> StoreErrorCode {
        match self {
            Self::InvalidInput(_) | Self::Protocol(_) => StoreErrorCode::InvalidInput,
            Self::Conflict(_) => StoreErrorCode::Conflict,
            Self::Corrupt(_) | Self::Encoding(_) => StoreErrorCode::Corrupt,
            Self::Busy(_) => StoreErrorCode::Busy,
            Self::Io(_) => StoreErrorCode::Io,
            Self::Database(rusqlite::Error::SqliteFailure(error, _)) => match error.code {
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    StoreErrorCode::Busy
                }
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    StoreErrorCode::Corrupt
                }
                rusqlite::ErrorCode::ConstraintViolation => StoreErrorCode::Conflict,
                _ => StoreErrorCode::Io,
            },
            Self::Database(rusqlite::Error::QueryReturnedNoRows) => StoreErrorCode::Conflict,
            Self::Database(_) => StoreErrorCode::Io,
            Self::Package(_) => StoreErrorCode::Package,
            Self::External(_) => StoreErrorCode::External,
            Self::Unsupported(_) => StoreErrorCode::Unsupported,
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::Corrupt(message.into())
    }
    pub fn busy(message: impl Into<String>) -> Self {
        Self::Busy(message.into())
    }
    pub fn external(message: impl Into<String>) -> Self {
        Self::External(message.into())
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(v)
            | Self::Conflict(v)
            | Self::Corrupt(v)
            | Self::Busy(v)
            | Self::External(v)
            | Self::Unsupported(v) => f.write_str(v),
            Self::Io(v) => v.fmt(f),
            Self::Database(v) => v.fmt(f),
            Self::Package(v) => v.fmt(f),
            Self::Encoding(v) => v.fmt(f),
            Self::Protocol(v) => v.fmt(f),
        }
    }
}
impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(v) => Some(v),
            Self::Database(v) => Some(v),
            Self::Package(v) => Some(v),
            Self::Encoding(v) => Some(v),
            Self::Protocol(v) => Some(v),
            _ => None,
        }
    }
}
impl From<io::Error> for StoreError {
    fn from(v: io::Error) -> Self {
        Self::Io(v)
    }
}
impl From<rusqlite::Error> for StoreError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Database(v)
    }
}
impl From<PackageError> for StoreError {
    fn from(v: PackageError) -> Self {
        Self::Package(v)
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(v: serde_json::Error) -> Self {
        Self::Encoding(v)
    }
}
impl From<extension_api::ApiError> for StoreError {
    fn from(v: extension_api::ApiError) -> Self {
        Self::Protocol(v)
    }
}
impl From<std::time::SystemTimeError> for StoreError {
    fn from(v: std::time::SystemTimeError) -> Self {
        Self::external(v.to_string())
    }
}
