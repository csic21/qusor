//! Stable application failure categories for user-facing handling.
use std::fmt;
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
}
impl fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for KnowledgeError {}
impl From<&str> for KnowledgeError {
    fn from(v: &str) -> Self {
        Self::new(KnowledgeErrorCode::Integrity, v)
    }
}
impl From<String> for KnowledgeError {
    fn from(v: String) -> Self {
        Self::new(KnowledgeErrorCode::Integrity, v)
    }
}
impl From<serde_json::Error> for KnowledgeError {
    fn from(_: serde_json::Error) -> Self {
        Self::new(KnowledgeErrorCode::InvalidInput, "invalid knowledge JSON")
    }
}
impl From<std::string::FromUtf8Error> for KnowledgeError {
    fn from(_: std::string::FromUtf8Error) -> Self {
        Self::new(KnowledgeErrorCode::Integrity, "invalid UTF-8 snapshot")
    }
}
impl From<base64::DecodeError> for KnowledgeError {
    fn from(_: base64::DecodeError) -> Self {
        Self::new(
            KnowledgeErrorCode::Integrity,
            "invalid archive blob encoding",
        )
    }
}
impl From<crate::contract::ContractError> for KnowledgeError {
    fn from(v: crate::contract::ContractError) -> Self {
        use crate::contract::ContractErrorCode as C;
        let code = match v.code {
            C::BudgetExceeded => KnowledgeErrorCode::BudgetExceeded,
            C::UnsupportedVersion => KnowledgeErrorCode::UnsupportedVersion,
            C::NotFound => KnowledgeErrorCode::NotFound,
            C::VersionMismatch => KnowledgeErrorCode::Conflict,
            _ => KnowledgeErrorCode::InvalidInput,
        };
        Self::new(code, v.message)
    }
}
impl From<crate::pdf_contract::PdfContractError> for KnowledgeError {
    fn from(v: crate::pdf_contract::PdfContractError) -> Self {
        use crate::pdf_contract::PdfContractErrorCode as C;
        let code = match v.code {
            C::BudgetExceeded => KnowledgeErrorCode::BudgetExceeded,
            C::UnsupportedVersion => KnowledgeErrorCode::UnsupportedVersion,
            C::HashMismatch | C::OriginalMismatch => KnowledgeErrorCode::Integrity,
            _ => KnowledgeErrorCode::InvalidInput,
        };
        Self::new(code, v.message)
    }
}
impl From<crate::epub_contract::EpubContractError> for KnowledgeError {
    fn from(v: crate::epub_contract::EpubContractError) -> Self {
        use crate::epub_contract::EpubContractErrorCode as C;
        let code = match v.code {
            C::BudgetExceeded => KnowledgeErrorCode::BudgetExceeded,
            C::UnsupportedVersion => KnowledgeErrorCode::UnsupportedVersion,
            C::HashMismatch | C::OriginalMismatch => KnowledgeErrorCode::Integrity,
            _ => KnowledgeErrorCode::InvalidInput,
        };
        Self::new(code, v.message)
    }
}
