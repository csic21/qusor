//! Restore durable intent into one live policy Host. Records never restore sessions.
//!
//! The trusted caller serializes journal mutations, Core version resolution and
//! the actual handler read/commit in this same dispatch gate or consistent snapshot. The source
//! must re-read current durable facts, not return a cached activation snapshot.
pub use crate::durable_surface::DurableSurface;
#[path = "durable_component.rs"]
mod component;
#[path = "durable_run.rs"]
mod run;
use crate::{Grant, Host, InstallationId, Session};
pub use component::{
    ComponentAuthoritySource, ComponentInstallation, MountedComponent, UserMountIntent,
};
use extension_api::{ApiError, ApiVersion, Capability, CapabilityRequest, ErrorCode, Manifest};
pub use run::{ManagedRun, RunPhase};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableGrant {
    pub grant: Grant,
    pub expires_at: u64,
    /// Preserved durable source/version constraint; None means explicitly unversioned.
    pub resource_version: Option<String>,
}

/// Host-resolved current version, not a version self-reported by the guest.
/// Resolve it under the same gate/snapshot as the handler's actual data access.
pub struct VersionedRequest {
    pub request: CapabilityRequest,
    pub resource_version: Option<String>,
}

/// Trusted storage facts, not a permit. The verifier/source owns byte authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritySnapshot {
    pub installation_id: i64,
    pub revision: i64,
    pub generation: i64,
    pub manifest: Manifest,
    pub digest: String,
    pub vault_id: String,
    pub enable_intent: bool,
    /// V1 did not freeze a grant-record API; V2 must retain its explicit API.
    pub required_api: Option<ApiVersion>,
    pub grants: Vec<DurableGrant>,
}

#[derive(Debug, Clone, Copy)]
pub enum AuthorityRead<'a> {
    /// May perform bounded private-state preflight, before a VM invocation starts.
    Activation,
    /// No guest execution or provider I/O in the per-message authorization gate.
    Current { run_id: Option<&'a str> },
}

/// Trusted journal tombstone, never a guest-supplied cleanup approval.
#[derive(Debug, Clone)]
pub struct RemovedInstallation {
    pub installation_id: i64,
    pub package_id: String,
    pub vault_id: String,
    pub revision: i64,
    pub generation: i64,
}

/// Implemented by trusted durable infrastructure. Neither method grants a capability.
pub trait AuthoritySource {
    fn authority_snapshot(
        &self,
        package_id: &str,
        read: AuthorityRead<'_>,
    ) -> Result<AuthoritySnapshot, ApiError>;
    fn removed_installation(&self, _package_id: &str) -> Result<RemovedInstallation, ApiError> {
        Err(ApiError::new(
            ErrorCode::InvalidState,
            "durable uninstall proof unavailable",
        ))
    }
}

/// Host-only handle. No wire deserialization and no old process session resurrection.
#[derive(Debug, Clone)]
pub struct DurableSession {
    installation: InstallationId,
    persistent_id: i64,
    package_id: String,
    session: Session,
}

struct Registration {
    installation: InstallationId,
    manifest: Manifest,
    digest: String,
    revision: i64,
    generation: i64,
    composition: Option<(extension_api::composition::CompositionDescriptor, String)>,
}

struct Binding {
    installation: InstallationId,
    snapshot: AuthoritySnapshot,
    session_id: Option<u64>,
}

/// One vault, one Host, many restored packages; storage has no parallel policy Host.
pub struct DurableHost {
    host: Host,
    vault_id: String,
    registrations: BTreeMap<i64, Registration>,
    bindings: BTreeMap<i64, Binding>,
    component_bindings: BTreeMap<String, ComponentInstallation>,
    last_time: u64,
}

fn denied(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::PermissionDenied, message)
}

impl DurableHost {
    pub fn new(
        vault_id: impl Into<String>,
        supported: impl IntoIterator<Item = Capability>,
    ) -> Result<Self, ApiError> {
        let vault_id = vault_id.into();
        Ok(Self {
            host: Host::new(vault_id.clone(), supported)?,
            vault_id,
            registrations: BTreeMap::new(),
            bindings: BTreeMap::new(),
            component_bindings: BTreeMap::new(),
            last_time: 0,
        })
    }

    /// Check a proposed durable grant against this actual Host, without enabling it
    /// or issuing a session before the journal commits the user's intent.
    pub fn validate_activation_intent(
        &self,
        manifest: &Manifest,
        digest: &str,
        grants: &[Grant],
        api: ApiVersion,
    ) -> Result<(), ApiError> {
        self.host.validate_package(manifest, digest)?;
        self.host.validate_grants(manifest, grants)?;
        if !extension_api::is_supported_api(api) || !manifest.supports_api(api) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "activation API is unsupported",
            ));
        }
        if grants
            .iter()
            .any(|g| !extension_api::capability_resource_valid(g.capability, &g.resource, api))
        {
            return Err(denied("durable grant requires another negotiated API"));
        }
        if !crate::required_granted(manifest, grants) {
            return Err(denied("required capabilities need explicit grants"));
        }
        Ok(())
    }

    fn time(&mut self, now: u64) -> u64 {
        self.last_time = self.last_time.max(now);
        self.last_time
    }

    fn valid(&self, snapshot: &AuthoritySnapshot, package: &str, now: u64) -> bool {
        snapshot.installation_id > 0
            && snapshot.revision > 0
            && snapshot.generation > 0
            && snapshot.manifest.package_id == package
            && snapshot.vault_id == self.vault_id
            && snapshot.enable_intent
            && snapshot.grants.iter().all(|g| g.expires_at > now)
    }

    fn remove(&mut self, id: i64) {
        if let Some(binding) = self.bindings.get_mut(&id) {
            binding.session_id = None;
        }
        if let Some(registration) = self.registrations.get(&id) {
            let _ = self.host.disable(registration.installation);
        }
        self.component_bindings
            .retain(|_, snapshot| snapshot.installation_id != id);
    }

    /// One directory for both durable-grant sessions and temporary components.
    /// Unchanged facts preserve identity/epoch; changed facts revoke both paths.
    fn bind_registration(
        &mut self,
        persistent_id: i64,
        revision: i64,
        generation: i64,
        manifest: &Manifest,
        digest: &str,
    ) -> Result<InstallationId, ApiError> {
        self.host.validate_package(manifest, digest)?;
        if persistent_id <= 0 || revision <= 0 || generation <= 0 {
            return Err(denied("invalid installation counters"));
        }
        if let Some(old) = self.registrations.get(&persistent_id) {
            if old.manifest.package_id != manifest.package_id
                || old.manifest.publisher_id != manifest.publisher_id
                || revision < old.revision
                || generation < old.generation
                || ((revision == old.revision || generation == old.generation)
                    && (revision != old.revision
                        || generation != old.generation
                        || old.manifest != *manifest
                        || old.digest != digest))
            {
                return Err(denied(
                    "installation identity or counters changed inconsistently",
                ));
            }
            let id = old.installation;
            if old.revision == revision && old.generation == generation {
                return Ok(id);
            }
            self.remove(persistent_id);
            self.host
                .restore_registration(id, manifest.clone(), digest.into())?;
            self.registrations.insert(
                persistent_id,
                Registration {
                    installation: id,
                    manifest: manifest.clone(),
                    digest: digest.into(),
                    revision,
                    generation,
                    composition: None,
                },
            );
            return Ok(id);
        }
        let previous = self
            .registrations
            .iter()
            .find_map(|(id, old)| (old.manifest.package_id == manifest.package_id).then_some(*id));
        if previous.is_some() {
            // New installation facts cannot prove that the predecessor was really
            // uninstalled or that its native resources were released. The trusted
            // tombstone + dual-resource retirement path must run first.
            return Err(denied(
                "previous package installation must be retired before rebinding",
            ));
        }
        let installation = self.host.register(manifest.clone(), digest)?;
        self.registrations.insert(
            persistent_id,
            Registration {
                installation,
                manifest: manifest.clone(),
                digest: digest.into(),
                revision,
                generation,
                composition: None,
            },
        );
        Ok(installation)
    }

    pub fn activate(
        &mut self,
        source: &dyn AuthoritySource,
        package_id: &str,
        api: ApiVersion,
        now: u64,
    ) -> Result<DurableSession, ApiError> {
        // Even a failed reactivation cannot leave the old package session active.
        let previous = self.registrations.iter().find_map(|(id, registration)| {
            (registration.manifest.package_id == package_id).then_some(*id)
        });
        if let Some(id) = previous {
            self.remove(id);
        }
        let now = self.time(now);
        let snapshot = source.authority_snapshot(package_id, AuthorityRead::Activation)?;
        if !self.valid(&snapshot, package_id, now) {
            return Err(denied("durable activation intent is invalid or expired"));
        }
        if snapshot
            .required_api
            .is_some_and(|required| required != api)
        {
            return Err(denied("activation API differs from durable authorization"));
        }
        if self
            .bindings
            .get(&snapshot.installation_id)
            .is_some_and(|b| b.snapshot.manifest.package_id != package_id)
        {
            return Err(denied(
                "durable installation identity aliases another package",
            ));
        }
        if self
            .bindings
            .get(&snapshot.installation_id)
            .is_some_and(|binding| {
                snapshot.revision < binding.snapshot.revision
                    || snapshot.generation < binding.snapshot.generation
                    || ((snapshot.revision == binding.snapshot.revision
                        || snapshot.generation == binding.snapshot.generation)
                        && snapshot != binding.snapshot)
            })
        {
            return Err(denied(
                "durable authority counters regressed or changed without a new generation",
            ));
        }
        self.validate_activation_intent(
            &snapshot.manifest,
            &snapshot.digest,
            &snapshot
                .grants
                .iter()
                .map(|g| g.grant.clone())
                .collect::<Vec<_>>(),
            api,
        )?;
        let existing = self.registrations.contains_key(&snapshot.installation_id);
        let installation = self.bind_registration(
            snapshot.installation_id,
            snapshot.revision,
            snapshot.generation,
            &snapshot.manifest,
            &snapshot.digest,
        )?;
        // Reader activation retains its established reset semantics.
        self.host.restore_registration(
            installation,
            snapshot.manifest.clone(),
            snapshot.digest.clone(),
        )?;
        let result = (|| {
            self.host.replace_grants(
                installation,
                snapshot.grants.iter().map(|g| g.grant.clone()).collect(),
            )?;
            self.host.enable(installation)?;
            self.host.open_session_for_api(installation, api)
        })();
        let session = match result {
            Ok(session) => session,
            Err(error) => {
                if existing {
                    let _ = self.host.disable(installation);
                } else {
                    let _ = self.host.unregister(installation);
                    self.registrations.remove(&snapshot.installation_id);
                }
                return Err(error);
            }
        };
        let persistent_id = snapshot.installation_id;
        self.bindings.insert(
            persistent_id,
            Binding {
                installation,
                snapshot,
                session_id: Some(session.id),
            },
        );
        Ok(DurableSession {
            installation,
            persistent_id,
            package_id: package_id.into(),
            session,
        })
    }

    /// Re-read durable state, then authorize and dispatch within the caller's shared gate.
    /// A run id comes only from the trusted process controller, never from guest JSON.
    pub fn dispatch<T>(
        &mut self,
        source: &dyn AuthoritySource,
        session: &DurableSession,
        now: u64,
        run_id: Option<&str>,
        request: VersionedRequest,
        handler: impl FnOnce(&crate::AuthorizedContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let expected = self.revalidate(source, session, now, run_id)?;
        if !expected.grants.iter().any(|grant| {
            crate::covers(&grant.grant, &request.request)
                && grant
                    .resource_version
                    .as_ref()
                    .is_none_or(|version| request.resource_version.as_ref() == Some(version))
        }) {
            return Err(denied(
                "current resource version differs from durable grant",
            ));
        }
        let permit = self.host.authorize(&session.session, request.request)?;
        self.host.dispatch(permit, handler)
    }

    /// Revalidate execution readiness before Boot or result acceptance, within
    /// the caller's management gate. This is not a capability permit: every
    /// subsequent call still needs dispatch and trusted Core version resolution.
    /// `run_id` must come from the bound controller/lease, never guest bytes.
    pub fn validate_current(
        &mut self,
        source: &dyn AuthoritySource,
        session: &DurableSession,
        now: u64,
        run_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.revalidate(source, session, now, run_id)?;
        self.host.session_api(&session.session).map(|_| ())
    }

    fn revalidate(
        &mut self,
        source: &dyn AuthoritySource,
        session: &DurableSession,
        now: u64,
        run_id: Option<&str>,
    ) -> Result<AuthoritySnapshot, ApiError> {
        let binding = self
            .bindings
            .get(&session.persistent_id)
            .filter(|b| {
                b.installation == session.installation && b.session_id == Some(session.session.id)
            })
            .ok_or_else(|| denied("unknown or stale durable session"))?;
        let expected = binding.snapshot.clone();
        let now = self.time(now);
        let fresh =
            source.authority_snapshot(&session.package_id, AuthorityRead::Current { run_id });
        match fresh {
            Ok(snapshot)
                if snapshot == expected && self.valid(&snapshot, &session.package_id, now) => {}
            _ => {
                self.remove(session.persistent_id);
                return Err(denied("durable authority changed or is unavailable"));
            }
        }
        Ok(expected)
    }

    /// A short native/guest-operation gate, never guest execution or provider work.
    /// The restricted facade checks ticket ownership and exact durable versions;
    /// it does not expose a mutable Host or a raw Session to the application.
    pub fn with_surface<T>(
        &mut self,
        source: &dyn AuthoritySource,
        session: &DurableSession,
        now: u64,
        run_id: Option<&str>,
        action: impl FnOnce(&mut DurableSurface<'_>) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let snapshot = self.revalidate(source, session, now, run_id)?;
        action(&mut DurableSurface::new(
            &mut self.host,
            &session.session,
            &snapshot.grants,
        ))
    }

    pub fn configure_surface_identity(&mut self, nonce: [u8; 16]) -> Result<(), ApiError> {
        self.host.configure_surface_identity(nonce)
    }

    pub fn pending_surface_cleanup(&self) -> Vec<crate::surface::NativeCleanupTicket> {
        self.host.pending_surface_cleanup()
    }

    /// Called by trusted native teardown only after actual release acknowledgement.
    pub fn confirm_surface_closed(
        &mut self,
        ticket: &crate::surface::NativeCleanupTicket,
    ) -> Result<(), ApiError> {
        self.host.confirm_surface_closed(ticket)
    }

    pub fn invalidate_document_surfaces(
        &mut self,
        resource: &extension_api::Resource,
    ) -> Result<(), ApiError> {
        self.host.invalidate_document_surfaces(resource)
    }

    /// Free a native registration only after durable uninstall and actual resource
    /// release. A merely closed/disabled installation keeps its identity and quota.
    pub fn retire_removed(
        &mut self,
        source: &dyn AuthoritySource,
        installation_id: i64,
        package_id: &str,
    ) -> Result<(), ApiError> {
        let registration = self
            .registrations
            .get(&installation_id)
            .ok_or_else(|| denied("durable installation binding not found"))?;
        let removed = source.removed_installation(package_id)?;
        if registration.manifest.package_id != package_id
            || removed.package_id != package_id
            || removed.vault_id != self.vault_id
            || removed.installation_id != installation_id
            || removed.revision <= registration.revision
            || removed.generation <= registration.generation
        {
            return Err(denied("durable uninstall does not match this installation"));
        }
        let installation = registration.installation;
        self.remove(installation_id);
        if self.host.has_surface_resources(installation)
            || self.host.has_component_resources(installation)
        {
            return Err(ApiError::new(
                ErrorCode::InvalidState,
                "native resources are not confirmed released",
            ));
        }
        self.host.unregister(installation)?;
        self.bindings.remove(&installation_id);
        self.registrations.remove(&installation_id);
        Ok(())
    }

    pub fn close(&mut self, session: &DurableSession) -> Result<(), ApiError> {
        let binding = self
            .bindings
            .get(&session.persistent_id)
            .filter(|b| {
                b.installation == session.installation && b.session_id == Some(session.session.id)
            })
            .ok_or_else(|| denied("unknown or stale durable session"))?;
        self.host.close_session(&session.session)?;
        let _ = binding;
        self.remove(session.persistent_id);
        Ok(())
    }
}
