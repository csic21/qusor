//! Read-only provider projection. Bounds and identity validation do not prove
//! paragraph semantics, EOF honesty, authority, or freshness. A registry must
//! validate windows/metadata against its trusted context before publication.
use crate::{
    ApiError, ApiVersion, Capability, ErrorCode, HOST_API_V0_3, Resource,
    capability_resource_valid,
    continuation::{MAX_CHECKPOINT_BYTES, MAX_CHECKPOINT_CHUNK_BYTES, decode_checkpoint},
    decode_json,
    draft::validate_hex_id,
    encode_json, validate_opaque_id,
};
use serde::{Deserialize, Deserializer, Serialize};

// An explicit null is legal; omission is not.
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub const PROJECTION_PROTOCOL: u16 = 1;
pub const MAX_PROJECTION_REQUEST_BYTES: usize = 65536;
pub const MAX_PROJECTION_RESPONSE_BYTES: usize = 4096;
pub const MAX_PROJECTION_ROWS: usize = 8;
pub const MAX_PROJECTION_TEXT_BYTES: u32 = 1048576;
fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}
fn header(api: ApiVersion, protocol: u16, request: &str) -> Result<(), ApiError> {
    if !crate::is_supported_api(api) || api < HOST_API_V0_3 || protocol != PROJECTION_PROTOCOL {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "projection requires minimum API0.3 protocol1",
        ));
    }
    if request.len() > 32 {
        return Err(invalid("request_id exceeds budget"));
    }
    validate_opaque_id(request, "request_id")
}
fn binding(resource: &Resource, mount: &str, query: &str, revision: u64) -> Result<(), ApiError> {
    resource.validate()?;
    validate_hex_id(mount, "mount_id")?;
    validate_hex_id(query, "query_id")?;
    if revision == 0
        || !capability_resource_valid(Capability::NoteDraftEdit, resource, HOST_API_V0_3)
    {
        return Err(invalid("invalid projection binding"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionContext {
    pub resource: Resource,
    pub mount_id: String,
    pub query_id: String,
    pub base_revision: u64,
    pub query_generation: u64,
    pub first_paragraph: u32,
    pub max_rows: u8,
    pub total_bytes: u32,
}
impl ProjectionContext {
    pub fn validate(&self) -> Result<(), ApiError> {
        binding(
            &self.resource,
            &self.mount_id,
            &self.query_id,
            self.base_revision,
        )?;
        if self.query_generation == 0
            || self.max_rows == 0
            || usize::from(self.max_rows) > MAX_PROJECTION_ROWS
            || self.first_paragraph > MAX_PROJECTION_TEXT_BYTES
            || self.total_bytes > MAX_PROJECTION_TEXT_BYTES
        {
            return Err(invalid("invalid projection context bounds"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowSpan {
    pub ordinal: u32,
    pub start: u32,
    pub end: u32,
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionWindow {
    pub rows: Vec<RowSpan>,
    pub reached_eof: bool,
}
impl ProjectionWindow {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.rows.len() > MAX_PROJECTION_ROWS {
            return Err(invalid("too many projection rows"));
        }
        for (i, row) in self.rows.iter().enumerate() {
            if row.ordinal > MAX_PROJECTION_TEXT_BYTES
                || row.start > row.end
                || row.end > MAX_PROJECTION_TEXT_BYTES
                || (!row.complete && (i + 1 != self.rows.len() || self.reached_eof))
            {
                return Err(invalid("invalid projection row"));
            }
            if i > 0 {
                let previous = &self.rows[i - 1];
                if previous.ordinal.checked_add(1) != Some(row.ordinal) || previous.end > row.start
                {
                    return Err(invalid("unordered projection rows"));
                }
            }
        }
        if self.rows.is_empty() && !self.reached_eof {
            return Err(invalid("empty projection requires EOF"));
        }
        Ok(())
    }
    /// Returns derived readiness. UTF-8 boundaries require trusted source bytes;
    /// ordinal and EOF semantic correctness remain the provider's responsibility.
    pub fn validate_for_context(&self, context: &ProjectionContext) -> Result<bool, ApiError> {
        context.validate()?;
        self.validate()?;
        if self.rows.len() > usize::from(context.max_rows)
            || self
                .rows
                .first()
                .is_some_and(|r| r.ordinal != context.first_paragraph)
            || self.rows.iter().any(|r| r.end > context.total_bytes)
            || (self.rows.is_empty() && context.first_paragraph == 0)
        {
            return Err(invalid("window does not match context"));
        }
        if context.total_bytes == 0
            && context.first_paragraph == 0
            && (self.rows.len() != 1
                || self.rows[0]
                    != (RowSpan {
                        ordinal: 0,
                        start: 0,
                        end: 0,
                        complete: true,
                    })
                || !self.reached_eof)
        {
            return Err(invalid("empty draft requires one complete empty row"));
        }
        Ok(self.rows.iter().all(|r| r.complete)
            && (self.rows.len() == usize::from(context.max_rows) || self.reached_eof))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionState {
    Active,
    Completed,
    Cancelled,
    Invalidated,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionMetadata {
    pub checkpoint_revision: u64,
    pub checkpoint_bytes: u16,
    pub state: ProjectionState,
    #[serde(deserialize_with = "required_option")]
    pub window: Option<ProjectionWindow>,
    pub window_ready: bool,
}
impl ProjectionMetadata {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.checkpoint_revision == 0
            || usize::from(self.checkpoint_bytes) > MAX_CHECKPOINT_BYTES
        {
            return Err(invalid("invalid projection checkpoint"));
        }
        if let Some(w) = &self.window {
            w.validate()?;
        }
        if self.window.is_none() && self.window_ready
            || self.state == ProjectionState::Completed && !self.window_ready
        {
            return Err(invalid("invalid projection readiness"));
        }
        Ok(())
    }
    pub fn validate_for_context(&self, context: &ProjectionContext) -> Result<(), ApiError> {
        context.validate()?;
        self.validate()?;
        let ready = self
            .window
            .as_ref()
            .map(|w| w.validate_for_context(context))
            .transpose()?
            .unwrap_or(false);
        if ready != self.window_ready {
            return Err(invalid("projection readiness mismatch"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionOperation {
    ReadMetadata {},
    ReadCheckpointChunk {
        checkpoint_revision: u64,
        offset: u16,
        max_bytes: u16,
    },
    AdvanceProjection {
        expected_checkpoint_revision: u64,
        checkpoint_base64: String,
        #[serde(deserialize_with = "required_option")]
        window: Option<ProjectionWindow>,
        complete: bool,
    },
    Cancel {
        expected_checkpoint_revision: u64,
    },
}
impl ProjectionOperation {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::ReadMetadata {} => {}
            Self::ReadCheckpointChunk {
                checkpoint_revision,
                offset,
                max_bytes,
            } => {
                if *checkpoint_revision == 0
                    || usize::from(*offset) > MAX_CHECKPOINT_BYTES
                    || *max_bytes == 0
                    || usize::from(*max_bytes) > MAX_CHECKPOINT_CHUNK_BYTES
                {
                    return Err(invalid("invalid checkpoint read"));
                }
            }
            Self::AdvanceProjection {
                expected_checkpoint_revision,
                checkpoint_base64,
                window,
                ..
            } => {
                if *expected_checkpoint_revision == 0 {
                    return Err(invalid("zero checkpoint revision"));
                }
                decode_checkpoint(checkpoint_base64, MAX_CHECKPOINT_BYTES)?;
                if let Some(w) = window {
                    w.validate()?;
                }
            }
            Self::Cancel {
                expected_checkpoint_revision,
            } => {
                if *expected_checkpoint_revision == 0 {
                    return Err(invalid("zero checkpoint revision"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionRequest {
    ReadContext {
        api_version: ApiVersion,
        projection_protocol: u16,
        request_id: String,
    },
    Query {
        api_version: ApiVersion,
        projection_protocol: u16,
        request_id: String,
        resource: Resource,
        mount_id: String,
        query_id: String,
        base_revision: u64,
        operation: ProjectionOperation,
    },
}
impl ProjectionRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::ReadContext {
                api_version,
                projection_protocol,
                request_id,
            } => header(*api_version, *projection_protocol, request_id),
            Self::Query {
                api_version,
                projection_protocol,
                request_id,
                resource,
                mount_id,
                query_id,
                base_revision,
                operation,
            } => {
                header(*api_version, *projection_protocol, request_id)?;
                binding(resource, mount_id, query_id, *base_revision)?;
                operation.validate()
            }
        }
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_PROJECTION_REQUEST_BYTES,
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
            MAX_PROJECTION_REQUEST_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionOutcome {
    Metadata {
        metadata: ProjectionMetadata,
    },
    CheckpointChunk {
        checkpoint_revision: u64,
        offset: u16,
        total_bytes: u16,
        checkpoint_base64: String,
    },
}
impl ProjectionOutcome {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::Metadata { metadata } => metadata.validate(),
            Self::CheckpointChunk {
                checkpoint_revision,
                offset,
                total_bytes,
                checkpoint_base64,
            } => {
                let bytes = decode_checkpoint(checkpoint_base64, MAX_CHECKPOINT_CHUNK_BYTES)?;
                if *checkpoint_revision == 0
                    || usize::from(*total_bytes) > MAX_CHECKPOINT_BYTES
                    || offset > total_bytes
                    || usize::from(*offset) + bytes.len() > usize::from(*total_bytes)
                    || (bytes.is_empty() && offset != total_bytes)
                {
                    return Err(invalid("invalid checkpoint chunk"));
                }
                Ok(())
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionResponse {
    Context {
        api_version: ApiVersion,
        projection_protocol: u16,
        request_id: String,
        context: ProjectionContext,
    },
    Query {
        api_version: ApiVersion,
        projection_protocol: u16,
        request_id: String,
        resource: Resource,
        mount_id: String,
        query_id: String,
        base_revision: u64,
        outcome: ProjectionOutcome,
    },
}
impl ProjectionResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        match self {
            Self::Context {
                api_version,
                projection_protocol,
                request_id,
                context,
            } => {
                header(*api_version, *projection_protocol, request_id)?;
                context.validate()
            }
            Self::Query {
                api_version,
                projection_protocol,
                request_id,
                resource,
                mount_id,
                query_id,
                base_revision,
                outcome,
            } => {
                header(*api_version, *projection_protocol, request_id)?;
                binding(resource, mount_id, query_id, *base_revision)?;
                outcome.validate()
            }
        }
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_PROJECTION_RESPONSE_BYTES,
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
            MAX_PROJECTION_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
