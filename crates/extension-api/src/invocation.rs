//! Read this invocation's bounded command parameters, never another invocation.
//! Returning parameters grants no capability, identity or native user approval.
//! The broker rechecks its trusted session/resource/operation binding before
//! returning bytes. Parameters use the command's own typed wire (SurfaceRequest
//! for a reader operation); they are not a generic metadata-read service.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_2, MAX_REQUEST_BYTES, decode_json, encode_json,
    validate_opaque_id,
};
use serde::{Deserialize, Serialize};
pub const INVOCATION_PROTOCOL: u16 = 1;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationRequest {
    pub invocation_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
}
impl InvocationRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.invocation_protocol != INVOCATION_PROTOCOL || self.api_version != HOST_API_V0_2 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "invocation parameters require Host API0.2",
            ));
        }
        validate_opaque_id(&self.request_id, "request_id")
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}
