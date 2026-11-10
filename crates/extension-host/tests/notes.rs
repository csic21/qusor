use extension_api::{notes::*, *};
use extension_host::{Grant, Host, InstallationId, Session};

fn manifest(name: &str) -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: name.into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 4,
            max_minor: 4,
        },
        required_capabilities: vec![Capability::NotesRead],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: format!("{name}.workspace"),
            title: "Notes".into(),
            kind: ContributionKind::NotesWorkspace,
            required_capabilities: vec![Capability::NotesRead],
        }],
    }
}

fn page_request(cursor: Option<&str>) -> NotesRequest {
    NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_4,
        request_id: "request".into(),
        operation: NotesOperation::Page {
            cursor: cursor.map(str::to_owned),
            limit: 4,
        },
    }
}

fn grants() -> Vec<Grant> {
    vec![Grant {
        capability: Capability::NotesRead,
        resource: Resource::Vault {
            vault_id: "vault".into(),
        },
    }]
}

fn setup() -> (Host, InstallationId, Session) {
    let mut host = Host::new("vault", [Capability::NotesRead]).unwrap();
    let id = host
        .register(manifest("org.test.notes"), "a".repeat(64))
        .unwrap();
    host.replace_grants(id, grants()).unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_4).unwrap();
    (host, id, session)
}

#[test]
fn notes_read_requires_its_grant_and_dispatches_the_bound_vault() {
    let (mut host, _, session) = setup();
    let request = page_request(Some("n1:*:7:3"));
    let permit = host.authorize_notes(&session, &request).unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.vault_id, "vault");
        assert_eq!(context.package_id, "org.test.notes");
        assert_eq!(context.request.capability, Capability::NotesRead);
        Ok(())
    })
    .unwrap();
}

#[test]
fn old_sessions_and_wire_versions_are_rejected() {
    let (mut host, id, session) = setup();
    assert_eq!(
        host.open_session_for_api(id, HOST_API_V0_3)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );

    let mut old_api = page_request(None);
    old_api.api_version = HOST_API_V0_3;
    assert_eq!(
        host.authorize_notes(&session, &old_api).unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
    let mut old_protocol = page_request(None);
    old_protocol.notes_protocol = 2;
    assert_eq!(
        host.authorize_notes(&session, &old_protocol)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
    let mut bad_budget = page_request(None);
    if let NotesOperation::Page { limit, .. } = &mut bad_budget.operation {
        *limit = 0;
    }
    assert_eq!(
        host.authorize_notes(&session, &bad_budget)
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn notes_authority_is_exact_to_notes_read_and_the_vault() {
    let (mut host, id, session) = setup();
    let document_scope = CapabilityRequest {
        api_version: HOST_API_V0_4,
        request_id: "request".into(),
        capability: Capability::NotesRead,
        resource: Resource::Document {
            vault_id: "vault".into(),
            document_id: "document".into(),
        },
    };
    assert_eq!(
        host.authorize(&session, document_scope).unwrap_err().code,
        ErrorCode::PermissionDenied
    );

    let wrong_vault = vec![Grant {
        capability: Capability::NotesRead,
        resource: Resource::Vault {
            vault_id: "other".into(),
        },
    }];
    assert_eq!(
        host.replace_grants(id, wrong_vault).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
}

#[test]
fn revoking_grants_or_disabling_invalidates_notes_permits() {
    let (mut host, id, session) = setup();
    let permit = host.authorize_notes(&session, &page_request(None)).unwrap();
    host.disable(id).unwrap();
    assert_eq!(
        host.dispatch(permit, |_| Ok(())).unwrap_err().code,
        ErrorCode::StaleContext
    );
    assert_eq!(
        host.authorize_notes(&session, &page_request(None))
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );
}

#[test]
fn a_host_without_notes_read_support_rejects_the_package() {
    let mut host = Host::new("vault", []).unwrap();
    assert_eq!(
        host.register(manifest("org.test.notes"), "a".repeat(64))
            .unwrap_err()
            .code,
        ErrorCode::CapabilityUnavailable
    );
}
