use extension_api::{composition::*, continuation::*, *};
use extension_host::{Host, component::*, durable::*};
use std::cell::{Cell, RefCell};

fn facts() -> ComponentInstallation {
    ComponentInstallation {
        installation_id: 1,
        revision: 1,
        generation: 1,
        digest: "a".repeat(64),
        vault_id: "vault".into(),
        manifest: Manifest {
            manifest_version: 1,
            package_id: "org.test.input".into(),
            publisher_id: "org.test".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 3,
                max_minor: 3,
            },
            required_capabilities: vec![Capability::NoteDraftEdit],
            optional_capabilities: vec![],
            contributions: vec![Contribution {
                id: "org.test.input.editor".into(),
                title: "Editor".into(),
                kind: ContributionKind::ComponentProvider,
                required_capabilities: vec![Capability::NoteDraftEdit],
            }],
            orthogonal_capabilities: vec![],
            provides: vec![],
            requires: vec![],
        },
        composition: CompositionDescriptor {
            composition_protocol: COMPOSITION_PROTOCOL,
            api_version: HOST_API_V0_3,
            contribution_id: "org.test.input.editor".into(),
            role: ComponentRole::AnnotationPlainTextEditor,
            contract: ANNOTATION_EDITOR_CONTRACT.into(),
            view: NativeEditorView::TextEditor {
                label: "Thought".into(),
                placeholder: "Write".into(),
            },
        },
        composition_sha256: "b".repeat(64),
    }
}
fn resource(n: u8) -> Resource {
    Resource::Draft {
        vault_id: "vault".into(),
        draft_id: format!("{n:032x}"),
    }
}
fn context() -> ComponentContextRequest {
    ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: COMPONENT_CONTEXT_PROTOCOL,
        request_id: "context".into(),
        operation: ContextOperation::ReadContext {},
    }
}
fn continuation(m: &MountHandle) -> ContinuationRequest {
    ContinuationRequest {
        api_version: HOST_API_V0_3,
        continuation_protocol: CONTINUATION_PROTOCOL,
        request_id: "continue".into(),
        resource: m.resource().clone(),
        mount_id: m.mount_id().into(),
        sequence: 1,
        base_revision: 1,
        operation: ContinuationOperation::ReadMetadata {},
    }
}
fn pure() -> (Host, MountHandle) {
    let f = facts();
    let mut h = Host::new("vault", [Capability::NoteDraftEdit]).unwrap();
    h.configure_component_identity([23; 16]).unwrap();
    let id = h.register(f.manifest, &f.digest).unwrap();
    let m = h
        .open_component_mount(
            id,
            &f.composition.contribution_id,
            resource(1),
            &f.composition,
        )
        .unwrap();
    (h, m)
}
#[test]
fn context_is_own_mount_and_claimed_identity_or_approval_is_not_wire() {
    let (mut h, m) = pure();
    let identity = h
        .dispatch_component_context(&m, context(), |c| Ok(c.clone()))
        .unwrap();
    assert_eq!(identity.installation_id, m.installation_id());
    assert_eq!(identity.resource, resource(1));
    assert_eq!(identity.mount_id, m.mount_id());
    assert_eq!(identity.grant_epoch, m.epoch());
    assert_eq!(identity.package_digest, "a".repeat(64));
    let (foreign, _) = pure();
    assert!(
        foreign
            .dispatch_component_context(&m, context(), |_| -> Result<(), ApiError> {
                panic!("foreign context")
            })
            .is_err()
    );
    let mut old = context();
    old.api_version = HOST_API_V0_2;
    assert!(h.dispatch_component_context(&m, old, |_| Ok(())).is_err());
    // Context has no guest resource, mount, run identity or approval field.
    for field in [
        "\"approved\":true",
        "\"mount_id\":\"00000000000000000000000000000001\"",
        "\"run_id\":\"another\"",
        "\"resource\":{\"kind\":\"vault\",\"vault_id\":\"vault\"}",
    ] {
        let valid = context().to_json().unwrap();
        let raw = format!("{{{field},{}", &valid[1..]);
        assert!(ComponentContextRequest::from_slice(raw.as_bytes()).is_err());
    }
    let raw = context().to_json().unwrap().replace(
        "\"kind\":\"read_context\"",
        "\"kind\":\"read_context\",\"approved\":true",
    );
    assert!(ComponentContextRequest::from_slice(raw.as_bytes()).is_err());
    // Ordinary grant/session establishment cannot manufacture a mount capability.
    assert!(
        h.open_session_for_api(m.installation_id(), HOST_API_V0_3)
            .is_err()
    );
}
#[test]
fn continuation_checks_exact_mount_and_resource_but_not_app_event_revision() {
    let (mut h, m) = pure();
    let f = facts();
    let b = h
        .open_component_mount(
            m.installation_id(),
            &f.composition.contribution_id,
            resource(2),
            &f.composition,
        )
        .unwrap();
    assert!(
        h.dispatch_continuation(&m, continuation(&b), |_| Ok(()))
            .is_err()
    );
    let mut wrong = continuation(&m);
    wrong.resource = resource(2);
    assert!(h.dispatch_continuation(&m, wrong, |_| Ok(())).is_err());
    let mut wrong = continuation(&m);
    wrong.api_version = HOST_API_V0_2;
    assert!(h.dispatch_continuation(&m, wrong, |_| Ok(())).is_err());
    let mut current = continuation(&m);
    current.base_revision = 123;
    current.sequence = 456;
    // The app checks Completed/base-revision semantics. Host only proves live identity.
    assert_eq!(
        h.dispatch_continuation(&m, current, |c| Ok(c.resource.clone()))
            .unwrap(),
        resource(1)
    );
    let identity = h
        .dispatch_component_context(&m, context(), |c| Ok(c.clone()))
        .unwrap();
    h.close_component_mount(&m).unwrap();
    assert_eq!(identity.resource, resource(1)); // Cloning context does not keep access alive.
    assert!(
        h.dispatch_continuation(&m, continuation(&m), |_| Ok(()))
            .is_err()
    );
    assert!(
        h.dispatch_component_context(&m, context(), |_| Ok(()))
            .is_err()
    );
    h.validate_component_mount(&b).unwrap();
    h.disable(b.installation_id()).unwrap();
    assert!(
        h.dispatch_component_context(&b, context(), |_| Ok(()))
            .is_err()
    );
}
struct Source {
    facts: RefCell<ComponentInstallation>,
    reads: Cell<usize>,
}
impl ComponentAuthoritySource for Source {
    fn component_installation(
        &self,
        _: &str,
        read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError> {
        self.reads.set(self.reads.get() + 1);
        if let AuthorityRead::Current { run_id } = read
            && run_id != Some("own-run")
        {
            return Err(ApiError::new(ErrorCode::PermissionDenied, "not own worker"));
        }
        Ok(self.facts.borrow().clone())
    }
}
#[test]
fn durable_context_and_continuation_each_revalidate_current_worker_and_installation() {
    let s = Source {
        facts: RefCell::new(facts()),
        reads: Cell::new(0),
    };
    let mut h = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
    h.configure_component_identity([29; 16]).unwrap();
    let intent = UserMountIntent::new(s.facts.borrow().clone(), resource(1)).unwrap();
    let m = h.mount_component(&s, intent, 1).unwrap();
    h.dispatch_component_context(&s, m.handle(), 2, Some("own-run"), context(), |_| Ok(()))
        .unwrap();
    let after_context = s.reads.get();
    h.dispatch_continuation(
        &s,
        m.handle(),
        3,
        Some("own-run"),
        continuation(m.handle()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(s.reads.get(), after_context + 1);
    {
        let mut facts = s.facts.borrow_mut();
        facts.generation += 1;
        facts.revision += 1;
    }
    assert!(
        h.dispatch_continuation(
            &s,
            m.handle(),
            4,
            Some("own-run"),
            continuation(m.handle()),
            |_| -> Result<(), ApiError> { panic!("revoked continuation") }
        )
        .is_err()
    );
    let reads = s.reads.get();
    assert!(
        h.dispatch_component_context(&s, m.handle(), 4, Some("own-run"), context(), |_| Ok(()))
            .is_err()
    );
    assert_eq!(reads, s.reads.get());
    let intent = UserMountIntent::new(s.facts.borrow().clone(), resource(1)).unwrap();
    let next = h.mount_component(&s, intent, 5).unwrap();
    assert!(
        h.dispatch_component_context(
            &s,
            next.handle(),
            6,
            Some("foreign-run"),
            context(),
            |_| -> Result<(), ApiError> { panic!("foreign input") }
        )
        .is_err()
    );
}

fn projection_context() -> projection::ProjectionRequest {
    projection::ProjectionRequest::ReadContext {
        api_version: HOST_API_V0_3,
        projection_protocol: projection::PROJECTION_PROTOCOL,
        request_id: "projection".into(),
    }
}
fn bootstrap() -> component_invocation::ComponentInvocationRequest {
    component_invocation::ComponentInvocationRequest {
        api_version: HOST_API_V0_3,
        component_invocation_protocol: component_invocation::COMPONENT_INVOCATION_PROTOCOL,
        request_id: "purpose".into(),
        operation: component_invocation::ComponentInvocationOperation::ReadContext {},
    }
}
#[test]
fn bootstrap_uses_trusted_mount_and_fresh_installation_without_event_parameters() {
    let s = Source {
        facts: RefCell::new(facts()),
        reads: Cell::new(0),
    };
    let mut h = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
    h.configure_component_identity([37; 16]).unwrap();
    let m = h
        .mount_component(
            &s,
            UserMountIntent::new(s.facts.borrow().clone(), resource(1)).unwrap(),
            1,
        )
        .unwrap();
    let identity = h
        .dispatch_component_invocation(&s, m.handle(), 2, Some("own-run"), bootstrap(), |c| {
            Ok(c.clone())
        })
        .unwrap();
    assert_eq!(identity.mount_id, m.handle().mount_id());
    assert_eq!(identity.resource, resource(1));
    {
        let mut f = s.facts.borrow_mut();
        f.revision += 1;
        f.generation += 1;
    }
    assert!(
        h.dispatch_component_invocation(
            &s,
            m.handle(),
            3,
            Some("own-run"),
            bootstrap(),
            |_| -> Result<(), ApiError> { panic!("revoked purpose request") }
        )
        .is_err()
    );
    let reads = s.reads.get();
    assert!(
        h.dispatch_component_invocation(
            &s,
            m.handle(),
            4,
            Some("own-run"),
            bootstrap(),
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(reads, s.reads.get());
}
fn projection_query(m: &MountHandle) -> projection::ProjectionRequest {
    projection::ProjectionRequest::Query {
        api_version: HOST_API_V0_3,
        projection_protocol: projection::PROJECTION_PROTOCOL,
        request_id: "projection-query".into(),
        resource: m.resource().clone(),
        mount_id: m.mount_id().into(),
        query_id: "c".repeat(32),
        base_revision: 1,
        operation: projection::ProjectionOperation::ReadMetadata {},
    }
}
#[test]
fn projection_is_exact_mount_scoped_without_an_edit_event() {
    let (mut h, m) = pure();
    let identity = h
        .dispatch_projection(&m, projection_context(), |c| Ok(c.clone()))
        .unwrap();
    assert_eq!(identity.resource, resource(1));
    assert_eq!(identity.mount_id, m.mount_id());
    h.dispatch_projection(&m, projection_query(&m), |_| Ok(()))
        .unwrap();
    for wrong_resource in [false, true] {
        let mut request = projection_query(&m);
        let projection::ProjectionRequest::Query {
            resource: r,
            mount_id,
            ..
        } = &mut request
        else {
            unreachable!()
        };
        if wrong_resource {
            *r = resource(2);
        } else {
            *mount_id = "d".repeat(32);
        }
        assert!(
            h.dispatch_projection(&m, request, |_| -> Result<(), ApiError> {
                panic!("foreign projection reached application")
            })
            .is_err()
        );
    }
    let (foreign, _) = pure();
    assert!(
        foreign
            .dispatch_projection(&m, projection_context(), |_| Ok(()))
            .is_err()
    );
    h.close_component_mount(&m).unwrap();
    assert!(
        h.dispatch_projection(&m, projection_context(), |_| Ok(()))
            .is_err()
    );
}
#[test]
fn durable_projection_rechecks_each_run_and_installation_generation() {
    let s = Source {
        facts: RefCell::new(facts()),
        reads: Cell::new(0),
    };
    let mut h = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
    h.configure_component_identity([31; 16]).unwrap();
    let m = h
        .mount_component(
            &s,
            UserMountIntent::new(s.facts.borrow().clone(), resource(1)).unwrap(),
            1,
        )
        .unwrap();
    let reads = s.reads.get();
    h.dispatch_projection(
        &s,
        m.handle(),
        2,
        Some("own-run"),
        projection_context(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(s.reads.get(), reads + 1);
    h.dispatch_projection(
        &s,
        m.handle(),
        3,
        Some("own-run"),
        projection_query(m.handle()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(s.reads.get(), reads + 2);
    {
        let mut f = s.facts.borrow_mut();
        f.revision += 1;
        f.generation += 1;
    }
    assert!(
        h.dispatch_projection(
            &s,
            m.handle(),
            4,
            Some("own-run"),
            projection_query(m.handle()),
            |_| -> Result<(), ApiError> { panic!("revoked projection") }
        )
        .is_err()
    );
    let reads = s.reads.get();
    assert!(
        h.dispatch_projection(
            &s,
            m.handle(),
            5,
            Some("own-run"),
            projection_context(),
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(
        s.reads.get(),
        reads,
        "stale mount is refused before fresh source lookup"
    );
    let next = h
        .mount_component(
            &s,
            UserMountIntent::new(s.facts.borrow().clone(), resource(1)).unwrap(),
            6,
        )
        .unwrap();
    assert!(
        h.dispatch_projection(
            &s,
            next.handle(),
            7,
            Some("foreign-run"),
            projection_context(),
            |_| -> Result<(), ApiError> { panic!("foreign worker projection") }
        )
        .is_err()
    );
}
