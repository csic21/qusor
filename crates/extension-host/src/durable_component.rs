//! Temporary component authorization over the same installation directory and Host.
use super::*;
use crate::component::{
    AuthorizedComponentContext, AuthorizedDraftContext, ComponentCleanupTicket, MountHandle,
};
use extension_api::{
    Resource,
    component_invocation::ComponentInvocationRequest,
    composition::{ComponentContextRequest, CompositionDescriptor},
    continuation::ContinuationRequest,
    draft::DraftRequest,
    projection::ProjectionRequest,
};

/// Fresh authenticated installation facts, never a grant or an enabled session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentInstallation {
    pub installation_id: i64,
    pub revision: i64,
    pub generation: i64,
    pub manifest: Manifest,
    pub digest: String,
    pub vault_id: String,
    pub composition: CompositionDescriptor,
    pub composition_sha256: String,
}
pub trait ComponentAuthoritySource {
    fn component_installation(
        &self,
        package_id: &str,
        read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError>;
}

/// Only trusted native user interaction may construct this local approval.
/// Preferences, installation existence and guest JSON never imply this intent.
#[derive(Debug)]
pub struct UserMountIntent {
    expected: ComponentInstallation,
    resource: Resource,
}
impl UserMountIntent {
    pub fn new(expected: ComponentInstallation, resource: Resource) -> Result<Self, ApiError> {
        resource.validate()?;
        if !matches!(&resource, Resource::Draft { .. }) || resource.vault_id() != expected.vault_id
        {
            return Err(denied("component intent requires an exact bound Draft"));
        }
        expected
            .composition
            .validate_for_manifest(&expected.manifest)?;
        Ok(Self { expected, resource })
    }
}
/// A host-owned mount, with authenticated module/asset identity for its controller.
#[derive(Clone, Debug)]
pub struct MountedComponent {
    handle: MountHandle,
    installation: ComponentInstallation,
}
impl MountedComponent {
    pub fn handle(&self) -> &MountHandle {
        &self.handle
    }
    pub fn installation(&self) -> &ComponentInstallation {
        &self.installation
    }
}
impl DurableHost {
    fn valid_component(&self, snapshot: &ComponentInstallation) -> bool {
        snapshot.installation_id > 0
            && snapshot.revision > 0
            && snapshot.generation > 0
            && snapshot.vault_id == self.vault_id
            && snapshot.composition_sha256.len() == 64
            && snapshot
                .composition_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && snapshot
                .composition
                .validate_for_manifest(&snapshot.manifest)
                .is_ok()
    }
    pub fn configure_component_identity(&mut self, nonce: [u8; 16]) -> Result<(), ApiError> {
        self.host.configure_component_identity(nonce)
    }
    pub fn mount_component(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        intent: UserMountIntent,
        now: u64,
    ) -> Result<MountedComponent, ApiError> {
        self.time(now);
        let expected = intent.expected;
        let snapshot = source
            .component_installation(&expected.manifest.package_id, AuthorityRead::Activation)?;
        if snapshot != expected || !self.valid_component(&snapshot) {
            return Err(denied(
                "component installation differs from the explicit user intent",
            ));
        }
        if let Some(old) = self.registrations.get(&snapshot.installation_id)
            && old.revision == snapshot.revision
            && old.generation == snapshot.generation
            && old.composition.as_ref().is_some_and(|prior| {
                prior
                    != &(
                        snapshot.composition.clone(),
                        snapshot.composition_sha256.clone(),
                    )
            })
        {
            return Err(denied(
                "component asset changed without new installation counters",
            ));
        }
        let existed = self.registrations.contains_key(&snapshot.installation_id);
        let installation = self.bind_registration(
            snapshot.installation_id,
            snapshot.revision,
            snapshot.generation,
            &snapshot.manifest,
            &snapshot.digest,
        )?;
        let handle = match self.host.open_component_mount(
            installation,
            &snapshot.composition.contribution_id,
            intent.resource,
            &snapshot.composition,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                if !existed {
                    let _ = self.host.unregister(installation);
                    self.registrations.remove(&snapshot.installation_id);
                }
                return Err(error);
            }
        };
        self.registrations
            .get_mut(&snapshot.installation_id)
            .expect("registration bound")
            .composition = Some((
            snapshot.composition.clone(),
            snapshot.composition_sha256.clone(),
        ));
        self.component_bindings
            .insert(handle.mount_id().into(), snapshot.clone());
        Ok(MountedComponent {
            handle,
            installation: snapshot,
        })
    }
    /// Also used immediately before Boot and authenticated Result acceptance.
    /// No execution permit is returned or retained across this gate.
    pub fn validate_component_current(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
    ) -> Result<(), ApiError> {
        // Reject a foreign/stale mount before any Source read can fence a successor.
        self.host.validate_component_mount(mount)?;
        let expected = self
            .component_bindings
            .get(mount.mount_id())
            .cloned()
            .ok_or_else(|| denied("component durable binding is unavailable"))?;
        self.time(now);
        let fresh = source.component_installation(
            &expected.manifest.package_id,
            AuthorityRead::Current { run_id },
        );
        match fresh {
            Ok(snapshot) if snapshot == expected && self.valid_component(&snapshot) => Ok(()),
            _ => {
                self.remove(expected.installation_id);
                Err(denied("component authority changed or is unavailable"))
            }
        }
    }
    pub fn dispatch_draft<T>(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
        request: DraftRequest,
        handler: impl FnOnce(&AuthorizedDraftContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.validate_component_current(source, mount, now, run_id)?;
        self.host.dispatch_draft(mount, request, handler)
    }
    /// Fresh own-run authorization before the application reads invocation input.
    /// No resource scope is inferred from guest-supplied context claims.
    pub fn dispatch_component_context<T>(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
        request: ComponentContextRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.validate_component_current(source, mount, now, run_id)?;
        self.host
            .dispatch_component_context(mount, request, handler)
    }
    /// Completed-event reconciliation still needs this current mount authority;
    /// application base-revision changes never substitute for installation identity.
    pub fn dispatch_continuation<T>(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
        request: ContinuationRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.validate_component_current(source, mount, now, run_id)?;
        self.host.dispatch_continuation(mount, request, handler)
    }
    /// Purpose remains coordinator-owned, after fresh own-run/mount authority.
    pub fn dispatch_component_invocation<T>(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
        request: ComponentInvocationRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.validate_component_current(source, mount, now, run_id)?;
        self.host
            .dispatch_component_invocation(mount, request, handler)
    }
    /// Projection has no text effects. Query freshness and read-only task
    /// admission remain separate from this fresh installation/mount check.
    pub fn dispatch_projection<T>(
        &mut self,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        now: u64,
        run_id: Option<&str>,
        request: ProjectionRequest,
        handler: impl FnOnce(&AuthorizedComponentContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.validate_component_current(source, mount, now, run_id)?;
        self.host.dispatch_projection(mount, request, handler)
    }
    pub fn close_component(&mut self, mount: &MountHandle) -> Result<(), ApiError> {
        self.host.close_component_mount(mount)?;
        self.component_bindings.remove(mount.mount_id());
        Ok(())
    }
    pub fn pending_component_cleanup(&self) -> Vec<ComponentCleanupTicket> {
        self.host.pending_component_cleanup()
    }
    /// Only trusted native teardown may report actual mount/job release here.
    pub fn confirm_component_released(
        &mut self,
        ticket: &ComponentCleanupTicket,
    ) -> Result<(), ApiError> {
        self.host.confirm_component_released(ticket)
    }
}
