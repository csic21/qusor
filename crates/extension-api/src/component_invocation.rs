//! API0.3 component-purpose bootstrap. This reveals only the trusted run purpose,
//! never identity, text, grants or user approval. Legacy invocation/context wire
//! remains unchanged; a provider must explicitly support this new entry point.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_3, decode_json, encode_json, validate_opaque_id,
};
use serde::{Deserialize, Serialize};
pub const COMPONENT_INVOCATION_PROTOCOL: u16 = 1;
pub const MAX_COMPONENT_INVOCATION_REQUEST_BYTES: usize = 65536;
pub const MAX_COMPONENT_INVOCATION_RESPONSE_BYTES: usize = 4096;
fn header(api: ApiVersion, protocol: u16, request: &str) -> Result<(), ApiError> {
    if api != HOST_API_V0_3 || protocol != COMPONENT_INVOCATION_PROTOCOL {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "component invocation requires API0.3 protocol1",
        ));
    }
    if request.len() > 32 {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "request_id exceeds budget",
        ));
    }
    validate_opaque_id(request, "request_id")
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentInvocationOperation {
    ReadContext {},
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentInvocationRequest {
    pub api_version: ApiVersion,
    pub component_invocation_protocol: u16,
    pub request_id: String,
    pub operation: ComponentInvocationOperation,
}
impl ComponentInvocationRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        header(
            self.api_version,
            self.component_invocation_protocol,
            &self.request_id,
        )
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_COMPONENT_INVOCATION_REQUEST_BYTES,
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
            MAX_COMPONENT_INVOCATION_REQUEST_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentInvocationContext {
    EditEvent {},
    ParagraphProjection {},
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentInvocationResponse {
    pub api_version: ApiVersion,
    pub component_invocation_protocol: u16,
    pub request_id: String,
    pub context: ComponentInvocationContext,
}
impl ComponentInvocationResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        header(
            self.api_version,
            self.component_invocation_protocol,
            &self.request_id,
        )
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(
            bytes,
            MAX_COMPONENT_INVOCATION_RESPONSE_BYTES,
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
            MAX_COMPONENT_INVOCATION_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}
