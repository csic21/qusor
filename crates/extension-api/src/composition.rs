//! Explicit signed native composition and own-invocation context, never an authorization.
use crate::{
    ApiError, ApiVersion, Capability, ContributionKind, ErrorCode, HOST_API_V0_3, Manifest,
    Resource, decode_json,
    draft::{MAX_DRAFT_CHUNK_BYTES, MAX_DRAFT_TEXT_BYTES, validate_hex_id},
    encode_json,
};
use serde::{Deserialize, Serialize};
pub const COMPOSITION_PROTOCOL: u16 = 1;
pub const COMPONENT_CONTEXT_PROTOCOL: u16 = 1;
pub const MAX_COMPOSITION_BYTES: usize = 16 * 1024;
pub const MAX_CONTEXT_REQUEST_BYTES: usize = 4096;
pub const MAX_CONTEXT_RESPONSE_BYTES: usize = 4096;
/// Broker pending-input storage, independent of committed draft/upload budgets.
pub const MAX_PENDING_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_SESSION_PENDING_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const ANNOTATION_EDITOR_CONTRACT: &str = "annotation.plain-text-editor/v1";
fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}
fn version(api: ApiVersion, protocol: u16) -> Result<(), ApiError> {
    if !crate::is_supported_api(api) || api < HOST_API_V0_3 || protocol != 1 {
        Err(ApiError::new(
            ErrorCode::UnsupportedVersion,
            "component protocol requires version1 and minimum Host API0.3",
        ))
    } else {
        Ok(())
    }
}
fn request_id(id: &str) -> Result<(), ApiError> {
    if id.len() > 32 {
        return Err(invalid("request_id exceeds budget"));
    }
    crate::validate_opaque_id(id, "request_id")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentRole {
    #[serde(rename = "annotation.plain-text-editor")]
    AnnotationPlainTextEditor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeEditorView {
    TextEditor {
        label: String,
        placeholder: String,
    },
    ParagraphEditor {
        label: String,
        placeholder: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocols: Option<ParagraphProtocols>,
    },
}
/// Signed protocol support declaration; it grants no capability or approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphProtocols {
    pub component_invocation: u16,
    pub paragraph_projection: u16,
}
impl ParagraphProtocols {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.component_invocation != 1 || self.paragraph_projection != 1 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "unsupported paragraph protocols",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionDescriptor {
    pub composition_protocol: u16,
    pub api_version: ApiVersion,
    pub contribution_id: String,
    pub role: ComponentRole,
    pub contract: String,
    pub view: NativeEditorView,
}
impl CompositionDescriptor {
    /// Reports explicit valid support only; this does not authorize a mount.
    pub fn supports_paragraph_projection(&self) -> bool {
        matches!(&self.view, NativeEditorView::ParagraphEditor { protocols: Some(p), .. } if p.validate().is_ok())
    }
    pub fn validate(&self) -> Result<(), ApiError> {
        version(self.api_version, self.composition_protocol)?;
        if self.contract != ANNOTATION_EDITOR_CONTRACT {
            return Err(invalid("unsupported component contract"));
        }
        crate::validate_namespace(
            &self.contribution_id,
            crate::MAX_CONTRIBUTION_ID_BYTES,
            "contribution_id",
            ErrorCode::InvalidRequest,
        )?;
        if let NativeEditorView::ParagraphEditor {
            protocols: Some(p), ..
        } = &self.view
        {
            p.validate()?;
        }
        let (label, placeholder) = match &self.view {
            NativeEditorView::TextEditor { label, placeholder }
            | NativeEditorView::ParagraphEditor {
                label, placeholder, ..
            } => (label, placeholder),
        };
        if label.is_empty()
            || label.len() > 128
            || placeholder.len() > 256
            || label
                .chars()
                .chain(placeholder.chars())
                .any(char::is_control)
        {
            return Err(invalid("invalid bounded native editor labels"));
        }
        Ok(())
    }
    pub fn validate_for_manifest(&self, manifest: &Manifest) -> Result<(), ApiError> {
        self.validate()?;
        manifest.validate()?;
        if !manifest.supports_api(self.api_version)
            || !manifest.contributions.iter().any(|c| {
                c.id == self.contribution_id
                    && c.kind == ContributionKind::ComponentProvider
                    && c.required_capabilities.contains(&Capability::NoteDraftEdit)
            })
        {
            return Err(invalid(
                "composition does not bind a compatible declared provider",
            ));
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_COMPOSITION_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_COMPOSITION_BYTES, ErrorCode::InvalidRequest)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentAction {
    InputChanged { total_bytes: u32 },
    MoveParagraph { from: u32, to: u32 },
    ReplaceParagraph { index: u32, input_bytes: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextOperation {
    ReadContext {},
    ReadInputChunk {
        sequence: u64,
        expected_revision: u64,
        offset: u32,
        max_bytes: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentContextRequest {
    pub api_version: ApiVersion,
    pub context_protocol: u16,
    pub request_id: String,
    pub operation: ContextOperation,
}
impl ComponentContextRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        version(self.api_version, self.context_protocol)?;
        request_id(&self.request_id)?;
        if let ContextOperation::ReadInputChunk {
            sequence,
            expected_revision,
            offset,
            max_bytes,
        } = self.operation
            && (sequence == 0
                || expected_revision == 0
                || offset as usize > MAX_DRAFT_TEXT_BYTES
                || max_bytes == 0
                || usize::from(max_bytes) > MAX_DRAFT_CHUNK_BYTES)
        {
            return Err(invalid("invalid invocation input chunk"));
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self = decode_json(bytes, MAX_CONTEXT_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_CONTEXT_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextOutcome {
    Context {
        action: ComponentAction,
    },
    InputChunk {
        offset: u32,
        total_bytes: u32,
        text: String,
    },
}
/// Broker fills all identity fields from its current trusted run binding, never request claims.
/// Each mount has one latest <=1MiB input buffer, with <=8MiB across the session.
/// Access requires fresh own-mount draft authorization, not just a run identifier.
/// New events retire older sequence reads;
/// every read rechecks mount, installation, sequence and expected revision under the same gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentContextResponse {
    pub api_version: ApiVersion,
    pub context_protocol: u16,
    pub request_id: String,
    pub resource: Resource,
    pub mount_id: String,
    pub sequence: u64,
    pub expected_revision: u64,
    pub outcome: ContextOutcome,
}
impl ComponentContextResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        version(self.api_version, self.context_protocol)?;
        request_id(&self.request_id)?;
        self.resource.validate()?;
        validate_hex_id(&self.mount_id, "mount_id")?;
        if !matches!(self.resource, Resource::Draft { .. })
            || self.sequence == 0
            || self.expected_revision == 0
        {
            return Err(invalid("invalid invocation identity"));
        }
        match &self.outcome {
            ContextOutcome::Context {
                action: ComponentAction::InputChanged { total_bytes },
            } => {
                if *total_bytes as usize > MAX_DRAFT_TEXT_BYTES {
                    return Err(invalid("input exceeds text budget"));
                }
            }
            ContextOutcome::Context {
                action: ComponentAction::MoveParagraph { from, to },
            } => {
                if *from as usize > MAX_DRAFT_TEXT_BYTES || *to as usize > MAX_DRAFT_TEXT_BYTES {
                    return Err(invalid("paragraph index exceeds budget"));
                }
            }
            ContextOutcome::Context {
                action: ComponentAction::ReplaceParagraph { index, input_bytes },
            } => {
                if *index as usize > MAX_DRAFT_TEXT_BYTES
                    || *input_bytes as usize > MAX_DRAFT_TEXT_BYTES
                {
                    return Err(invalid("paragraph replacement exceeds budget"));
                }
            }
            ContextOutcome::InputChunk {
                offset,
                total_bytes,
                text,
            } => {
                if *total_bytes as usize > MAX_DRAFT_TEXT_BYTES
                    || offset > total_bytes
                    || text.len() > MAX_DRAFT_CHUNK_BYTES
                    || (*offset as usize) + text.len() > *total_bytes as usize
                    || (text.is_empty() && offset != total_bytes)
                {
                    return Err(invalid("invalid invocation input snapshot"));
                }
            }
        }
        Ok(())
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ApiError> {
        let value: Self =
            decode_json(bytes, MAX_CONTEXT_RESPONSE_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_CONTEXT_RESPONSE_BYTES, ErrorCode::InvalidRequest)
    }
}
