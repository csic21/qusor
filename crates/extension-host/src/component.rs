//! Host-local temporary component mounts, separate from ordinary sessions and grants.
//! Only trusted management code creates mounts or confirms actual native release.
use super::*;
use extension_api::{
    ContributionKind, HOST_API_V0_3,
    component_invocation::ComponentInvocationRequest,
    composition::{ComponentContextRequest, CompositionDescriptor},
    continuation::ContinuationRequest,
    draft::DraftRequest,
    projection::ProjectionRequest,
};

const MAX_MOUNTS: usize = 64;
const MAX_PER_INSTALLATION: usize = 16;

#[derive(Debug)]
struct Identity {
    mount_id: String,
    installation: InstallationId,
    epoch: u64,
    resource: Resource,
    descriptor: CompositionDescriptor,
}
/// Opaque host-owned mount, never a guest wire value or an ordinary Session.
#[derive(Clone, Debug)]
pub struct MountHandle {
    owner: Arc<()>,
    identity: Arc<Identity>,
}
impl MountHandle {
    pub fn mount_id(&self) -> &str {
        &self.identity.mount_id
    }
    pub fn installation_id(&self) -> InstallationId {
        self.identity.installation
    }
    pub fn epoch(&self) -> u64 {
        self.identity.epoch
    }
    pub fn resource(&self) -> &Resource {
        &self.identity.resource
    }
    pub fn descriptor(&self) -> &CompositionDescriptor {
        &self.identity.descriptor
    }
}
/// A request to the trusted native renderer, not an acknowledgement of release.
#[derive(Clone, Debug)]
pub struct ComponentCleanupTicket {
    owner: Arc<()>,
    identity: Arc<Identity>,
}
impl ComponentCleanupTicket {
    pub fn mount_id(&self) -> &str {
        &self.identity.mount_id
    }
    pub fn installation_id(&self) -> InstallationId {
        self.identity.installation
    }
    pub fn resource(&self) -> &Resource {
        &self.identity.resource
    }
}
/// Descriptive context passed only in the synchronous authorized callback.
/// Cloning this value grants no authority and cannot bypass a later fresh dispatch.
#[derive(Clone, Debug)]
pub struct AuthorizedDraftContext {
    pub installation_id: InstallationId,
    pub package_id: String,
    pub package_digest: String,
    pub vault_id: String,
    pub grant_epoch: u64,
    pub mount_id: String,
    pub resource: Resource,
    pub request: DraftRequest,
}
/// Own-mount identity established only inside a synchronous dispatch callback.
/// This descriptive value carries neither a reusable permit nor guest claims.
#[derive(Clone, Debug)]
pub struct AuthorizedComponentContext {
    pub installation_id: InstallationId,
    pub package_id: String,
    pub package_digest: String,
    pub vault_id: String,
    pub grant_epoch: u64,
    pub mount_id: String,
    pub resource: Resource,
}
struct Record {
    identity: Arc<Identity>,
    closing: bool,
}
#[derive(Default)]
pub(super) struct ComponentRegistry {
    nonce: Option<[u8; 16]>,
    next: u64,
    records: BTreeMap<String, Record>,
}
impl ComponentRegistry {
    pub(super) fn invalidate_installation(&mut self, installation: InstallationId) {
        for record in self
            .records
            .values_mut()
            .filter(|r| r.identity.installation == installation)
        {
            record.closing = true;
        }
    }
    fn identity(&mut self) -> Result<String, ApiError> {
        let mut bytes = self.nonce.ok_or_else(|| {
            error(
                ErrorCode::InvalidState,
                "component identity is not configured",
            )
        })?;
        let counter = take_id(&mut self.next)?;
        // A fixed 128-bit startup nonce and a checked sequence yield a one-to-one
        // 32hex namespace. Ownership is checked independently, not by ID secrecy.
        for (byte, sequence) in bytes[8..].iter_mut().zip(counter.to_be_bytes()) {
            *byte ^= sequence;
        }
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
}
impl Host {
    pub(super) fn has_component_resources(&self, installation: InstallationId) -> bool {
        self.components
            .records
            .values()
            .any(|r| r.identity.installation == installation)
    }
    /// Configure once with trusted startup entropy; no OS I/O or guest equivalent.
    pub fn configure_component_identity(&mut self, nonce: [u8; 16]) -> Result<(), ApiError> {
        if nonce == [0; 16]
            || self.components.nonce.is_some()
            || !self.components.records.is_empty()
        {
            return Err(error(
                ErrorCode::InvalidState,
                "component nonce must be nonzero and configured only once",
            ));
        }
        self.components.nonce = Some(nonce);
        self.components.next = 1;
        Ok(())
    }
    /// Trusted management entry. This does not enable ordinary sessions or grants.
    /// The caller must authenticate the descriptor's signed package asset first.
    pub fn open_component_mount(
        &mut self,
        installation: InstallationId,
        contribution_id: &str,
        resource: Resource,
        descriptor: &CompositionDescriptor,
    ) -> Result<MountHandle, ApiError> {
        resource.validate()?;
        if !matches!(resource, Resource::Draft { .. }) || resource.vault_id() != self.vault_id {
            return Err(error(
                ErrorCode::PermissionDenied,
                "component mount requires an exact local Draft",
            ));
        }
        let item = self.installation(installation)?;
        descriptor.validate_for_manifest(&item.manifest)?;
        if item.manifest.host_api.min_minor != 3
            || !item.manifest.supports_api(HOST_API_V0_3)
            || !self.supported.contains(&Capability::NoteDraftEdit)
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "component API or draft capability unavailable",
            ));
        }
        let contribution = item
            .manifest
            .contributions
            .iter()
            .find(|c| c.id == contribution_id && c.kind == ContributionKind::ComponentProvider)
            .ok_or_else(|| error(ErrorCode::NotFound, "component contribution unavailable"))?;
        if descriptor.contribution_id != contribution_id
            || !contribution
                .required_capabilities
                .contains(&Capability::NoteDraftEdit)
            || contribution
                .required_capabilities
                .iter()
                .chain(&item.manifest.required_capabilities)
                .any(|c| *c != Capability::NoteDraftEdit)
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "mount does not satisfy all required capabilities",
            ));
        }
        let epoch = item.epoch;
        if self.components.records.len() >= MAX_MOUNTS
            || self
                .components
                .records
                .values()
                .filter(|r| r.identity.installation == installation)
                .count()
                >= MAX_PER_INSTALLATION
        {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "component mount resources are still in use",
            ));
        }
        let mount_id = self.components.identity()?;
        let identity = Arc::new(Identity {
            mount_id: mount_id.clone(),
            installation,
            epoch,
            resource,
            descriptor: descriptor.clone(),
        });
        self.components.records.insert(
            mount_id,
            Record {
                identity: identity.clone(),
                closing: false,
            },
        );
        Ok(MountHandle {
            owner: self.owner.clone(),
            identity,
        })
    }
    fn component_record(&self, mount: &MountHandle) -> Result<&Record, ApiError> {
        self.check_owner(&mount.owner)?;
        let record = self
            .components
            .records
            .get(mount.mount_id())
            .ok_or_else(stale)?;
        if !Arc::ptr_eq(&record.identity, &mount.identity) || record.closing {
            return Err(stale());
        }
        let item = self.installation(record.identity.installation)?;
        if item.epoch != record.identity.epoch {
            return Err(stale());
        }
        if !self.supported.contains(&Capability::NoteDraftEdit) {
            return Err(error(
                ErrorCode::PermissionDenied,
                "draft capability unavailable",
            ));
        }
        Ok(record)
    }
    /// Check this mount's live host ownership and installation epoch.
    pub fn validate_component_mount(&self, mount: &MountHandle) -> Result<(), ApiError> {
        self.component_record(mount).map(|_| ())
    }
    /// No reusable permit escapes: every call validates exact scope immediately.
    pub fn dispatch_draft<T>(
        &self,
        mount: &MountHandle,
        request: DraftRequest,
        handler: impl FnOnce(&AuthorizedDraftContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        request.validate()?;
        let record = self.component_record(mount)?;
        if request.api_version != HOST_API_V0_3
            || request.mount_id != record.identity.mount_id
            || request.resource != record.identity.resource
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "draft request differs from mount scope",
            ));
        }
        let item = self.installation(record.identity.installation)?;
        handler(&AuthorizedDraftContext {
            installation_id: record.identity.installation,
            package_id: item.manifest.package_id.clone(),
            package_digest: item.digest.clone(),
            vault_id: self.vault_id.clone(),
            grant_epoch: record.identity.epoch,
            mount_id: record.identity.mount_id.clone(),
            resource: record.identity.resource.clone(),
            request,
        })
    }
    fn authorized_component_context(
        &self,
        mount: &MountHandle,
    ) -> Result<AuthorizedComponentContext, ApiError> {
        let record = self.component_record(mount)?;
        let item = self.installation(record.identity.installation)?;
        Ok(AuthorizedComponentContext {
            installation_id: record.identity.installation,
            package_id: item.manifest.package_id.clone(),
            package_digest: item.digest.clone(),
            vault_id: self.vault_id.clone(),
            grant_epoch: record.identity.epoch,
            mount_id: record.identity.mount_id.clone(),
            resource: record.identity.resource.clone(),
        })
    }
    /// The trusted process binding supplies the mount; the wire request cannot
    /// choose another mount, resource or run. Event/input state belongs to the app.
    pub fn dispatch_component_context<T>(
        &self,
        mount: &MountHandle,
        request: ComponentContextRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        request.validate()?;
        let context = self.authorized_component_context(mount)?;
        handler(&context)
    }
    /// Authorize only exact live-mount access. The application independently
    /// checks event sequence, base revision and atomic checkpoint/effect state.
    pub fn dispatch_continuation<T>(
        &self,
        mount: &MountHandle,
        request: ContinuationRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        request.validate()?;
        let context = self.authorized_component_context(mount)?;
        if request.api_version != HOST_API_V0_3
            || request.mount_id != context.mount_id
            || request.resource != context.resource
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "continuation differs from mount scope",
            ));
        }
        handler(&context)
    }
    /// Authorize purpose discovery for the bound invocation. The trusted
    /// coordinator supplies purpose; the guest cannot select it in this request.
    pub fn dispatch_component_invocation<T>(
        &self,
        mount: &MountHandle,
        request: ComponentInvocationRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        request.validate()?;
        handler(&self.authorized_component_context(mount)?)
    }
    /// Read-only projection protocol access to this exact mount. The application
    /// must still bind query identity/revision and the coordinator must admit the
    /// live invocation; a mount grant is not a query or execution permission.
    pub fn dispatch_projection<T>(
        &self,
        mount: &MountHandle,
        request: ProjectionRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        request.validate()?;
        let context = self.authorized_component_context(mount)?;
        if let ProjectionRequest::Query {
            resource, mount_id, ..
        } = &request
            && (*resource != context.resource || *mount_id != context.mount_id)
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "projection differs from mount scope",
            ));
        }
        handler(&context)
    }
    /// Close only this mount; Closing retains quota until an actual release ack.
    pub fn close_component_mount(&mut self, mount: &MountHandle) -> Result<(), ApiError> {
        self.check_owner(&mount.owner)?;
        if let Some(record) = self.components.records.get_mut(mount.mount_id()) {
            if !Arc::ptr_eq(&record.identity, &mount.identity) {
                return Err(stale());
            }
            record.closing = true;
        }
        Ok(())
    }
    pub fn pending_component_cleanup(&self) -> Vec<ComponentCleanupTicket> {
        self.components
            .records
            .values()
            .filter(|r| r.closing)
            .map(|r| ComponentCleanupTicket {
                owner: self.owner.clone(),
                identity: r.identity.clone(),
            })
            .collect()
    }
    /// Trusted renderer acknowledgement only; JS cleanup is not a release proof.
    pub fn confirm_component_released(
        &mut self,
        ticket: &ComponentCleanupTicket,
    ) -> Result<(), ApiError> {
        self.check_owner(&ticket.owner)?;
        if let Some(record) = self.components.records.get(ticket.mount_id()) {
            if !record.closing || !Arc::ptr_eq(&record.identity, &ticket.identity) {
                return Err(stale());
            }
            self.components.records.remove(ticket.mount_id());
        }
        Ok(())
    }
}
