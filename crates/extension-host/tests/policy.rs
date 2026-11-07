use extension_api::*;
use extension_host::{Grant, Host, InstallationId, Session, Status};

fn manifest() -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.example.capture".into(),
        publisher_id: "org.example".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        required_capabilities: vec![Capability::SelectionRead],
        optional_capabilities: vec![Capability::DocumentMetadataRead, Capability::CapturePropose],
        contributions: vec![
            Contribution {
                id: "org.example.capture.inspect".into(),
                title: "Inspect selection".into(),
                kind: ContributionKind::SelectionAction,
                required_capabilities: vec![Capability::SelectionRead],
            },
            Contribution {
                id: "org.example.capture.propose".into(),
                title: "Propose capture".into(),
                kind: ContributionKind::Command,
                required_capabilities: vec![Capability::CapturePropose],
            },
        ],
    }
}
fn selection(vault: &str, id: &str) -> Resource {
    Resource::Selection {
        vault_id: vault.into(),
        selection_id: id.into(),
    }
}
fn request(resource: Resource, capability: Capability) -> CapabilityRequest {
    CapabilityRequest {
        api_version: HOST_API,
        request_id: "request-1".into(),
        capability,
        resource,
    }
}
fn selection_request() -> CapabilityRequest {
    request(selection("vault-a", "selected"), Capability::SelectionRead)
}
fn selection_grant() -> Grant {
    Grant {
        capability: Capability::SelectionRead,
        resource: selection("vault-a", "selected"),
    }
}
fn host() -> Host {
    Host::new(
        "vault-a",
        [
            Capability::SelectionRead,
            Capability::DocumentMetadataRead,
            Capability::CapturePropose,
        ],
    )
    .unwrap()
}
fn ready() -> (Host, InstallationId, Session) {
    let mut host = host();
    let id = host.register(manifest(), "a".repeat(64)).unwrap();
    host.replace_grants(id, vec![selection_grant()]).unwrap();
    host.enable(id).unwrap();
    let session = host.open_session(id).unwrap();
    (host, id, session)
}
fn assert_error<T: std::fmt::Debug>(result: Result<T, ApiError>, expected: ErrorCode) {
    assert_eq!(result.unwrap_err().code, expected);
}

#[test]
fn registration_is_not_authorization_or_activation() {
    let mut host = host();
    let id = host.register(manifest(), "a".repeat(64)).unwrap();
    assert_eq!(host.status(id).unwrap(), Status::Disabled);
    assert!(host.grants(id).unwrap().is_empty());
    assert_error(host.open_session(id), ErrorCode::Disabled);
    assert_error(host.enable(id), ErrorCode::PermissionDenied);
    assert!(host.contributions().is_empty());
    assert_error(
        host.register(manifest(), "a".repeat(64)),
        ErrorCode::AlreadyInstalled,
    );
}

#[test]
fn dispatch_binds_identity_and_only_exposes_authorized_contributions() {
    let (mut host, id, session) = ready();
    assert_eq!(host.contributions().len(), 1);
    let permit = host.authorize(&session, selection_request()).unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.installation_id, id);
        assert_eq!(context.package_id, "org.example.capture");
        assert_eq!(context.package_digest, "a".repeat(64));
        assert_eq!(context.vault_id, "vault-a");
        Ok(())
    })
    .unwrap();
}

#[test]
fn exact_scope_cannot_read_other_selection_vault_or_optional_capability() {
    let (host, _, session) = ready();
    for req in [
        request(selection("vault-b", "selected"), Capability::SelectionRead),
        request(selection("vault-a", "other"), Capability::SelectionRead),
        request(
            Resource::Vault {
                vault_id: "vault-a".into(),
            },
            Capability::SelectionRead,
        ),
        request(
            Resource::Vault {
                vault_id: "vault-a".into(),
            },
            Capability::CapturePropose,
        ),
    ] {
        assert_error(host.authorize(&session, req), ErrorCode::PermissionDenied);
    }
}

#[test]
fn metadata_vault_scope_only_covers_metadata_in_its_vault() {
    let (mut host, id, _) = ready();
    host.replace_grants(
        id,
        vec![
            selection_grant(),
            Grant {
                capability: Capability::DocumentMetadataRead,
                resource: Resource::Vault {
                    vault_id: "vault-a".into(),
                },
            },
        ],
    )
    .unwrap();
    let session = host.open_session(id).unwrap();
    host.authorize(
        &session,
        request(
            Resource::Document {
                vault_id: "vault-a".into(),
                document_id: "document-2".into(),
            },
            Capability::DocumentMetadataRead,
        ),
    )
    .unwrap();
    assert_error(
        host.authorize(
            &session,
            request(selection("vault-a", "other"), Capability::SelectionRead),
        ),
        ErrorCode::PermissionDenied,
    );
    assert_error(
        host.authorize(
            &session,
            request(
                Resource::Document {
                    vault_id: "vault-b".into(),
                    document_id: "document-2".into(),
                },
                Capability::DocumentMetadataRead,
            ),
        ),
        ErrorCode::PermissionDenied,
    );
    host.replace_grants(
        id,
        vec![
            selection_grant(),
            Grant {
                capability: Capability::DocumentMetadataRead,
                resource: Resource::Document {
                    vault_id: "vault-a".into(),
                    document_id: "document-2".into(),
                },
            },
        ],
    )
    .unwrap();
    let session = host.open_session(id).unwrap();
    assert_error(
        host.authorize(
            &session,
            request(
                Resource::Vault {
                    vault_id: "vault-a".into(),
                },
                Capability::DocumentMetadataRead,
            ),
        ),
        ErrorCode::PermissionDenied,
    );
}

#[test]
fn every_revocation_path_prevents_an_in_flight_callback() {
    for operation in 0..6 {
        let (mut host, id, session) = ready();
        let permit = host.authorize(&session, selection_request()).unwrap();
        match operation {
            0 => host.replace_grants(id, vec![]).unwrap(),
            1 => host.replace_grants(id, vec![selection_grant()]).unwrap(),
            2 => {
                host.disable(id).unwrap();
                host.enable(id).unwrap();
            }
            3 => host.close_session(&session).unwrap(),
            4 => {
                let mut updated = manifest();
                updated.package_version.patch = 1;
                host.update(id, updated, "b".repeat(64)).unwrap();
            }
            _ => {
                host.unregister(id).unwrap();
                let replacement = host.register(manifest(), "a".repeat(64)).unwrap();
                assert_ne!(replacement, id);
                host.replace_grants(replacement, vec![selection_grant()])
                    .unwrap();
                host.enable(replacement).unwrap();
            }
        }
        let mut called = false;
        assert_error(
            host.dispatch(permit, |_| {
                called = true;
                Ok(())
            }),
            ErrorCode::StaleContext,
        );
        assert!(!called, "revocation path {operation} ran callback");
        assert_error(
            host.authorize(&session, selection_request()),
            ErrorCode::StaleContext,
        );
    }
}

#[test]
fn handles_and_management_ids_cannot_cross_hosts_even_in_same_vault() {
    let (first, first_id, first_session) = ready();
    let (mut second, second_id, second_session) = ready();
    assert_ne!(first_id, second_id);
    assert_error(second.disable(first_id), ErrorCode::NotFound);
    assert_error(
        second.close_session(&first_session),
        ErrorCode::StaleContext,
    );
    assert_error(
        second.authorize(&first_session, selection_request()),
        ErrorCode::StaleContext,
    );
    let permit = first
        .authorize(&first_session, selection_request())
        .unwrap();
    assert_error(
        second.dispatch::<()>(permit, |_| panic!("cross-host callback")),
        ErrorCode::StaleContext,
    );
    second
        .authorize(&second_session, selection_request())
        .unwrap();
}

#[test]
fn invalid_grants_leave_live_authorization_unchanged() {
    let (mut host, id, session) = ready();
    let bad = Grant {
        capability: Capability::SelectionRead,
        resource: selection("vault-b", "selected"),
    };
    assert_error(
        host.replace_grants(id, vec![bad]),
        ErrorCode::PermissionDenied,
    );
    assert_error(
        host.replace_grants(id, vec![selection_grant(), selection_grant()]),
        ErrorCode::InvalidRequest,
    );
    assert_eq!(host.status(id).unwrap(), Status::Enabled);
    host.authorize(&session, selection_request()).unwrap();
}

#[test]
fn incompatible_and_identity_changing_updates_leave_old_session_alive() {
    let (mut host, id, session) = ready();
    for variant in 0..5 {
        let mut updated = manifest();
        updated.package_version.patch = 1;
        match variant {
            0 => updated.host_api.major = 2,
            1 => updated.publisher_id = "org.attacker".into(),
            2 => {
                updated.package_id = "org.other".into();
                updated.contributions.clear();
            }
            3 => updated.package_version.patch = 0,
            _ => {}
        }
        let digest = if variant == 4 {
            "not-a-digest".into()
        } else {
            "b".repeat(64)
        };
        assert!(host.update(id, updated, digest).is_err());
        host.authorize(&session, selection_request()).unwrap();
        assert_eq!(host.manifest(id).unwrap().package_version.patch, 0);
    }
}

#[test]
fn successful_update_never_inherits_old_or_expanded_permissions() {
    let (mut host, id, _) = ready();
    let mut updated = manifest();
    updated.package_version.patch = 1;
    updated
        .optional_capabilities
        .retain(|cap| *cap != Capability::CapturePropose);
    updated
        .required_capabilities
        .push(Capability::CapturePropose);
    host.update(id, updated, "b".repeat(64)).unwrap();
    assert!(host.grants(id).unwrap().is_empty());
    assert_eq!(host.status(id).unwrap(), Status::Disabled);
    assert!(host.contributions().is_empty());
    assert_error(host.enable(id), ErrorCode::PermissionDenied);
    host.replace_grants(id, vec![selection_grant()]).unwrap();
    assert_error(host.enable(id), ErrorCode::PermissionDenied);
    host.replace_grants(
        id,
        vec![
            selection_grant(),
            Grant {
                capability: Capability::CapturePropose,
                resource: Resource::Vault {
                    vault_id: "vault-a".into(),
                },
            },
        ],
    )
    .unwrap();
    assert_eq!(host.status(id).unwrap(), Status::Disabled);
    host.enable(id).unwrap();
    assert_eq!(host.contributions().len(), 2);
}

#[test]
fn unavailable_required_capability_rejects_but_optional_can_degrade() {
    let mut host = Host::new("vault-a", []).unwrap();
    assert_error(
        host.register(manifest(), "a".repeat(64)),
        ErrorCode::CapabilityUnavailable,
    );
    let mut optional = manifest();
    optional.required_capabilities.clear();
    optional
        .optional_capabilities
        .push(Capability::SelectionRead);
    let id = host.register(optional, "a".repeat(64)).unwrap();
    host.enable(id).unwrap();
    assert!(host.contributions().is_empty());
    assert_error(
        host.replace_grants(id, vec![selection_grant()]),
        ErrorCode::PermissionDenied,
    );
    let session = host.open_session(id).unwrap();
    assert_error(
        host.authorize(&session, selection_request()),
        ErrorCode::PermissionDenied,
    );
}

#[test]
fn request_version_is_checked_on_typed_entrypoint_too() {
    let (host, _, session) = ready();
    let mut req = selection_request();
    req.api_version.major = 42;
    assert_error(host.authorize(&session, req), ErrorCode::UnsupportedVersion);
}

#[test]
fn independent_packages_follow_the_same_policy() {
    let mut host = host();
    for package in ["org.qusor.selection", "org.external.selection"] {
        let mut declared = manifest();
        declared.package_id = package.into();
        declared.contributions.clear();
        let id = host.register(declared, "a".repeat(64)).unwrap();
        assert_error(host.enable(id), ErrorCode::PermissionDenied);
        host.replace_grants(id, vec![selection_grant()]).unwrap();
        host.enable(id).unwrap();
        let session = host.open_session(id).unwrap();
        let permit = host.authorize(&session, selection_request()).unwrap();
        host.dispatch(permit, |context| {
            assert_eq!(context.package_id, package);
            Ok(())
        })
        .unwrap();
    }
}
