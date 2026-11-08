//! Versioned controlled-reader messages. Raw input and frame readiness are never
//! guest operations. Host policy binds identity, grants, sources and native leases.
use crate::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_2, MAX_REQUEST_BYTES, Resource, decode_json,
    encode_json, validate_namespace, validate_opaque_id,
};
use serde::{Deserialize, Serialize};

pub const SURFACE_PROTOCOL: u16 = 1;
pub const MAX_SURFACE_BYTES: usize = MAX_REQUEST_BYTES;
pub const MIN_SURFACE_ZOOM: f64 = 0.0625;
pub const MAX_SURFACE_ZOOM: f64 = 64.0;
pub const MAX_FRAME_EDGE: u32 = 8192;

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}
fn protocol(surface_protocol: u16, api_version: ApiVersion) -> Result<(), ApiError> {
    if surface_protocol != SURFACE_PROTOCOL || api_version != HOST_API_V0_2 {
        return Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "Surface requires protocol1 and Host API0.2",
        ));
    }
    Ok(())
}
fn document(resource: &Resource) -> Result<(), ApiError> {
    resource.validate()?;
    if !matches!(resource, Resource::Document { .. }) {
        return Err(invalid("Surface requires an exact document resource"));
    }
    Ok(())
}
fn generation(value: u64) -> Result<(), ApiError> {
    if value == 0 {
        return Err(invalid("Surface generation must be positive"));
    }
    Ok(())
}
pub fn validate_zoom(value: f64) -> Result<(), ApiError> {
    if !value.is_finite() || !(MIN_SURFACE_ZOOM..=MAX_SURFACE_ZOOM).contains(&value) {
        return Err(invalid("invalid Surface zoom"));
    }
    Ok(())
}

/// Semantic source identity; independent from page image/frame hashes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersion {
    pub original_sha256: String,
    pub extraction_revision: String,
}
impl SourceVersion {
    pub fn validate(&self) -> Result<(), ApiError> {
        for digest in [&self.original_sha256, &self.extraction_revision] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("invalid source digest"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRequest {
    pub surface_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub operation: SurfaceOperation,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceOperation {
    Open {
        resource: Resource,
        expected: SourceVersion,
        contribution_id: String,
    },
    Configure {
        handle: String,
        generation: u64,
        page_index: u32,
        zoom: f64,
    },
    ReadSelection {
        handle: String,
        generation: u64,
        selection_lease: String,
    },
    Close {
        handle: String,
    },
}
impl SurfaceRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        protocol(self.surface_protocol, self.api_version)?;
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.operation {
            SurfaceOperation::Open {
                resource,
                expected,
                contribution_id,
            } => {
                document(resource)?;
                expected.validate()?;
                validate_namespace(
                    contribution_id,
                    crate::MAX_CONTRIBUTION_ID_BYTES,
                    "contribution_id",
                    ErrorCode::InvalidRequest,
                )?;
            }
            SurfaceOperation::Configure {
                handle,
                generation: value,
                zoom,
                ..
            } => {
                validate_opaque_id(handle, "surface handle")?;
                generation(*value)?;
                validate_zoom(*zoom)?;
            }
            SurfaceOperation::ReadSelection {
                handle,
                generation: value,
                selection_lease,
            } => {
                validate_opaque_id(handle, "surface handle")?;
                generation(*value)?;
                validate_opaque_id(selection_lease, "selection lease")?;
            }
            SurfaceOperation::Close { handle } => validate_opaque_id(handle, "surface handle")?,
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_SURFACE_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_SURFACE_BYTES, ErrorCode::InvalidRequest)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceStatus {
    Pending,
    Ready,
    Closing,
    Closed,
    Failed,
}
/// Public bounded display metadata, not a native frame-ready ticket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameMetadata {
    pub frame_id: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}
impl FrameMetadata {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.frame_id, "frame id")?;
        if self.pixel_width == 0
            || self.pixel_height == 0
            || self.pixel_width > MAX_FRAME_EDGE
            || self.pixel_height > MAX_FRAME_EDGE
        {
            return Err(invalid("invalid native frame dimensions"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceDescriptor {
    pub handle: String,
    pub resource: Resource,
    pub source: SourceVersion,
    pub generation: u64,
    pub page_index: u32,
    pub page_count: u32,
    pub zoom: f64,
    pub status: SurfaceStatus,
    pub frame: Option<FrameMetadata>,
    pub selection_lease: Option<String>,
}
impl SurfaceDescriptor {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.handle, "surface handle")?;
        document(&self.resource)?;
        self.source.validate()?;
        generation(self.generation)?;
        validate_zoom(self.zoom)?;
        if self.page_count == 0 || self.page_index >= self.page_count {
            return Err(invalid("invalid Surface page range"));
        }
        if let Some(frame) = &self.frame {
            frame.validate()?;
        }
        if let Some(lease) = &self.selection_lease {
            validate_opaque_id(lease, "selection lease")?;
        }
        match self.status {
            SurfaceStatus::Ready if self.frame.is_some() => (),
            SurfaceStatus::Ready => return Err(invalid("ready Surface requires a frame")),
            _ if self.frame.is_some() || self.selection_lease.is_some() => {
                return Err(invalid("inactive Surface cannot expose frame or selection"));
            }
            _ => (),
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResponse {
    pub surface_protocol: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub result: SurfaceResult,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceResult {
    State {
        surface: SurfaceDescriptor,
    },
    /// Entire canonical Core draft; never a truncated first-segment projection.
    /// Native service validates the capture contract before publishing a lease.
    Selection {
        handle: String,
        generation: u64,
        selection_lease: String,
        capture_protocol: u16,
        draft_json: String,
    },
}
impl SurfaceResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        protocol(self.surface_protocol, self.api_version)?;
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.result {
            SurfaceResult::State { surface } => surface.validate()?,
            SurfaceResult::Selection {
                handle,
                generation: value,
                selection_lease,
                capture_protocol,
                draft_json,
            } => {
                validate_opaque_id(handle, "surface handle")?;
                generation(*value)?;
                validate_opaque_id(selection_lease, "selection lease")?;
                if ![1, 2].contains(capture_protocol) {
                    return Err(ApiError::new(
                        ErrorCode::UnsupportedVersion,
                        "unsupported capture protocol",
                    ));
                }
                // The API treats this as an opaque contract-specific payload. It
                // never converts it through Value or accepts it as a source fact.
                if draft_json.trim().is_empty() {
                    return Err(invalid("invalid complete selection payload"));
                }
                if draft_json.len() > MAX_SURFACE_BYTES {
                    return Err(ApiError::new(
                        ErrorCode::BudgetExceeded,
                        "complete selection payload exceeds byte budget",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_SURFACE_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_SURFACE_BYTES, ErrorCode::InvalidRequest)
    }
}
