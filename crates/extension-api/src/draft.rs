//! Host API0.3 bounded draft editing wire. Validation never grants access.
use crate::{
    ApiError, ApiVersion, Capability, ErrorCode, HOST_API_V0_3, Resource,
    capability_resource_valid, decode_json, encode_json, validate_opaque_id,
};
use serde::{Deserialize, Serialize};
pub const DRAFT_PROTOCOL: u16 = 1;
pub const MAX_DRAFT_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_DRAFT_CHUNK_BYTES: usize = 512;
pub const MAX_DRAFT_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_DRAFT_RESPONSE_BYTES: usize = 4096;
fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}
pub(crate) fn validate_hex_id(id: &str, name: &'static str) -> Result<(), ApiError> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("{name} must be 32 lowercase hex bytes"),
        ));
    }
    Ok(())
}
fn request_id(id: &str) -> Result<(), ApiError> {
    if id.len() > 32 {
        return Err(invalid("request_id exceeds 32 bytes"));
    }
    validate_opaque_id(id, "request_id")
}
fn revision(value: u64) -> Result<(), ApiError> {
    if value == 0 {
        Err(invalid("revision must be nonzero"))
    } else {
        Ok(())
    }
}
fn offset(value: u32) -> Result<(), ApiError> {
    if value as usize > MAX_DRAFT_TEXT_BYTES {
        Err(invalid("offset exceeds text budget"))
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftRequest {
    pub api_version: ApiVersion,
    pub draft_protocol: u16,
    pub request_id: String,
    pub resource: Resource,
    pub mount_id: String,
    pub operation: DraftOperation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DraftOperation {
    ReadChunk {
        revision: Option<u64>,
        offset: u32,
        max_bytes: u16,
    },
    BeginReplace {
        expected_revision: u64,
        total_bytes: u32,
    },
    AppendChunk {
        upload_id: String,
        offset: u32,
        text: String,
    },
    CommitReplace {
        upload_id: String,
    },
    CancelReplace {
        upload_id: String,
    },
}
impl DraftOperation {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::ReadChunk {
                revision: rev,
                offset: start,
                max_bytes,
            } => {
                if let Some(r) = rev {
                    revision(*r)?;
                }
                offset(*start)?;
                if *max_bytes == 0 || usize::from(*max_bytes) > MAX_DRAFT_CHUNK_BYTES {
                    return Err(invalid("invalid chunk size"));
                }
            }
            Self::BeginReplace {
                expected_revision,
                total_bytes,
            } => {
                revision(*expected_revision)?;
                offset(*total_bytes)?;
            }
            Self::AppendChunk {
                upload_id,
                offset: start,
                text,
            } => {
                validate_hex_id(upload_id, "upload_id")?;
                offset(*start)?;
                if text.is_empty()
                    || text.len() > MAX_DRAFT_CHUNK_BYTES
                    || (*start as usize) + text.len() > MAX_DRAFT_TEXT_BYTES
                {
                    return Err(invalid("invalid upload chunk"));
                }
            }
            Self::CommitReplace { upload_id } | Self::CancelReplace { upload_id } => {
                validate_hex_id(upload_id, "upload_id")?
            }
        }
        Ok(())
    }
}
impl DraftRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.api_version != HOST_API_V0_3 || self.draft_protocol != DRAFT_PROTOCOL {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "draft wire requires protocol1 and Host API0.3",
            ));
        }
        request_id(&self.request_id)?;
        self.resource.validate()?;
        validate_hex_id(&self.mount_id, "mount_id")?;
        if !capability_resource_valid(Capability::NoteDraftEdit, &self.resource, self.api_version) {
            return Err(invalid("draft request requires exact Draft resource"));
        }
        self.operation.validate()
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_DRAFT_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_DRAFT_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftResponse {
    pub api_version: ApiVersion,
    pub draft_protocol: u16,
    pub request_id: String,
    pub draft_id: String,
    pub mount_id: String,
    pub outcome: DraftOutcome,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DraftOutcome {
    Snapshot {
        revision: u64,
        offset: u32,
        total_bytes: u32,
        text: String,
    },
    UploadStarted {
        upload_id: String,
    },
    ChunkAccepted {
        upload_id: String,
        next_offset: u32,
    },
    Replaced {
        revision: u64,
    },
    Cancelled {
        upload_id: String,
    },
}
impl DraftResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.api_version != HOST_API_V0_3 || self.draft_protocol != DRAFT_PROTOCOL {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "unsupported draft response protocol",
            ));
        }
        request_id(&self.request_id)?;
        validate_hex_id(&self.draft_id, "draft_id")?;
        validate_hex_id(&self.mount_id, "mount_id")?;
        match &self.outcome {
            DraftOutcome::Snapshot {
                revision: rev,
                offset: start,
                total_bytes,
                text,
            } => {
                revision(*rev)?;
                offset(*start)?;
                offset(*total_bytes)?;
                if text.len() > MAX_DRAFT_CHUNK_BYTES
                    || start > total_bytes
                    || (*start as usize) + text.len() > *total_bytes as usize
                    || (text.is_empty() && start != total_bytes)
                {
                    return Err(invalid("invalid draft snapshot chunk"));
                }
            }
            DraftOutcome::UploadStarted { upload_id } | DraftOutcome::Cancelled { upload_id } => {
                validate_hex_id(upload_id, "upload_id")?
            }
            DraftOutcome::ChunkAccepted {
                upload_id,
                next_offset,
            } => {
                validate_hex_id(upload_id, "upload_id")?;
                offset(*next_offset)?;
            }
            DraftOutcome::Replaced { revision: rev } => revision(*rev)?,
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_DRAFT_RESPONSE_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_DRAFT_RESPONSE_BYTES, ErrorCode::InvalidRequest)
    }
}
