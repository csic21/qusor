//! Stable application failure categories and shared contract error primitives.
use std::fmt;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeErrorCode {
    InvalidInput,
    Integrity,
    Conflict,
    NotFound,
    Storage,
    BudgetExceeded,
    UnsupportedVersion,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeError {
    pub code: KnowledgeErrorCode,
    pub message: String,
}

pub type KnowledgeResult<T> = std::result::Result<T, KnowledgeError>;

impl KnowledgeError {
    pub fn new(code: KnowledgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::InvalidInput, message)
    }

    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::Integrity, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::Conflict, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::NotFound, message)
    }

    pub fn budget_exceeded(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::BudgetExceeded, message)
    }

    pub fn unsupported_version(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::UnsupportedVersion, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::Unavailable, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(KnowledgeErrorCode::Storage, message)
    }
}

impl fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for KnowledgeError {}

impl From<&str> for KnowledgeError {
    fn from(v: &str) -> Self {
        Self::integrity(v)
    }
}

impl From<String> for KnowledgeError {
    fn from(v: String) -> Self {
        Self::integrity(v)
    }
}

impl From<serde_json::Error> for KnowledgeError {
    fn from(_: serde_json::Error) -> Self {
        Self::invalid_input("invalid knowledge JSON")
    }
}

impl From<std::string::FromUtf8Error> for KnowledgeError {
    fn from(_: std::string::FromUtf8Error) -> Self {
        Self::integrity("invalid UTF-8 snapshot")
    }
}

impl From<base64::DecodeError> for KnowledgeError {
    fn from(_: base64::DecodeError) -> Self {
        Self::integrity("invalid archive blob encoding")
    }
}

/// Shared interface for bounded wire-contract error types.
pub(crate) trait ContractWireError: Sized {
    fn malformed(message: &'static str) -> Self;
    fn budget_exceeded(message: &'static str) -> Self;
}

/// Declare a typed contract error struct, `Display`, `Error`, `ContractWireError`,
/// and `From<...> for KnowledgeError` from one mapping table.
#[macro_export]
#[doc(hidden)]
macro_rules! define_contract_error {
    (
        $(#[$meta:meta])*
        pub struct $Error:ident ($Code:ident) {
            $( $variant:ident => $kcode:ident ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $Error {
            pub code: $Code,
            pub message: &'static str,
        }

        impl $Error {
            pub const fn new(code: $Code, message: &'static str) -> Self {
                Self { code, message }
            }
        }

        impl ::std::fmt::Display for $Error {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{:?}: {}", self.code, self.message)
            }
        }

        impl ::std::error::Error for $Error {}

        impl $crate::error::ContractWireError for $Error {
            fn malformed(message: &'static str) -> Self {
                Self::new($Code::Malformed, message)
            }

            fn budget_exceeded(message: &'static str) -> Self {
                Self::new($Code::BudgetExceeded, message)
            }
        }

        impl From<$Error> for $crate::error::KnowledgeError {
            fn from(value: $Error) -> Self {
                let code = match value.code {
                    $( $Code::$variant => $crate::error::KnowledgeErrorCode::$kcode, )*
                };
                Self::new(code, value.message)
            }
        }
    };
}

/// Parse JSON bytes within a strict byte limit using a unified contract error.
pub(crate) fn parse_bounded_json<T, E>(
    bytes: &[u8],
    limit: usize,
    budget_msg: &'static str,
    malformed_msg: &'static str,
) -> std::result::Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: ContractWireError,
{
    if bytes.len() > limit {
        return Err(E::budget_exceeded(budget_msg));
    }
    serde_json::from_slice(bytes).map_err(|_| E::malformed(malformed_msg))
}

/// Encode canonical JSON bytes within a strict byte limit using a unified contract error.
pub(crate) fn encode_bounded_json<T, E>(
    value: &T,
    limit: usize,
    malformed_msg: &'static str,
    budget_msg: &'static str,
) -> std::result::Result<Vec<u8>, E>
where
    T: serde::Serialize,
    E: ContractWireError,
{
    let bytes = serde_json::to_vec(value).map_err(|_| E::malformed(malformed_msg))?;
    if bytes.len() > limit {
        return Err(E::budget_exceeded(budget_msg));
    }
    Ok(bytes)
}

pub(crate) fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn is_bounded_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

pub(crate) fn is_bounded_label(value: &str, max: usize, required: bool) -> bool {
    if required && value.is_empty() {
        return false;
    }
    value.len() <= max && !value.chars().any(char::is_control)
}

pub(crate) fn is_bounded_text(
    value: &str,
    max: usize,
    required: bool,
    allow_newline: bool,
    allow_cr_tab: bool,
) -> bool {
    if required && value.is_empty() {
        return false;
    }
    value.len() <= max
        && value.chars().all(|ch| {
            !ch.is_control()
                || (allow_newline && ch == '\n')
                || (allow_cr_tab && matches!(ch, '\r' | '\t'))
        })
}

pub(crate) fn is_grapheme_boundary(text: &str, byte: usize) -> bool {
    byte == text.len() || text.grapheme_indices(true).any(|(idx, _)| idx == byte)
}
