use extension_api::{vocab::*, *};
use extension_host::{Grant, Host, InstallationId, Session};

fn manifest(required: &[Capability], optional: &[Capability]) -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.test.vocab".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 5,
            max_minor: 5,
        },
        required_capabilities: required.to_vec(),
        optional_capabilities: optional.to_vec(),
        contributions: vec![Contribution {
            id: "org.test.vocab.workspace".into(),
            title: "Vocabulary".into(),
            kind: ContributionKind::VocabWorkspace,
            required_capabilities: vec![Capability::VocabRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}

fn request(operation: VocabOperation) -> VocabRequest {
    VocabRequest {
        vocab_protocol: VOCAB_PROTOCOL,
        api_version: HOST_API_V0_5,
        request_id: "request".into(),
        operation,
    }
}

fn grants(capabilities: &[Capability]) -> Vec<Grant> {
    capabilities
        .iter()
        .map(|capability| Grant {
            capability: *capability,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        })
        .collect()
}

fn setup(granted: &[Capability]) -> (Host, InstallationId, Session) {
    let mut host = Host::new("vault", [Capability::VocabRead, Capability::VocabReview]).unwrap();
    let id = host
        .register(
            manifest(&[Capability::VocabRead], &[Capability::VocabReview]),
            "a".repeat(64),
        )
        .unwrap();
    host.replace_grants(id, grants(granted)).unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_5).unwrap();
    (host, id, session)
}

#[test]
fn reads_and_reviews_require_their_exact_capability_and_bind_the_vault() {
    let (mut host, _, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);

    for operation in [
        VocabOperation::Decks {},
        VocabOperation::Queue {
            wordbook_id: 1,
            limit: 3,
        },
    ] {
        let permit = host.authorize_vocab(&session, &request(operation)).unwrap();
        host.dispatch(permit, |context| {
            assert_eq!(context.vault_id, "vault");
            assert_eq!(context.package_id, "org.test.vocab");
            assert_eq!(context.request.capability, Capability::VocabRead);
            Ok(())
        })
        .unwrap();
    }

    let permit = host
        .authorize_vocab(
            &session,
            &request(VocabOperation::Review {
                card_id: 1,
                rating: VocabRating::Good,
            }),
        )
        .unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.request.capability, Capability::VocabReview);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_read_only_package_cannot_schedule_reviews() {
    let (host, _, session) = setup(&[Capability::VocabRead]);
    assert!(
        host.authorize_vocab(&session, &request(VocabOperation::Decks {}))
            .is_ok()
    );
    assert_eq!(
        host.authorize_vocab(
            &session,
            &request(VocabOperation::Review {
                card_id: 1,
                rating: VocabRating::Again,
            }),
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );
}

#[test]
fn old_sessions_and_wire_versions_are_rejected() {
    let (mut host, id, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let _ = session;
    assert_eq!(
        host.open_session_for_api(id, HOST_API_V0_4)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );

    let (host, _, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let mut old_api = request(VocabOperation::Decks {});
    old_api.api_version = HOST_API_V0_4;
    assert_eq!(
        host.authorize_vocab(&session, &old_api).unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    let mut old_protocol = request(VocabOperation::Decks {});
    old_protocol.vocab_protocol = 2;
    assert_eq!(
        host.authorize_vocab(&session, &old_protocol)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );

    let mut bad_budget = request(VocabOperation::Queue {
        wordbook_id: 1,
        limit: 0,
    });
    if let VocabOperation::Queue { limit, .. } = &mut bad_budget.operation {
        *limit = 0;
    }
    assert_eq!(
        host.authorize_vocab(&session, &bad_budget)
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn vocab_authority_is_exact_to_the_vault_scope() {
    let (host, _, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let document_scope = CapabilityRequest {
        api_version: HOST_API_V0_5,
        request_id: "request".into(),
        capability: Capability::VocabRead,
        resource: Resource::Document {
            vault_id: "vault".into(),
            document_id: "document".into(),
        },
    };
    assert_eq!(
        host.authorize(&session, document_scope).unwrap_err().code,
        ErrorCode::PermissionDenied
    );

    let (mut host, id, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let other_vault = vec![Grant {
        capability: Capability::VocabRead,
        resource: Resource::Vault {
            vault_id: "other".into(),
        },
    }];
    assert_eq!(
        host.replace_grants(id, other_vault).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    assert!(
        host.authorize_vocab(&session, &request(VocabOperation::Decks {}))
            .is_ok()
    );
}

#[test]
fn revoking_grants_disabling_or_closing_invalidates_permits() {
    let (mut host, id, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let permit = host
        .authorize_vocab(&session, &request(VocabOperation::Decks {}))
        .unwrap();
    host.disable(id).unwrap();
    assert_eq!(
        host.dispatch(permit, |_| Ok(())).unwrap_err().code,
        ErrorCode::StaleContext
    );
    assert_eq!(
        host.authorize_vocab(&session, &request(VocabOperation::Decks {}))
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );

    let (mut host, _, session) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    let permit = host
        .authorize_vocab(&session, &request(VocabOperation::Decks {}))
        .unwrap();
    host.close_session(&session).unwrap();
    assert_eq!(
        host.dispatch(permit, |_| Ok(())).unwrap_err().code,
        ErrorCode::StaleContext
    );
}

#[test]
fn a_host_without_vocab_support_rejects_the_package() {
    let mut host = Host::new("vault", []).unwrap();
    assert_eq!(
        host.register(manifest(&[Capability::VocabRead], &[]), "a".repeat(64))
            .unwrap_err()
            .code,
        ErrorCode::CapabilityUnavailable
    );
}

#[test]
fn the_vocab_workspace_contribution_is_visible_with_its_grant() {
    let (host, _, _) = setup(&[Capability::VocabRead]);
    let contributions = host.contributions();
    assert_eq!(contributions.len(), 1);
    assert_eq!(contributions[0].1.kind, ContributionKind::VocabWorkspace);
    assert_eq!(contributions[0].1.title, "Vocabulary");

    let (host, _, _) = setup(&[Capability::VocabRead, Capability::VocabReview]);
    assert_eq!(host.contributions().len(), 1);
}
