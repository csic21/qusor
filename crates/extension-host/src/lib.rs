//! Runtime-independent extension policy. No plugin code, I/O or knowledge storage runs here.
//!
//! Registration accepts metadata from a **trusted future package verifier**. Digest syntax
//! checks do not authenticate bytes or publishers. Sessions and permits are native host-only
//! handles; never deserialize them from plugin input or expose Rust handles over the wire.
//! All state is process-local. Restart requires registration and explicit grants again.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_HOST: AtomicU64 = AtomicU64::new(1);

use extension_api::{
    ApiError, ApiVersion, Capability, CapabilityRequest, Contribution, ErrorCode, HOST_API,
    Manifest, Resource, SUPPORTED_HOST_APIS, capability_available_in, capability_resource_valid,
    is_supported_api,
};

pub mod component;
pub mod durable;
mod durable_surface;
pub mod library;
pub mod notes;
pub mod platform;
pub mod surface;
pub mod vocab;

const MAX_INSTALLATIONS: usize = 256;
const MAX_SESSIONS: usize = 1024;
const MAX_GRANTS: usize = 256;

/// Host-local installation identity, never a package-supplied identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallationId {
    host: u64,
    sequence: u64,
}

impl InstallationId {
    /// Host-local sequence used for audit provenance, never an authorization token.
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}

/// The policy state does not claim that a plugin runtime has started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Enabled,
}

/// A host-approved capability and exact resource scope.
/// Only a vault-wide metadata grant covers individual documents. Capture proposals
/// target a vault directly; selection reads always require an exact selection grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub capability: Capability,
    pub resource: Resource,
}

/// Opaque handle bound to one host instance and one live connection.
/// The runtime adapter keeps it outside untrusted plugin payloads.
#[derive(Debug, Clone)]
pub struct Session {
    owner: Arc<()>,
    id: u64,
}

/// An in-flight policy decision. Final dispatch consumes and revalidates it.
/// This is not an approval of a content payload, a transaction or an idempotency receipt.
#[derive(Debug)]
pub struct Permit {
    owner: Arc<()>,
    session: u64,
    epoch: u64,
    request: CapabilityRequest,
}

/// Host-authenticated context supplied to the trusted application adapter.
/// This descriptive value is not itself an authorization token.
#[derive(Debug, Clone)]
pub struct AuthorizedContext {
    pub installation_id: InstallationId,
    pub package_id: String,
    pub publisher_id: String,
    pub package_digest: String,
    pub vault_id: String,
    pub grant_epoch: u64,
    pub request: CapabilityRequest,
}

#[derive(Debug)]
struct Installation {
    manifest: Manifest,
    digest: String,
    grants: Vec<Grant>,
    epoch: u64,
    status: Status,
}

#[derive(Debug)]
struct Connection {
    installation: InstallationId,
    epoch: u64,
    api_version: ApiVersion,
}

/// One vault-bound policy authority. No builtin/official privilege branch exists.
/// A separate host instance, including one for the same vault, rejects this host's handles.
pub struct Host {
    identity: u64,
    owner: Arc<()>,
    vault_id: String,
    supported: BTreeSet<Capability>,
    installations: BTreeMap<InstallationId, Installation>,
    sessions: BTreeMap<u64, Connection>,
    next_installation: u64,
    next_session: u64,
    surfaces: surface::SurfaceRegistry,
    components: component::ComponentRegistry,
}

impl Host {
    pub fn new(
        vault_id: impl Into<String>,
        supported: impl IntoIterator<Item = Capability>,
    ) -> Result<Self, ApiError> {
        let vault_id = vault_id.into();
        Resource::Vault {
            vault_id: vault_id.clone(),
        }
        .validate()?;
        let identity = NEXT_HOST
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| error(ErrorCode::BudgetExceeded, "host identity space exhausted"))?;
        Ok(Self {
            identity,
            owner: Arc::new(()),
            vault_id,
            supported: supported.into_iter().collect(),
            installations: BTreeMap::new(),
            sessions: BTreeMap::new(),
            next_installation: 1,
            next_session: 1,
            surfaces: surface::SurfaceRegistry::default(),
            components: component::ComponentRegistry::default(),
        })
    }

    /// Register already-verified metadata, disabled and without any grants.
    /// This does not install files, authenticate a publisher, or execute a package.
    pub fn register(
        &mut self,
        manifest: Manifest,
        package_digest: impl Into<String>,
    ) -> Result<InstallationId, ApiError> {
        let digest = package_digest.into();
        self.validate_package(&manifest, &digest)?;
        if self
            .installations
            .values()
            .any(|item| item.manifest.package_id == manifest.package_id)
        {
            return Err(error(
                ErrorCode::AlreadyInstalled,
                "package already registered",
            ));
        }
        if self.installations.len() >= MAX_INSTALLATIONS {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "installation limit reached",
            ));
        }
        let id = InstallationId {
            host: self.identity,
            sequence: take_id(&mut self.next_installation)?,
        };
        self.installations.insert(
            id,
            Installation {
                manifest,
                digest,
                grants: Vec::new(),
                epoch: 0,
                status: Status::Disabled,
            },
        );
        Ok(id)
    }

    pub fn status(&self, id: InstallationId) -> Result<Status, ApiError> {
        Ok(self.installation(id)?.status)
    }

    /// Rebind the same durable installation to trusted, reverified journal facts.
    /// Unlike an ordinary update this may be an explicit paired rollback. Keep
    /// the native identity so unreleased Surface resources retain their quota.
    fn restore_registration(
        &mut self,
        id: InstallationId,
        manifest: Manifest,
        digest: String,
    ) -> Result<(), ApiError> {
        self.validate_package(&manifest, &digest)?;
        let current = self.installation(id)?;
        if current.manifest.package_id != manifest.package_id
            || current.manifest.publisher_id != manifest.publisher_id
        {
            return Err(error(
                ErrorCode::InvalidState,
                "durable installation identity changed",
            ));
        }
        let epoch = self.advance_epoch(id)?;
        let installed = self
            .installations
            .get_mut(&id)
            .expect("installation checked");
        installed.manifest = manifest;
        installed.digest = digest;
        installed.grants.clear();
        installed.epoch = epoch;
        installed.status = Status::Disabled;
        self.drop_sessions(id);
        Ok(())
    }

    /// The vault this policy authority is bound to; never supplied by a guest.
    pub fn vault_id(&self) -> &str {
        &self.vault_id
    }

    pub fn manifest(&self, id: InstallationId) -> Result<&Manifest, ApiError> {
        Ok(&self.installation(id)?.manifest)
    }

    pub fn grants(&self, id: InstallationId) -> Result<&[Grant], ApiError> {
        Ok(&self.installation(id)?.grants)
    }

    /// Replace explicit user grants after trusted host authorization UI/policy.
    /// Validation is atomic: invalid input leaves existing grants and sessions intact.
    /// Even narrowing to an equivalent set invalidates every old connection and permit.
    pub fn replace_grants(
        &mut self,
        id: InstallationId,
        grants: Vec<Grant>,
    ) -> Result<(), ApiError> {
        let installed = self.installation(id)?;
        self.validate_grants(&installed.manifest, &grants)?;
        let required = required_granted(&installed.manifest, &grants);
        let epoch = self.advance_epoch(id)?;
        let installed = self
            .installations
            .get_mut(&id)
            .expect("installation checked");
        installed.epoch = epoch;
        installed.grants = grants;
        if !required {
            installed.status = Status::Disabled;
        }
        self.drop_sessions(id);
        Ok(())
    }

    fn validate_grants(&self, manifest: &Manifest, grants: &[Grant]) -> Result<(), ApiError> {
        if grants.len() > MAX_GRANTS {
            return Err(error(ErrorCode::BudgetExceeded, "grant limit reached"));
        }
        for (index, grant) in grants.iter().enumerate() {
            grant.resource.validate()?;
            if grant.capability == Capability::NoteDraftEdit {
                return Err(error(
                    ErrorCode::PermissionDenied,
                    "draft editing requires a live component mount",
                ));
            }
            if !self.supported.contains(&grant.capability)
                || !declared(manifest, grant.capability)
                || grant.resource.vault_id() != self.vault_id
                || !SUPPORTED_HOST_APIS.iter().any(|api| {
                    manifest.supports_api(*api)
                        && capability_resource_valid(grant.capability, &grant.resource, *api)
                })
            {
                return Err(error(
                    ErrorCode::PermissionDenied,
                    "grant exceeds declared capability or vault scope",
                ));
            }
            if grants[..index].contains(grant) {
                return Err(error(ErrorCode::InvalidRequest, "duplicate grant"));
            }
        }
        Ok(())
    }

    pub fn enable(&mut self, id: InstallationId) -> Result<(), ApiError> {
        let installed = self.installation(id)?;
        if installed.epoch == u64::MAX {
            return Err(error(ErrorCode::BudgetExceeded, "grant epoch exhausted"));
        }
        if !required_granted(&installed.manifest, &installed.grants) {
            return Err(error(
                ErrorCode::PermissionDenied,
                "required capabilities need explicit grants",
            ));
        }
        self.installations
            .get_mut(&id)
            .expect("installation checked")
            .status = Status::Enabled;
        Ok(())
    }

    /// Keep confirmed grants, but revoke all current sessions and in-flight decisions.
    pub fn disable(&mut self, id: InstallationId) -> Result<(), ApiError> {
        let epoch = self.advance_epoch(id)?;
        let installed = self
            .installations
            .get_mut(&id)
            .expect("installation checked");
        installed.epoch = epoch;
        installed.status = Status::Disabled;
        self.drop_sessions(id);
        Ok(())
    }

    /// Commit a verified metadata update. Every successful code change requires fresh grants.
    /// No migration/rollback of package files or private state is implemented by this method.
    pub fn update(
        &mut self,
        id: InstallationId,
        manifest: Manifest,
        package_digest: impl Into<String>,
    ) -> Result<(), ApiError> {
        let digest = package_digest.into();
        self.validate_package(&manifest, &digest)?;
        let old = self.installation(id)?;
        if old.manifest.package_id != manifest.package_id
            || old.manifest.publisher_id != manifest.publisher_id
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "package or publisher identity changed",
            ));
        }
        if manifest.package_version <= old.manifest.package_version || digest == old.digest {
            return Err(error(
                ErrorCode::InvalidState,
                "update needs a newer version and different digest",
            ));
        }
        let epoch = self.advance_epoch(id)?;
        self.installations.insert(
            id,
            Installation {
                manifest,
                digest,
                grants: Vec::new(),
                epoch,
                status: Status::Disabled,
            },
        );
        self.drop_sessions(id);
        Ok(())
    }

    /// Remove runtime metadata only. There is no access to Core knowledge or source blobs.
    pub fn unregister(&mut self, id: InstallationId) -> Result<(), ApiError> {
        self.installation(id)?;
        self.drop_sessions(id);
        self.installations.remove(&id);
        Ok(())
    }

    /// The compatibility entry always requests the legacy 0.1 protocol.
    pub fn open_session(&mut self, id: InstallationId) -> Result<Session, ApiError> {
        self.open_session_for_api(id, HOST_API)
    }

    /// Open a session at the highest protocol version shared by the package
    /// manifest and `supported`.
    pub fn open_negotiated_session(
        &mut self,
        id: InstallationId,
        supported: &[ApiVersion],
    ) -> Result<Session, ApiError> {
        let negotiated = self
            .installation(id)?
            .manifest
            .negotiate_api(supported)
            .ok_or_else(|| {
                error(
                    ErrorCode::UnsupportedVersion,
                    "session API is incompatible with the package",
                )
            })?;
        self.open_session_for_api(id, negotiated)
    }

    /// Freeze one explicitly negotiated API version into the connection.
    pub fn open_session_for_api(
        &mut self,
        id: InstallationId,
        api_version: ApiVersion,
    ) -> Result<Session, ApiError> {
        let installed = self.installation(id)?;
        if !is_supported_api(api_version)
            || !installed.manifest.supports_api(api_version)
            || installed
                .manifest
                .required_capabilities
                .iter()
                .any(|cap| !capability_available_in(*cap, api_version))
        {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "session API is incompatible with the package",
            ));
        }
        if installed.status != Status::Enabled {
            return Err(error(ErrorCode::Disabled, "installation is disabled"));
        }
        let epoch = installed.epoch;
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(error(ErrorCode::BudgetExceeded, "session limit reached"));
        }
        let session_id = take_id(&mut self.next_session)?;
        self.sessions.insert(
            session_id,
            Connection {
                installation: id,
                epoch,
                api_version,
            },
        );
        Ok(Session {
            owner: Arc::clone(&self.owner),
            id: session_id,
        })
    }

    /// Return the selected protocol only for a currently valid native session.
    pub fn session_api(&self, session: &Session) -> Result<ApiVersion, ApiError> {
        self.check_owner(&session.owner)?;
        let connection = self.sessions.get(&session.id).ok_or_else(stale)?;
        let installed = self.installation(connection.installation)?;
        if installed.status != Status::Enabled || installed.epoch != connection.epoch {
            return Err(stale());
        }
        Ok(connection.api_version)
    }

    /// Disconnecting a runtime connection also revokes all of its outstanding permits.
    pub fn close_session(&mut self, session: &Session) -> Result<(), ApiError> {
        self.check_owner(&session.owner)?;
        if self.sessions.remove(&session.id).is_none() {
            return Err(stale());
        }
        self.surfaces.invalidate_session(session.id);
        Ok(())
    }

    /// Validate a request from a host-authenticated runtime connection.
    pub fn authorize(
        &self,
        session: &Session,
        request: CapabilityRequest,
    ) -> Result<Permit, ApiError> {
        self.check_owner(&session.owner)?;
        let (_, installed) = self.check_request(session.id, &request)?;
        Ok(Permit {
            owner: Arc::clone(&self.owner),
            session: session.id,
            epoch: installed.epoch,
            request,
        })
    }

    /// Recheck immediately before a trusted synchronous action and consume the decision.
    ///
    /// Exclusive host access serializes this policy decision against revocation. The callback
    /// must be a short trusted adapter action, never plugin execution, network I/O or queued
    /// work that treats the context as a reusable permit. Real Core transactions must add their
    /// own payload/version checks, durable audit/outbox and a shared revocation ordering point.
    pub fn dispatch<T>(
        &mut self,
        permit: Permit,
        action: impl FnOnce(&AuthorizedContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.check_owner(&permit.owner)?;
        let (connection, installed) = self.check_request(permit.session, &permit.request)?;
        if permit.epoch != installed.epoch {
            return Err(stale());
        }
        action(&AuthorizedContext {
            installation_id: connection.installation,
            package_id: installed.manifest.package_id.clone(),
            publisher_id: installed.manifest.publisher_id.clone(),
            package_digest: installed.digest.clone(),
            vault_id: self.vault_id.clone(),
            grant_epoch: installed.epoch,
            request: permit.request,
        })
    }

    /// Only enabled and capability-authorized contributions are visible.
    /// Actual invocation must still authorize its exact resource through a live session.
    pub fn contributions(&self) -> Vec<(InstallationId, &Contribution)> {
        self.installations
            .iter()
            .filter(|(_, installed)| installed.status == Status::Enabled)
            .flat_map(|(id, installed)| {
                installed
                    .manifest
                    .contributions
                    .iter()
                    .filter(|contribution| {
                        contribution.required_capabilities.iter().all(|cap| {
                            installed
                                .grants
                                .iter()
                                .any(|grant| grant.capability == *cap)
                        })
                    })
                    .map(move |contribution| (*id, contribution))
            })
            .collect()
    }

    fn validate_package(&self, manifest: &Manifest, digest: &str) -> Result<(), ApiError> {
        manifest.validate()?;
        if manifest.negotiate_api(&SUPPORTED_HOST_APIS).is_none() {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "incompatible host API",
            ));
        }
        if manifest
            .required_capabilities
            .iter()
            .any(|cap| !self.supported.contains(cap))
        {
            return Err(error(
                ErrorCode::CapabilityUnavailable,
                "required capability unavailable",
            ));
        }
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(error(
                ErrorCode::InvalidManifest,
                "expected lowercase SHA-256 digest syntax",
            ));
        }
        Ok(())
    }

    fn installation(&self, id: InstallationId) -> Result<&Installation, ApiError> {
        self.installations
            .get(&id)
            .ok_or_else(|| error(ErrorCode::NotFound, "installation not found"))
    }

    fn check_owner(&self, owner: &Arc<()>) -> Result<(), ApiError> {
        if !Arc::ptr_eq(&self.owner, owner) {
            return Err(stale());
        }
        Ok(())
    }

    fn check_request(
        &self,
        session: u64,
        request: &CapabilityRequest,
    ) -> Result<(&Connection, &Installation), ApiError> {
        request.validate()?;
        let connection = self.sessions.get(&session).ok_or_else(stale)?;
        if request.api_version != connection.api_version {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "request API differs from its negotiated session",
            ));
        }
        let installed = self
            .installations
            .get(&connection.installation)
            .ok_or_else(stale)?;
        if installed.status != Status::Enabled || connection.epoch != installed.epoch {
            return Err(stale());
        }
        if request.resource.vault_id() != self.vault_id
            || !capability_resource_valid(
                request.capability,
                &request.resource,
                connection.api_version,
            )
            || !self.supported.contains(&request.capability)
            || !declared(&installed.manifest, request.capability)
            || !installed.grants.iter().any(|grant| covers(grant, request))
        {
            return Err(error(
                ErrorCode::PermissionDenied,
                "request outside granted capability or resource",
            ));
        }
        Ok((connection, installed))
    }

    fn advance_epoch(&mut self, id: InstallationId) -> Result<u64, ApiError> {
        match next_epoch(self.installation(id)?.epoch) {
            Ok(epoch) => Ok(epoch),
            Err(err) => {
                self.installations
                    .get_mut(&id)
                    .expect("installation checked")
                    .status = Status::Disabled;
                self.drop_sessions(id);
                Err(err)
            }
        }
    }

    fn drop_sessions(&mut self, id: InstallationId) {
        self.surfaces.invalidate_installation(id);
        self.components.invalidate_installation(id);
        self.sessions
            .retain(|_, session| session.installation != id);
    }
}

fn error(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}
fn stale() -> ApiError {
    error(
        ErrorCode::StaleContext,
        "session or policy decision is no longer valid",
    )
}
fn next_epoch(epoch: u64) -> Result<u64, ApiError> {
    epoch
        .checked_add(1)
        .ok_or_else(|| error(ErrorCode::BudgetExceeded, "grant epoch exhausted"))
}
fn take_id(counter: &mut u64) -> Result<u64, ApiError> {
    let next = counter
        .checked_add(1)
        .ok_or_else(|| error(ErrorCode::BudgetExceeded, "identity space exhausted"))?;
    let value = *counter;
    *counter = next;
    Ok(value)
}
fn declared(manifest: &Manifest, capability: Capability) -> bool {
    manifest.required_capabilities.contains(&capability)
        || manifest.optional_capabilities.contains(&capability)
}
fn required_granted(manifest: &Manifest, grants: &[Grant]) -> bool {
    manifest
        .required_capabilities
        .iter()
        .all(|cap| grants.iter().any(|grant| grant.capability == *cap))
}
fn covers(grant: &Grant, request: &CapabilityRequest) -> bool {
    grant.capability == request.capability
        && grant.resource.vault_id() == request.resource.vault_id()
        && (grant.resource == request.resource
            || matches!(
                (&grant.resource, &request.resource),
                (Resource::Vault { .. }, Resource::Document { .. })
            ) && grant.capability == Capability::DocumentMetadataRead)
}

#[cfg(test)]
mod internal_tests {
    use super::*;

    #[test]
    fn exhausted_epoch_revocation_fails_closed() {
        let mut host = Host::new("vault", []).unwrap();
        let manifest = Manifest {
            manifest_version: 1,
            package_id: "org.test.empty".into(),
            publisher_id: "org.test".into(),
            package_version: extension_api::PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: extension_api::ApiRange {
                major: 0,
                min_minor: 1,
                max_minor: 1,
            },
            required_capabilities: vec![],
            optional_capabilities: vec![],
            contributions: vec![],
        };
        let id = host.register(manifest, "a".repeat(64)).unwrap();
        host.enable(id).unwrap();
        let session = host.open_session(id).unwrap();
        host.installations.get_mut(&id).unwrap().epoch = u64::MAX;
        assert_eq!(
            host.replace_grants(id, vec![]).unwrap_err().code,
            ErrorCode::BudgetExceeded
        );
        assert_eq!(host.status(id).unwrap(), Status::Disabled);
        assert_eq!(
            host.close_session(&session).unwrap_err().code,
            ErrorCode::StaleContext
        );
        assert_eq!(host.enable(id).unwrap_err().code, ErrorCode::BudgetExceeded);
    }

    #[test]
    fn exhausted_identifiers_fail_without_reuse() {
        let mut counter = u64::MAX;
        assert_eq!(
            take_id(&mut counter).unwrap_err().code,
            ErrorCode::BudgetExceeded
        );
        assert_eq!(counter, u64::MAX);
        assert_eq!(
            next_epoch(u64::MAX).unwrap_err().code,
            ErrorCode::BudgetExceeded
        );
    }
}
