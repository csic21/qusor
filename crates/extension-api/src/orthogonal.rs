//! Orthogonal namespaced capabilities, per-capability semver negotiation types,
//! service endpoint declarations, and the bounded Core Knowledge wire (`0.8+`).
//!
//! Host policy uses these contracts to replace domain-specific enum proliferation
//! (ADR 0049 Part 2) while strictly separating read-only knowledge access from
//! mutation (`core.knowledge.write`) and keeping provenance host-authenticated.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ApiError, ApiVersion, Capability, ErrorCode, HOST_API_V0_8, MAX_ID_BYTES, decode_json,
    encode_json, notes::NoteField, validate_opaque_id,
};

/// Wire protocol version for orthogonal Core Knowledge requests and responses.
pub const CORE_KNOWLEDGE_PROTOCOL: u16 = 1;

/// Standard orthogonal capability constants (`major = 1`).
pub const CAP_CORE_KNOWLEDGE_READ: &str = "core.knowledge.read";
pub const CAP_CORE_KNOWLEDGE_WRITE: &str = "core.knowledge.write";
pub const CAP_CORE_CAPTURE_PROPOSE: &str = "core.capture.propose";
pub const CAP_CORE_VOCAB_STUDY: &str = "core.vocab.study";
pub const CAP_HOST_UI_SLOT: &str = "host.ui.slot";
pub const CAP_HOST_STORAGE_KV: &str = "host.storage.kv";
pub const CAP_NATIVE_DOCUMENT_RENDER: &str = "native.document.render";

/// Closed registry of standard host/core orthogonal capability identifiers.
pub const STANDARD_ORTHOGONAL_CAPABILITIES: [&str; 7] = [
    CAP_CORE_KNOWLEDGE_READ,
    CAP_CORE_KNOWLEDGE_WRITE,
    CAP_CORE_CAPTURE_PROPOSE,
    CAP_CORE_VOCAB_STUDY,
    CAP_HOST_UI_SLOT,
    CAP_HOST_STORAGE_KV,
    CAP_NATIVE_DOCUMENT_RENDER,
];

/// Maximum orthogonal capability/requirement declarations in one manifest.
pub const MAX_ORTHOGONAL_CAPABILITIES: usize = 64;
/// Maximum service endpoints provided by one manifest.
pub const MAX_SERVICE_ENDPOINTS: usize = 32;

/// Bounded Core Knowledge wire limits (independent from internal storage limits).
pub const MAX_KNOWLEDGE_REQUEST_BYTES: usize = 8 * 1024;
pub const MAX_KNOWLEDGE_RESPONSE_BYTES: usize = 16 * 1024;
pub const MAX_KNOWLEDGE_PAGE: u16 = 16;
pub const MAX_KNOWLEDGE_TITLE_BYTES: usize = 128;
pub const MAX_KNOWLEDGE_PREVIEW_BYTES: usize = 96;
pub const MAX_KNOWLEDGE_CURSOR_BYTES: usize = 128;
pub const MAX_KNOWLEDGE_SECTIONS: u32 = 4096;
pub const MAX_KNOWLEDGE_TOC_ENTRIES: usize = 256;
pub const MAX_KNOWLEDGE_TOC_DEPTH: u8 = 16;
pub const MIN_KNOWLEDGE_CHUNK_BYTES: u16 = 4;
pub const MAX_KNOWLEDGE_CHUNK_BYTES: u16 = 2048;
pub const MAX_KNOWLEDGE_TEXT_OFFSET: u32 = 16 * 1024 * 1024;
pub const MAX_KNOWLEDGE_SELECTION_SCALARS: u32 = 100_000;
pub const MAX_KNOWLEDGE_SAVED_SELECTIONS: usize = 64;
pub const MAX_KNOWLEDGE_QUOTE_BYTES: usize = 1024;
pub const MAX_KNOWLEDGE_NOTE_BYTES: usize = 2048;

/// Maximum opaque frame size routed through `ExtensionRouter`.
pub const MAX_SERVICE_FRAME_BYTES: usize = 16 * 1024;

/// Strictly validated orthogonal capability or extension service identifier.
///
/// Only the 7 standard host/core capabilities or `ext.<provider_ns>.<endpoint>`
/// extension namespaces are accepted; unknown namespaces fail closed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Parse and validate an orthogonal capability identifier, failing closed on
    /// syntax violations or unknown non-`ext.` namespaces.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ApiError> {
        Self::parse_with_code(value.as_ref(), ErrorCode::InvalidManifest)
    }

    /// Parse and validate using an explicit error code (`InvalidManifest` or `InvalidRequest`).
    pub fn parse_with_code(value: &str, code: ErrorCode) -> Result<Self, ApiError> {
        validate_capability_id_str(value, code)?;
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_standard(&self) -> bool {
        STANDARD_ORTHOGONAL_CAPABILITIES.contains(&self.0.as_str())
    }

    pub fn is_extension(&self) -> bool {
        self.0.starts_with("ext.")
    }

    pub fn validate_with(&self, code: ErrorCode) -> Result<(), ApiError> {
        validate_capability_id_str(&self.0, code)
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CapabilityId {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl AsRef<str> for CapabilityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for CapabilityId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for CapabilityId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

fn validate_capability_id_str(value: &str, code: ErrorCode) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES {
        return Err(ApiError::new(
            code,
            "capability id length is outside supported bounds",
        ));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err(ApiError::new(
            code,
            "capability id must use lowercase ASCII letters, digits, '.', '-', or '_'",
        ));
    }
    let first = value.as_bytes().first().copied();
    let last = value.as_bytes().last().copied();
    if first.is_some_and(|b| matches!(b, b'.' | b'-' | b'_'))
        || last.is_some_and(|b| matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(ApiError::new(
            code,
            "capability id must not start or end with a separator",
        ));
    }
    if value.contains("..") || value.contains("--") || value.contains("__") {
        return Err(ApiError::new(
            code,
            "capability id must not contain adjacent separators",
        ));
    }
    let segments: Vec<&str> = value.split('.').collect();
    if segments.len() < 2
        || segments.iter().any(|segment| {
            let bytes = segment.as_bytes();
            bytes.first().is_none_or(|b| !b.is_ascii_lowercase())
                || bytes
                    .last()
                    .is_none_or(|b| !b.is_ascii_lowercase() && !b.is_ascii_digit())
        })
    {
        return Err(ApiError::new(
            code,
            "capability id has invalid dotted segment syntax",
        ));
    }
    if STANDARD_ORTHOGONAL_CAPABILITIES.contains(&value) {
        return Ok(());
    }
    if segments[0] == "ext" && segments.len() >= 3 {
        return Ok(());
    }
    Err(ApiError::new(code, "unknown orthogonal capability id"))
}

/// Per-capability semver requirement declared in a manifest or service consumer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub id: CapabilityId,
    pub major: u16,
    pub min_minor: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_minor: Option<u16>,
    #[serde(default)]
    pub optional: bool,
}

impl CapabilityRequirement {
    pub fn validate(&self, code: ErrorCode) -> Result<(), ApiError> {
        self.id.validate_with(code)?;
        if let Some(max_minor) = self.max_minor
            && max_minor < self.min_minor
        {
            return Err(ApiError::new(
                code,
                "capability requirement minor range is inverted",
            ));
        }
        Ok(())
    }

    /// Returns whether a concrete `(major, minor)` version satisfies this requirement.
    pub fn matches_version(&self, major: u16, minor: u16) -> bool {
        self.major == major
            && self.min_minor <= minor
            && self.max_minor.is_none_or(|max| minor <= max)
    }

    /// Negotiate the highest minor within `major` compatible with both this
    /// requirement and a provider/host supporting up to `supported_max_minor`.
    pub fn negotiate_minor(&self, supported_major: u16, supported_max_minor: u16) -> Option<u16> {
        if self.major != supported_major || supported_max_minor < self.min_minor {
            return None;
        }
        Some(
            self.max_minor
                .map_or(supported_max_minor, |max| max.min(supported_max_minor)),
        )
    }
}

/// Service endpoint exported by a provider manifest (`provides`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceEndpoint {
    pub service: CapabilityId,
    pub major: u16,
    pub minor: u16,
}

impl ServiceEndpoint {
    pub fn validate(&self, code: ErrorCode) -> Result<(), ApiError> {
        self.service.validate_with(code)
    }

    pub fn matches_requirement(&self, req: &CapabilityRequirement) -> bool {
        self.service == req.id && req.matches_version(self.major, self.minor)
    }
}

/// Project one legacy v1 `Capability` into its read-safe orthogonal primitive.
///
/// Read-only capabilities never map to `core.knowledge.write`.
pub fn project_legacy_capability(capability: Capability) -> &'static str {
    match capability {
        Capability::SelectionRead
        | Capability::DocumentMetadataRead
        | Capability::NotesRead
        | Capability::LibraryRead => CAP_CORE_KNOWLEDGE_READ,
        Capability::CapturePropose | Capability::NoteDraftEdit => CAP_CORE_CAPTURE_PROPOSE,
        Capability::VocabRead | Capability::VocabReview => CAP_CORE_VOCAB_STUDY,
        Capability::ReaderSurface => CAP_NATIVE_DOCUMENT_RENDER,
        Capability::UiSlot => CAP_HOST_UI_SLOT,
    }
}

/// Document format supported by the orthogonal Core Knowledge wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFormat {
    Pdf,
    Epub,
}

/// Bounded orthogonal Core Knowledge request; the Host binds vault and provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreKnowledgeRequest {
    pub protocol_version: u16,
    pub api_version: ApiVersion,
    pub request_id: String,
    pub operation: CoreKnowledgeOperation,
}

/// Orthogonal Core Knowledge operations partitioned by required capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CoreKnowledgeOperation {
    /// Paginated document summaries (`core.knowledge.read`).
    DocumentsPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<DocumentFormat>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        limit: u16,
    },
    /// Bounded table of contents for a document (`core.knowledge.read`).
    DocumentToc {
        format: DocumentFormat,
        document_id: i64,
    },
    /// Bounded UTF-8 text chunk for a PDF page or EPUB chapter (`core.knowledge.read`).
    DocumentChunk {
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
        offset: u32,
        max_bytes: u16,
    },
    /// Saved scalar ranges on one document section (`core.knowledge.read`).
    SavedSelections {
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
    },
    /// Paginated capture/note previews (`core.knowledge.read`).
    NotesPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<DocumentFormat>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_id: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        limit: u16,
    },
    /// Bounded quote or note body chunk (`core.knowledge.read`).
    NoteBody {
        format: DocumentFormat,
        capture_id: i64,
        field: NoteField,
        offset: u32,
        max_bytes: u16,
    },
    /// Bounded PDF metadata summary (`core.knowledge.read`).
    PdfMetadata { document_id: i64 },
    /// Mutate a stored capture note (`core.knowledge.write`).
    UpdateCaptureNote {
        format: DocumentFormat,
        capture_id: i64,
        note: String,
    },
    /// Delete a stored capture (`core.knowledge.write`).
    DeleteCapture {
        format: DocumentFormat,
        capture_id: i64,
    },
    /// Delete a stored document (`core.knowledge.write`).
    DeleteDocument {
        format: DocumentFormat,
        document_id: i64,
    },
    /// Propose a new EPUB capture selection (`core.capture.propose`).
    /// Host binds provenance; caller-supplied installation/publisher fields are forbidden.
    CaptureEpubSelection {
        operation_id: String,
        document_id: i64,
        chapter_index: u32,
        start: u32,
        end: u32,
        quote: String,
        note: String,
    },
}

impl CoreKnowledgeOperation {
    /// Standard orthogonal capability required to authorize this operation.
    pub const fn required_capability(&self) -> &'static str {
        match self {
            Self::DocumentsPage { .. }
            | Self::DocumentToc { .. }
            | Self::DocumentChunk { .. }
            | Self::SavedSelections { .. }
            | Self::NotesPage { .. }
            | Self::NoteBody { .. }
            | Self::PdfMetadata { .. } => CAP_CORE_KNOWLEDGE_READ,
            Self::UpdateCaptureNote { .. }
            | Self::DeleteCapture { .. }
            | Self::DeleteDocument { .. } => CAP_CORE_KNOWLEDGE_WRITE,
            Self::CaptureEpubSelection { .. } => CAP_CORE_CAPTURE_PROPOSE,
        }
    }

    /// True only for operations requiring `core.knowledge.write`.
    pub const fn is_write(&self) -> bool {
        matches!(
            self,
            Self::UpdateCaptureNote { .. }
                | Self::DeleteCapture { .. }
                | Self::DeleteDocument { .. }
        )
    }
}

impl CoreKnowledgeRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.protocol_version != CORE_KNOWLEDGE_PROTOCOL
            || !crate::is_supported_api(self.api_version)
            || self.api_version < HOST_API_V0_8
        {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "core knowledge wire requires protocol 1 and minimum Host API 0.8",
            ));
        }
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.operation {
            CoreKnowledgeOperation::DocumentsPage { cursor, limit, .. } => {
                if *limit == 0 || *limit > MAX_KNOWLEDGE_PAGE {
                    return Err(budget("knowledge page limit exceeds budget"));
                }
                if let Some(cursor) = cursor {
                    validate_cursor(cursor)?;
                }
            }
            CoreKnowledgeOperation::DocumentToc { document_id, .. }
            | CoreKnowledgeOperation::PdfMetadata { document_id }
            | CoreKnowledgeOperation::DeleteDocument { document_id, .. } => {
                positive(*document_id, "invalid knowledge document id")?;
            }
            CoreKnowledgeOperation::DocumentChunk {
                document_id,
                section_index,
                offset,
                max_bytes,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                if *section_index >= MAX_KNOWLEDGE_SECTIONS {
                    return Err(budget("knowledge section index exceeds budget"));
                }
                if *offset > MAX_KNOWLEDGE_TEXT_OFFSET {
                    return Err(invalid("knowledge text offset exceeds budget"));
                }
                if !(MIN_KNOWLEDGE_CHUNK_BYTES..=MAX_KNOWLEDGE_CHUNK_BYTES).contains(max_bytes) {
                    return Err(budget("knowledge chunk size is outside supported range"));
                }
            }
            CoreKnowledgeOperation::SavedSelections {
                document_id,
                section_index,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                if *section_index >= MAX_KNOWLEDGE_SECTIONS {
                    return Err(budget("knowledge section index exceeds budget"));
                }
            }
            CoreKnowledgeOperation::NotesPage {
                document_id,
                cursor,
                limit,
                ..
            } => {
                if let Some(document_id) = document_id {
                    positive(*document_id, "invalid knowledge document id")?;
                }
                if *limit == 0 || *limit > MAX_KNOWLEDGE_PAGE {
                    return Err(budget("knowledge notes page limit exceeds budget"));
                }
                if let Some(cursor) = cursor {
                    validate_cursor(cursor)?;
                }
            }
            CoreKnowledgeOperation::NoteBody {
                capture_id,
                offset,
                max_bytes,
                ..
            } => {
                positive(*capture_id, "invalid knowledge capture id")?;
                if *offset > MAX_KNOWLEDGE_TEXT_OFFSET {
                    return Err(invalid("knowledge note offset exceeds budget"));
                }
                if !(MIN_KNOWLEDGE_CHUNK_BYTES..=MAX_KNOWLEDGE_CHUNK_BYTES).contains(max_bytes) {
                    return Err(budget("knowledge chunk size is outside supported range"));
                }
            }
            CoreKnowledgeOperation::UpdateCaptureNote {
                capture_id, note, ..
            } => {
                positive(*capture_id, "invalid knowledge capture id")?;
                validate_multiline_text(note, MAX_KNOWLEDGE_NOTE_BYTES, false, "knowledge note")?;
            }
            CoreKnowledgeOperation::DeleteCapture { capture_id, .. } => {
                positive(*capture_id, "invalid knowledge capture id")?;
            }
            CoreKnowledgeOperation::CaptureEpubSelection {
                operation_id,
                document_id,
                chapter_index,
                start,
                end,
                quote,
                note,
            } => {
                validate_opaque_id(operation_id, "operation_id")?;
                positive(*document_id, "invalid knowledge document id")?;
                if *chapter_index >= MAX_KNOWLEDGE_SECTIONS {
                    return Err(budget("epub chapter index exceeds budget"));
                }
                if start >= end
                    || *end > MAX_KNOWLEDGE_SELECTION_SCALARS
                    || end - start > MAX_KNOWLEDGE_SELECTION_SCALARS
                {
                    return Err(budget("epub selection scalar range is outside budget"));
                }
                validate_multiline_text(
                    quote,
                    MAX_KNOWLEDGE_QUOTE_BYTES,
                    true,
                    "epub selection quote",
                )?;
                validate_multiline_text(
                    note,
                    MAX_KNOWLEDGE_NOTE_BYTES,
                    false,
                    "epub selection note",
                )?;
            }
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let request: Self =
            decode_json(json, MAX_KNOWLEDGE_REQUEST_BYTES, ErrorCode::InvalidRequest)?;
        request.validate()?;
        Ok(request)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_KNOWLEDGE_REQUEST_BYTES, ErrorCode::InvalidRequest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeDocumentSummary {
    pub document_id: i64,
    pub format: DocumentFormat,
    pub commit_rev: i64,
    pub title: String,
    pub section_count: u32,
}

impl KnowledgeDocumentSummary {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.document_id, "invalid knowledge document id")?;
        positive(self.commit_rev, "invalid knowledge commit revision")?;
        validate_singleline_text(&self.title, MAX_KNOWLEDGE_TITLE_BYTES, "document title")?;
        if self.section_count == 0 || self.section_count > MAX_KNOWLEDGE_SECTIONS {
            return Err(budget("document section count exceeds budget"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeTocEntry {
    pub title: String,
    pub section_index: u32,
    pub depth: u8,
}

impl KnowledgeTocEntry {
    fn validate(&self) -> Result<(), ApiError> {
        validate_singleline_text(&self.title, MAX_KNOWLEDGE_TITLE_BYTES, "toc title")?;
        if self.section_index >= MAX_KNOWLEDGE_SECTIONS {
            return Err(budget("toc section index exceeds budget"));
        }
        if self.depth > MAX_KNOWLEDGE_TOC_DEPTH {
            return Err(budget("toc depth exceeds budget"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSavedRange {
    pub capture_id: i64,
    pub start: u32,
    pub end: u32,
}

impl KnowledgeSavedRange {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.capture_id, "invalid knowledge capture id")?;
        if self.start >= self.end
            || self.end > MAX_KNOWLEDGE_SELECTION_SCALARS
            || self.end - self.start > MAX_KNOWLEDGE_SELECTION_SCALARS
        {
            return Err(invalid("saved selection range is invalid"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeNoteSummary {
    pub format: DocumentFormat,
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub document_title: String,
    pub section_index: u32,
    pub quote_preview: String,
    pub quote_bytes: u64,
    pub quote_truncated: bool,
    pub note_preview: String,
    pub note_bytes: u64,
    pub note_truncated: bool,
}

impl KnowledgeNoteSummary {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.capture_id, "invalid knowledge capture id")?;
        positive(self.commit_rev, "invalid knowledge commit revision")?;
        positive(self.document_id, "invalid knowledge document id")?;
        validate_singleline_text(
            &self.document_title,
            MAX_KNOWLEDGE_TITLE_BYTES,
            "note document title",
        )?;
        if self.section_index >= MAX_KNOWLEDGE_SECTIONS {
            return Err(budget("note section index exceeds budget"));
        }
        for (preview, bytes, truncated) in [
            (&self.quote_preview, self.quote_bytes, self.quote_truncated),
            (&self.note_preview, self.note_bytes, self.note_truncated),
        ] {
            if preview.len() > MAX_KNOWLEDGE_PREVIEW_BYTES {
                return Err(budget("knowledge preview exceeds budget"));
            }
            if preview.chars().any(|ch| ch.is_control() && ch != '\n') {
                return Err(invalid("knowledge preview contains control characters"));
            }
            let expected =
                u64::try_from(preview.len()).map_err(|_| invalid("invalid preview length"))?;
            if truncated {
                if bytes <= expected {
                    return Err(invalid("truncated preview bytes are inconsistent"));
                }
            } else if bytes != expected {
                return Err(invalid("complete preview bytes are inconsistent"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeNoteBody {
    pub format: DocumentFormat,
    pub capture_id: i64,
    pub commit_rev: i64,
    pub document_id: i64,
    pub field: NoteField,
    pub offset: u32,
    pub total_bytes: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u32>,
}

impl KnowledgeNoteBody {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.capture_id, "invalid knowledge capture id")?;
        positive(self.commit_rev, "invalid knowledge commit revision")?;
        positive(self.document_id, "invalid knowledge document id")?;
        validate_chunk_span(self.offset, self.total_bytes, &self.text, self.next_offset)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePdfMetadata {
    pub document_id: i64,
    pub commit_rev: i64,
    pub title: String,
    pub original_sha256: String,
    pub original_bytes: u64,
    pub extraction_revision: String,
    pub page_count: u32,
}

impl KnowledgePdfMetadata {
    fn validate(&self) -> Result<(), ApiError> {
        positive(self.document_id, "invalid knowledge document id")?;
        positive(self.commit_rev, "invalid knowledge commit revision")?;
        validate_singleline_text(&self.title, MAX_KNOWLEDGE_TITLE_BYTES, "pdf title")?;
        validate_digest(&self.original_sha256, "invalid pdf original digest")?;
        validate_digest(&self.extraction_revision, "invalid pdf extraction revision")?;
        if self.original_bytes == 0 || self.original_bytes > 16 * 1024 * 1024 {
            return Err(budget("pdf original size is outside supported range"));
        }
        if self.page_count == 0 || self.page_count > MAX_KNOWLEDGE_SECTIONS {
            return Err(budget("pdf page count is outside supported range"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CoreKnowledgeResult {
    DocumentsPage {
        documents: Vec<KnowledgeDocumentSummary>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
    DocumentToc {
        format: DocumentFormat,
        document_id: i64,
        entries: Vec<KnowledgeTocEntry>,
    },
    DocumentChunk {
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
        offset: u32,
        total_bytes: u64,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_offset: Option<u32>,
    },
    SavedSelections {
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
        ranges: Vec<KnowledgeSavedRange>,
    },
    NotesPage {
        notes: Vec<KnowledgeNoteSummary>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
    NoteBody(KnowledgeNoteBody),
    PdfMetadata(KnowledgePdfMetadata),
    NoteUpdated {
        format: DocumentFormat,
        capture_id: i64,
        commit_rev: i64,
    },
    CaptureDeleted {
        format: DocumentFormat,
        capture_id: i64,
        commit_rev: i64,
    },
    DocumentDeleted {
        format: DocumentFormat,
        document_id: i64,
        commit_rev: i64,
    },
    CaptureCreated {
        format: DocumentFormat,
        capture_id: i64,
        commit_rev: i64,
    },
    Error {
        error: ApiError,
    },
}

/// Bounded orthogonal Core Knowledge response; adapter correlates `request_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreKnowledgeResponse {
    pub request_id: String,
    pub result: CoreKnowledgeResult,
}

impl CoreKnowledgeResponse {
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_opaque_id(&self.request_id, "request_id")?;
        match &self.result {
            CoreKnowledgeResult::DocumentsPage {
                documents,
                next_cursor,
            } => {
                if documents.len() > usize::from(MAX_KNOWLEDGE_PAGE) {
                    return Err(budget("documents page exceeds budget"));
                }
                for doc in documents {
                    doc.validate()?;
                }
                if let Some(next) = next_cursor {
                    validate_cursor(next)?;
                }
            }
            CoreKnowledgeResult::DocumentToc {
                document_id,
                entries,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                if entries.len() > MAX_KNOWLEDGE_TOC_ENTRIES {
                    return Err(budget("document toc exceeds budget"));
                }
                for entry in entries {
                    entry.validate()?;
                }
            }
            CoreKnowledgeResult::DocumentChunk {
                document_id,
                section_index,
                offset,
                total_bytes,
                text,
                next_offset,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                if *section_index >= MAX_KNOWLEDGE_SECTIONS {
                    return Err(budget("knowledge section index exceeds budget"));
                }
                validate_chunk_span(*offset, *total_bytes, text, *next_offset)?;
            }
            CoreKnowledgeResult::SavedSelections {
                document_id,
                section_index,
                ranges,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                if *section_index >= MAX_KNOWLEDGE_SECTIONS {
                    return Err(budget("knowledge section index exceeds budget"));
                }
                if ranges.len() > MAX_KNOWLEDGE_SAVED_SELECTIONS {
                    return Err(budget("saved selections exceed budget"));
                }
                for range in ranges {
                    range.validate()?;
                }
            }
            CoreKnowledgeResult::NotesPage { notes, next_cursor } => {
                if notes.len() > usize::from(MAX_KNOWLEDGE_PAGE) {
                    return Err(budget("notes page exceeds budget"));
                }
                for note in notes {
                    note.validate()?;
                }
                if let Some(next) = next_cursor {
                    validate_cursor(next)?;
                }
            }
            CoreKnowledgeResult::NoteBody(body) => body.validate()?,
            CoreKnowledgeResult::PdfMetadata(meta) => meta.validate()?,
            CoreKnowledgeResult::NoteUpdated {
                capture_id,
                commit_rev,
                ..
            }
            | CoreKnowledgeResult::CaptureDeleted {
                capture_id,
                commit_rev,
                ..
            }
            | CoreKnowledgeResult::CaptureCreated {
                capture_id,
                commit_rev,
                ..
            } => {
                positive(*capture_id, "invalid knowledge capture id")?;
                positive(*commit_rev, "invalid knowledge commit revision")?;
            }
            CoreKnowledgeResult::DocumentDeleted {
                document_id,
                commit_rev,
                ..
            } => {
                positive(*document_id, "invalid knowledge document id")?;
                positive(*commit_rev, "invalid knowledge commit revision")?;
            }
            CoreKnowledgeResult::Error { error } => error.validate()?,
        }
        Ok(())
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let response: Self = decode_json(
            json,
            MAX_KNOWLEDGE_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )?;
        response.validate()?;
        Ok(response)
    }

    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(
            self,
            MAX_KNOWLEDGE_RESPONSE_BYTES,
            ErrorCode::InvalidRequest,
        )
    }
}

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn budget(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::BudgetExceeded, message)
}

fn positive(value: i64, field: &'static str) -> Result<(), ApiError> {
    if value <= 0 {
        return Err(invalid(field));
    }
    Ok(())
}

fn validate_cursor(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > MAX_KNOWLEDGE_CURSOR_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'*')
        })
    {
        return Err(invalid("invalid knowledge cursor"));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), ApiError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(field));
    }
    Ok(())
}

fn validate_singleline_text(value: &str, limit: usize, field: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid {field}"),
        ));
    }
    Ok(())
}

fn validate_multiline_text(
    value: &str,
    limit: usize,
    require_non_empty: bool,
    field: &str,
) -> Result<(), ApiError> {
    if (require_non_empty && value.trim().is_empty())
        || value.len() > limit
        || value.chars().any(|ch| ch.is_control() && ch != '\n')
    {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid {field}"),
        ));
    }
    Ok(())
}

fn validate_chunk_span(
    offset: u32,
    total_bytes: u64,
    text: &str,
    next_offset: Option<u32>,
) -> Result<(), ApiError> {
    if offset > MAX_KNOWLEDGE_TEXT_OFFSET {
        return Err(invalid("knowledge chunk offset exceeds supported range"));
    }
    if text.len() > usize::from(MAX_KNOWLEDGE_CHUNK_BYTES) {
        return Err(budget("knowledge chunk exceeds budget"));
    }
    let end = u64::from(offset)
        .checked_add(u64::try_from(text.len()).map_err(|_| invalid("invalid chunk text"))?)
        .ok_or_else(|| invalid("invalid chunk text"))?;
    if end > total_bytes {
        return Err(invalid("knowledge chunk exceeds stored content"));
    }
    match next_offset {
        Some(next) => {
            if u64::from(next) != end || end >= total_bytes {
                return Err(invalid("knowledge chunk next offset is inconsistent"));
            }
        }
        None => {
            if end != total_bytes {
                return Err(invalid("knowledge chunk end is inconsistent"));
            }
        }
    }
    Ok(())
}
