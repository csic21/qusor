//! Version 0.1 of the UI-independent extension policy wire contract.
//!
//! These DTOs describe requested capabilities, contribution registration, and an
//! authorization query. They do not execute plugin code or read/write knowledge.
//! A manifest's publisher identifier is a claim, not verified publisher identity.
//!
//! Use the bounded `from_slice` / `from_json` methods at untrusted JSON boundaries.
//! Serde deserialization rejects unknown fields and variants but callers using it
//! directly must also call `validate` before accepting the value. Host policy is
//! responsible for binding caller identity and checking grants and resource scope.

use std::{collections::BTreeSet, error::Error, fmt};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// The only request protocol implemented by this crate.
pub const HOST_API: ApiVersion = ApiVersion { major: 0, minor: 1 };
/// Maximum encoded manifest size, including JSON whitespace, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Maximum encoded policy request size, including JSON whitespace, in bytes.
pub const MAX_REQUEST_BYTES: usize = 4 * 1024;
/// Maximum namespace, request, or resource identifier length in ASCII bytes.
pub const MAX_ID_BYTES: usize = 128;
/// Contribution identifiers include their package namespace.
pub const MAX_CONTRIBUTION_ID_BYTES: usize = 192;
/// Maximum UTF-8 bytes in a contribution's display title.
pub const MAX_TITLE_BYTES: usize = 256;
/// Maximum number of contributions in one manifest.
pub const MAX_CONTRIBUTIONS: usize = 64;
/// Number of distinct capabilities in version 0.1.
pub const MAX_CAPABILITIES: usize = 3;
/// Maximum UTF-8 bytes in a public error message.
pub const MAX_ERROR_MESSAGE_BYTES: usize = 512;
/// Maximum encoded error response size in bytes.
pub const MAX_ERROR_BYTES: usize = 4 * 1024;

/// API protocol version; independent from package and manifest versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiVersion {
    pub major: u16,
    pub minor: u16,
}

/// Inclusive minor range within a single major protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRange {
    pub major: u16,
    pub min_minor: u16,
    pub max_minor: u16,
}

impl ApiRange {
    /// Returns false for an inverted range and for a different major version.
    pub fn supports(&self, version: ApiVersion) -> bool {
        self.major == version.major
            && self.min_minor <= version.minor
            && version.minor <= self.max_minor
    }
}

/// Numeric package version. Pre-release tags and build metadata are not part of 0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Recognized capability names; unknown names are never silently ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "selection.read")]
    SelectionRead,
    #[serde(rename = "document.metadata.read")]
    DocumentMetadataRead,
    #[serde(rename = "capture.propose")]
    CapturePropose,
}

/// Declarative entry types currently understood by the policy registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    Command,
    SelectionAction,
}

/// Metadata for a discoverable entry. No executable code or native object is carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contribution {
    pub id: String,
    pub title: String,
    pub kind: ContributionKind,
    pub required_capabilities: Vec<Capability>,
}

/// Policy manifest, not a complete installable package or a signature envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub manifest_version: u16,
    pub package_id: String,
    pub publisher_id: String,
    pub package_version: PackageVersion,
    pub host_api: ApiRange,
    pub required_capabilities: Vec<Capability>,
    pub optional_capabilities: Vec<Capability>,
    pub contributions: Vec<Contribution>,
}

impl Manifest {
    /// Validate structure, namespace containment, declarations, and manifest version.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.manifest_version != 1 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "unsupported manifest_version; expected 1",
            ));
        }
        let code = ErrorCode::InvalidManifest;
        validate_namespace(&self.package_id, MAX_ID_BYTES, "package_id", code)?;
        validate_namespace(&self.publisher_id, MAX_ID_BYTES, "publisher_id", code)?;
        if self.host_api.min_minor > self.host_api.max_minor {
            return Err(ApiError::new(code, "host_api range is inverted"));
        }

        let mut declared = validate_capabilities(&self.required_capabilities, code)?;
        for capability in validate_capabilities(&self.optional_capabilities, code)? {
            if !declared.insert(capability) {
                return Err(ApiError::new(
                    code,
                    "required and optional capabilities overlap",
                ));
            }
        }
        if self.contributions.len() > MAX_CONTRIBUTIONS {
            return Err(ApiError::new(code, "too many contributions"));
        }
        let prefix = format!("{}.", self.package_id);
        let mut contribution_ids = BTreeSet::new();
        for contribution in &self.contributions {
            validate_namespace(
                &contribution.id,
                MAX_CONTRIBUTION_ID_BYTES,
                "contribution id",
                code,
            )?;
            if !contribution.id.starts_with(&prefix) {
                return Err(ApiError::new(
                    code,
                    "contribution id must be inside its package namespace",
                ));
            }
            if !contribution_ids.insert(&contribution.id) {
                return Err(ApiError::new(code, "duplicate contribution id"));
            }
            validate_text(
                &contribution.title,
                MAX_TITLE_BYTES,
                "contribution title",
                code,
            )?;
            for capability in validate_capabilities(&contribution.required_capabilities, code)? {
                if !declared.contains(&capability) {
                    return Err(ApiError::new(
                        code,
                        "contribution requires an undeclared capability",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Decode bounded UTF-8 JSON and validate it before returning a manifest.
    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let manifest: Self = decode_json(json, MAX_MANIFEST_BYTES, ErrorCode::InvalidManifest)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Decode bounded JSON text and validate it before returning a manifest.
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    /// Validate and encode compact JSON within the same wire limit as decoding.
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_MANIFEST_BYTES, ErrorCode::InvalidManifest)
    }
}

/// Resource scope claimed by the request; the host must bind it to trusted context.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Resource {
    Vault {
        vault_id: String,
    },
    Document {
        vault_id: String,
        document_id: String,
    },
    Selection {
        vault_id: String,
        selection_id: String,
    },
}

impl Resource {
    pub fn vault_id(&self) -> &str {
        match self {
            Self::Vault { vault_id }
            | Self::Document { vault_id, .. }
            | Self::Selection { vault_id, .. } => vault_id,
        }
    }

    /// Check bounded opaque identifiers; existence and ownership require host checks.
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(self.vault_id(), "vault_id")?;
        match self {
            Self::Vault { .. } => Ok(()),
            Self::Document { document_id, .. } => validate_opaque_id(document_id, "document_id"),
            Self::Selection { selection_id, .. } => {
                validate_opaque_id(selection_id, "selection_id")
            }
        }
    }
}

/// A policy authorization query, without plugin-supplied actor or session identity.
///
/// This does not perform the operation named by the capability. The host checks
/// resource/capability compatibility as well as actual grants and caller context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequest {
    pub api_version: ApiVersion,
    pub request_id: String,
    pub capability: Capability,
    pub resource: Resource,
}

impl CapabilityRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.api_version != HOST_API {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "unsupported request api_version; expected 0.1",
            ));
        }
        validate_opaque_id(&self.request_id, "request_id")?;
        self.resource.validate()
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

/// Stable machine-readable errors; message text must not be used for branching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidManifest,
    InvalidRequest,
    UnsupportedVersion,
    CapabilityUnavailable,
    PermissionDenied,
    StaleContext,
    NotFound,
    AlreadyInstalled,
    Disabled,
    InvalidState,
    BudgetExceeded,
}

/// Public diagnostics. Callers should provide safe text, without credentials or data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn validate(&self) -> Result<(), Self> {
        validate_text(
            &self.message,
            MAX_ERROR_MESSAGE_BYTES,
            "error message",
            ErrorCode::InvalidRequest,
        )
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, Self> {
        let error: Self = decode_json(json, MAX_ERROR_BYTES, ErrorCode::InvalidRequest)?;
        error.validate()?;
        Ok(error)
    }

    pub fn from_json(json: &str) -> Result<Self, Self> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, Self> {
        self.validate()?;
        encode_json(self, MAX_ERROR_BYTES, ErrorCode::InvalidRequest)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ApiError {}

fn validate_capabilities(
    capabilities: &[Capability],
    code: ErrorCode,
) -> Result<BTreeSet<Capability>, ApiError> {
    if capabilities.len() > MAX_CAPABILITIES {
        return Err(ApiError::new(code, "too many capabilities"));
    }
    let unique: BTreeSet<_> = capabilities.iter().copied().collect();
    if unique.len() != capabilities.len() {
        return Err(ApiError::new(code, "duplicate capability"));
    }
    Ok(unique)
}

fn validate_namespace(
    value: &str,
    limit: usize,
    field: &str,
    code: ErrorCode,
) -> Result<(), ApiError> {
    let valid_segment = |segment: &str| {
        segment
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    };
    if value.len() > limit || !value.contains('.') || !value.split('.').all(valid_segment) {
        return Err(ApiError::new(code, format!("invalid {field} namespace")));
    }
    Ok(())
}

fn validate_opaque_id(value: &str, field: &str) -> Result<(), ApiError> {
    if value.len() > MAX_ID_BYTES
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid {field}"),
        ));
    }
    Ok(())
}

fn validate_text(value: &str, limit: usize, field: &str, code: ErrorCode) -> Result<(), ApiError> {
    if value.len() > limit || value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ApiError::new(code, format!("invalid {field}")));
    }
    Ok(())
}

fn decode_json<T: DeserializeOwned>(
    json: &[u8],
    limit: usize,
    code: ErrorCode,
) -> Result<T, ApiError> {
    if json.len() > limit {
        return Err(ApiError::new(
            ErrorCode::BudgetExceeded,
            "JSON exceeds wire byte limit",
        ));
    }
    // Do not echo serde diagnostics: unknown keys and values are untrusted text
    // that may contain sensitive payloads. Error categories remain machine-readable.
    serde_json::from_slice(json).map_err(|_| ApiError::new(code, "invalid JSON shape or value"))
}

fn encode_json<T: Serialize>(value: &T, limit: usize, code: ErrorCode) -> Result<String, ApiError> {
    let json =
        serde_json::to_string(value).map_err(|_| ApiError::new(code, "could not encode JSON"))?;
    if json.len() > limit {
        return Err(ApiError::new(
            ErrorCode::BudgetExceeded,
            "JSON exceeds wire byte limit",
        ));
    }
    Ok(json)
}
