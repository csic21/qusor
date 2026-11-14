//! Authority-free durable extension installation, grant and recovery port contracts.
//!
//! This crate sits below both `extension-host` (policy authority) and
//! `extension-store` (SQLite persistence adapter) so storage adapters never
//! depend on the host policy crate.

use extension_api::{
    ApiError, ApiVersion, Capability, CapabilityRequest, ErrorCode, Manifest, Resource,
    capability_resource_valid, composition::CompositionDescriptor, is_supported_api,
};

/// A host-approved capability and exact resource scope.
/// Only a vault-wide metadata grant covers individual documents. Capture proposals
/// target a vault directly; selection reads always require an exact selection grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub capability: Capability,
    pub resource: Resource,
}

/// Persisted grant bound to a host-side expiration timestamp and optional exact source version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableGrant {
    pub grant: Grant,
    pub expires_at: u64,
    /// Preserved durable source/version constraint; None means explicitly unversioned.
    pub resource_version: Option<String>,
}

/// Host-resolved current version, not a version self-reported by the guest.
/// Resolve it under the same gate/snapshot as the handler's actual data access.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Freshness mode for reading durable installation authority facts.
#[derive(Debug, Clone, Copy)]
pub enum AuthorityRead<'a> {
    /// May perform bounded private-state preflight, before a VM invocation starts.
    Activation,
    /// No guest execution or provider I/O in the per-message authorization gate.
    Current { run_id: Option<&'a str> },
}

/// Trusted journal tombstone, never a guest-supplied cleanup approval.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Implemented by trusted durable infrastructure for component mount authority checks.
pub trait ComponentAuthoritySource {
    fn component_installation(
        &self,
        package_id: &str,
        read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError>;
}

/// Pure policy decision for existing durable grants during an in-place package upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantUpgradeDecision {
    /// Same-privilege upgrade: existing grants remain valid (narrowed to capabilities
    /// still declared by the upgraded manifest) and may be preserved across the upgrade.
    Preserve(Vec<Grant>),
    /// Privilege expansion or incompatible manifest/API: existing grants must be cleared
    /// and the user must re-authorize before activation.
    RequireReauthorization,
}

/// Evaluate whether `existing_grants` can be preserved across a verified package upgrade.
///
/// Rejects package/publisher identity changes and non-advancing version/digest upgrades.
/// Preserves grants only when `existing_grants` is non-empty, `next` supports `api`,
/// at least one still-declared grant remains valid under `api`, and every capability in
/// `next.required_capabilities` is already covered by the preserved grants.
pub fn preserve_grants(
    previous: &Manifest,
    previous_digest: &str,
    next: &Manifest,
    next_digest: &str,
    existing_grants: &[Grant],
    api: ApiVersion,
) -> Result<GrantUpgradeDecision, ApiError> {
    previous.validate()?;
    next.validate()?;
    if previous.package_id != next.package_id || previous.publisher_id != next.publisher_id {
        return Err(ApiError::new(
            ErrorCode::PermissionDenied,
            "package or publisher identity changed",
        ));
    }
    if next.package_version <= previous.package_version || next_digest == previous_digest {
        return Err(ApiError::new(
            ErrorCode::InvalidState,
            "upgrade requires a newer version and different digest",
        ));
    }
    if existing_grants.is_empty() || !is_supported_api(api) || !next.supports_api(api) {
        return Ok(GrantUpgradeDecision::RequireReauthorization);
    }

    let kept: Vec<Grant> = existing_grants
        .iter()
        .filter(|grant| {
            let still_declared = next.required_capabilities.contains(&grant.capability)
                || next.optional_capabilities.contains(&grant.capability);
            still_declared
                && grant.resource.validate().is_ok()
                && capability_resource_valid(grant.capability, &grant.resource, api)
        })
        .cloned()
        .collect();

    if kept.is_empty() {
        return Ok(GrantUpgradeDecision::RequireReauthorization);
    }

    let all_required_covered = next
        .required_capabilities
        .iter()
        .all(|cap| kept.iter().any(|grant| grant.capability == *cap));
    if !all_required_covered {
        return Ok(GrantUpgradeDecision::RequireReauthorization);
    }

    Ok(GrantUpgradeDecision::Preserve(kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use extension_api::{ApiRange, HOST_API, HOST_API_V0_2, PackageVersion};

    fn manifest(
        version: u32,
        min_minor: u16,
        max_minor: u16,
        required: Vec<Capability>,
        optional: Vec<Capability>,
    ) -> Manifest {
        Manifest {
            manifest_version: 1,
            package_id: "org.example.reader".into(),
            publisher_id: "org.example".into(),
            package_version: PackageVersion {
                major: version,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor,
                max_minor,
            },
            required_capabilities: required,
            optional_capabilities: optional,
            contributions: vec![],
            orthogonal_capabilities: vec![],
            provides: vec![],
            requires: vec![],
        }
    }

    fn doc_grant(capability: Capability) -> Grant {
        Grant {
            capability,
            resource: Resource::Document {
                vault_id: "vault".into(),
                document_id: "doc-1".into(),
            },
        }
    }

    #[test]
    fn same_privilege_upgrade_preserves_grants_and_narrows_dropped_optional() {
        let v1 = manifest(
            1,
            2,
            2,
            vec![Capability::ReaderSurface],
            vec![Capability::SelectionRead],
        );
        let v2 = manifest(2, 2, 2, vec![Capability::ReaderSurface], vec![]);
        let existing = vec![
            doc_grant(Capability::ReaderSurface),
            doc_grant(Capability::SelectionRead),
        ];

        let decision = preserve_grants(
            &v1,
            &"a".repeat(64),
            &v2,
            &"b".repeat(64),
            &existing,
            HOST_API_V0_2,
        )
        .unwrap();

        assert_eq!(
            decision,
            GrantUpgradeDecision::Preserve(vec![doc_grant(Capability::ReaderSurface)])
        );
    }

    #[test]
    fn privilege_expansion_requires_reauthorization() {
        let v1 = manifest(1, 2, 2, vec![Capability::DocumentMetadataRead], vec![]);
        let v2 = manifest(
            2,
            2,
            2,
            vec![Capability::DocumentMetadataRead, Capability::SelectionRead],
            vec![],
        );
        let existing = vec![doc_grant(Capability::DocumentMetadataRead)];

        let decision = preserve_grants(
            &v1,
            &"a".repeat(64),
            &v2,
            &"b".repeat(64),
            &existing,
            HOST_API_V0_2,
        )
        .unwrap();

        assert_eq!(decision, GrantUpgradeDecision::RequireReauthorization);
    }

    #[test]
    fn api_incompatibility_requires_reauthorization() {
        let v1 = manifest(1, 1, 2, vec![Capability::DocumentMetadataRead], vec![]);
        let v2 = manifest(2, 2, 2, vec![Capability::DocumentMetadataRead], vec![]);
        let existing = vec![doc_grant(Capability::DocumentMetadataRead)];

        let decision = preserve_grants(
            &v1,
            &"a".repeat(64),
            &v2,
            &"b".repeat(64),
            &existing,
            HOST_API,
        )
        .unwrap();

        assert_eq!(decision, GrantUpgradeDecision::RequireReauthorization);
    }

    #[test]
    fn identity_or_version_regression_fails_closed() {
        let v1 = manifest(2, 1, 2, vec![Capability::DocumentMetadataRead], vec![]);
        let mut wrong_pub = manifest(3, 1, 2, vec![Capability::DocumentMetadataRead], vec![]);
        wrong_pub.publisher_id = "org.attacker".into();

        assert_eq!(
            preserve_grants(
                &v1,
                &"a".repeat(64),
                &wrong_pub,
                &"b".repeat(64),
                &[doc_grant(Capability::DocumentMetadataRead)],
                HOST_API,
            )
            .unwrap_err()
            .code,
            ErrorCode::PermissionDenied
        );

        let same_ver = manifest(2, 1, 2, vec![Capability::DocumentMetadataRead], vec![]);
        assert_eq!(
            preserve_grants(
                &v1,
                &"a".repeat(64),
                &same_ver,
                &"b".repeat(64),
                &[doc_grant(Capability::DocumentMetadataRead)],
                HOST_API,
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidState
        );
    }
}
