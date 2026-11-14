#![cfg_attr(not(target_arch = "wasm32"), forbid(unsafe_code))]
#![deny(unsafe_code)]

//! Public guest SDK for QUSOR sandboxed Wasm plugins and declarative UI slots.
//!
//! Only [`ffi`] interacts with the `wasm32` C ABI boundary; all SDK abstractions
//! and host-testable clients forbid `unsafe`.

#[doc(hidden)]
pub mod ffi;
pub mod ui_builder;

pub use extension_api::{
    self, ApiError, ApiVersion, ErrorCode, HOST_API_V0_7, HOST_API_V0_8,
    notes::NoteField,
    orthogonal::{self, *},
    ui,
    vocab::{
        self, VOCAB_PROTOCOL, VocabCardView, VocabDeck, VocabOperation, VocabRating, VocabRequest,
        VocabResponse, VocabResult, VocabReviewOutcome, VocabStage,
    },
};

use extension_api::ui::{UiEventEnvelope, UiPatch, UiTree};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt;

/// Error returned by SDK serialization, transport, or host API calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SdkError {
    Serialize(String),
    Deserialize(String),
    Transport(String),
    Api(ApiError),
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize(msg) => write!(formatter, "serialize error: {msg}"),
            Self::Deserialize(msg) => write!(formatter, "deserialize error: {msg}"),
            Self::Transport(msg) => write!(formatter, "transport error: {msg}"),
            Self::Api(err) => write!(formatter, "host api error: {err}"),
        }
    }
}

impl std::error::Error for SdkError {}

/// Abstract guest-to-host broker transport, swappable in unit tests.
pub trait HostTransport {
    fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError>;
}

/// Default transport backed by `qusor.call` on `wasm32` targets.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTransport;

impl HostTransport for DefaultTransport {
    fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
        ffi::broker_call(request).map_err(SdkError::Transport)
    }
}

static DEFAULT_TRANSPORT: DefaultTransport = DefaultTransport;

/// Serializes `request` to JSON, sends it through `qusor.call`, and decodes `Resp`.
pub fn host_call<Req: Serialize, Resp: DeserializeOwned>(request: &Req) -> Result<Resp, SdkError> {
    host_call_with(&DEFAULT_TRANSPORT, request)
}

/// Serializes `request` to JSON, calls `transport`, and decodes `Resp`.
pub fn host_call_with<Req: Serialize, Resp: DeserializeOwned>(
    transport: &dyn HostTransport,
    request: &Req,
) -> Result<Resp, SdkError> {
    let encoded =
        serde_json::to_vec(request).map_err(|err| SdkError::Serialize(err.to_string()))?;
    let reply = transport.call_bytes(&encoded)?;
    serde_json::from_slice(&reply).map_err(|err| SdkError::Deserialize(err.to_string()))
}

/// Page of queued vocabulary cards returned by [`VocabClient::queue`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VocabQueuePage {
    pub cards: Vec<VocabCardView>,
    pub remaining_due: u32,
    pub remaining_new: u32,
}

/// Safe typed client over the versioned `extension-api::vocab` wire contract.
pub struct VocabClient<'a> {
    transport: &'a dyn HostTransport,
    api_version: ApiVersion,
}

impl Default for VocabClient<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl VocabClient<'static> {
    /// Creates a client using the default `wasm32` FFI broker transport and Host API 0.7.
    pub fn new() -> Self {
        Self {
            transport: &DEFAULT_TRANSPORT,
            api_version: HOST_API_V0_7,
        }
    }
}

impl<'a> VocabClient<'a> {
    /// Creates a client backed by a caller-supplied [`HostTransport`].
    pub fn with_transport(transport: &'a dyn HostTransport) -> Self {
        Self {
            transport,
            api_version: HOST_API_V0_7,
        }
    }

    /// Overrides the negotiated Host API version sent in [`VocabRequest`].
    pub fn with_api_version(mut self, api_version: ApiVersion) -> Self {
        self.api_version = api_version;
        self
    }

    fn send(&self, request_id: &str, operation: VocabOperation) -> Result<VocabResult, SdkError> {
        let request = VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: self.api_version,
            request_id: request_id.to_owned(),
            operation,
        };
        request
            .validate()
            .map_err(|err| SdkError::Serialize(err.to_string()))?;
        let response: VocabResponse = host_call_with(self.transport, &request)?;
        if response.request_id != request.request_id {
            return Err(SdkError::Deserialize(format!(
                "mismatched response request_id `{}` (expected `{}`)",
                response.request_id, request.request_id
            )));
        }
        match response.result {
            VocabResult::Error { error } => Err(SdkError::Api(error)),
            other => Ok(other),
        }
    }

    /// Lists wordbooks and card counters in the Host-bound vault.
    pub fn decks(&self, request_id: &str) -> Result<Vec<VocabDeck>, SdkError> {
        match self.send(request_id, VocabOperation::Decks {})? {
            VocabResult::Decks { decks } => Ok(decks),
            other => Err(SdkError::Deserialize(format!(
                "unexpected vocab response for decks: {other:?}"
            ))),
        }
    }

    /// Fetches a bounded page of due/new cards for `wordbook_id`.
    pub fn queue(
        &self,
        request_id: &str,
        wordbook_id: i64,
        limit: u16,
    ) -> Result<VocabQueuePage, SdkError> {
        match self.send(request_id, VocabOperation::Queue { wordbook_id, limit })? {
            VocabResult::Queue {
                cards,
                remaining_due,
                remaining_new,
            } => Ok(VocabQueuePage {
                cards,
                remaining_due,
                remaining_new,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected vocab response for queue: {other:?}"
            ))),
        }
    }

    /// Submits one idempotent FSRS review grade; `request_id` acts as the idempotency key.
    pub fn review(
        &self,
        request_id: &str,
        card_id: i64,
        rating: VocabRating,
    ) -> Result<VocabReviewOutcome, SdkError> {
        match self.send(request_id, VocabOperation::Review { card_id, rating })? {
            VocabResult::Review(outcome) => Ok(outcome),
            other => Err(SdkError::Deserialize(format!(
                "unexpected vocab response for review: {other:?}"
            ))),
        }
    }
}

/// Paginated response returned by [`OrthogonalKnowledgeClient::documents_page`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeDocumentsPage {
    pub documents: Vec<KnowledgeDocumentSummary>,
    pub next_cursor: Option<String>,
}

/// Text chunk returned by [`OrthogonalKnowledgeClient::document_chunk`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeDocumentChunk {
    pub format: DocumentFormat,
    pub document_id: i64,
    pub section_index: u32,
    pub offset: u32,
    pub total_bytes: u64,
    pub text: String,
    pub next_offset: Option<u32>,
}

/// Paginated response returned by [`OrthogonalKnowledgeClient::notes_page`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeNotesPage {
    pub notes: Vec<KnowledgeNoteSummary>,
    pub next_cursor: Option<String>,
}

/// Receipt returned by [`OrthogonalKnowledgeClient`] capture mutations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeCaptureReceipt {
    pub format: DocumentFormat,
    pub capture_id: i64,
    pub commit_rev: i64,
}

/// Receipt returned by [`OrthogonalKnowledgeClient::delete_document`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeDocumentReceipt {
    pub format: DocumentFormat,
    pub document_id: i64,
    pub commit_rev: i64,
}

/// Typed client for the Host API `0.8+` orthogonal `core.knowledge` broker protocol.
pub struct OrthogonalKnowledgeClient<'a> {
    transport: &'a dyn HostTransport,
    api_version: ApiVersion,
}

impl Default for OrthogonalKnowledgeClient<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl OrthogonalKnowledgeClient<'static> {
    /// Creates a client using the default `wasm32` FFI broker transport and Host API `0.8`.
    pub fn new() -> Self {
        Self {
            transport: &DEFAULT_TRANSPORT,
            api_version: HOST_API_V0_8,
        }
    }
}

impl<'a> OrthogonalKnowledgeClient<'a> {
    /// Creates a client backed by a caller-supplied [`HostTransport`].
    pub fn with_transport(transport: &'a dyn HostTransport) -> Self {
        Self {
            transport,
            api_version: HOST_API_V0_8,
        }
    }

    /// Overrides the negotiated Host API version sent in [`CoreKnowledgeRequest`].
    pub fn with_api_version(mut self, api_version: ApiVersion) -> Self {
        self.api_version = api_version;
        self
    }

    /// Sends one validated [`CoreKnowledgeOperation`] and returns its non-error [`CoreKnowledgeResult`].
    pub fn send(
        &self,
        request_id: &str,
        operation: CoreKnowledgeOperation,
    ) -> Result<CoreKnowledgeResult, SdkError> {
        let request = CoreKnowledgeRequest {
            protocol_version: CORE_KNOWLEDGE_PROTOCOL,
            api_version: self.api_version,
            request_id: request_id.to_owned(),
            operation,
        };
        request
            .validate()
            .map_err(|err| SdkError::Serialize(err.to_string()))?;
        let response: CoreKnowledgeResponse = host_call_with(self.transport, &request)?;
        if response.request_id != request.request_id {
            return Err(SdkError::Deserialize(format!(
                "mismatched response request_id `{}` (expected `{}`)",
                response.request_id, request.request_id
            )));
        }
        match response.result {
            CoreKnowledgeResult::Error { error } => Err(SdkError::Api(error)),
            other => Ok(other),
        }
    }

    /// Lists PDF/EPUB document summaries (`core.knowledge.read`).
    pub fn documents_page(
        &self,
        request_id: &str,
        format: Option<DocumentFormat>,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<KnowledgeDocumentsPage, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::DocumentsPage {
                format,
                cursor: cursor.map(str::to_owned),
                limit,
            },
        )? {
            CoreKnowledgeResult::DocumentsPage {
                documents,
                next_cursor,
            } => Ok(KnowledgeDocumentsPage {
                documents,
                next_cursor,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for documents_page: {other:?}"
            ))),
        }
    }

    /// Reads the bounded table of contents for a document (`core.knowledge.read`).
    pub fn document_toc(
        &self,
        request_id: &str,
        format: DocumentFormat,
        document_id: i64,
    ) -> Result<Vec<KnowledgeTocEntry>, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::DocumentToc {
                format,
                document_id,
            },
        )? {
            CoreKnowledgeResult::DocumentToc { entries, .. } => Ok(entries),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for document_toc: {other:?}"
            ))),
        }
    }

    /// Reads one bounded UTF-8 text chunk of a PDF page or EPUB chapter (`core.knowledge.read`).
    pub fn document_chunk(
        &self,
        request_id: &str,
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> Result<KnowledgeDocumentChunk, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::DocumentChunk {
                format,
                document_id,
                section_index,
                offset,
                max_bytes,
            },
        )? {
            CoreKnowledgeResult::DocumentChunk {
                format,
                document_id,
                section_index,
                offset,
                total_bytes,
                text,
                next_offset,
            } => Ok(KnowledgeDocumentChunk {
                format,
                document_id,
                section_index,
                offset,
                total_bytes,
                text,
                next_offset,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for document_chunk: {other:?}"
            ))),
        }
    }

    /// Lists saved selection scalar ranges in one PDF page or EPUB chapter (`core.knowledge.read`).
    pub fn saved_selections(
        &self,
        request_id: &str,
        format: DocumentFormat,
        document_id: i64,
        section_index: u32,
    ) -> Result<Vec<KnowledgeSavedRange>, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::SavedSelections {
                format,
                document_id,
                section_index,
            },
        )? {
            CoreKnowledgeResult::SavedSelections { ranges, .. } => Ok(ranges),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for saved_selections: {other:?}"
            ))),
        }
    }

    /// Lists PDF/EPUB note summaries (`core.knowledge.read`).
    pub fn notes_page(
        &self,
        request_id: &str,
        format: Option<DocumentFormat>,
        document_id: Option<i64>,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<KnowledgeNotesPage, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::NotesPage {
                format,
                document_id,
                cursor: cursor.map(str::to_owned),
                limit,
            },
        )? {
            CoreKnowledgeResult::NotesPage { notes, next_cursor } => {
                Ok(KnowledgeNotesPage { notes, next_cursor })
            }
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for notes_page: {other:?}"
            ))),
        }
    }

    /// Reads one bounded UTF-8 chunk of a saved capture quote or user note (`core.knowledge.read`).
    pub fn note_body(
        &self,
        request_id: &str,
        format: DocumentFormat,
        capture_id: i64,
        field: NoteField,
        offset: u32,
        max_bytes: u16,
    ) -> Result<KnowledgeNoteBody, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::NoteBody {
                format,
                capture_id,
                field,
                offset,
                max_bytes,
            },
        )? {
            CoreKnowledgeResult::NoteBody(body) => Ok(body),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for note_body: {other:?}"
            ))),
        }
    }

    /// Reads PDF metadata (`core.knowledge.read`).
    pub fn pdf_metadata(
        &self,
        request_id: &str,
        document_id: i64,
    ) -> Result<KnowledgePdfMetadata, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::PdfMetadata { document_id },
        )? {
            CoreKnowledgeResult::PdfMetadata(meta) => Ok(meta),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for pdf_metadata: {other:?}"
            ))),
        }
    }

    /// Updates a capture's user note (`core.knowledge.write`).
    pub fn update_capture_note(
        &self,
        request_id: &str,
        format: DocumentFormat,
        capture_id: i64,
        note: &str,
    ) -> Result<KnowledgeCaptureReceipt, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::UpdateCaptureNote {
                format,
                capture_id,
                note: note.to_owned(),
            },
        )? {
            CoreKnowledgeResult::NoteUpdated {
                format,
                capture_id,
                commit_rev,
            } => Ok(KnowledgeCaptureReceipt {
                format,
                capture_id,
                commit_rev,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for update_capture_note: {other:?}"
            ))),
        }
    }

    /// Deletes a capture (`core.knowledge.write`).
    pub fn delete_capture(
        &self,
        request_id: &str,
        format: DocumentFormat,
        capture_id: i64,
    ) -> Result<KnowledgeCaptureReceipt, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::DeleteCapture { format, capture_id },
        )? {
            CoreKnowledgeResult::CaptureDeleted {
                format,
                capture_id,
                commit_rev,
            } => Ok(KnowledgeCaptureReceipt {
                format,
                capture_id,
                commit_rev,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for delete_capture: {other:?}"
            ))),
        }
    }

    /// Deletes a document (`core.knowledge.write`).
    pub fn delete_document(
        &self,
        request_id: &str,
        format: DocumentFormat,
        document_id: i64,
    ) -> Result<KnowledgeDocumentReceipt, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::DeleteDocument {
                format,
                document_id,
            },
        )? {
            CoreKnowledgeResult::DocumentDeleted {
                format,
                document_id,
                commit_rev,
            } => Ok(KnowledgeDocumentReceipt {
                format,
                document_id,
                commit_rev,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for delete_document: {other:?}"
            ))),
        }
    }

    /// Proposes capturing a scalar selection in an EPUB chapter (`core.capture.propose`).
    #[allow(clippy::too_many_arguments)]
    pub fn capture_epub_selection(
        &self,
        request_id: &str,
        operation_id: &str,
        document_id: i64,
        chapter_index: u32,
        start: u32,
        end: u32,
        quote: &str,
        note: &str,
    ) -> Result<KnowledgeCaptureReceipt, SdkError> {
        match self.send(
            request_id,
            CoreKnowledgeOperation::CaptureEpubSelection {
                operation_id: operation_id.to_owned(),
                document_id,
                chapter_index,
                start,
                end,
                quote: quote.to_owned(),
                note: note.to_owned(),
            },
        )? {
            CoreKnowledgeResult::CaptureCreated {
                format,
                capture_id,
                commit_rev,
            } => Ok(KnowledgeCaptureReceipt {
                format,
                capture_id,
                commit_rev,
            }),
            other => Err(SdkError::Deserialize(format!(
                "unexpected core.knowledge response for capture_epub_selection: {other:?}"
            ))),
        }
    }
}

/// Host-supplied context when rendering a `ui_slot` contribution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotRenderContext {
    pub contribution_id: String,
    pub mount_id: String,
    pub generation: u64,
    pub api_version: Option<ApiVersion>,
    pub compact: bool,
}

/// Result returned by [`SlotPlugin::handle_event`]: either a full [`UiTree`] or a [`UiPatch`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SlotUpdate {
    Tree(UiTree),
    Patch(UiPatch),
}

impl SlotUpdate {
    /// Validates and serializes the UI update to compact JSON.
    pub fn to_json(&self) -> Result<String, SdkError> {
        match self {
            Self::Tree(tree) => tree.to_json().map_err(SdkError::Api),
            Self::Patch(patch) => patch.to_json().map_err(SdkError::Api),
        }
    }
}

/// Trait implemented by QUSOR plugins that provide a declarative `ui_slot`.
pub trait SlotPlugin: Default + Send {
    /// Renders the initial or refreshed [`UiTree`] for the mounted slot.
    fn render(
        &mut self,
        ctx: &SlotRenderContext,
        host: &dyn HostTransport,
    ) -> Result<UiTree, String>;

    /// Handles a generation-fenced [`UiEventEnvelope`] and returns the next UI state.
    fn handle_event(
        &mut self,
        event: &UiEventEnvelope,
        host: &dyn HostTransport,
    ) -> Result<SlotUpdate, String>;
}

#[doc(hidden)]
pub fn dispatch_render<P: SlotPlugin>(plugin: &mut P, input: &[u8]) -> Vec<u8> {
    let ctx = if input.is_empty() {
        SlotRenderContext::default()
    } else {
        match serde_json::from_slice::<SlotRenderContext>(input) {
            Ok(parsed) => parsed,
            Err(err) => {
                return fallback_error_tree_bytes(&format!("invalid render context: {err}"));
            }
        }
    };
    match plugin.render(&ctx, &DEFAULT_TRANSPORT) {
        Ok(tree) => match tree.to_json() {
            Ok(json) => json.into_bytes(),
            Err(err) => fallback_error_tree_bytes(&format!("invalid UiTree: {err}")),
        },
        Err(err) => fallback_error_tree_bytes(&err),
    }
}

#[doc(hidden)]
pub fn dispatch_event<P: SlotPlugin>(plugin: &mut P, input: &[u8]) -> Vec<u8> {
    let envelope = match UiEventEnvelope::from_slice(input) {
        Ok(parsed) => parsed,
        Err(err) => return fallback_error_tree_bytes(&format!("invalid event envelope: {err}")),
    };
    match plugin.handle_event(&envelope, &DEFAULT_TRANSPORT) {
        Ok(update) => match update.to_json() {
            Ok(json) => json.into_bytes(),
            Err(err) => fallback_error_tree_bytes(&format!("invalid UI update: {err}")),
        },
        Err(err) => fallback_error_tree_bytes(&err),
    }
}

fn fallback_error_tree_bytes(message: &str) -> Vec<u8> {
    let sanitized: String = message
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n')
        .take(200)
        .collect();
    let description = if sanitized.trim().is_empty() {
        "插件渲染失败。".to_owned()
    } else {
        sanitized
    };
    let tree = ui_builder::ui_tree(
        1,
        ui_builder::empty_state("plugin_error", "插件暂时不可用", description),
    );
    tree.to_json()
        .map(String::into_bytes)
        .unwrap_or_else(|_| br#"{"protocol_version":1,"revision":1,"root":{"id":"err","component":{"kind":"empty_state","title":"Error","description":"Plugin error"}}}"#.to_vec())
}

/// Exports the QUSOR `wasm32` slot ABI (`qusor_alloc`, `render_slot`, `handle_slot_event`, `run`)
/// for a [`SlotPlugin`] implementation.
#[macro_export]
macro_rules! export_slot_plugin {
    ($plugin:ty) => {
        #[cfg(target_arch = "wasm32")]
        #[allow(unsafe_code)]
        mod __qusor_wasm_slot_exports {
            use std::sync::Mutex;

            static PLUGIN_INSTANCE: Mutex<Option<$plugin>> = Mutex::new(None);

            #[unsafe(no_mangle)]
            pub extern "C" fn qusor_alloc(len: i32) -> i32 {
                $crate::ffi::alloc_input(len)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn render_slot(ptr: i32, len: i32) -> i64 {
                let input = $crate::ffi::take_input(ptr, len);
                let Ok(mut guard) = PLUGIN_INSTANCE.lock() else {
                    return 0;
                };
                let plugin = guard.get_or_insert_with(Default::default);
                let output = $crate::dispatch_render(plugin, &input);
                $crate::ffi::pack_output(output)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn handle_slot_event(ptr: i32, len: i32) -> i64 {
                let input = $crate::ffi::take_input(ptr, len);
                let Ok(mut guard) = PLUGIN_INSTANCE.lock() else {
                    return 0;
                };
                let plugin = guard.get_or_insert_with(Default::default);
                let output = $crate::dispatch_event(plugin, &input);
                $crate::ffi::pack_output(output)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn run(ptr: i32, len: i32) -> i64 {
                render_slot(ptr, len)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use extension_api::ui::{PLUGIN_UI_PROTOCOL, UiAction, UiBudgets};
    use std::cell::RefCell;

    type TransportHandler<'a> = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, SdkError> + 'a>;

    struct MockTransport<'a> {
        handler: TransportHandler<'a>,
    }

    impl HostTransport for MockTransport<'_> {
        fn call_bytes(&self, request: &[u8]) -> Result<Vec<u8>, SdkError> {
            (self.handler)(request)
        }
    }

    #[test]
    fn vocab_client_roundtrips_decks_queue_and_review() {
        let seen = RefCell::new(Vec::<VocabRequest>::new());
        let transport = MockTransport {
            handler: Box::new(|bytes| {
                let req = VocabRequest::from_slice(bytes).expect("valid request");
                let resp = match &req.operation {
                    VocabOperation::Decks {} => VocabResponse {
                        request_id: req.request_id.clone(),
                        result: VocabResult::Decks {
                            decks: vec![VocabDeck {
                                wordbook_id: 1,
                                title: "Core Book".into(),
                                total: 2,
                                due: 1,
                                new: 1,
                            }],
                        },
                    },
                    VocabOperation::Queue { wordbook_id, limit } => {
                        assert_eq!(*wordbook_id, 1);
                        assert_eq!(*limit, 6);
                        VocabResponse {
                            request_id: req.request_id.clone(),
                            result: VocabResult::Queue {
                                cards: vec![VocabCardView {
                                    card_id: 10,
                                    term: "plugin".into(),
                                    meaning: "插件".into(),
                                    example: "Run in sandbox.".into(),
                                    stage: VocabStage::New,
                                }],
                                remaining_due: 0,
                                remaining_new: 1,
                            },
                        }
                    }
                    VocabOperation::Review { card_id, rating } => {
                        assert_eq!(*card_id, 10);
                        assert_eq!(*rating, VocabRating::Good);
                        VocabResponse {
                            request_id: req.request_id.clone(),
                            result: VocabResult::Review(VocabReviewOutcome {
                                card_id: 10,
                                stage: VocabStage::Learning,
                                due_at_ms: 1_700_000_600_000,
                                scheduled_minutes: 10,
                                remaining_due: 0,
                                remaining_new: 0,
                            }),
                        }
                    }
                };
                seen.borrow_mut().push(req);
                Ok(serde_json::to_vec(&resp).unwrap())
            }),
        };

        let client = VocabClient::with_transport(&transport);
        let decks = client.decks("req-1").unwrap();
        assert_eq!(decks.len(), 1);
        let page = client.queue("req-2", 1, 6).unwrap();
        assert_eq!(page.cards[0].term, "plugin");
        let outcome = client.review("req-3", 10, VocabRating::Good).unwrap();
        assert_eq!(outcome.scheduled_minutes, 10);
        assert_eq!(seen.borrow().len(), 3);
    }

    #[derive(Default)]
    struct EchoPlugin {
        clicks: u64,
    }

    impl SlotPlugin for EchoPlugin {
        fn render(
            &mut self,
            ctx: &SlotRenderContext,
            _host: &dyn HostTransport,
        ) -> Result<UiTree, String> {
            Ok(ui_builder::ui_tree(
                1,
                ui_builder::vstack(
                    "root",
                    ui::SpacingScale::Md,
                    ui::SpacingScale::Md,
                    vec![ui_builder::button(
                        "btn",
                        format!("Slot {}", ctx.contribution_id),
                        "ping",
                        true,
                        false,
                        false,
                    )],
                ),
            ))
        }

        fn handle_event(
            &mut self,
            event: &UiEventEnvelope,
            _host: &dyn HostTransport,
        ) -> Result<SlotUpdate, String> {
            self.clicks += 1;
            let tree = ui_builder::ui_tree(
                self.clicks + 1,
                ui_builder::text(
                    "status",
                    format!("Clicked {}", event.action_id),
                    ui::TypographyRole::Body,
                    ui::SemanticToken::TextPrimary,
                ),
            );
            Ok(SlotUpdate::Tree(tree))
        }
    }

    #[test]
    fn slot_plugin_renders_and_handles_events_with_valid_ui_tree() {
        let mut plugin = EchoPlugin::default();
        let tree = plugin
            .render(
                &SlotRenderContext {
                    contribution_id: "org.example.slot".into(),
                    mount_id: "mount-1".into(),
                    generation: 1,
                    api_version: Some(HOST_API_V0_7),
                    compact: false,
                },
                &DefaultTransport,
            )
            .unwrap();
        tree.validate(UiBudgets::default()).unwrap();

        let update = plugin
            .handle_event(
                &UiEventEnvelope {
                    protocol_version: PLUGIN_UI_PROTOCOL,
                    mount_id: "mount-1".into(),
                    generation: 1,
                    node_id: "btn".into(),
                    action_id: "ping".into(),
                    action: UiAction::Press,
                },
                &DefaultTransport,
            )
            .unwrap();
        match update {
            SlotUpdate::Tree(updated) => updated.validate(UiBudgets::default()).unwrap(),
            SlotUpdate::Patch(_) => panic!("expected tree"),
        }
    }

    struct MockKnowledgeTransport;

    impl HostTransport for MockKnowledgeTransport {
        fn call_bytes(&self, input: &[u8]) -> Result<Vec<u8>, SdkError> {
            let req = CoreKnowledgeRequest::from_slice(input).map_err(SdkError::Api)?;
            let result = match req.operation {
                CoreKnowledgeOperation::DocumentsPage { .. } => {
                    CoreKnowledgeResult::DocumentsPage {
                        documents: vec![KnowledgeDocumentSummary {
                            document_id: 1,
                            format: DocumentFormat::Epub,
                            commit_rev: 2,
                            title: "Mock Book".to_owned(),
                            section_count: 3,
                        }],
                        next_cursor: None,
                    }
                }
                CoreKnowledgeOperation::DocumentToc {
                    format,
                    document_id,
                } => CoreKnowledgeResult::DocumentToc {
                    format,
                    document_id,
                    entries: vec![KnowledgeTocEntry {
                        title: "Intro".to_owned(),
                        section_index: 0,
                        depth: 0,
                    }],
                },
                CoreKnowledgeOperation::DocumentChunk {
                    format,
                    document_id,
                    section_index,
                    offset,
                    ..
                } => CoreKnowledgeResult::DocumentChunk {
                    format,
                    document_id,
                    section_index,
                    offset,
                    total_bytes: 5,
                    text: "Hello".to_owned(),
                    next_offset: None,
                },
                CoreKnowledgeOperation::SavedSelections {
                    format,
                    document_id,
                    section_index,
                } => CoreKnowledgeResult::SavedSelections {
                    format,
                    document_id,
                    section_index,
                    ranges: vec![KnowledgeSavedRange {
                        capture_id: 9,
                        start: 0,
                        end: 5,
                    }],
                },
                CoreKnowledgeOperation::NotesPage { .. } => CoreKnowledgeResult::NotesPage {
                    notes: vec![KnowledgeNoteSummary {
                        format: DocumentFormat::Pdf,
                        capture_id: 9,
                        commit_rev: 4,
                        document_id: 1,
                        document_title: "Mock Book".to_owned(),
                        section_index: 0,
                        quote_preview: "Hello".to_owned(),
                        quote_bytes: 5,
                        quote_truncated: false,
                        note_preview: "Note".to_owned(),
                        note_bytes: 4,
                        note_truncated: false,
                    }],
                    next_cursor: None,
                },
                CoreKnowledgeOperation::NoteBody {
                    format,
                    capture_id,
                    field,
                    offset,
                    ..
                } => CoreKnowledgeResult::NoteBody(KnowledgeNoteBody {
                    format,
                    capture_id,
                    commit_rev: 4,
                    document_id: 1,
                    field,
                    offset,
                    total_bytes: 4,
                    text: "Note".to_owned(),
                    next_offset: None,
                }),
                CoreKnowledgeOperation::PdfMetadata { document_id } => {
                    CoreKnowledgeResult::PdfMetadata(KnowledgePdfMetadata {
                        document_id,
                        commit_rev: 2,
                        title: "Mock PDF".to_owned(),
                        original_sha256: "a".repeat(64),
                        original_bytes: 1024,
                        extraction_revision: "b".repeat(64),
                        page_count: 10,
                    })
                }
                CoreKnowledgeOperation::UpdateCaptureNote {
                    format,
                    capture_id,
                    note,
                } => {
                    if note == "deny" {
                        CoreKnowledgeResult::Error {
                            error: ApiError::new(ErrorCode::PermissionDenied, "write denied"),
                        }
                    } else {
                        CoreKnowledgeResult::NoteUpdated {
                            format,
                            capture_id,
                            commit_rev: 5,
                        }
                    }
                }
                CoreKnowledgeOperation::DeleteCapture { format, capture_id } => {
                    CoreKnowledgeResult::CaptureDeleted {
                        format,
                        capture_id,
                        commit_rev: 6,
                    }
                }
                CoreKnowledgeOperation::DeleteDocument {
                    format,
                    document_id,
                } => CoreKnowledgeResult::DocumentDeleted {
                    format,
                    document_id,
                    commit_rev: 7,
                },
                CoreKnowledgeOperation::CaptureEpubSelection { .. } => {
                    CoreKnowledgeResult::CaptureCreated {
                        format: DocumentFormat::Epub,
                        capture_id: 12,
                        commit_rev: 8,
                    }
                }
            };
            let reply = CoreKnowledgeResponse {
                request_id: req.request_id,
                result,
            };
            serde_json::to_vec(&reply).map_err(|err| SdkError::Serialize(err.to_string()))
        }
    }

    #[test]
    fn orthogonal_knowledge_client_roundtrips_all_operations_and_api_errors() {
        let client = OrthogonalKnowledgeClient::with_transport(&MockKnowledgeTransport);

        let docs = client.documents_page("r1", None, None, 10).unwrap();
        assert_eq!(docs.documents.len(), 1);
        assert_eq!(docs.documents[0].title, "Mock Book");

        let toc = client.document_toc("r2", DocumentFormat::Epub, 1).unwrap();
        assert_eq!(toc.len(), 1);

        let chunk = client
            .document_chunk("r3", DocumentFormat::Epub, 1, 0, 0, 64)
            .unwrap();
        assert_eq!(chunk.text, "Hello");

        let sels = client
            .saved_selections("r4", DocumentFormat::Epub, 1, 0)
            .unwrap();
        assert_eq!(sels.len(), 1);

        let notes = client.notes_page("r5", None, None, None, 10).unwrap();
        assert_eq!(notes.notes.len(), 1);

        let body = client
            .note_body("r6", DocumentFormat::Pdf, 9, NoteField::Note, 0, 64)
            .unwrap();
        assert_eq!(body.text, "Note");

        let meta = client.pdf_metadata("r7", 1).unwrap();
        assert_eq!(meta.page_count, 10);

        let upd = client
            .update_capture_note("r8", DocumentFormat::Epub, 9, "ok")
            .unwrap();
        assert_eq!(upd.commit_rev, 5);

        let cap = client
            .capture_epub_selection("r9", "op-1", 1, 0, 0, 5, "Hello", "note")
            .unwrap();
        assert_eq!(cap.capture_id, 12);

        let del_cap = client
            .delete_capture("r10", DocumentFormat::Pdf, 9)
            .unwrap();
        assert_eq!(del_cap.commit_rev, 6);

        let del_doc = client
            .delete_document("r11", DocumentFormat::Epub, 1)
            .unwrap();
        assert_eq!(del_doc.commit_rev, 7);

        let denied = client
            .update_capture_note("r12", DocumentFormat::Epub, 9, "deny")
            .unwrap_err();
        assert!(matches!(
            denied,
            SdkError::Api(ApiError {
                code: ErrorCode::PermissionDenied,
                ..
            })
        ));
    }

    #[test]
    fn ui_builder_new_components_pass_strict_ui_validation() {
        let root = ui_builder::split(
            "root-split",
            35,
            ui::SpacingScale::Md,
            vec![
                ui_builder::with_compact_hide(ui_builder::scroll(
                    "left-scroll",
                    ui::SpacingScale::Sm,
                    vec![
                        ui_builder::tabs(
                            "nav-tabs",
                            "Library sections",
                            "docs",
                            vec![
                                ui_builder::tab_option("docs", "Documents"),
                                ui_builder::tab_option("notes", "Notes"),
                            ],
                            "switch_tab",
                        ),
                        ui_builder::divider("div-1"),
                        ui_builder::input(
                            "search-input",
                            "Search notes",
                            "",
                            "Filter...",
                            false,
                            false,
                            "search_changed",
                            Some("search_submit".to_owned()),
                        ),
                        ui_builder::select(
                            "format-select",
                            "Format filter",
                            "all",
                            vec![
                                ui_builder::select_option("all", "All"),
                                ui_builder::select_option("epub", "EPUB"),
                            ],
                            false,
                            "format_changed",
                        ),
                        ui_builder::toggle(
                            "show-archived",
                            "Show details",
                            true,
                            false,
                            "toggle_details",
                        ),
                    ],
                )),
                ui_builder::with_flex_grow(ui_builder::card(
                    "right-card",
                    ui::SpacingScale::Sm,
                    ui::SpacingScale::Md,
                    vec![
                        ui_builder::section(
                            "toolbar",
                            "Reader",
                            ui::SpacingScale::Sm,
                            vec![ui_builder::icon_button(
                                "bookmark-btn",
                                "bookmark",
                                "Bookmark selection",
                                "bookmark_add",
                                false,
                            )],
                        ),
                        ui_builder::controlled_surface_ref(
                            "reader-surface",
                            "surface-epub-10",
                            ui::ControlledSurfaceKind::ReaderDocument,
                        ),
                    ],
                )),
            ],
        );

        let tree = ui_builder::ui_tree(1, root);
        tree.validate(UiBudgets::default())
            .expect("all constructed declarative components must validate");
    }
}
