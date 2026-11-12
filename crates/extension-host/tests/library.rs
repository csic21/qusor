use extension_api::{library::*, *};
use extension_host::{Grant, Host, InstallationId, Session};

fn manifest() -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.test.library".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 6,
            max_minor: 6,
        },
        required_capabilities: vec![Capability::LibraryRead, Capability::CapturePropose],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.test.library.workspace".into(),
            title: "Library".into(),
            kind: ContributionKind::LibraryWorkspace,
            required_capabilities: vec![Capability::LibraryRead, Capability::CapturePropose],
        }],
    }
}

fn request(cursor: Option<&str>) -> LibraryRequest {
    LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_6,
        request_id: "request".into(),
        operation: LibraryOperation::Page {
            cursor: cursor.map(str::to_owned),
            limit: 4,
        },
    }
}

fn grants() -> Vec<Grant> {
    vec![
        Grant {
            capability: Capability::LibraryRead,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        },
        Grant {
            capability: Capability::CapturePropose,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        },
    ]
}

fn setup() -> (Host, InstallationId, Session) {
    let mut host = Host::new(
        "vault",
        [Capability::LibraryRead, Capability::CapturePropose],
    )
    .unwrap();
    let id = host.register(manifest(), "a".repeat(64)).unwrap();
    host.replace_grants(id, grants()).unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_6).unwrap();
    (host, id, session)
}

#[test]
fn library_read_requires_its_grant_and_dispatches_the_bound_vault() {
    let (mut host, _, session) = setup();
    let permit = host
        .authorize_library(&session, &request(Some("l1:7:3")))
        .unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.vault_id, "vault");
        assert_eq!(context.package_id, "org.test.library");
        assert_eq!(context.request.capability, Capability::LibraryRead);
        Ok(())
    })
    .unwrap();
}

#[test]
fn library_page_text_uses_the_same_exact_grant() {
    let (mut host, _, session) = setup();
    let request = LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_6,
        request_id: "text-request".into(),
        operation: LibraryOperation::PageText {
            document_id: 1,
            page_index: 0,
            offset: 0,
            max_bytes: 4,
        },
    };
    let permit = host.authorize_library(&session, &request).unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.request.capability, Capability::LibraryRead);
        Ok(())
    })
    .unwrap();
}

#[test]
fn library_capture_requires_capture_propose() {
    let (mut host, _, session) = setup();
    let request = LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_6,
        request_id: "capture-request".into(),
        operation: LibraryOperation::CaptureSelection {
            document_id: 1,
            page_index: 0,
            start: 0,
            end: 5,
            note: String::new(),
        },
    };
    let permit = host.authorize_library(&session, &request).unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.request.capability, Capability::CapturePropose);
        Ok(())
    })
    .unwrap();
}

#[test]
fn old_sessions_and_wire_versions_are_rejected() {
    let (mut host, id, session) = setup();
    assert_eq!(
        host.open_session_for_api(id, HOST_API_V0_5)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
    let mut old_api = request(None);
    old_api.api_version = HOST_API_V0_5;
    assert_eq!(
        host.authorize_library(&session, &old_api).unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
    let mut old_protocol = request(None);
    old_protocol.library_protocol = 2;
    assert_eq!(
        host.authorize_library(&session, &old_protocol)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
    let mut bad_budget = request(None);
    if let LibraryOperation::Page { limit, .. } = &mut bad_budget.operation {
        *limit = 0;
    } else {
        panic!("expected page request");
    }
    assert_eq!(
        host.authorize_library(&session, &bad_budget)
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn library_authority_is_exact_to_library_read_and_the_vault() {
    let (mut host, id, session) = setup();
    let document_scope = CapabilityRequest {
        api_version: HOST_API_V0_6,
        request_id: "request".into(),
        capability: Capability::LibraryRead,
        resource: Resource::Document {
            vault_id: "vault".into(),
            document_id: "document".into(),
        },
    };
    assert_eq!(
        host.authorize(&session, document_scope).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    assert_eq!(
        host.replace_grants(
            id,
            vec![Grant {
                capability: Capability::LibraryRead,
                resource: Resource::Vault {
                    vault_id: "other".into(),
                },
            }],
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );
}

#[test]
fn revoking_or_disabling_invalidates_library_permits() {
    let (mut host, id, session) = setup();
    let permit = host.authorize_library(&session, &request(None)).unwrap();
    host.disable(id).unwrap();
    assert_eq!(
        host.dispatch(permit, |_| Ok(())).unwrap_err().code,
        ErrorCode::StaleContext
    );
    assert_eq!(
        host.authorize_library(&session, &request(None))
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );
}

#[test]
fn a_host_without_library_read_support_rejects_the_package() {
    let mut host = Host::new("vault", []).unwrap();
    assert_eq!(
        host.register(manifest(), "a".repeat(64)).unwrap_err().code,
        ErrorCode::CapabilityUnavailable
    );
}
