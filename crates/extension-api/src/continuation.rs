//! Explicit own-event continuation wire. Parsing neither applies effects nor proves atomicity.
//! `request_id` only correlates a reply and must be excluded from effect identity/hash.
//! Checkpoint CAS, event-owned upload, effect and completion must advance atomically
//! in the application. `base_revision` remains the event's original revision after
//! commit; last_effect reports the resulting revision for recovery. Completion is
//! allowed only with no active upload and a matching final draft revision.
//! New events retire old sequences. No metadata restores permission or a VM.
use crate::{
    ApiError, ApiVersion, Capability, ErrorCode, HOST_API_V0_3, Resource,
    capability_resource_valid, decode_json, draft::validate_hex_id, encode_json,
    validate_opaque_id,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
pub const CONTINUATION_PROTOCOL: u16 = 1;
pub const MAX_CHECKPOINT_BYTES: usize = 8192;
pub const MAX_CHECKPOINT_CHUNK_BYTES: usize = 2048;
pub const MAX_CONTINUATION_REQUEST_BYTES: usize = 65536;
pub const MAX_CONTINUATION_RESPONSE_BYTES: usize = 4096;
/// Encode bounded checkpoint bytes for the public wire; this grants no access.
pub fn encode_checkpoint(bytes: &[u8]) -> Result<String, ApiError> {
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(ApiError::new(
            ErrorCode::BudgetExceeded,
            "checkpoint exceeds budget",
        ));
    }
    Ok(STANDARD.encode(bytes))
}
fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}
/// Bounded allocation, strict standard padded base64, including canonical pad bits.
pub fn decode_checkpoint(encoded: &str, max_bytes: usize) -> Result<Vec<u8>, ApiError> {
    if max_bytes > MAX_CHECKPOINT_BYTES || encoded.len() > max_bytes.div_ceil(3) * 4 {
        return Err(invalid("checkpoint encoding exceeds budget"));
    }
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| invalid("invalid checkpoint base64"))?;
    if decoded.len() > max_bytes || STANDARD.encode(&decoded) != encoded {
        return Err(invalid("noncanonical checkpoint base64"));
    }
    Ok(decoded)
}
fn identity(
    api: ApiVersion,
    protocol: u16,
    request: &str,
    resource: &Resource,
    mount: &str,
    sequence: u64,
    revision: u64,
) -> Result<(), ApiError> {
    if !crate::is_supported_api(api) || api < HOST_API_V0_3 || protocol != CONTINUATION_PROTOCOL {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "continuation requires minimum Host API0.3 and protocol1",
        ));
    }
    if request.len() > 32 {
        return Err(invalid("request_id exceeds budget"));
    }
    validate_opaque_id(request, "request_id")?;
    resource.validate()?;
    validate_hex_id(mount, "mount_id")?;
    if !capability_resource_valid(Capability::NoteDraftEdit, resource, api)
        || sequence == 0
        || revision == 0
    {
        return Err(invalid("invalid continuation event identity"));
    }
    Ok(())
}
/// Effects operate only on this event's internally owned upload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventEffect {
    BeginReplace {
        expected_revision: u64,
        total_bytes: u32,
    },
    AppendChunk {
        offset: u32,
        text: String,
    },
    CommitReplace {},
    CancelReplace {},
}
impl EventEffect {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::BeginReplace {
                expected_revision,
                total_bytes,
            } => {
                if *expected_revision == 0 || *total_bytes > 1048576 {
                    return Err(invalid("invalid begin effect"));
                }
            }
            Self::AppendChunk { offset, text } => {
                if text.is_empty()
                    || text.len() > 512
                    || u64::from(*offset) + text.len() as u64 > 1048576
                {
                    return Err(invalid("invalid append effect"));
                }
            }
            Self::CommitReplace {} | Self::CancelReplace {} => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventEffectOutcome {
    Started {},
    ChunkAccepted { next_offset: u32 },
    Replaced { revision: u64 },
    Cancelled {},
}
impl EventEffectOutcome {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::ChunkAccepted { next_offset } if *next_offset > 1048576 => {
                Err(invalid("invalid effect offset"))
            }
            Self::Replaced { revision: 0 } => Err(invalid("invalid effect revision")),
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContinuationOperation {
    ReadMetadata {},
    ReadCheckpointChunk {
        checkpoint_revision: u64,
        offset: u16,
        max_bytes: u16,
    },
    AdvanceEvent {
        expected_checkpoint_revision: u64,
        checkpoint_base64: String,
        effect: Option<EventEffect>,
        complete: bool,
    },
    CancelEvent {
        expected_checkpoint_revision: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationRequest {
    pub api_version: ApiVersion,
    pub continuation_protocol: u16,
    pub request_id: String,
    pub resource: Resource,
    pub mount_id: String,
    pub sequence: u64,
    pub base_revision: u64,
    pub operation: ContinuationOperation,
}
impl ContinuationRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        identity(
            self.api_version,
            self.continuation_protocol,
            &self.request_id,
            &self.resource,
            &self.mount_id,
            self.sequence,
            self.base_revision,
        )?;
        match &self.operation {
            ContinuationOperation::ReadMetadata {} => {}
            ContinuationOperation::ReadCheckpointChunk {
                checkpoint_revision,
                offset,
                max_bytes,
            } => {
                if *checkpoint_revision == 0
                    || usize::from(*offset) > MAX_CHECKPOINT_BYTES
                    || *max_bytes == 0
                    || usize::from(*max_bytes) > MAX_CHECKPOINT_CHUNK_BYTES
                {
                    return Err(invalid("invalid checkpoint read bounds"));
                }
            }
            ContinuationOperation::AdvanceEvent {
                expected_checkpoint_revision,
                checkpoint_base64,
                effect,
                complete: _,
            } => {
                if *expected_checkpoint_revision == 0 {
                    return Err(invalid("checkpoint revision must be nonzero"));
                }
                decode_checkpoint(checkpoint_base64, MAX_CHECKPOINT_BYTES)?;
                if let Some(effect) = effect {
                    effect.validate()?;
                }
            }
            ContinuationOperation::CancelEvent {
                expected_checkpoint_revision,
            } => {
                if *expected_checkpoint_revision == 0 {
                    return Err(invalid("checkpoint revision must be nonzero"));
                }
            }
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_CONTINUATION_REQUEST_BYTES,
            ErrorCode::InvalidRequest,
        )?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(
            self,
            MAX_CONTINUATION_REQUEST_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationState {
    Active,
    Completed,
    Cancelled,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationMetadata {
    pub checkpoint_revision: u64,
    pub checkpoint_bytes: u16,
    pub state: ContinuationState,
    pub last_effect: Option<EventEffectOutcome>,
}
impl ContinuationMetadata {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.checkpoint_revision == 0
            || usize::from(self.checkpoint_bytes) > MAX_CHECKPOINT_BYTES
        {
            return Err(invalid("invalid checkpoint metadata"));
        }
        if let Some(effect) = &self.last_effect {
            effect.validate()?;
        }

        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContinuationOutcome {
    Metadata {
        metadata: ContinuationMetadata,
    },
    CheckpointChunk {
        checkpoint_revision: u64,
        offset: u16,
        total_bytes: u16,
        checkpoint_base64: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationResponse {
    pub api_version: ApiVersion,
    pub continuation_protocol: u16,
    pub request_id: String,
    pub resource: Resource,
    pub mount_id: String,
    pub sequence: u64,
    pub base_revision: u64,
    pub outcome: ContinuationOutcome,
}
impl ContinuationResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        identity(
            self.api_version,
            self.continuation_protocol,
            &self.request_id,
            &self.resource,
            &self.mount_id,
            self.sequence,
            self.base_revision,
        )?;
        match &self.outcome {
            ContinuationOutcome::Metadata { metadata } => metadata.validate()?,
            ContinuationOutcome::CheckpointChunk {
                checkpoint_revision,
                offset,
                total_bytes,
                checkpoint_base64,
            } => {
                let chunk = decode_checkpoint(checkpoint_base64, MAX_CHECKPOINT_CHUNK_BYTES)?;
                if *checkpoint_revision == 0
                    || usize::from(*total_bytes) > MAX_CHECKPOINT_BYTES
                    || offset > total_bytes
                    || usize::from(*offset) + chunk.len() > usize::from(*total_bytes)
                    || (chunk.is_empty() && offset != total_bytes)
                {
                    return Err(invalid("invalid checkpoint response bounds"));
                }
            }
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_CONTINUATION_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(
            self,
            MAX_CONTINUATION_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
