//! Host-owned reading leases. Native tickets are intentionally not serializable.
//! The trusted backend validates source data and owns rendering/input/resources;
//! this module owns identity, grants, generations and result acceptance only.
use super::*;
use extension_api::{
    ContributionKind,
    surface::{
        FrameMetadata, SURFACE_PROTOCOL, SourceVersion, SurfaceDescriptor, SurfaceOperation,
        SurfaceRequest, SurfaceResponse, SurfaceResult, SurfaceStatus,
    },
};

const MAX_SURFACES: usize = 64;
const MAX_SURFACES_PER_INSTALLATION: usize = 16;
const MAX_SELECTION_BYTES: usize = 4096;

/// Source truth supplied by a trusted Core/document adapter, never by guest JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedSurfaceDocument {
    pub resource: Resource,
    pub source: SourceVersion,
    pub page_count: u32,
}
/// A complete native selection, already validated against the immutable source.
/// The Host preserves these bytes, without truncating or interpreting Core schema.
#[derive(Clone, Debug)]
pub struct TrustedSurfaceSelection {
    pub source: SourceVersion,
    pub capture_protocol: u16,
    pub draft_json: String,
}

#[derive(Clone, Debug)]
pub struct NativeFrameTicket {
    owner: Arc<()>,
    handle: String,
    generation: u64,
    frame_id: String,
    mount_id: String,
    page_index: u32,
    zoom: f64,
    source: SourceVersion,
    resource: Resource,
}
impl NativeFrameTicket {
    pub fn handle(&self) -> &str {
        &self.handle
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn frame_id(&self) -> &str {
        &self.frame_id
    }
    pub fn mount_id(&self) -> &str {
        &self.mount_id
    }
    pub fn page_index(&self) -> u32 {
        self.page_index
    }
    pub fn zoom(&self) -> f64 {
        self.zoom
    }
    pub fn source(&self) -> &SourceVersion {
        &self.source
    }
    pub fn resource(&self) -> &Resource {
        &self.resource
    }
}
#[derive(Clone, Debug)]
pub struct NativeInputTicket {
    frame: NativeFrameTicket,
}
impl NativeInputTicket {
    pub fn mount_id(&self) -> &str {
        self.frame.mount_id()
    }
    pub fn handle(&self) -> &str {
        self.frame.handle()
    }
    pub fn generation(&self) -> u64 {
        self.frame.generation()
    }
    pub fn frame_id(&self) -> &str {
        self.frame.frame_id()
    }
    pub fn source(&self) -> &SourceVersion {
        self.frame.source()
    }
    pub fn resource(&self) -> &Resource {
        self.frame.resource()
    }
}
#[derive(Clone, Debug)]
pub struct NativeGestureTicket {
    input: NativeInputTicket,
    sequence: u64,
}
impl NativeGestureTicket {
    pub fn frame_id(&self) -> &str {
        self.input.frame_id()
    }
    pub fn mount_id(&self) -> &str {
        self.input.mount_id()
    }
    pub fn handle(&self) -> &str {
        self.input.handle()
    }
    pub fn generation(&self) -> u64 {
        self.input.generation()
    }
    pub fn source(&self) -> &SourceVersion {
        self.input.source()
    }
    pub fn resource(&self) -> &Resource {
        self.input.resource()
    }
}
#[derive(Clone, Debug)]
pub struct NativeCleanupTicket {
    owner: Arc<()>,
    handle: String,
    generation: u64,
    mount_id: String,
}
impl NativeCleanupTicket {
    pub fn mount_id(&self) -> &str {
        &self.mount_id
    }
    pub fn handle(&self) -> &str {
        &self.handle
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
#[derive(Clone, Debug)]
pub struct SurfaceTransition {
    pub descriptor: SurfaceDescriptor,
    pub frame_ticket: NativeFrameTicket,
}
#[derive(Clone, Debug)]
struct Selection {
    lease: String,
    capture_protocol: u16,
    draft_json: String,
}
#[derive(Clone, Debug)]
struct Record {
    installation: InstallationId,
    session: u64,
    epoch: u64,
    package_digest: String,
    handle: String,
    resource: Resource,
    source: SourceVersion,
    page_count: u32,
    page_index: u32,
    zoom: f64,
    generation: u64,
    frame_id: String,
    mount_id: String,
    status: SurfaceStatus,
    frame: Option<FrameMetadata>,
    gesture: Option<u64>,
    selection: Option<Selection>,
}
impl Record {
    fn descriptor(&self) -> SurfaceDescriptor {
        SurfaceDescriptor {
            handle: self.handle.clone(),
            resource: self.resource.clone(),
            source: self.source.clone(),
            page_count: self.page_count,
            generation: self.generation,
            page_index: self.page_index,
            zoom: self.zoom,
            status: self.status,
            frame: self.frame.clone(),
            selection_lease: self.selection.as_ref().map(|s| s.lease.clone()),
        }
    }
    fn frame_ticket(&self, owner: &Arc<()>) -> NativeFrameTicket {
        NativeFrameTicket {
            owner: Arc::clone(owner),
            handle: self.handle.clone(),
            generation: self.generation,
            frame_id: self.frame_id.clone(),
            mount_id: self.mount_id.clone(),
            page_index: self.page_index,
            zoom: self.zoom,
            source: self.source.clone(),
            resource: self.resource.clone(),
        }
    }
    fn closing(&mut self) {
        if self.status != SurfaceStatus::Closing {
            self.generation = self.generation.saturating_add(1);
            self.status = SurfaceStatus::Closing;
            self.frame = None;
            self.gesture = None;
            self.selection = None;
        }
    }
}
#[derive(Default)]
pub(super) struct SurfaceRegistry {
    boot: Option<[u8; 16]>,
    next: u64,
    records: BTreeMap<String, Record>,
}
impl SurfaceRegistry {
    fn identity(&mut self, prefix: &str, host: u64) -> Result<String, ApiError> {
        let boot = self.boot.ok_or_else(|| {
            error(
                ErrorCode::InvalidState,
                "surface startup identity is not configured",
            )
        })?;
        let sequence = take_id(&mut self.next)?;
        let nonce = boot
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(format!("{prefix}.{nonce}.{host}.{sequence}"))
    }
    pub(super) fn invalidate_session(&mut self, session: u64) {
        for record in self.records.values_mut().filter(|r| r.session == session) {
            record.closing();
        }
    }
    pub(super) fn invalidate_installation(&mut self, installation: InstallationId) {
        for record in self
            .records
            .values_mut()
            .filter(|r| r.installation == installation)
        {
            record.closing();
        }
    }
}
impl Host {
    pub(super) fn has_surface_resources(&self, installation: InstallationId) -> bool {
        self.surfaces
            .records
            .values()
            .any(|record| record.installation == installation)
    }
    /// Configure once from startup entropy supplied by the trusted platform adapter.
    /// The policy crate performs no OS I/O; guest wire has no equivalent operation.
    pub fn configure_surface_identity(&mut self, nonce: [u8; 16]) -> Result<(), ApiError> {
        if nonce == [0; 16] || self.surfaces.boot.is_some() {
            return Err(error(
                ErrorCode::InvalidState,
                "surface startup identity must be nonzero and set only once",
            ));
        }
        self.surfaces.boot = Some(nonce);
        self.surfaces.next = 1;
        Ok(())
    }
    /// Non-mutating validation for a trusted invocation broker before exposing
    /// command parameters. The eventual operation rechecks the same authority.
    pub fn preflight_surface_request(
        &self,
        session: &Session,
        request: &SurfaceRequest,
    ) -> Result<(), ApiError> {
        request.validate()?;
        match &request.operation {
            SurfaceOperation::Open {
                resource,
                contribution_id,
                ..
            } => {
                self.surface_open_authority(
                    session,
                    request.api_version,
                    &request.request_id,
                    resource,
                    contribution_id,
                )?;
            }
            SurfaceOperation::Configure {
                handle,
                generation,
                page_index,
                ..
            } => {
                let record = self.surface_for_session(session, handle)?;
                if record.generation != *generation || *page_index >= record.page_count {
                    return Err(stale());
                }
            }
            SurfaceOperation::ReadSelection { .. } => {
                self.read_surface_selection(session, request)?;
            }
            SurfaceOperation::Close { handle } => {
                self.surface_for_session(session, handle)?;
            }
        }
        Ok(())
    }
    fn surface_open_authority(
        &self,
        session: &Session,
        api_version: ApiVersion,
        request_id: &str,
        resource: &Resource,
        contribution_id: &str,
    ) -> Result<(InstallationId, u64, String), ApiError> {
        self.check_owner(&session.owner)?;
        let policy = surface_policy(
            api_version,
            request_id,
            Capability::ReaderSurface,
            resource.clone(),
        );
        let (connection, installed) = self.check_request(session.id, &policy)?;
        let contribution = installed
            .manifest
            .contributions
            .iter()
            .find(|c| {
                c.id == contribution_id
                    && c.kind == ContributionKind::ReaderWorkspace
                    && c.required_capabilities.contains(&Capability::ReaderSurface)
            })
            .ok_or_else(|| {
                error(
                    ErrorCode::PermissionDenied,
                    "reading workspace contribution is not available to this installation",
                )
            })?;
        for capability in &contribution.required_capabilities {
            let scope = match capability {
                Capability::CapturePropose => Resource::Vault {
                    vault_id: self.vault_id.clone(),
                },
                _ => resource.clone(),
            };
            self.check_request(
                session.id,
                &surface_policy(api_version, request_id, *capability, scope),
            )?;
        }
        Ok((
            connection.installation,
            installed.epoch,
            installed.digest.clone(),
        ))
    }
    pub fn open_surface(
        &mut self,
        session: &Session,
        request: &SurfaceRequest,
        document: TrustedSurfaceDocument,
    ) -> Result<SurfaceTransition, ApiError> {
        request.validate()?;
        let SurfaceOperation::Open {
            resource,
            expected,
            contribution_id,
        } = &request.operation
        else {
            return Err(invalid_operation());
        };
        let (installation, epoch, digest) = self.surface_open_authority(
            session,
            request.api_version,
            &request.request_id,
            resource,
            contribution_id,
        )?;
        if document.resource != *resource
            || document.source != *expected
            || document.page_count == 0
        {
            return Err(error(
                ErrorCode::StaleContext,
                "document source does not match the requested immutable version",
            ));
        }
        document.source.validate()?;
        if self.surfaces.records.len() >= MAX_SURFACES
            || self
                .surfaces
                .records
                .values()
                .filter(|record| record.installation == installation)
                .count()
                >= MAX_SURFACES_PER_INSTALLATION
        {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "surface limit reached; pending resources must be closed",
            ));
        }
        let handle = self.surfaces.identity("sf", self.identity)?;
        let frame_id = self.surfaces.identity("fr", self.identity)?;
        let mount_id = self.surfaces.identity("mt", self.identity)?;
        let record = Record {
            installation,
            session: session.id,
            epoch,
            package_digest: digest,
            handle: handle.clone(),
            resource: document.resource,
            source: document.source,
            page_count: document.page_count,
            page_index: 0,
            zoom: 1.0,
            generation: 1,
            frame_id,
            mount_id,
            status: SurfaceStatus::Pending,
            frame: None,
            gesture: None,
            selection: None,
        };
        let result = SurfaceTransition {
            descriptor: record.descriptor(),
            frame_ticket: record.frame_ticket(&self.owner),
        };
        result.descriptor.validate()?;
        self.surfaces.records.insert(handle, record);
        Ok(result)
    }
    pub fn configure_surface(
        &mut self,
        session: &Session,
        request: &SurfaceRequest,
    ) -> Result<SurfaceTransition, ApiError> {
        request.validate()?;
        let SurfaceOperation::Configure {
            handle,
            generation,
            page_index,
            zoom,
        } = &request.operation
        else {
            return Err(invalid_operation());
        };
        let record = self.surface_for_session(session, handle)?;
        if record.generation != *generation || *page_index >= record.page_count {
            return Err(stale());
        }
        let next_generation = record
            .generation
            .checked_add(1)
            .ok_or_else(|| error(ErrorCode::BudgetExceeded, "surface generation exhausted"))?;
        let frame_id = self.surfaces.identity("fr", self.identity)?;
        let record = self.surfaces.records.get_mut(handle).ok_or_else(stale)?;
        record.generation = next_generation;
        record.page_index = *page_index;
        record.zoom = *zoom;
        record.frame_id = frame_id;
        record.status = SurfaceStatus::Pending;
        record.frame = None;
        record.gesture = None;
        record.selection = None;
        Ok(SurfaceTransition {
            descriptor: record.descriptor(),
            frame_ticket: record.frame_ticket(&self.owner),
        })
    }
    /// Accept readiness only from the exact pending native frame job.
    pub fn native_frame_ready(
        &mut self,
        ticket: &NativeFrameTicket,
        frame: FrameMetadata,
    ) -> Result<SurfaceDescriptor, ApiError> {
        frame.validate()?;
        let record = self.record_for_frame(ticket)?;
        if record.status != SurfaceStatus::Pending || frame.frame_id != ticket.frame_id {
            return Err(stale());
        }
        let record = self
            .surfaces
            .records
            .get_mut(ticket.handle())
            .ok_or_else(stale)?;
        record.status = SurfaceStatus::Ready;
        record.frame = Some(frame);
        Ok(record.descriptor())
    }
    /// Report a terminal frame-job failure without claiming resource release.
    /// Uncertain running work still requires the Closing/cleanup handshake.
    pub fn native_frame_failed(
        &mut self,
        ticket: &NativeFrameTicket,
    ) -> Result<SurfaceDescriptor, ApiError> {
        let record = self.record_for_frame(ticket)?;
        if record.status != SurfaceStatus::Pending {
            return Err(stale());
        }
        let record = self
            .surfaces
            .records
            .get_mut(ticket.handle())
            .ok_or_else(stale)?;
        record.status = SurfaceStatus::Failed;
        record.frame = None;
        record.gesture = None;
        record.selection = None;
        Ok(record.descriptor())
    }

    pub fn native_input_ticket(
        &self,
        frame: &NativeFrameTicket,
    ) -> Result<NativeInputTicket, ApiError> {
        let record = self.record_for_frame(frame)?;
        if record.status != SurfaceStatus::Ready {
            return Err(stale());
        }
        Ok(NativeInputTicket {
            frame: frame.clone(),
        })
    }
    pub fn surface_begin_gesture(
        &mut self,
        input: &NativeInputTicket,
    ) -> Result<NativeGestureTicket, ApiError> {
        self.ready_record(input)?;
        let sequence = take_id(&mut self.surfaces.next)?;
        let record = self
            .surfaces
            .records
            .get_mut(input.handle())
            .ok_or_else(stale)?;
        record.gesture = Some(sequence);
        record.selection = None;
        Ok(NativeGestureTicket {
            input: input.clone(),
            sequence,
        })
    }
    pub fn publish_surface_selection(
        &mut self,
        gesture: &NativeGestureTicket,
        selection: TrustedSurfaceSelection,
    ) -> Result<String, ApiError> {
        let record = self.ready_record(&gesture.input)?;
        if record.gesture != Some(gesture.sequence) || selection.source != record.source {
            return Err(stale());
        }
        if !matches!(selection.capture_protocol, 1 | 2) || selection.draft_json.is_empty() {
            return Err(error(
                ErrorCode::InvalidRequest,
                "unsupported or empty complete selection",
            ));
        }
        if selection.draft_json.len() > MAX_SELECTION_BYTES {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "complete selection exceeds wire budget",
            ));
        }
        let session_api = self
            .sessions
            .get(&record.session)
            .ok_or_else(stale)?
            .api_version;
        let lease = self.surfaces.identity("sl", self.identity)?;
        // Reject the whole lease before publishing when any legal response id
        // would exceed the bounded broker wire. Never truncate draft segments.
        let response = SurfaceResponse {
            surface_protocol: SURFACE_PROTOCOL,
            api_version: session_api,
            request_id: "a".repeat(extension_api::MAX_ID_BYTES),
            result: SurfaceResult::Selection {
                handle: gesture.handle().into(),
                generation: gesture.generation(),
                selection_lease: lease.clone(),
                capture_protocol: selection.capture_protocol,
                draft_json: selection.draft_json.clone(),
            },
        };
        response.to_json()?;
        let record = self
            .surfaces
            .records
            .get_mut(gesture.handle())
            .ok_or_else(stale)?;
        record.gesture = None;
        record.selection = Some(Selection {
            lease: lease.clone(),
            capture_protocol: selection.capture_protocol,
            draft_json: selection.draft_json,
        });
        Ok(lease)
    }
    pub fn cancel_surface_gesture(
        &mut self,
        gesture: &NativeGestureTicket,
    ) -> Result<(), ApiError> {
        let record = self.ready_record(&gesture.input)?;
        if record.gesture != Some(gesture.sequence) {
            return Err(stale());
        }
        let record = self
            .surfaces
            .records
            .get_mut(gesture.handle())
            .ok_or_else(stale)?;
        record.gesture = None;
        record.selection = None;
        Ok(())
    }
    /// A new trusted UI clear intent (for example a fresh blank down), not a late
    /// gesture callback. Late callbacks must use `cancel_surface_gesture` so they
    /// cannot erase a successor from the same ready frame.
    pub fn clear_surface_selection(&mut self, input: &NativeInputTicket) -> Result<(), ApiError> {
        self.ready_record(input)?;
        let record = self
            .surfaces
            .records
            .get_mut(input.handle())
            .ok_or_else(stale)?;
        record.gesture = None;
        record.selection = None;
        Ok(())
    }
    pub fn read_surface_selection(
        &self,
        session: &Session,
        request: &SurfaceRequest,
    ) -> Result<SurfaceResponse, ApiError> {
        request.validate()?;
        let SurfaceOperation::ReadSelection {
            handle,
            generation,
            selection_lease,
        } = &request.operation
        else {
            return Err(invalid_operation());
        };
        let record = self.surface_for_session(session, handle)?;
        let policy = surface_policy(
            request.api_version,
            &request.request_id,
            Capability::SelectionRead,
            record.resource.clone(),
        );
        self.check_request(session.id, &policy)?;
        let selection = record.selection.as_ref().ok_or_else(stale)?;
        if record.status != SurfaceStatus::Ready
            || record.generation != *generation
            || selection.lease != *selection_lease
        {
            return Err(stale());
        }
        let response = SurfaceResponse {
            surface_protocol: SURFACE_PROTOCOL,
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            result: SurfaceResult::Selection {
                handle: handle.clone(),
                generation: *generation,
                selection_lease: selection.lease.clone(),
                capture_protocol: selection.capture_protocol,
                draft_json: selection.draft_json.clone(),
            },
        };
        response.validate()?;
        Ok(response)
    }
    /// Fresh authorized state for trusted invocation/context brokers. This is
    /// not a separate guest operation and does not grant input or read leases.
    pub fn surface_state(
        &self,
        session: &Session,
        handle: &str,
    ) -> Result<SurfaceDescriptor, ApiError> {
        Ok(self.surface_for_session(session, handle)?.descriptor())
    }

    /// Fresh state used only for trusted VM-result acceptance. Closing carries
    /// no frame/selection and is allowed here so a Close result can be checked
    /// before the backend release acknowledgement. It grants no further action.
    pub fn surface_result_state(
        &self,
        session: &Session,
        handle: &str,
    ) -> Result<SurfaceDescriptor, ApiError> {
        self.check_owner(&session.owner)?;
        let record = self.surfaces.records.get(handle).ok_or_else(stale)?;
        if record.session != session.id {
            return Err(stale());
        }
        self.record_authority(record)?;
        Ok(record.descriptor())
    }

    pub fn close_surface(
        &mut self,
        session: &Session,
        request: &SurfaceRequest,
    ) -> Result<SurfaceDescriptor, ApiError> {
        request.validate()?;
        let SurfaceOperation::Close { handle } = &request.operation else {
            return Err(invalid_operation());
        };
        self.surface_for_session(session, handle)?;
        let record = self.surfaces.records.get_mut(handle).ok_or_else(stale)?;
        record.closing();
        Ok(record.descriptor())
    }
    /// Trusted Core adapters call this on source replacement/removal. No guest entry exists.
    pub fn invalidate_document_surfaces(&mut self, resource: &Resource) -> Result<(), ApiError> {
        resource.validate()?;
        if !matches!(resource, Resource::Document { .. }) || resource.vault_id() != self.vault_id {
            return Err(error(
                ErrorCode::InvalidRequest,
                "invalid document invalidation scope",
            ));
        }
        for record in self
            .surfaces
            .records
            .values_mut()
            .filter(|r| r.resource == *resource)
        {
            record.closing();
        }
        Ok(())
    }
    /// Invalidated leases remain Closing until the native backend acknowledges release.
    pub fn pending_surface_cleanup(&self) -> Vec<NativeCleanupTicket> {
        self.surfaces
            .records
            .values()
            .filter(|r| r.status == SurfaceStatus::Closing)
            .map(|r| NativeCleanupTicket {
                owner: Arc::clone(&self.owner),
                handle: r.handle.clone(),
                generation: r.generation,
                mount_id: r.mount_id.clone(),
            })
            .collect()
    }
    pub fn confirm_surface_closed(&mut self, ticket: &NativeCleanupTicket) -> Result<(), ApiError> {
        self.check_owner(&ticket.owner)?;
        if let Some(record) = self.surfaces.records.get(&ticket.handle) {
            if record.status != SurfaceStatus::Closing
                || record.generation != ticket.generation
                || record.mount_id != ticket.mount_id
            {
                return Err(stale());
            }
            self.surfaces.records.remove(&ticket.handle);
        }
        Ok(())
    }
    fn surface_for_session(&self, session: &Session, handle: &str) -> Result<&Record, ApiError> {
        self.check_owner(&session.owner)?;
        let record = self.surfaces.records.get(handle).ok_or_else(stale)?;
        if record.session != session.id {
            return Err(stale());
        }
        self.current_record(record)?;
        Ok(record)
    }
    fn current_record(&self, record: &Record) -> Result<(), ApiError> {
        if record.status == SurfaceStatus::Closing || record.status == SurfaceStatus::Closed {
            return Err(stale());
        }
        self.record_authority(record)
    }
    fn record_authority(&self, record: &Record) -> Result<(), ApiError> {
        let session_api = self
            .sessions
            .get(&record.session)
            .ok_or_else(stale)?
            .api_version;
        let request = surface_policy(
            session_api,
            "native-surface",
            Capability::ReaderSurface,
            record.resource.clone(),
        );
        let (connection, installed) = self.check_request(record.session, &request)?;
        if connection.installation != record.installation
            || connection.epoch != record.epoch
            || installed.digest != record.package_digest
        {
            return Err(stale());
        }
        Ok(())
    }
    fn record_for_frame(&self, ticket: &NativeFrameTicket) -> Result<&Record, ApiError> {
        self.check_owner(&ticket.owner)?;
        let record = self
            .surfaces
            .records
            .get(&ticket.handle)
            .ok_or_else(stale)?;
        self.current_record(record)?;
        if record.generation != ticket.generation
            || record.frame_id != ticket.frame_id
            || record.mount_id != ticket.mount_id
            || record.page_index != ticket.page_index
            || record.zoom != ticket.zoom
            || record.source != ticket.source
            || record.resource != ticket.resource
        {
            return Err(stale());
        }
        Ok(record)
    }
    fn ready_record(&self, input: &NativeInputTicket) -> Result<&Record, ApiError> {
        let record = self.record_for_frame(&input.frame)?;
        if record.status != SurfaceStatus::Ready {
            return Err(stale());
        }
        Ok(record)
    }
}
fn surface_policy(
    api_version: ApiVersion,
    id: &str,
    capability: Capability,
    resource: Resource,
) -> CapabilityRequest {
    CapabilityRequest {
        api_version,
        request_id: id.into(),
        capability,
        resource,
    }
}
fn invalid_operation() -> ApiError {
    error(ErrorCode::InvalidRequest, "unexpected Surface operation")
}
