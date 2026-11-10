//! Ordinary signed-reader qualification over the public Surface wire and unique Host authority.
//! This is trusted native composition, not code linked into the reader package.
use crate::reader_authority::ReaderAuthority;
use core_api::{
    contract::CaptureResponse,
    knowledge::{PdfDocumentRecord, PdfReceipt, Provenance},
    pdf_compound::{PdfCompoundCaptureRequest, PdfCompoundDraft},
    pdf_contract::{PDF_TEXT_UNIT, PdfCaptureRequest, PdfDraft, PdfSnapshot},
};
use extension_api::{
    Capability, CapabilityRequest, ContributionKind, HOST_API_V0_2, Resource, SUPPORTED_HOST_APIS,
    invocation::InvocationRequest, surface::*,
};
use extension_host::{
    durable::DurableSession,
    surface::{
        NativeFrameTicket, NativeGestureTicket, NativeInputTicket, TrustedSurfaceDocument,
        TrustedSurfaceSelection,
    },
};
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_pdf_provider_probe::{
    DocumentReport,
    selection::{DragSelection, Point, PointerOutcome, PointerSession, VersionBinding, Viewport},
};
use runtime_wasm::{ExecutionLimits, ModuleExecutor, RuntimeError, WasmiRuntime};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};
use storage_sqlite::SqliteStore;
type Result<T> = std::result::Result<T, String>;
fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let f = std::fs::File::open(path).map_err(text)?;
    if !f.metadata().map_err(text)?.is_file() {
        return Err("input must be a regular file".into());
    }
    let mut bytes = Vec::new();
    f.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(text)?;
    if bytes.len() > limit {
        return Err("native input budget exceeded".into());
    }
    Ok(bytes)
}
#[derive(Clone)]
enum Draft {
    Single(PdfDraft),
    Compound(PdfCompoundDraft),
}
impl Draft {
    fn protocol(&self) -> u16 {
        match self {
            Self::Single(_) => 1,
            Self::Compound(_) => 2,
        }
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Single(d) => d.canonical_bytes().map_err(text),
            Self::Compound(d) => d.canonical_bytes().map_err(text),
        }
    }
    fn hash(&self) -> Result<String> {
        match self {
            Self::Single(d) => d.draft_hash().map_err(text),
            Self::Compound(d) => d.draft_hash().map_err(text),
        }
    }
    fn exact(&self) -> &str {
        match self {
            Self::Single(d) => &d.exact,
            Self::Compound(d) => &d.exact,
        }
    }
    fn resolve(&self, snapshot: &PdfSnapshot) -> Result<()> {
        match self {
            Self::Single(d) => {
                snapshot.resolve(d).map_err(text)?;
            }
            Self::Compound(d) => {
                d.resolve(snapshot).map_err(text)?;
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Candidate {
    id: String,
    operation: String,
    selection_request: SurfaceRequest,
    draft: Draft,
    hash: String,
    receipt: Option<PdfReceipt>,
    saved_note: Option<String>,
}
struct Gate {
    host: ReaderAuthority,
    session: DurableSession,
    core: KnowledgeApplication<SqliteStore>,
    document: PdfDocumentRecord,
    report: Arc<DocumentReport>,
    report_digest: String,
    origin: Provenance,
    descriptor: Option<SurfaceDescriptor>,
    candidate: Option<Candidate>,
    boot: String,
    next: u64,
}
struct NativeState {
    frame: NativeFrameTicket,
    report: Arc<DocumentReport>,
    input: Option<NativeInputTicket>,
    session: PointerSession,
    binding: VersionBinding,
    viewport: Viewport,
    active: Option<(u64, u64, NativeGestureTicket)>,
}
struct Service {
    gate: Mutex<Gate>,
    native: Mutex<BTreeMap<String, NativeState>>,
    wasm: Vec<u8>,
    contribution: String,
}
static SERVICE: Mutex<Option<Arc<Service>>> = Mutex::new(None);
fn service() -> Result<Arc<Service>> {
    SERVICE
        .lock()
        .map_err(|_| "native service unavailable")?
        .as_ref()
        .cloned()
        .ok_or("native service unavailable".into())
}
fn mutex<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(|_| "native state lock poisoned".into())
}
fn policy(
    g: &mut Gate,
    capability: Capability,
    resource: Resource,
    request_id: &str,
) -> Result<()> {
    let version = matches!(resource, Resource::Document { .. })
        .then(|| g.document.receipt.extraction_revision.clone());
    g.host.policy(
        &g.session,
        CapabilityRequest {
            api_version: HOST_API_V0_2,
            request_id: request_id.into(),
            capability,
            resource,
        },
        version,
    )
}
fn next(g: &mut Gate, prefix: &str) -> Result<String> {
    g.next = g.next.checked_add(1).ok_or("native sequence exhausted")?;
    Ok(format!("{prefix}-{}-{}", g.boot, g.next))
}
fn document_resource(g: &Gate) -> Resource {
    Resource::Document {
        vault_id: g.document.vault.clone(),
        document_id: g.document.receipt.document_id.to_string(),
    }
}
fn source(g: &Gate) -> SourceVersion {
    SourceVersion {
        original_sha256: g.document.receipt.original_sha256.clone(),
        extraction_revision: g.document.receipt.extraction_revision.clone(),
    }
}
fn source_document(g: &Gate) -> TrustedSurfaceDocument {
    TrustedSurfaceDocument {
        resource: document_resource(g),
        source: source(g),
        page_count: g.document.snapshot.pages.len() as u32,
    }
}
/// Native startup accepts fixed fixture resources and an explicitly trusted test key.
/// It persists the signed package first; no manifest auto-grants privileges.
pub fn initialize(resources: &Path, data: &Path) -> Result<()> {
    let report_bytes = read_bounded(&resources.join("report.json"), 8 * 1024 * 1024)?;
    let report: DocumentReport = serde_json::from_slice(&report_bytes).map_err(text)?;
    let snapshot: PdfSnapshot = serde_json::from_slice(&read_bounded(
        &resources.join("snapshot.json"),
        8 * 1024 * 1024,
    )?)
    .map_err(text)?;
    let original = read_bounded(&resources.join("original.pdf"), 16 * 1024 * 1024)?;
    snapshot.validate_original(&original).map_err(text)?;
    if snapshot.original_sha256 != report.original_sha256
        || snapshot.engine_report_sha256 != report.extraction_sha256
    {
        return Err("native report and retained source disagree".into());
    }
    initialize_document(
        resources,
        data,
        PreparedDocument {
            report,
            report_digest: qusor_pdf_provider_probe::sha256(&report_bytes),
            snapshot,
            original,
            title: "text-two-pages.pdf".into(),
            operation: "fixture-document-import".into(),
        },
    )
}

/// Trusted native worker output, never deserialized from guest input.
pub struct PreparedDocument {
    pub report: DocumentReport,
    pub report_digest: String,
    pub snapshot: PdfSnapshot,
    pub original: Vec<u8>,
    pub title: String,
    pub operation: String,
}
/// Project the trusted provider report using the same neutral contract as Core.
pub fn snapshot_from_report(report: &DocumentReport) -> Result<PdfSnapshot> {
    let mut extraction = serde_json::to_value(&report.extractor).map_err(text)?;
    extraction
        .as_object_mut()
        .ok_or("invalid extractor")?
        .insert("provider".into(), json!("pdfium"));
    let pages = report.pages.iter().map(|page| {
        let rotation = match page.rotation.as_str() { "None"=>0, "Degrees90"=>90, "Degrees180"=>180, "Degrees270"=>270, _=>return Err("unsupported rotation".into()) };
        let characters = page.characters.iter().map(|character| {
            let mut value = serde_json::to_value(character).map_err(text)?;
            let object = value.as_object_mut().ok_or("invalid character")?;
            object.remove("crop_normalized_quad");object.remove("pixel_quad"); Ok(value)
        }).collect::<Result<Vec<Value>>>()?;
        Ok(json!({"page_index":page.page_index,"engine_text":page.page_text,"text_sha256":page.page_text_sha256,
            "rotation_degrees":rotation,"media_box":page.media_box,"crop_box":page.crop_box,"characters":characters}))
    }).collect::<Result<Vec<Value>>>()?;
    serde_json::from_value(json!({"protocol_version":1,"original_sha256":report.original_sha256,"original_bytes":report.original_bytes,
        "extraction":extraction,"engine_report_sha256":report.extraction_sha256,"pages":pages})).map_err(text)
}
fn validate_prepared(document: &PreparedDocument) -> Result<()> {
    document
        .snapshot
        .validate_original(&document.original)
        .map_err(text)?;
    if document.snapshot.original_sha256 != document.report.original_sha256
        || document.snapshot.engine_report_sha256 != document.report.extraction_sha256
    {
        return Err("native report and retained source disagree".into());
    }
    if snapshot_from_report(&document.report)? != document.snapshot {
        return Err("report and Core semantic snapshot disagree".into());
    }
    for page in 0..document.report.pages.len() {
        VersionBinding::from_report(&document.report, page).map_err(text)?;
    }
    Ok(())
}
/// Reuse the same package/Core writer across document changes in one process.
pub enum DocumentInitialization {
    Ready,
    ImportedUnavailable { message: String },
}
pub fn initialize_document(
    resources: &Path,
    data: &Path,
    prepared: PreparedDocument,
) -> Result<()> {
    match initialize_document_mode(resources, data, prepared, false)? {
        DocumentInitialization::Ready => Ok(()),
        DocumentInitialization::ImportedUnavailable { message } => Err(message),
    }
}
pub fn initialize_document_checked(
    resources: &Path,
    data: &Path,
    prepared: PreparedDocument,
) -> Result<DocumentInitialization> {
    initialize_document_mode(resources, data, prepared, true)
}
fn initialize_document_mode(
    resources: &Path,
    data: &Path,
    prepared: PreparedDocument,
    allow_scope_change: bool,
) -> Result<DocumentInitialization> {
    let committed = std::cell::Cell::new(false);
    match initialize_document_inner(resources, data, prepared, &committed, allow_scope_change) {
        Ok(()) => Ok(DocumentInitialization::Ready),
        Err(error) if committed.get() => {
            eprintln!("READER_IMPORT_COMMITTED_AUTHORITY_FAILED {error}");
            Ok(DocumentInitialization::ImportedUnavailable {
                message: "资料已导入，但阅读权限未准备好。".into(),
            })
        }
        Err(error) => Err(error),
    }
}
fn initialize_document_inner(
    resources: &Path,
    data: &Path,
    prepared: PreparedDocument,
    committed: &std::cell::Cell<bool>,
    allow_scope_change: bool,
) -> Result<()> {
    validate_prepared(&prepared)?;
    if let Ok(existing) = service() {
        return existing.replace_document(prepared, committed, allow_scope_change);
    }
    let key = String::from_utf8(read_bounded(&resources.join("publisher-key.txt"), 512)?)
        .map_err(text)?;
    let bytes = read_bounded(
        &resources.join("reader.package"),
        extension_package::package::MAX_PACKAGE_BYTES,
    )?;
    let verified =
        extension_package::package::verify_for_host(&bytes, key.trim(), &SUPPORTED_HOST_APIS)
            .map_err(text)?;
    let mut nonce = [0; 16];
    getrandom::getrandom(&mut nonce).map_err(text)?;
    let (mut host, installation, package) =
        ReaderAuthority::install(&data.join("packages"), &verified, nonce)?;
    let mut core =
        KnowledgeApplication::new(SqliteStore::open(data.join("knowledge")).map_err(text)?);
    let receipt = core
        .import_document(
            &prepared.operation,
            "surface-vault",
            &prepared.title,
            &prepared.original,
            &prepared.snapshot,
        )
        .map_err(text)?;
    committed.set(true);
    let document = core.query_document(receipt.document_id).map_err(text)?;
    // A cold start can import a different document while retaining the journal.
    // Each trusted open action needs its own operation identity; persisted grants
    // still retain their original expiry, capability set and disabled state.
    let grant_operation = format!("reader-grants-{}", qusor_pdf_provider_probe::sha256(&nonce));
    let session = host.authorize_document(&document, &grant_operation, allow_scope_change)?;
    let contribution = package
        .manifest()
        .contributions
        .iter()
        .find(|c| c.kind == ContributionKind::ReaderWorkspace)
        .ok_or("package has no reading workspace")?
        .id
        .clone();
    let origin = Provenance {
        package_id: package.manifest().package_id.clone(),
        publisher_id: package.manifest().publisher_id.clone(),
        package_digest: package.digest().into(),
        installation_id: installation.installation_id,
    };
    let wasm = package.module_bytes().to_vec();
    let state = Service {
        gate: Mutex::new(Gate {
            host,
            session,
            core,
            document,
            report: Arc::new(prepared.report),
            report_digest: prepared.report_digest,
            origin,
            descriptor: None,
            candidate: None,
            boot: nonce.iter().map(|b| format!("{b:02x}")).collect(),
            next: 0,
        }),
        native: Mutex::new(BTreeMap::new()),
        wasm,
        contribution,
    };
    *SERVICE.lock().map_err(|_| "native service lock poisoned")? = Some(Arc::new(state));
    Ok(())
}
impl Service {
    fn replace_document(
        &self,
        prepared: PreparedDocument,
        committed: &std::cell::Cell<bool>,
        allow_scope_change: bool,
    ) -> Result<()> {
        // Native parsing/rendering already completed outside this short Core/Host gate.
        // A failed Core import must leave the current document and its draft usable.
        let mut g = mutex(&self.gate)?;
        let receipt = g
            .core
            .import_document(
                &prepared.operation,
                "surface-vault",
                &prepared.title,
                &prepared.original,
                &prepared.snapshot,
            )
            .map_err(text)?;
        committed.set(true);
        let document = g.core.query_document(receipt.document_id).map_err(text)?;
        let operation = next(&mut g, "grant-document")?;
        g.session = g
            .host
            .authorize_document(&document, &operation, allow_scope_change)?;
        for native in mutex(&self.native)?.values_mut() {
            native.session.cancel();
            native.active = None;
            native.input = None;
        }
        g.document = document;
        g.report = Arc::new(prepared.report);
        g.report_digest = prepared.report_digest;
        g.descriptor = None;
        g.candidate = None;
        Ok(())
    }
    fn transition(
        &self,
        g: &mut Gate,
        t: extension_host::surface::SurfaceTransition,
    ) -> Result<SurfaceDescriptor> {
        let d = t.descriptor;
        let binding =
            VersionBinding::from_report(&g.report, d.page_index as usize).map_err(text)?;
        let viewport = Viewport {
            generation: d.generation,
            zoom: d.zoom,
            ..Viewport::default()
        };
        let session = PointerSession::new(binding.clone(), viewport.clone()).map_err(text)?;
        mutex(&self.native)?.insert(
            d.handle.clone(),
            NativeState {
                frame: t.frame_ticket,
                report: g.report.clone(),
                input: None,
                session,
                binding,
                viewport,
                active: None,
            },
        );
        g.candidate = None;
        g.descriptor = Some(d.clone());
        Ok(d)
    }
    fn request(&self, operation: SurfaceOperation) -> Result<SurfaceRequest> {
        let mut g = mutex(&self.gate)?;
        Ok(SurfaceRequest {
            surface_protocol: SURFACE_PROTOCOL,
            api_version: HOST_API_V0_2,
            request_id: next(&mut g, "reader")?,
            operation,
        })
    }
    fn current(&self, handle: &str, generation: u64) -> Result<SurfaceDescriptor> {
        let g = mutex(&self.gate)?;
        let d = g.host.surface_state(&g.session, handle).map_err(text)?;
        if d.generation != generation {
            return Err("stale Surface presentation".into());
        }
        Ok(d)
    }
    fn invoke(&self, request: SurfaceRequest) -> Result<Value> {
        let context = request.to_json().map_err(text)?.into_bytes();
        let mut proposal: Option<Candidate> = None;
        let mut expected_response: Option<String> = None;
        let mut selection_seen: Option<SurfaceResponse> = None;
        let mut surface_called = false;
        let mut proposal_called = false;
        let mut touched: Option<(String, bool)> = None;
        let previous = mutex(&self.gate)?.descriptor.clone();
        let executed = {
            let mut broker = |bytes: &[u8]| -> std::result::Result<Vec<u8>, RuntimeError> {
                let inner = (|| -> Result<Vec<u8>> {
                    let mut g = mutex(&self.gate)?;
                    if InvocationRequest::from_slice(bytes).is_ok() {
                        g.host
                            .preflight_surface_request(&g.session, &request)
                            .map_err(text)?;
                        if let SurfaceOperation::Open {
                            resource, expected, ..
                        } = &request.operation
                            && (resource != &document_resource(&g) || expected != &source(&g))
                        {
                            return Err("invocation source changed".into());
                        }
                        let cap = if matches!(
                            request.operation,
                            SurfaceOperation::ReadSelection { .. }
                        ) {
                            Capability::SelectionRead
                        } else {
                            Capability::ReaderSurface
                        };
                        let resource = document_resource(&g);
                        policy(&mut g, cap, resource, &request.request_id)?;
                        return Ok(context.clone());
                    }
                    if let Ok(surface) = SurfaceRequest::from_slice(bytes) {
                        if surface != request {
                            return Err("guest changed this invocation's command binding".into());
                        }
                        if surface_called {
                            return Err("one invocation cannot repeat a Surface operation".into());
                        }
                        surface_called = true;
                        if let SurfaceOperation::Configure { zoom, .. } = &surface.operation
                            && !(0.25..=1.25).contains(zoom)
                        {
                            return Err("native display zoom budget exceeded".into());
                        }
                        let result = match &surface.operation {
                            SurfaceOperation::Open { .. } => {
                                let doc = source_document(&g);
                                let session = g.session.clone();
                                let transition =
                                    g.host.open_surface(&session, &surface, doc).map_err(text)?;
                                touched = Some((transition.descriptor.handle.clone(), true));
                                SurfaceResult::State {
                                    surface: self.transition(&mut g, transition)?,
                                }
                            }
                            SurfaceOperation::Configure { .. } => {
                                let session = g.session.clone();
                                let transition =
                                    g.host.configure_surface(&session, &surface).map_err(text)?;
                                touched = Some((transition.descriptor.handle.clone(), false));
                                SurfaceResult::State {
                                    surface: self.transition(&mut g, transition)?,
                                }
                            }
                            SurfaceOperation::ReadSelection { .. } => {
                                let result = g
                                    .host
                                    .read_surface_selection(&g.session, &surface)
                                    .map_err(text)?;
                                selection_seen = Some(result.clone());
                                result.result
                            }
                            SurfaceOperation::Close { .. } => {
                                let session = g.session.clone();
                                let d = g.host.close_surface(&session, &surface).map_err(text)?;
                                touched = Some((d.handle.clone(), false));
                                g.descriptor = Some(d.clone());
                                g.candidate = None;
                                if let Some(native) = mutex(&self.native)?.get_mut(&d.handle) {
                                    native.session.cancel();
                                    native.active = None;
                                    native.input = None;
                                }
                                SurfaceResult::State { surface: d }
                            }
                        };
                        let response = SurfaceResponse {
                            surface_protocol: SURFACE_PROTOCOL,
                            api_version: HOST_API_V0_2,
                            request_id: surface.request_id.clone(),
                            result,
                        };
                        let output = response.to_json().map_err(text)?;
                        expected_response = Some(output.clone());
                        return Ok(output.into_bytes());
                    }
                    let seen = selection_seen
                        .as_ref()
                        .ok_or("capture proposal did not read this Surface selection")?;
                    if proposal_called {
                        return Err("one invocation cannot repeat a capture proposal".into());
                    }
                    proposal_called = true;
                    let SurfaceResult::Selection {
                        capture_protocol,
                        draft_json,
                        ..
                    } = &seen.result
                    else {
                        return Err("selection response expected".into());
                    };
                    // Validate current authorization/lease again; no filesystem I/O occurs in the VM broker.
                    let fresh = g
                        .host
                        .read_surface_selection(&g.session, &request)
                        .map_err(text)?;
                    if fresh != *seen {
                        return Err("selection changed during proposal".into());
                    }
                    let (draft, request_id) = match capture_protocol {
                        1 => {
                            let p = PdfCaptureRequest::from_slice(bytes).map_err(text)?;
                            let expected =
                                PdfDraft::from_slice(draft_json.as_bytes()).map_err(text)?;
                            if p.draft != expected {
                                return Err("proposal changed the complete native selection".into());
                            }
                            (Draft::Single(p.draft), p.request_id)
                        }
                        2 => {
                            let p = PdfCompoundCaptureRequest::from_slice(bytes).map_err(text)?;
                            let expected = PdfCompoundDraft::from_slice(draft_json.as_bytes())
                                .map_err(text)?;
                            if p.draft != expected {
                                return Err("proposal changed the complete native selection".into());
                            }
                            (Draft::Compound(p.draft), p.request_id)
                        }
                        _ => return Err("unsupported complete selection protocol".into()),
                    };
                    let resource = Resource::Vault {
                        vault_id: g.document.vault.clone(),
                    };
                    policy(&mut g, Capability::CapturePropose, resource, &request_id)?;
                    let id = next(&mut g, "candidate")?;
                    let operation = next(&mut g, "surface-save")?;
                    let hash = draft.hash()?;
                    let candidate = Candidate {
                        id: id.clone(),
                        operation,
                        selection_request: request.clone(),
                        draft,
                        hash: hash.clone(),
                        receipt: None,
                        saved_note: None,
                    };
                    let response = CaptureResponse {
                        capture_protocol: *capture_protocol,
                        request_id,
                        candidate_id: id,
                        draft_hash: hash,
                    };
                    let output = serde_json::to_string(&response).map_err(text)?;
                    proposal = Some(candidate);
                    expected_response = Some(output.clone());
                    Ok(output.into_bytes())
                })();
                inner.map_err(|error| {
                    eprintln!("READER_BROKER_REJECT {error}");
                    RuntimeError::new("reader_broker_rejected")
                })
            };
            WasmiRuntime.execute(&self.wasm, &ExecutionLimits::default(), 4096, &mut broker)
        };
        let result = match executed {
            Ok(result) => result,
            Err(error) => {
                self.reject_invocation(&touched, &previous)?;
                return Err(text(error));
            }
        };
        if expected_response.as_deref() != Some(result.output.as_str()) {
            self.reject_invocation(&touched, &previous)?;
            return Err("guest response is not the complete accepted invocation result".into());
        }
        let mut g = mutex(&self.gate)?;
        if let Some(candidate) = proposal {
            let _ = g
                .host
                .read_surface_selection(&g.session, &candidate.selection_request)
                .map_err(text)?;
            candidate.draft.resolve(&g.document.snapshot)?; // Outside VM: verify the immutable Core map.
            let value = json!({"kind":"candidate","candidate_id":candidate.id,"draft_hash":candidate.hash,"text":candidate.draft.exact(),"capture_protocol":candidate.draft.protocol(),"handle":match &candidate.selection_request.operation{SurfaceOperation::ReadSelection{handle,..}=>handle,_=>unreachable!()},"generation":match &candidate.selection_request.operation{SurfaceOperation::ReadSelection{generation,..}=>*generation,_=>0}});
            g.candidate = Some(candidate);
            eprintln!("READER_GUEST_RESULT {value}");
            return Ok(value);
        }
        let response = SurfaceResponse::from_json(&result.output).map_err(text)?;
        if let SurfaceResult::State { surface } = &response.result {
            let fresh = if matches!(request.operation, SurfaceOperation::Close { .. }) {
                g.host.surface_result_state(&g.session, &surface.handle)
            } else {
                g.host.surface_state(&g.session, &surface.handle)
            };
            if fresh.as_ref().ok() != Some(surface) {
                drop(g);
                self.reject_invocation(&touched, &previous)?;
                return Err("Surface changed before invocation completion".into());
            }
        }
        let value = serde_json::to_value(response).map_err(text)?;
        eprintln!("READER_GUEST_RESULT {value}");
        Ok(value)
    }
    /// A failed guest never leaves a usable, unowned Surface behind.
    fn reject_invocation(
        &self,
        touched: &Option<(String, bool)>,
        previous: &Option<SurfaceDescriptor>,
    ) -> Result<()> {
        let Some((handle, unopened)) = touched else {
            return Ok(());
        };
        let mut g = mutex(&self.gate)?;
        let close = SurfaceRequest {
            surface_protocol: SURFACE_PROTOCOL,
            api_version: HOST_API_V0_2,
            request_id: "failed-invocation-close".into(),
            operation: SurfaceOperation::Close {
                handle: handle.clone(),
            },
        };
        let session = g.session.clone();
        let _ = g.host.close_surface(&session, &close);
        g.candidate = None;
        let mut map = mutex(&self.native)?;
        if let Some(native) = map.get_mut(handle) {
            native.session.cancel();
            native.active = None;
            native.input = None;
        }
        if *unopened {
            // This fixture backend starts no rendering job during Open: only
            // neutral PointerSession state is allocated. A failed VM has never
            // returned the handle to TSX, so no Image/native mount can exist.
            map.remove(handle);
            for ticket in g
                .host
                .pending_surface_cleanup()?
                .into_iter()
                .filter(|t| t.handle() == handle)
            {
                g.host.confirm_surface_closed(&ticket).map_err(text)?;
            }
            g.descriptor = previous
                .as_ref()
                .and_then(|old| g.host.surface_state(&g.session, &old.handle).ok());
        } else if let Ok(closing) = g.host.surface_result_state(&g.session, handle) {
            // A previously mounted frame has no NSH release acknowledgement;
            // preserve Closing until actual process termination.
            g.descriptor = Some(closing);
        }
        eprintln!(
            "READER_INVOCATION_REJECTED {}",
            json!({"handle":handle,"unmounted_open":unopened,"pending_cleanup":g.host.pending_surface_cleanup()?.len()})
        );
        Ok(())
    }
    fn render_config(&self, d: &SurfaceDescriptor) -> Result<Value> {
        let g = mutex(&self.gate)?;
        if d.resource != document_resource(&g) || d.source != source(&g) {
            return Err("stale rendered document".into());
        }
        let live = g.host.surface_state(&g.session, &d.handle)?;
        if live.generation != d.generation
            || live.page_index != d.page_index
            || live.page_count != d.page_count
            || live.zoom != d.zoom
            || live.source != d.source
            || live.resource != d.resource
        {
            return Err("stale rendered frame".into());
        }
        let map = mutex(&self.native)?;
        let native = map.get(&d.handle).ok_or("native frame missing")?;
        if native.frame.generation() != d.generation
            || native.frame.page_index() != d.page_index
            || native.frame.source() != &d.source
            || native.frame.resource() != &d.resource
            || live
                .frame
                .as_ref()
                .is_some_and(|frame| frame.frame_id != native.frame.frame_id())
        {
            return Err("native frame metadata changed".into());
        }
        let p = native
            .report
            .pages
            .get(d.page_index as usize)
            .ok_or("native page missing")?;
        Ok(
            json!({"handle":d.handle,"generation":d.generation,"frame_id":native.frame.frame_id(),
            "page":d.page_index,"pages":d.page_count,"zoom":d.zoom,"width":p.pixel_width as f64*d.zoom,
            "height":p.pixel_height as f64*d.zoom,"status":live.status,"report_digest":g.report_digest,
            "document_id":g.document.receipt.document_id,"title":g.document.title}),
        )
    }

    fn notes(&self) -> Result<Value> {
        let g = mutex(&self.gate)?;
        let notes = g
            .core
            .pdf_notes(
                &g.document.vault,
                None,
                learning_application::pdf_notes::MAX_NOTE_QUERY,
            )
            .map_err(text)?;
        Ok(json!({
            "current_document_id": g.document.receipt.document_id,
            "notes": notes.into_iter().map(|note| json!({
                "capture_id": note.capture_id,
                "commit_rev": note.commit_rev,
                "operation_id": note.operation_id,
                "document_id": note.document_id,
                "document_title": note.document_title,
                "page_index": note.page_index,
                "quote_preview": note.quote_preview,
                "quote_bytes": note.quote_bytes,
                "quote_truncated": note.quote_truncated,
                "note_preview": note.note_preview,
                "note_bytes": note.note_bytes,
                "note_truncated": note.note_truncated,
                "compound": note.compound,
            })).collect::<Vec<_>>()
        }))
    }

    fn draft(&self, g: &Gate, selection: &DragSelection) -> Result<Draft> {
        if selection.binding.original_sha256 != g.report.original_sha256
            || selection.binding.frame_report_sha256 != g.report.extraction_sha256
        {
            return Err("selection belongs to another rendered source".into());
        }
        let page = selection.binding.page_index;
        let mapped = &g.document.snapshot.pages[page];
        let segments = selection
            .segments
            .iter()
            .map(|s| PdfDraft {
                protocol_version: 1,
                original_sha256: g.document.receipt.original_sha256.clone(),
                extraction_revision: g.document.receipt.extraction_revision.clone(),
                page_index: page as u32,
                page_text_sha256: mapped.text_sha256.clone(),
                unit: PDF_TEXT_UNIT.into(),
                start: s.start as u64,
                end: s.end as u64,
                exact: s.exact.clone(),
            })
            .collect::<Vec<_>>();
        let draft = if segments.len() == 1 {
            Draft::Single(segments[0].clone())
        } else {
            Draft::Compound(PdfCompoundDraft {
                protocol_version: 2,
                original_sha256: g.document.receipt.original_sha256.clone(),
                extraction_revision: g.document.receipt.extraction_revision.clone(),
                selection_profile: selection.layout_profile.clone(),
                separator: selection.separator.clone(),
                segments,
                exact: selection.joined_text.clone(),
            })
        };
        draft.resolve(&g.document.snapshot)?;
        Ok(draft)
    }
}
/// Immutable metadata bound to a live native frame; never given to guest code.
#[derive(Clone)]
pub struct NativeFrameContext {
    // Trusted Rust-only ticket. Never serialized into guest/JS input.
    pub(crate) ticket: NativeFrameTicket,
    pub config: Value,
    pub report: Arc<DocumentReport>,
    pub report_digest: String,
    pub page_index: usize,
}
pub fn native_frame_context(
    handle: &str,
    generation: u64,
    frame_id: &str,
) -> Result<NativeFrameContext> {
    let s = service()?;
    let d = s.current(handle, generation)?;
    let (report, ticket) = {
        let g = mutex(&s.gate)?;
        let map = mutex(&s.native)?;
        let n = map.get(handle).ok_or("native frame missing")?;
        if n.frame.frame_id() != frame_id
            || n.frame.generation() != generation
            || d.source != source(&g)
        {
            return Err("stale native frame request".into());
        }
        (n.report.clone(), n.frame.clone())
    };
    let config = s.render_config(&d)?;
    if config["frame_id"].as_str() != Some(frame_id)
        || config["generation"].as_u64() != Some(generation)
    {
        return Err("native frame changed during image request".into());
    }
    Ok(NativeFrameContext {
        ticket,
        report_digest: config["report_digest"]
            .as_str()
            .ok_or("report digest missing")?
            .into(),
        config,
        report,
        page_index: d.page_index as usize,
    })
}
pub fn current_report_digest() -> Result<String> {
    Ok(mutex(&service()?.gate)?.report_digest.clone())
}
#[module]
pub struct ReaderSurface;
fn answer(f: impl FnOnce(Arc<Service>) -> Result<Value>) -> String {
    let result = service().and_then(f);
    let value = match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    let output = value.to_string();
    eprintln!("READER_NATIVE {output}");
    output
}
#[module]
impl ReaderSurface {
    pub fn notes() -> String {
        answer(|s| s.notes())
    }
    pub fn open(report_digest: String) -> String {
        answer(|s| {
            let (resource, expected) = {
                let g = mutex(&s.gate)?;
                if report_digest != g.report_digest {
                    return Err("display/report binding mismatch".into());
                }
                (document_resource(&g), source(&g))
            };
            s.invoke(s.request(SurfaceOperation::Open {
                resource,
                expected,
                contribution_id: s.contribution.clone(),
            })?)?;
            let d = mutex(&s.gate)?
                .descriptor
                .clone()
                .ok_or("Surface open missing")?;
            s.render_config(&d)
        })
    }
    pub fn configure(handle: String, generation: u64, page: u32, zoom: f64) -> String {
        answer(|s| {
            s.current(&handle, generation)?;
            s.invoke(s.request(SurfaceOperation::Configure {
                handle: handle.clone(),
                generation,
                page_index: page,
                zoom,
            })?)?;
            let d = mutex(&s.gate)?
                .descriptor
                .clone()
                .ok_or("Surface configure missing")?;
            s.render_config(&d)
        })
    }
    pub fn ready(
        handle: String,
        generation: u64,
        frame_id: String,
        report_digest: String,
    ) -> String {
        answer(|s| {
            let d = s.current(&handle, generation)?;
            let mut g = mutex(&s.gate)?;
            if report_digest != g.report_digest {
                return Err("display/report binding mismatch".into());
            }
            let mut map = mutex(&s.native)?;
            let native = map.get_mut(&handle).ok_or("Surface backend missing")?;
            if native.frame.generation() != generation || native.frame.frame_id() != frame_id {
                return Err("stale loaded frame".into());
            }
            let p = &native.report.pages[d.page_index as usize];
            let visible = g
                .host
                .native_frame_ready(
                    &native.frame,
                    FrameMetadata {
                        frame_id,
                        pixel_width: u32::try_from(p.pixel_width).map_err(text)?,
                        pixel_height: u32::try_from(p.pixel_height).map_err(text)?,
                    },
                )
                .map_err(text)?;
            g.descriptor = Some(visible);
            native.input = Some(g.host.native_input_ticket(&native.frame).map_err(text)?);
            Ok(json!({"handle":handle,"generation":generation,"ready":true}))
        })
    }
    pub fn down(handle: String, generation: u64, pointer: u64, x: f64, y: f64) -> String {
        answer(|s| {
            s.current(&handle, generation)?;
            let mut g = mutex(&s.gate)?;
            let mut map = mutex(&s.native)?;
            let n = map.get_mut(&handle).ok_or("Surface backend missing")?;
            let input = n.input.as_ref().ok_or("page frame is not visible")?;
            if n.active.is_some() {
                return Err("another pointer owns the active gesture".into());
            }
            g.host.native_input_ticket(&n.frame).map_err(text)?;
            g.candidate = None;
            let result =
                n.session
                    .pointer_down(&n.report, &n.binding, &n.viewport, pointer, Point { x, y });
            let result = match result {
                Ok(value) => value,
                Err(error) => {
                    g.host.clear_surface_selection(input).map_err(text)?;
                    return Err(text(error));
                }
            };
            let PointerOutcome::Dragging { gesture_id, .. } = result else {
                return Err("native down outcome invalid".into());
            };
            let ticket = match g.host.surface_begin_gesture(input) {
                Ok(ticket) => ticket,
                Err(error) => {
                    n.session.cancel();
                    return Err(text(error));
                }
            };
            n.active = Some((pointer, gesture_id, ticket));
            Ok(
                json!({"handle":handle,"generation":generation,"frame_id":n.frame.frame_id(),"gesture":gesture_id,"boxes":[],"text":""}),
            )
        })
    }
    pub fn motion(
        handle: String,
        generation: u64,
        pointer: u64,
        gesture: u64,
        x: f64,
        y: f64,
    ) -> String {
        answer(|s| s.pointer(&handle, generation, pointer, gesture, Point { x, y }, false))
    }
    pub fn up(
        handle: String,
        generation: u64,
        pointer: u64,
        gesture: u64,
        x: f64,
        y: f64,
    ) -> String {
        answer(|s| s.pointer(&handle, generation, pointer, gesture, Point { x, y }, true))
    }
    pub fn cancel(handle: String, generation: u64, pointer: u64, gesture: u64) -> String {
        answer(|s| {
            s.current(&handle, generation)?;
            let mut g = mutex(&s.gate)?;
            let mut map = mutex(&s.native)?;
            let n = map.get_mut(&handle).ok_or("Surface backend missing")?;
            if !n
                .active
                .as_ref()
                .is_some_and(|(p, t, _)| *p == pointer && *t == gesture)
            {
                return Err("stale gesture cancellation".into());
            }
            if let Some((_, _, ticket)) = n.active.take() {
                g.host.cancel_surface_gesture(&ticket).map_err(text)?;
            }
            n.session.cancel();
            g.candidate = None;
            Ok(json!({"handle":handle,"generation":generation,"boxes":[],"text":""}))
        })
    }
    pub fn save(candidate_id: String, note: String) -> String {
        answer(|s| {
            let mut g = mutex(&s.gate)?;
            let mut candidate = g.candidate.clone().ok_or("no complete proposal to save")?;
            if candidate.id != candidate_id {
                return Err("stale proposal".into());
            }
            let _ = g
                .host
                .read_surface_selection(&g.session, &candidate.selection_request)
                .map_err(text)?;
            let resource = Resource::Vault {
                vault_id: g.document.vault.clone(),
            };
            policy(&mut g, Capability::CapturePropose, resource, "native-save")?;
            if candidate
                .saved_note
                .as_ref()
                .is_some_and(|old| old != &note)
            {
                return Err("saved operation payload changed".into());
            }
            let document = g
                .core
                .query_document(g.document.receipt.document_id)
                .map_err(text)?;
            candidate.draft.resolve(&document.snapshot)?;
            let origin = g.origin.clone();
            let receipt = match &candidate.draft {
                Draft::Single(d) => g.core.capture_pdf(
                    &candidate.operation,
                    document.receipt.document_id,
                    d,
                    &origin,
                    &note,
                ),
                Draft::Compound(d) => g.core.capture_pdf_compound(
                    &candidate.operation,
                    document.receipt.document_id,
                    d,
                    &origin,
                    &note,
                ),
            }
            .map_err(text)?;
            candidate.receipt = Some(receipt.clone());
            candidate.saved_note = Some(note);
            g.candidate = Some(candidate.clone());
            Ok(json!({"candidate_id":candidate.id,"saved":true,"receipt":receipt}))
        })
    }
    pub fn failed(handle: String, generation: u64, frame_id: String) -> String {
        answer(|s| {
            s.current(&handle, generation)?;
            let mut g = mutex(&s.gate)?;
            let mut map = mutex(&s.native)?;
            let n = map.get_mut(&handle).ok_or("Surface backend missing")?;
            if n.frame.frame_id() != frame_id {
                return Err("stale failed frame".into());
            }
            let d = g.host.native_frame_failed(&n.frame).map_err(text)?;
            n.session.cancel();
            n.active = None;
            n.input = None;
            g.candidate = None;
            g.descriptor = Some(d);
            Ok(json!({"handle":handle,"generation":generation,"failed":true}))
        })
    }
    pub fn disable() -> String {
        answer(|s| {
            let mut g = mutex(&s.gate)?;
            let operation = next(&mut g, "disable")?;
            g.host.disable(&operation)?;
            g.candidate = None;
            for n in mutex(&s.native)?.values_mut() {
                n.session.cancel();
                n.active = None;
                n.input = None;
            }
            Ok(json!({"disabled":true,"cleanup_pending":g.host.pending_surface_cleanup()?.len()}))
        })
    }
    pub fn close(handle: String) -> String {
        answer(|s| s.invoke(s.request(SurfaceOperation::Close { handle })?))
    }
}
impl Service {
    fn pointer(
        &self,
        handle: &str,
        generation: u64,
        pointer: u64,
        gesture: u64,
        point: Point,
        up: bool,
    ) -> Result<Value> {
        self.current(handle, generation)?;
        let (preview, read) = {
            let mut g = mutex(&self.gate)?;
            let mut map = mutex(&self.native)?;
            let n = map.get_mut(handle).ok_or("Surface backend missing")?;
            let Some((owner, active, ticket)) = &n.active else {
                return Err("no active native gesture".into());
            };
            if *owner != pointer || *active != gesture {
                return Err("stale native gesture owner".into());
            }
            let ticket = ticket.clone();
            // Validate native authority on every event, not only its final publication.
            g.host.native_input_ticket(&n.frame).map_err(text)?;
            let outcome = if up {
                n.session
                    .pointer_up(&n.report, &n.binding, &n.viewport, pointer, gesture, point)
            } else {
                n.session
                    .pointer_move(&n.report, &n.binding, &n.viewport, pointer, gesture, point)
            };
            let outcome = match outcome {
                Ok(value) => value,
                Err(error) => {
                    n.session.cancel();
                    n.active = None;
                    g.host.cancel_surface_gesture(&ticket).map_err(text)?;
                    g.candidate = None;
                    return Err(text(error));
                }
            };
            let (selection, next_gesture) = match &outcome {
                PointerOutcome::Dragging {
                    preview,
                    gesture_id,
                } => (preview.as_ref(), Some(*gesture_id)),
                PointerOutcome::Ready { selection } => (Some(selection), None),
                _ => (None, None),
            };
            let boxes = selection
                .map(|s| {
                    s.segments
                        .iter()
                        .flat_map(|s| &s.characters)
                        .filter_map(|c| c.pixel_quad)
                        .map(|q| {
                            let left =
                                q.iter().map(|p| p[0]).min().unwrap() as f64 * n.viewport.zoom;
                            let top =
                                q.iter().map(|p| p[1]).min().unwrap() as f64 * n.viewport.zoom;
                            let right =
                                q.iter().map(|p| p[0]).max().unwrap() as f64 * n.viewport.zoom;
                            let bottom =
                                q.iter().map(|p| p[1]).max().unwrap() as f64 * n.viewport.zoom;
                            vec![left, top, right - left, bottom - top]
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut result = json!({"handle":handle,"generation":generation,"frame_id":n.frame.frame_id(),"gesture":next_gesture,"boxes":boxes,"text":selection.map(|s|s.joined_text.as_str()).unwrap_or("")});
            let read = if let PointerOutcome::Ready { selection } = &outcome {
                n.active = None;
                let published = (|| -> Result<String> {
                    let draft = self.draft(&g, selection)?;
                    let selected_source = source(&g);
                    g.host
                        .publish_surface_selection(
                            &ticket,
                            TrustedSurfaceSelection {
                                source: selected_source,
                                capture_protocol: draft.protocol(),
                                draft_json: String::from_utf8(draft.bytes()?).map_err(text)?,
                            },
                        )
                        .map_err(text)
                })();
                let lease = match published {
                    Ok(lease) => lease,
                    Err(error) => {
                        n.session.cancel();
                        let _ = g.host.cancel_surface_gesture(&ticket);
                        g.candidate = None;
                        return Err(error);
                    }
                };
                Some(SurfaceOperation::ReadSelection {
                    handle: handle.into(),
                    generation,
                    selection_lease: lease,
                })
            } else {
                if up {
                    n.active = None;
                    g.host.cancel_surface_gesture(&ticket).map_err(text)?;
                }
                None
            };
            result["native_outcome"] = serde_json::to_value(&outcome).map_err(text)?;
            (result, read)
        };
        if let Some(operation) = read {
            let candidate = self.invoke(self.request(operation)?)?;
            let mut result = preview;
            result["candidate"] = candidate;
            return Ok(result);
        }
        Ok(preview)
    }
}
/// Called only after the native window has actually stopped and released its nodes.
pub fn shutdown() -> Result<()> {
    // This gate also protects live-host callers: never take Service away while
    // NativeEditor still owns a run whose retirement needs this authority/Store.
    if !native_editor::shutdown() {
        return Err("editor retirement unconfirmed; Service and lease retained".into());
    }
    let Some(s) = SERVICE.lock().map_err(|_| "service lock poisoned")?.take() else {
        return Ok(());
    };
    let handle = mutex(&s.gate)?
        .descriptor
        .as_ref()
        .map(|d| d.handle.clone());
    if let Some(handle) = handle {
        let _ = s.invoke(s.request(SurfaceOperation::Close { handle })?);
    }
    let mut g = mutex(&s.gate)?;
    mutex(&s.native)?.clear();
    eprintln!(
        "READER_CLEANUP {:?}",
        json!({"pending_until_process_exit":g.host.pending_surface_cleanup()?.len(),"native_mount_release_ack":false})
    );
    let session = g.session.clone();
    g.host.close_session(&session).map_err(text)?;
    Ok(())
}
#[native_module("qusor/reader-host")]
fn native(registry: &mut Registry) {
    registry.module::<ReaderSurface>();
}

#[cfg(any(test, feature = "native-qualification"))]
pub(super) static SERVICE_TEST: Mutex<()> = Mutex::new(());
#[cfg(test)]
mod reader_guest_tests {
    pub(super) use super::SERVICE_TEST;
    use super::*;
    #[test]
    fn signed_guest_preserves_complete_json_escapes_and_rejects_invalid_surrogates() {
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/plugin-surface-resources");
        let key = std::fs::read_to_string(resources.join("publisher-key.txt")).unwrap();
        let package = extension_package::package::verify_for_host(
            &std::fs::read(resources.join("reader.package")).unwrap(),
            key.trim(),
            &SUPPORTED_HOST_APIS,
        )
        .unwrap();
        for protocol in [1, 2] {
            let draft = json!({"text":"quote\" slash\\ newline\n tab\t control\u{1} emoji😀","segments":[{"exact":"full tail"}]});
            let canonical = serde_json::to_string(&draft).unwrap();
            let response = json!({"capture_protocol":protocol,"draft_json":canonical}).to_string();
            let mut calls = 0;
            let mut broker = |bytes: &[u8]| {
                calls += 1;
                Ok(match calls {
                    1 => b"{}".to_vec(),
                    2 => response.as_bytes().to_vec(),
                    3 => {
                        let proposal: Value = serde_json::from_slice(bytes).unwrap();
                        assert_eq!(proposal["draft"], draft);
                        assert_eq!(proposal["capture_protocol"], protocol);
                        b"{}".to_vec()
                    }
                    _ => panic!("unexpected RPC"),
                })
            };
            let output = WasmiRuntime
                .execute(
                    package.module_bytes(),
                    &ExecutionLimits::default(),
                    4096,
                    &mut broker,
                )
                .unwrap();
            assert_eq!(output.output, "{}");
            assert_eq!(calls, 3);
        }
        for response in [
            r#"{"capture_protocol":1,"draft_json":"\uD800"}"#,
            r#"{"capture_protocol":1,"draft_json":"\q"}"#,
        ] {
            let mut calls = 0;
            let mut broker = |_: &[u8]| {
                calls += 1;
                Ok(if calls == 1 {
                    b"{}".to_vec()
                } else {
                    response.as_bytes().to_vec()
                })
            };
            assert!(
                WasmiRuntime
                    .execute(
                        package.module_bytes(),
                        &ExecutionLimits::default(),
                        4096,
                        &mut broker
                    )
                    .is_err()
            );
            assert_eq!(calls, 2, "invalid JSON must not reach CapturePropose");
        }
    }
    #[test]
    fn signed_guest_trap_and_timeout_close_unmounted_open() {
        let _test = SERVICE_TEST.lock().unwrap();
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/plugin-surface-resources");
        for variant in ["trap", "timeout", "wrong", "repeat"] {
            let test_root = std::env::temp_dir().join(format!(
                "qusor-reader-failure-{}-{variant}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&test_root);
            let input = test_root.join("input");
            let data = test_root.join("data");
            std::fs::create_dir_all(&input).unwrap();
            std::fs::create_dir_all(&data).unwrap();
            for name in [
                "report.json",
                "snapshot.json",
                "original.pdf",
                "publisher-key.txt",
            ] {
                std::fs::copy(resources.join(name), input.join(name)).unwrap();
            }
            std::fs::copy(
                resources.join(format!("reader.{variant}.package")),
                input.join("reader.package"),
            )
            .unwrap();
            initialize(&input, &data).unwrap();
            let service = SERVICE.lock().unwrap().take().unwrap();
            let (resource, expected) = {
                let g = service.gate.lock().unwrap();
                (document_resource(&g), source(&g))
            };
            let request = service
                .request(SurfaceOperation::Open {
                    resource,
                    expected,
                    contribution_id: service.contribution.clone(),
                })
                .unwrap();
            assert!(service.invoke(request).is_err());
            assert!(service.native.lock().unwrap().is_empty());
            let g = service.gate.lock().unwrap();
            assert!(g.descriptor.is_none());
            assert!(g.candidate.is_none());
            assert!(g.host.pending_surface_cleanup().unwrap().is_empty());
            drop(g);
            drop(service);
            std::fs::remove_dir_all(test_root).unwrap();
        }
    }

    #[test]
    fn native_approval_retries_notes_distinct_operations_and_revoke() {
        let _test = SERVICE_TEST.lock().unwrap();
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/plugin-surface-resources");
        let data = std::env::temp_dir().join(format!("qusor-reader-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        initialize(&resources, &data).unwrap();
        let service = service().unwrap();
        let report_digest = mutex(&service.gate).unwrap().report_digest.clone();
        let open: Value =
            serde_json::from_str(&ReaderSurface::open(report_digest.clone())).unwrap();
        assert_eq!(open["ok"], true);
        let cfg = &open["value"];
        let handle = cfg["handle"].as_str().unwrap().to_owned();
        let generation = cfg["generation"].as_u64().unwrap();
        let ready: Value = serde_json::from_str(&ReaderSurface::ready(
            handle.clone(),
            generation,
            cfg["frame_id"].as_str().unwrap().into(),
            report_digest,
        ))
        .unwrap();
        assert_eq!(ready["ok"], true);
        let select = |first: usize, last: usize, pointer: u64| -> String {
            let point = |index: usize| {
                let q = mutex(&service.gate).unwrap().report.pages[0].characters[index]
                    .pixel_quad
                    .unwrap();
                Point {
                    x: q.iter().map(|p| p[0] as f64).sum::<f64>() / 4.0,
                    y: q.iter().map(|p| p[1] as f64).sum::<f64>() / 4.0,
                }
            };
            let p = point(first);
            let q = point(last);
            let down: Value = serde_json::from_str(&ReaderSurface::down(
                handle.clone(),
                generation,
                pointer,
                p.x,
                p.y,
            ))
            .unwrap();
            assert_eq!(down["ok"], true);
            let token = down["value"]["gesture"].as_u64().unwrap();
            let second: Value = serde_json::from_str(&ReaderSurface::down(
                handle.clone(),
                generation,
                pointer + 1,
                p.x,
                p.y,
            ))
            .unwrap();
            assert_eq!(second["ok"], false);
            let up: Value = serde_json::from_str(&ReaderSurface::up(
                handle.clone(),
                generation,
                pointer,
                token,
                q.x,
                q.y,
            ))
            .unwrap();
            assert_eq!(up["ok"], true);
            up["value"]["candidate"]["candidate_id"]
                .as_str()
                .unwrap()
                .into()
        };
        let first = select(0, 4, 10);
        let save: Value =
            serde_json::from_str(&ReaderSurface::save(first.clone(), "one".into())).unwrap();
        assert_eq!(save["ok"], true);
        let retry: Value =
            serde_json::from_str(&ReaderSurface::save(first.clone(), "one".into())).unwrap();
        assert_eq!(retry, save);
        let changed: Value =
            serde_json::from_str(&ReaderSurface::save(first, "changed".into())).unwrap();
        assert_eq!(changed["ok"], false);
        let second = select(6, 11, 20);
        let next: Value =
            serde_json::from_str(&ReaderSurface::save(second.clone(), "two".into())).unwrap();
        assert_eq!(next["ok"], true);
        assert_ne!(
            save["value"]["receipt"]["operation_id"],
            next["value"]["receipt"]["operation_id"]
        );
        assert_ne!(
            save["value"]["receipt"]["capture_id"],
            next["value"]["receipt"]["capture_id"]
        );
        ReaderSurface::disable();
        let revoked: Value =
            serde_json::from_str(&ReaderSurface::save(second, "two".into())).unwrap();
        assert_eq!(revoked["ok"], false);
        drop(service);
        SERVICE.lock().unwrap().take();
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn cold_open_of_another_document_keeps_grant_expiry_and_uses_distinct_operation() {
        let _test = SERVICE_TEST.lock().unwrap();
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/plugin-surface-resources");
        let data = std::env::temp_dir().join(format!("qusor-reader-cold-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        initialize(&resources, &data).unwrap();
        let first = service().unwrap().gate.lock().unwrap().document.receipt.document_id;
        let saved = service().unwrap().gate.lock().unwrap().host.saved_authorization_for_test().unwrap();
        shutdown().unwrap();

        let mut next = fixture_prepared_for_cold_open(&resources);
        next.operation = "cold-open-second-import".into();
        assert!(initialize_document(&resources, &data, next).is_err(),
            "passive recovery must not change the document permission scope");
        assert!(service().is_err());
        let mut next = fixture_prepared_for_cold_open(&resources);
        next.operation = "cold-open-second-import".into();
        assert!(matches!(initialize_document_checked(&resources, &data, next).unwrap(),
            DocumentInitialization::Ready));
        let active = service().unwrap();
        let gate = active.gate.lock().unwrap();
        assert_ne!(first, gate.document.receipt.document_id);
        let restored = gate.host.saved_authorization_for_test().unwrap();
        match (saved, restored) {
            (extension_store::SavedAuthorization::V2(before), extension_store::SavedAuthorization::V2(after)) => {
                assert_eq!(before.scope.expires_at, after.scope.expires_at);
                assert_eq!(before.scope.grants.iter().map(|g| g.capability).collect::<Vec<_>>(),
                           after.scope.grants.iter().map(|g| g.capability).collect::<Vec<_>>());
            }
            _ => panic!("expected ordinary versioned grants"),
        }
        drop(gate);
        drop(active);
        shutdown().unwrap();
        std::fs::remove_dir_all(data).unwrap();
    }

    fn fixture_prepared_for_cold_open(resources: &Path) -> PreparedDocument {
        let bytes = std::fs::read(resources.join("report.json")).unwrap();
        PreparedDocument {
            report: serde_json::from_slice(&bytes).unwrap(),
            report_digest: qusor_pdf_provider_probe::sha256(&bytes),
            snapshot: serde_json::from_slice(&std::fs::read(resources.join("snapshot.json")).unwrap()).unwrap(),
            original: std::fs::read(resources.join("original.pdf")).unwrap(),
            title: "text-two-pages.pdf".into(),
            operation: String::new(),
        }
    }

    #[test]
    fn persisted_reader_set_restores_without_renewal_but_never_restores_old_frames_or_disable() {
        let _test = SERVICE_TEST.lock().unwrap();
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/plugin-surface-resources");
        let data =
            std::env::temp_dir().join(format!("qusor-reader-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let (saved, old_frame) = {
            initialize(&resources, &data).unwrap();
            let service = service().unwrap();
            let opened: Value =
                serde_json::from_str(&ReaderSurface::open(current_report_digest().unwrap()))
                    .unwrap();
            assert_eq!(opened["ok"], true);
            let handle = opened["value"]["handle"].as_str().unwrap();
            let frame = service
                .native
                .lock()
                .unwrap()
                .get(handle)
                .unwrap()
                .frame
                .clone();
            let ready: Value = serde_json::from_str(&ReaderSurface::ready(
                handle.into(),
                frame.generation(),
                frame.frame_id().into(),
                current_report_digest().unwrap(),
            ))
            .unwrap();
            assert_eq!(ready["ok"], true);
            assert!(
                service
                    .gate
                    .lock()
                    .unwrap()
                    .host
                    .native_input_ticket(&frame)
                    .is_ok()
            );
            let saved = service
                .gate
                .lock()
                .unwrap()
                .host
                .saved_authorization_for_test()
                .unwrap();
            shutdown().unwrap();
            drop(service);
            (saved, frame)
        };
        {
            initialize(&resources, &data).unwrap();
            let service = service().unwrap();
            let g = service.gate.lock().unwrap();
            assert_eq!(
                g.host.saved_authorization_for_test().unwrap(),
                saved,
                "restart must not renew or rewrite persisted intent"
            );
            assert!(g.candidate.is_none());
            assert!(g.host.native_input_ticket(&old_frame).is_err());
            drop(g);
            let opened: Value =
                serde_json::from_str(&ReaderSurface::open(current_report_digest().unwrap()))
                    .unwrap();
            assert_eq!(opened["ok"], true);
            assert_ne!(opened["value"]["handle"], old_frame.handle());
            let disabled: Value = serde_json::from_str(&ReaderSurface::disable()).unwrap();
            assert_eq!(disabled["ok"], true);
            shutdown().unwrap();
            drop(service);
        }
        assert!(
            initialize(&resources, &data).is_err(),
            "restart must not automatically regrant a disabled reader"
        );
        assert!(service().is_err());
        std::fs::remove_dir_all(data).unwrap();
    }
}

/// Temporary native component composition shares this service's Core and authority.
#[path = "native_editor.rs"]
pub mod native_editor;
