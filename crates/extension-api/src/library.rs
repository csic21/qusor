//! Versioned bounded PDF Library metadata wire for ordinary plugins.
//!
//! The guest asks for one newest-first page inside the vault bound by its exact
//! `library.read` grant. Responses contain document metadata only: no original
//! PDF bytes, CAS blobs, filesystem paths, notes, selections or authority.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_6, MAX_REQUEST_BYTES, decode_json, encode_json,
    validate_opaque_id,
};
use serde::{Deserialize, Serialize};

/// Version of the Library metadata wire itself, independent from Host/package versions.
pub const LIBRARY_PROTOCOL: u16 = 1;
/// Maximum documents in one page. This is a wire budget, not a product limit.
pub const MAX_LIBRARY_PAGE: u16 = 6;
/// Maximum UTF-8 bytes of a PDF title in one summary.
pub const MAX_LIBRARY_TITLE_BYTES: usize = 96;
/// Maximum opaque cursor bytes; canonical format remains a trusted-backend concern.
pub const MAX_LIBRARY_CURSOR_BYTES: usize = 128;
/// Maximum accepted original PDF bytes in this first Library profile.
pub const MAX_LIBRARY_ORIGINAL_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum pages in this first Library profile.
pub const MAX_LIBRARY_PAGES: u32 = 16;

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn budget(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::BudgetExceeded, message)
}

fn protocol(library_protocol: u16, api_version: ApiVersion) -> Result<(), ApiError> {
    if library_protocol != LIBRARY_PROTOCOL || api_version != HOST_API_V0_6 {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "library requires protocol1 and Host API0.6",
        ));
    }
    Ok(())
}

fn cursor(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > MAX_LIBRARY_CURSOR_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'*')
        })
    {
        return Err(invalid("invalid library cursor"));
    }
    Ok(())
}

fn positive(value: i64, field: &'static str) -> Result<(), ApiError> {
    if value <= 0 {
        return Err(invalid(field));
    }
    Ok(())
}

fn digest(value: &str, field: &'static str) -> Result<(), ApiError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(field));
    }
    Ok(())
}

/// One newest-first Library request from a guest; the Host supplies vault scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryRequest {
    pub library_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub operation: LibraryOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LibraryOperation {
    /// Newest-first PDF metadata page inside the Host-bound vault.
    Page { cursor: Option<String>, limit: u16 },
}

impl LibraryRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        protocol(self.library_protocol, self.api_version)?;
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.operation {
            LibraryOperation::Page {
                cursor: after,
                limit,
            } => {
                if *limit == 0 || *limit > MAX_LIBRARY_PAGE {
                    return Err(budget("library page limit is outside the supported range"));
                }
                if let Some(after) = after {
                    cursor(after)?;
                }
            }
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let request: Self = decode_json(json, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        request.validate()?;
        Ok(request)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}

/// One PDF document summary; original content remains behind a separate Reader path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryDocument {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub page_count: u32,
}

impl LibraryDocument {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.document_id, "invalid library document id")?;
        positive(self.commit_rev, "invalid library commit revision")?;
        if self.title.is_empty()
            || self.title.len() > MAX_LIBRARY_TITLE_BYTES
            || self.title.chars().any(char::is_control)
        {
            return Err(invalid("invalid library title"));
        }
        digest(&self.original_sha256, "invalid library original digest")?;
        digest(
            &self.extraction_revision,
            "invalid library extraction revision",
        )?;
        if self.original_bytes == 0 || self.original_bytes > MAX_LIBRARY_ORIGINAL_BYTES {
            return Err(budget(
                "library original size is outside the supported range",
            ));
        }
        if self.page_count == 0 || self.page_count > MAX_LIBRARY_PAGES {
            return Err(budget("library page count is outside the supported range"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LibraryResult {
    Page {
        documents: Vec<LibraryDocument>,
        next_cursor: Option<String>,
    },
    Error {
        error: ApiError,
    },
}

/// One bounded reply; the adapter must also correlate `request_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryResponse {
    pub request_id: String,
    pub result: LibraryResult,
}

impl LibraryResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.result {
            LibraryResult::Page {
                documents,
                next_cursor,
            } => {
                if documents.len() > usize::from(MAX_LIBRARY_PAGE) {
                    return Err(budget("library page exceeds its budget"));
                }
                for document in documents {
                    document.validate()?;
                }
                if let Some(next) = next_cursor {
                    cursor(next)?;
                }
            }
            LibraryResult::Error { error } => error.validate()?,
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let response: Self = decode_json(json, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        response.validate()?;
        Ok(response)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}
