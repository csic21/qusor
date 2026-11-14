//! Generic Host-mediated `ext.<provider_ns>.<endpoint>` service router.
//!
//! All cross-extension routing passes through the Host authority:
//! - Acyclic dependency validation via `capability_composition`.
//! - Live installation/status/epoch verification for both caller and provider.
//! - Strict `caller ∩ provider` delegated capability intersection (never escalates).
//! - Bounded opaque frame payloads (`1..=MAX_SERVICE_FRAME_BYTES`).

use std::collections::{BTreeMap, BTreeSet};

use crate::{Host, InstallationId, Status, error, stale};
use capability_composition::{
    CompositionError, CompositionLayer, CompositionPlan, CompositionProfile, Entry, ServiceId,
    ServiceProvision, ServiceRequirement, ServiceVersion, VersionRequirement,
};
use extension_api::{
    ApiError, ApiVersion, ErrorCode, HOST_API_V0_8, Resource, is_supported_api,
    orthogonal::{
        CapabilityId, CapabilityRequirement, MAX_ORTHOGONAL_CAPABILITIES, MAX_SERVICE_FRAME_BYTES,
        ServiceEndpoint,
    },
    validate_opaque_id,
};

const MAX_CALL_CHAIN_DEPTH: usize = 8;

/// Bounded opaque service invocation frame sent by a caller to an `ext.*` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCallRequest {
    pub api_version: ApiVersion,
    pub request_id: String,
    pub service: CapabilityId,
    pub resource: Resource,
    pub requested_capabilities: Vec<CapabilityId>,
    pub payload: Vec<u8>,
}

impl ServiceCallRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if !is_supported_api(self.api_version) || self.api_version < HOST_API_V0_8 {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "service call requires Host API >= 0.8",
            ));
        }
        validate_opaque_id(&self.request_id, "request_id")?;
        self.service.validate_with(ErrorCode::InvalidRequest)?;
        if !self.service.is_extension() {
            return Err(error(
                ErrorCode::InvalidRequest,
                "service target must use ext.<provider_ns>.<endpoint> namespace",
            ));
        }
        self.resource.validate()?;
        if self.requested_capabilities.len() > MAX_ORTHOGONAL_CAPABILITIES {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "too many requested delegated capabilities",
            ));
        }
        let mut seen = BTreeSet::new();
        for cap in &self.requested_capabilities {
            cap.validate_with(ErrorCode::InvalidRequest)?;
            if !seen.insert(cap.clone()) {
                return Err(error(
                    ErrorCode::InvalidRequest,
                    "duplicate capability in requested_capabilities",
                ));
            }
        }
        if self.payload.is_empty() || self.payload.len() > MAX_SERVICE_FRAME_BYTES {
            return Err(error(
                ErrorCode::InvalidRequest,
                "service frame payload must be 1..=MAX_SERVICE_FRAME_BYTES bytes",
            ));
        }
        Ok(())
    }
}

/// Active or deactivated provider endpoint registered in `ExtensionRouter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredService {
    pub provider: InstallationId,
    pub endpoint: ServiceEndpoint,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouterNode {
    installation: InstallationId,
    package_id: String,
    epoch: u64,
    provides: Vec<ServiceEndpoint>,
    requires: Vec<CapabilityRequirement>,
    active: bool,
}

/// In-flight cross-extension routing decision bound to both caller and provider epochs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCallPermit {
    host_identity: u64,
    caller_installation: InstallationId,
    caller_epoch: u64,
    provider_installation: InstallationId,
    provider_epoch: u64,
    service: CapabilityId,
    negotiated_major: u16,
    negotiated_minor: u16,
    resource: Resource,
    delegated_capabilities: BTreeSet<CapabilityId>,
    call_chain: Vec<InstallationId>,
    request_id: String,
    payload: Vec<u8>,
}

impl ServiceCallPermit {
    pub fn delegated_capabilities(&self) -> &BTreeSet<CapabilityId> {
        &self.delegated_capabilities
    }

    pub fn call_chain(&self) -> &[InstallationId] {
        &self.call_chain
    }
}

/// Host-authenticated context supplied to the trusted service dispatch handler.
#[derive(Debug, Clone)]
pub struct AuthorizedServiceCall<'a> {
    pub caller_installation: InstallationId,
    pub caller_package_id: String,
    pub provider_installation: InstallationId,
    pub provider_package_id: String,
    pub vault_id: String,
    pub service: CapabilityId,
    pub negotiated_major: u16,
    pub negotiated_minor: u16,
    pub resource: Resource,
    pub delegated_capabilities: BTreeSet<CapabilityId>,
    pub call_chain: &'a [InstallationId],
    pub request_id: &'a str,
    pub payload: &'a [u8],
}

/// Generic `ext.<provider_ns>.<endpoint>` service registry and forwarding guard.
#[derive(Debug, Clone)]
pub struct ExtensionRouter {
    host_identity: u64,
    nodes: BTreeMap<InstallationId, RouterNode>,
    services: BTreeMap<CapabilityId, RegisteredService>,
    plan: Option<CompositionPlan>,
}

impl ExtensionRouter {
    pub fn new(host: &Host) -> Self {
        Self {
            host_identity: host.host_identity(),
            nodes: BTreeMap::new(),
            services: BTreeMap::new(),
            plan: None,
        }
    }

    pub fn plan(&self) -> Option<&CompositionPlan> {
        self.plan.as_ref()
    }

    pub fn service(&self, service: &CapabilityId) -> Option<&RegisteredService> {
        self.services.get(service)
    }

    pub fn is_installation_active(&self, id: InstallationId) -> bool {
        self.nodes.get(&id).is_some_and(|node| node.active)
    }

    /// Stage one enabled installation and resolve the full `provides`/`requires` graph.
    pub fn register_installation(
        &mut self,
        host: &Host,
        id: InstallationId,
    ) -> Result<CompositionPlan, ApiError> {
        self.register_installations(host, &[id])
    }

    /// Stage a batch of enabled installations atomically and verify acyclicity and
    /// provider compatibility via `capability_composition`.
    pub fn register_installations(
        &mut self,
        host: &Host,
        ids: &[InstallationId],
    ) -> Result<CompositionPlan, ApiError> {
        if self.host_identity != host.host_identity() {
            return Err(stale());
        }
        let mut candidate_nodes = self.nodes.clone();
        for &id in ids {
            let (status, epoch, manifest, _) = host.live_installation(id)?;
            if status != Status::Enabled {
                return Err(error(
                    ErrorCode::Disabled,
                    "cannot register disabled installation in router",
                ));
            }
            candidate_nodes.insert(
                id,
                RouterNode {
                    installation: id,
                    package_id: manifest.package_id.clone(),
                    epoch,
                    provides: manifest.provides.clone(),
                    requires: manifest.requires.clone(),
                    active: true,
                },
            );
        }

        let mut candidate_services = BTreeMap::new();
        for node in candidate_nodes.values().filter(|node| node.active) {
            for endpoint in &node.provides {
                if candidate_services
                    .insert(
                        endpoint.service.clone(),
                        RegisteredService {
                            provider: node.installation,
                            endpoint: endpoint.clone(),
                            active: true,
                        },
                    )
                    .is_some()
                {
                    return Err(error(
                        ErrorCode::AlreadyInstalled,
                        "duplicate provider for service endpoint",
                    ));
                }
            }
        }

        let mut entries = Vec::new();
        for node in candidate_nodes.values().filter(|node| node.active) {
            let entry_id = format!("inst-{}", node.installation.sequence());
            let mut entry =
                Entry::new(entry_id, &node.package_id).map_err(map_composition_error)?;
            for endpoint in &node.provides {
                let service_id =
                    ServiceId::parse(endpoint.service.as_str()).map_err(map_composition_error)?;
                entry = entry.providing(ServiceProvision::new(
                    service_id,
                    ServiceVersion::new(endpoint.major, endpoint.minor),
                ));
            }
            for req in &node.requires {
                if !req.id.is_extension() {
                    continue;
                }
                if let Some(provider) = candidate_services.get(&req.id) {
                    if !provider.endpoint.matches_requirement(req) {
                        return Err(error(
                            ErrorCode::UnsupportedVersion,
                            "provider endpoint version violates requirement bounds",
                        ));
                    }
                } else if req.optional {
                    continue;
                }
                let service_id =
                    ServiceId::parse(req.id.as_str()).map_err(map_composition_error)?;
                entry = entry.requiring(ServiceRequirement::new(
                    service_id,
                    VersionRequirement::new(req.major, req.min_minor),
                ));
            }
            entries.push(entry);
        }

        let layer =
            CompositionLayer::new("active-routes", entries).map_err(map_composition_error)?;
        let plan = CompositionProfile::new("extension-router")
            .map_err(map_composition_error)?
            .with_layer(layer)
            .map_err(map_composition_error)?
            .resolve()
            .map_err(map_composition_error)?;

        self.nodes = candidate_nodes;
        self.services = candidate_services;
        self.plan = Some(plan.clone());
        Ok(plan)
    }

    /// Deactivate routes associated with an installation (on disable or grant revocation).
    pub fn deactivate_installation(&mut self, id: InstallationId) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.active = false;
        }
        for service in self.services.values_mut() {
            if service.provider == id {
                service.active = false;
            }
        }
        self.plan = None;
    }

    /// Remove an uninstalled package and all endpoints it provided from the router.
    pub fn remove_installation(&mut self, id: InstallationId) {
        self.nodes.remove(&id);
        self.services.retain(|_, service| service.provider != id);
        self.plan = None;
    }

    /// Reconcile router state with live Host installation status and grant epochs.
    pub fn sync_with_host(&mut self, host: &Host) -> Result<(), ApiError> {
        if self.host_identity != host.host_identity() {
            return Err(stale());
        }
        let tracked: Vec<(InstallationId, u64)> = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.epoch))
            .collect();
        for (id, recorded_epoch) in tracked {
            match host.live_installation(id) {
                Ok((status, live_epoch, _, _)) => {
                    if status != Status::Enabled || live_epoch != recorded_epoch {
                        self.deactivate_installation(id);
                    }
                }
                Err(_) => self.remove_installation(id),
            }
        }
        Ok(())
    }

    /// Authorize an initial cross-extension service call, computing the strict
    /// `caller ∩ provider` delegated capability intersection (never escalating).
    pub fn authorize_service_call(
        &self,
        host: &Host,
        session: &crate::Session,
        request: ServiceCallRequest,
    ) -> Result<ServiceCallPermit, ApiError> {
        self.authorize_service_call_inner(host, session, request, None)
    }

    /// Authorize a forwarded hop from an in-flight `AuthorizedServiceCall`,
    /// intersecting capabilities across `parent ∩ caller ∩ next_provider` and blocking loops.
    pub fn authorize_forwarded_call(
        &self,
        host: &Host,
        parent: &AuthorizedServiceCall<'_>,
        forwarder_session: &crate::Session,
        request: ServiceCallRequest,
    ) -> Result<ServiceCallPermit, ApiError> {
        let (forwarder_id, _, _, _) = host.live_session(forwarder_session)?;
        if forwarder_id != parent.provider_installation {
            return Err(error(
                ErrorCode::PermissionDenied,
                "forwarded call session does not match parent service provider",
            ));
        }
        if parent.call_chain.len() >= MAX_CALL_CHAIN_DEPTH {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "service forwarding call chain depth exceeded",
            ));
        }
        self.authorize_service_call_inner(host, forwarder_session, request, Some(parent))
    }

    fn authorize_service_call_inner(
        &self,
        host: &Host,
        session: &crate::Session,
        request: ServiceCallRequest,
        parent: Option<&AuthorizedServiceCall<'_>>,
    ) -> Result<ServiceCallPermit, ApiError> {
        if self.host_identity != host.host_identity() {
            return Err(stale());
        }
        host.check_session_owner(session)?;
        request.validate()?;

        let (caller_id, caller_epoch, session_api, caller_table) = host.live_session(session)?;
        if request.api_version != session_api {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "service request API differs from its negotiated session",
            ));
        }
        if request.resource.vault_id() != host.vault_id() {
            return Err(error(
                ErrorCode::PermissionDenied,
                "service request outside host vault scope",
            ));
        }

        let caller_node = self.nodes.get(&caller_id).ok_or_else(|| {
            error(
                ErrorCode::CapabilityUnavailable,
                "caller installation not registered in router",
            )
        })?;
        if !caller_node.active || caller_node.epoch != caller_epoch {
            return Err(stale());
        }

        let reg_service = self.services.get(&request.service).ok_or_else(|| {
            error(
                ErrorCode::CapabilityUnavailable,
                "requested service endpoint is not registered",
            )
        })?;
        if !reg_service.active {
            return Err(stale());
        }
        let provider_id = reg_service.provider;
        if provider_id == caller_id {
            return Err(error(
                ErrorCode::InvalidRequest,
                "installation cannot route a service call to itself",
            ));
        }
        if let Some(parent_ctx) = parent
            && parent_ctx.call_chain.contains(&provider_id)
        {
            return Err(error(
                ErrorCode::InvalidState,
                "service forwarding loop detected in call chain",
            ));
        }

        let provider_node = self.nodes.get(&provider_id).ok_or_else(stale)?;
        if !provider_node.active {
            return Err(stale());
        }
        let (provider_status, provider_epoch, provider_manifest, _) =
            host.live_installation(provider_id).map_err(|_| stale())?;
        if provider_status != Status::Enabled
            || provider_epoch != provider_node.epoch
            || !provider_manifest.provides.contains(&reg_service.endpoint)
        {
            return Err(stale());
        }

        let neg = caller_table.get(&request.service).ok_or_else(|| {
            error(
                ErrorCode::PermissionDenied,
                "caller session has not negotiated the target service",
            )
        })?;
        let negotiated_minor = match neg.negotiated_minor {
            Some(minor) if !neg.degraded => minor,
            _ => {
                return Err(error(
                    ErrorCode::CapabilityUnavailable,
                    "target service capability degraded in caller session",
                ));
            }
        };
        if reg_service.endpoint.major != neg.major || reg_service.endpoint.minor < negotiated_minor
        {
            return Err(stale());
        }

        let caller_granted =
            host.granted_orthogonal_for_resource(caller_id, &request.resource, Some(caller_table));
        if !caller_granted.contains(&request.service) {
            return Err(error(
                ErrorCode::PermissionDenied,
                "caller lacks explicit orthogonal grant for target service",
            ));
        }
        let provider_granted =
            host.granted_orthogonal_for_resource(provider_id, &request.resource, None);

        let mut intersection: BTreeSet<CapabilityId> = caller_granted
            .intersection(&provider_granted)
            .cloned()
            .collect();
        if let Some(parent_ctx) = parent {
            intersection = intersection
                .intersection(&parent_ctx.delegated_capabilities)
                .cloned()
                .collect();
        }

        let delegated_capabilities = if request.requested_capabilities.is_empty() {
            intersection
        } else {
            for cap in &request.requested_capabilities {
                if !intersection.contains(cap) {
                    return Err(error(
                        ErrorCode::PermissionDenied,
                        "requested capability exceeds caller ∩ provider intersection",
                    ));
                }
            }
            request.requested_capabilities.into_iter().collect()
        };

        let mut call_chain = match parent {
            Some(parent_ctx) => parent_ctx.call_chain.to_vec(),
            None => vec![caller_id],
        };
        call_chain.push(provider_id);

        Ok(ServiceCallPermit {
            host_identity: self.host_identity,
            caller_installation: caller_id,
            caller_epoch,
            provider_installation: provider_id,
            provider_epoch,
            service: request.service,
            negotiated_major: neg.major,
            negotiated_minor,
            resource: request.resource,
            delegated_capabilities,
            call_chain,
            request_id: request.request_id,
            payload: request.payload,
        })
    }

    /// Revalidate router routes, caller/provider liveness, grant epochs, and `caller ∩ provider`
    /// delegated capabilities immediately before executing the trusted forwarding callback.
    pub fn dispatch_service_call<T>(
        &self,
        host: &Host,
        permit: ServiceCallPermit,
        action: impl FnOnce(&AuthorizedServiceCall<'_>) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        if self.host_identity != host.host_identity() || permit.host_identity != self.host_identity
        {
            return Err(stale());
        }
        let caller_node = self
            .nodes
            .get(&permit.caller_installation)
            .ok_or_else(stale)?;
        let provider_node = self
            .nodes
            .get(&permit.provider_installation)
            .ok_or_else(stale)?;
        if !caller_node.active
            || !provider_node.active
            || caller_node.epoch != permit.caller_epoch
            || provider_node.epoch != permit.provider_epoch
        {
            return Err(stale());
        }
        let reg_service = self.services.get(&permit.service).ok_or_else(stale)?;
        if !reg_service.active || reg_service.provider != permit.provider_installation {
            return Err(stale());
        }

        let (caller_status, caller_epoch, caller_manifest, _) = host
            .live_installation(permit.caller_installation)
            .map_err(|_| stale())?;
        let (provider_status, provider_epoch, provider_manifest, _) = host
            .live_installation(permit.provider_installation)
            .map_err(|_| stale())?;
        if caller_status != Status::Enabled
            || caller_epoch != permit.caller_epoch
            || provider_status != Status::Enabled
            || provider_epoch != permit.provider_epoch
            || !provider_manifest.provides.contains(&reg_service.endpoint)
        {
            return Err(stale());
        }

        let caller_granted = host.granted_orthogonal_for_resource(
            permit.caller_installation,
            &permit.resource,
            None,
        );
        if !caller_granted.contains(&permit.service) {
            return Err(stale());
        }
        let provider_granted = host.granted_orthogonal_for_resource(
            permit.provider_installation,
            &permit.resource,
            None,
        );
        if !permit
            .delegated_capabilities
            .iter()
            .all(|cap| caller_granted.contains(cap) && provider_granted.contains(cap))
        {
            return Err(stale());
        }

        action(&AuthorizedServiceCall {
            caller_installation: permit.caller_installation,
            caller_package_id: caller_manifest.package_id.clone(),
            provider_installation: permit.provider_installation,
            provider_package_id: provider_manifest.package_id.clone(),
            vault_id: host.vault_id().to_owned(),
            service: permit.service,
            negotiated_major: permit.negotiated_major,
            negotiated_minor: permit.negotiated_minor,
            resource: permit.resource,
            delegated_capabilities: permit.delegated_capabilities,
            call_chain: &permit.call_chain,
            request_id: &permit.request_id,
            payload: &permit.payload,
        })
    }
}

fn map_composition_error(err: CompositionError) -> ApiError {
    match err {
        CompositionError::DependencyCycle { .. } => {
            error(ErrorCode::InvalidState, &err.to_string())
        }
        CompositionError::MissingProvider { .. } => {
            error(ErrorCode::CapabilityUnavailable, &err.to_string())
        }
        CompositionError::IncompatibleProvider { .. } => {
            error(ErrorCode::UnsupportedVersion, &err.to_string())
        }
        CompositionError::DuplicateProvider { .. } => {
            error(ErrorCode::AlreadyInstalled, &err.to_string())
        }
        CompositionError::TooManyEntries { .. } | CompositionError::TooManyServices { .. } => {
            error(ErrorCode::BudgetExceeded, &err.to_string())
        }
        CompositionError::InvalidIdentifier { .. }
        | CompositionError::DuplicateLayer { .. }
        | CompositionError::DuplicateEntryInLayer { .. }
        | CompositionError::DuplicateServiceInEntry { .. }
        | CompositionError::RequiredEntryDisabled { .. } => {
            error(ErrorCode::InvalidManifest, &err.to_string())
        }
    }
}
