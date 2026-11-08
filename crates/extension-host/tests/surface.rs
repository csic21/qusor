use extension_api::{surface::*, *};
use extension_host::{Grant, Host, InstallationId, Session, surface::*};

fn document(vault: &str, id: &str) -> Resource {
    Resource::Document {
        vault_id: vault.into(),
        document_id: id.into(),
    }
}
fn source() -> SourceVersion {
    SourceVersion {
        original_sha256: "a".repeat(64),
        extraction_revision: "b".repeat(64),
    }
}
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
            min_minor: 2,
            max_minor: 2,
        },
        required_capabilities: vec![Capability::ReaderSurface],
        optional_capabilities: vec![Capability::SelectionRead],
        contributions: vec![Contribution {
            id: format!("{name}.reader"),
            title: "Read".into(),
            kind: ContributionKind::ReaderWorkspace,
            required_capabilities: vec![Capability::ReaderSurface],
        }],
    }
}
fn request(operation: SurfaceOperation) -> SurfaceRequest {
    SurfaceRequest {
        surface_protocol: SURFACE_PROTOCOL,
        api_version: HOST_API_V0_2,
        request_id: "request".into(),
        operation,
    }
}
fn open(name: &str, vault: &str, id: &str) -> SurfaceRequest {
    request(SurfaceOperation::Open {
        resource: document(vault, id),
        expected: source(),
        contribution_id: format!("{name}.reader"),
    })
}
fn truth(vault: &str, id: &str) -> TrustedSurfaceDocument {
    TrustedSurfaceDocument {
        resource: document(vault, id),
        source: source(),
        page_count: 2,
    }
}
fn grants(vault: &str, id: &str) -> Vec<Grant> {
    [Capability::ReaderSurface, Capability::SelectionRead]
        .into_iter()
        .map(|capability| Grant {
            capability,
            resource: document(vault, id),
        })
        .collect()
}
fn setup() -> (Host, InstallationId, Session) {
    let mut host = Host::new(
        "vault",
        [Capability::ReaderSurface, Capability::SelectionRead],
    )
    .unwrap();
    host.configure_surface_identity([1; 16]).unwrap();
    let id = host
        .register(manifest("org.test.reader"), "a".repeat(64))
        .unwrap();
    host.replace_grants(id, grants("vault", "doc")).unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_2).unwrap();
    (host, id, session)
}
fn opened(host: &mut Host, session: &Session) -> SurfaceTransition {
    host.open_surface(
        session,
        &open("org.test.reader", "vault", "doc"),
        truth("vault", "doc"),
    )
    .unwrap()
}
fn frame(ticket: &NativeFrameTicket) -> FrameMetadata {
    FrameMetadata {
        frame_id: ticket.frame_id().into(),
        pixel_width: 1152,
        pixel_height: 1488,
    }
}
fn ready(host: &mut Host, transition: &SurfaceTransition) -> NativeInputTicket {
    host.native_frame_ready(&transition.frame_ticket, frame(&transition.frame_ticket))
        .unwrap();
    host.native_input_ticket(&transition.frame_ticket).unwrap()
}
fn selection() -> TrustedSurfaceSelection {
    TrustedSurfaceSelection {
        source: source(),
        capture_protocol: 1,
        draft_json: r#"{"protocol_version":1,"exact":"QUSOR"}"#.into(),
    }
}
fn selected(host: &mut Host, input: &NativeInputTicket) -> String {
    let gesture = host.surface_begin_gesture(input).unwrap();
    host.publish_surface_selection(&gesture, selection())
        .unwrap()
}
fn read(handle: &str, generation: u64, lease: &str) -> SurfaceRequest {
    request(SurfaceOperation::ReadSelection {
        handle: handle.into(),
        generation,
        selection_lease: lease.into(),
    })
}
fn error<T: std::fmt::Debug>(result: Result<T, ApiError>, code: ErrorCode) {
    assert_eq!(result.unwrap_err().code, code);
}

#[test]
fn session_protocol_is_explicit_and_legacy_selection_scope_is_not_broadened() {
    let (mut host, id, session) = setup();
    error(host.open_session(id), ErrorCode::UnsupportedVersion);
    assert_eq!(host.session_api(&session).unwrap(), HOST_API_V0_2);
    let metadata = CapabilityRequest {
        api_version: HOST_API,
        request_id: "wrong-version".into(),
        capability: Capability::SelectionRead,
        resource: document("vault", "doc"),
    };
    error(
        host.authorize(&session, metadata),
        ErrorCode::UnsupportedVersion,
    );
    let mut old = manifest("org.test.legacy");
    old.host_api = ApiRange {
        major: 0,
        min_minor: 1,
        max_minor: 2,
    };
    old.required_capabilities = vec![Capability::SelectionRead];
    old.optional_capabilities.clear();
    old.contributions.clear();
    let old = host.register(old, "c".repeat(64)).unwrap();
    host.replace_grants(
        old,
        vec![Grant {
            capability: Capability::SelectionRead,
            resource: document("vault", "doc"),
        }],
    )
    .unwrap();
    host.enable(old).unwrap();
    let legacy = host.open_session(old).unwrap();
    error(
        host.authorize(
            &legacy,
            CapabilityRequest {
                api_version: HOST_API,
                request_id: "old-read".into(),
                capability: Capability::SelectionRead,
                resource: document("vault", "doc"),
            },
        ),
        ErrorCode::PermissionDenied,
    );
    let modern = host.open_session_for_api(old, HOST_API_V0_2).unwrap();
    host.authorize(
        &modern,
        CapabilityRequest {
            api_version: HOST_API_V0_2,
            request_id: "new-read".into(),
            capability: Capability::SelectionRead,
            resource: document("vault", "doc"),
        },
    )
    .unwrap();
}
#[test]
fn workspace_scope_truth_and_boot_identity_are_checked_before_a_surface_exists() {
    let (mut host, _, session) = setup();
    error(
        host.configure_surface_identity([2; 16]),
        ErrorCode::InvalidState,
    );
    error(
        host.open_surface(
            &session,
            &open("org.test.other", "vault", "doc"),
            truth("vault", "doc"),
        ),
        ErrorCode::PermissionDenied,
    );
    error(
        host.open_surface(
            &session,
            &open("org.test.reader", "vault", "other"),
            truth("vault", "other"),
        ),
        ErrorCode::PermissionDenied,
    );
    error(
        host.open_surface(
            &session,
            &open("org.test.reader", "foreign", "doc"),
            truth("foreign", "doc"),
        ),
        ErrorCode::PermissionDenied,
    );
    let mut wrong = truth("vault", "doc");
    wrong.source.extraction_revision = "c".repeat(64);
    error(
        host.open_surface(&session, &open("org.test.reader", "vault", "doc"), wrong),
        ErrorCode::StaleContext,
    );
    assert!(host.pending_surface_cleanup().is_empty());
    let mut other = Host::new("vault", [Capability::ReaderSurface]).unwrap();
    error(
        other.configure_surface_identity([0; 16]),
        ErrorCode::InvalidState,
    );
    let id = other
        .register(manifest("org.test.reader"), "a".repeat(64))
        .unwrap();
    other
        .replace_grants(id, vec![grants("vault", "doc")[0].clone()])
        .unwrap();
    other.enable(id).unwrap();
    let s = other.open_session_for_api(id, HOST_API_V0_2).unwrap();
    error(
        other.open_surface(
            &s,
            &open("org.test.reader", "vault", "doc"),
            truth("vault", "doc"),
        ),
        ErrorCode::InvalidState,
    );
}
#[test]
fn pending_frames_and_old_jobs_cannot_accept_input() {
    let (mut host, _, session) = setup();
    let first = opened(&mut host, &session);
    assert_eq!(first.descriptor.status, SurfaceStatus::Pending);
    error(
        host.native_input_ticket(&first.frame_ticket),
        ErrorCode::StaleContext,
    );
    let mut fake = frame(&first.frame_ticket);
    fake.frame_id = "forged".into();
    error(
        host.native_frame_ready(&first.frame_ticket, fake),
        ErrorCode::StaleContext,
    );
    let second = host
        .configure_surface(
            &session,
            &request(SurfaceOperation::Configure {
                handle: first.descriptor.handle.clone(),
                generation: 1,
                page_index: 1,
                zoom: 0.7,
            }),
        )
        .unwrap();
    error(
        host.native_frame_ready(&first.frame_ticket, frame(&first.frame_ticket)),
        ErrorCode::StaleContext,
    );
    let input = ready(&mut host, &second);
    let lease = selected(&mut host, &input);
    assert_eq!(
        host.read_surface_selection(&session, &read(input.handle(), input.generation(), &lease))
            .unwrap()
            .result,
        SurfaceResult::Selection {
            handle: input.handle().into(),
            generation: input.generation(),
            selection_lease: lease,
            capture_protocol: 1,
            draft_json: selection().draft_json
        }
    );
    error(
        host.configure_surface(
            &session,
            &request(SurfaceOperation::Configure {
                handle: input.handle().into(),
                generation: 1,
                page_index: 0,
                zoom: 1.0,
            }),
        ),
        ErrorCode::StaleContext,
    );
}
#[test]
fn publication_is_bound_to_current_native_gesture_source_and_full_wire_budget() {
    let (mut host, _, session) = setup();
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let old = host.surface_begin_gesture(&input).unwrap();
    let current = host.surface_begin_gesture(&input).unwrap();
    error(
        host.publish_surface_selection(&old, selection()),
        ErrorCode::StaleContext,
    );
    error(host.cancel_surface_gesture(&old), ErrorCode::StaleContext);
    let mut wrong = selection();
    wrong.source.original_sha256 = "c".repeat(64);
    error(
        host.publish_surface_selection(&current, wrong),
        ErrorCode::StaleContext,
    );
    let mut huge = selection();
    huge.capture_protocol = 2;
    huge.draft_json = "x".repeat(4096);
    error(
        host.publish_surface_selection(&current, huge),
        ErrorCode::BudgetExceeded,
    );
    let exact = selection();
    let lease = host
        .publish_surface_selection(&current, exact.clone())
        .unwrap();
    let response = host
        .read_surface_selection(&session, &read(input.handle(), input.generation(), &lease))
        .unwrap();
    response.to_json().unwrap();
    assert!(
        matches!(response.result,SurfaceResult::Selection{draft_json,..} if draft_json==exact.draft_json)
    );
    error(
        host.publish_surface_selection(&current, selection()),
        ErrorCode::StaleContext,
    );
    let full = r#"{"protocol_version":2,"segments":[{"exact":"原文一"},{"exact":"原文二"}],"exact":"原文一\n原文二"}"#;
    let next = host.surface_begin_gesture(&input).unwrap();
    let lease = host
        .publish_surface_selection(
            &next,
            TrustedSurfaceSelection {
                source: source(),
                capture_protocol: 2,
                draft_json: full.into(),
            },
        )
        .unwrap();
    let response = host
        .read_surface_selection(&session, &read(input.handle(), input.generation(), &lease))
        .unwrap();
    assert!(
        matches!(response.result,SurfaceResult::Selection{capture_protocol:2,draft_json,..} if draft_json==full)
    );
}
#[test]
fn handles_and_native_tickets_cannot_cross_installations_sessions_or_hosts() {
    let (mut host, owner_id, session) = setup();
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let lease = selected(&mut host, &input);
    let id = host
        .register(manifest("org.test.other"), "d".repeat(64))
        .unwrap();
    host.replace_grants(id, grants("vault", "doc")).unwrap();
    host.enable(id).unwrap();
    let other = host.open_session_for_api(id, HOST_API_V0_2).unwrap();
    error(
        host.read_surface_selection(&other, &read(input.handle(), input.generation(), &lease)),
        ErrorCode::StaleContext,
    );
    let own_id = host
        .register(manifest("org.test.third"), "e".repeat(64))
        .unwrap();
    host.replace_grants(own_id, grants("vault", "doc")).unwrap();
    host.enable(own_id).unwrap();
    let third = host.open_session_for_api(own_id, HOST_API_V0_2).unwrap();
    error(
        host.close_surface(
            &third,
            &request(SurfaceOperation::Close {
                handle: input.handle().into(),
            }),
        ),
        ErrorCode::StaleContext,
    );
    let same_installation_session = host.open_session_for_api(owner_id, HOST_API_V0_2).unwrap();
    assert_eq!(
        host.surface_state(&session, input.handle())
            .unwrap()
            .selection_lease,
        Some(lease.clone())
    );
    error(
        host.surface_state(&same_installation_session, input.handle()),
        ErrorCode::StaleContext,
    );
    error(
        host.read_surface_selection(
            &same_installation_session,
            &read(input.handle(), input.generation(), &lease),
        ),
        ErrorCode::StaleContext,
    );
    let (mut other_host, _, other_session) = setup();
    error(
        other_host.native_frame_ready(&transition.frame_ticket, frame(&transition.frame_ticket)),
        ErrorCode::StaleContext,
    );
    error(
        other_host.read_surface_selection(
            &other_session,
            &read(input.handle(), input.generation(), &lease),
        ),
        ErrorCode::StaleContext,
    );
    let own = opened(&mut other_host, &other_session);
    assert_ne!(own.descriptor.handle, transition.descriptor.handle);
}
#[test]
fn selection_reads_need_their_own_document_grant_without_replacing_grants_per_gesture() {
    let (mut host, id, session) = setup();
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let a = selected(&mut host, &input);
    let b = selected(&mut host, &input);
    error(
        host.read_surface_selection(&session, &read(input.handle(), input.generation(), &a)),
        ErrorCode::StaleContext,
    );
    host.read_surface_selection(&session, &read(input.handle(), input.generation(), &b))
        .unwrap();
    assert_eq!(host.session_api(&session).unwrap(), HOST_API_V0_2);
    host.replace_grants(id, vec![grants("vault", "doc")[0].clone()])
        .unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_2).unwrap();
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let lease = selected(&mut host, &input);
    error(
        host.read_surface_selection(&session, &read(input.handle(), input.generation(), &lease)),
        ErrorCode::PermissionDenied,
    );
}
#[test]
fn every_lifecycle_invalidation_denies_results_until_native_cleanup_is_acknowledged() {
    for action in 0..6 {
        let (mut host, id, session) = setup();
        let transition = opened(&mut host, &session);
        let input = ready(&mut host, &transition);
        let lease = selected(&mut host, &input);
        match action {
            0 => host.disable(id).unwrap(),
            1 => host.replace_grants(id, grants("vault", "doc")).unwrap(),
            2 => {
                let mut next = manifest("org.test.reader");
                next.package_version.patch = 1;
                host.update(id, next, "f".repeat(64)).unwrap();
            }
            3 => host.unregister(id).unwrap(),
            4 => host.close_session(&session).unwrap(),
            _ => host
                .invalidate_document_surfaces(&document("vault", "doc"))
                .unwrap(),
        }
        error(
            host.read_surface_selection(
                &session,
                &read(input.handle(), input.generation(), &lease),
            ),
            ErrorCode::StaleContext,
        );
        error(
            host.native_frame_ready(&transition.frame_ticket, frame(&transition.frame_ticket)),
            ErrorCode::StaleContext,
        );
        error(
            host.clear_surface_selection(&input),
            ErrorCode::StaleContext,
        );
        let cleanup = host.pending_surface_cleanup();
        assert_eq!(cleanup.len(), 1);
        host.confirm_surface_closed(&cleanup[0]).unwrap();
        host.confirm_surface_closed(&cleanup[0]).unwrap();
        assert!(host.pending_surface_cleanup().is_empty());
    }
}
#[test]
fn explicit_close_and_blank_or_cancel_do_not_reuse_candidates() {
    let (mut host, _, session) = setup();
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let old = selected(&mut host, &input);
    host.clear_surface_selection(&input).unwrap();
    error(
        host.read_surface_selection(&session, &read(input.handle(), input.generation(), &old)),
        ErrorCode::StaleContext,
    );
    let gesture = host.surface_begin_gesture(&input).unwrap();
    host.cancel_surface_gesture(&gesture).unwrap();
    error(
        host.publish_surface_selection(&gesture, selection()),
        ErrorCode::StaleContext,
    );
    let closed = host
        .close_surface(
            &session,
            &request(SurfaceOperation::Close {
                handle: input.handle().into(),
            }),
        )
        .unwrap();
    assert_eq!(closed.status, SurfaceStatus::Closing);
    assert!(closed.frame.is_none());
    assert!(closed.selection_lease.is_none());
    error(
        host.native_input_ticket(&transition.frame_ticket),
        ErrorCode::StaleContext,
    );
    assert_eq!(host.pending_surface_cleanup().len(), 1);
}

#[test]
fn required_workspace_capabilities_are_checked_for_the_actual_document() {
    let mut host = Host::new(
        "vault",
        [Capability::ReaderSurface, Capability::SelectionRead],
    )
    .unwrap();
    host.configure_surface_identity([2; 16]).unwrap();
    let mut package = manifest("org.test.reader");
    package.contributions[0]
        .required_capabilities
        .push(Capability::SelectionRead);
    let id = host.register(package, "a".repeat(64)).unwrap();
    host.replace_grants(
        id,
        vec![
            Grant {
                capability: Capability::ReaderSurface,
                resource: document("vault", "doc"),
            },
            Grant {
                capability: Capability::SelectionRead,
                resource: document("vault", "other"),
            },
        ],
    )
    .unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_2).unwrap();
    error(
        host.open_surface(
            &session,
            &open("org.test.reader", "vault", "doc"),
            truth("vault", "doc"),
        ),
        ErrorCode::PermissionDenied,
    );
    host.replace_grants(id, grants("vault", "doc")).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_2).unwrap();
    opened(&mut host, &session);
}
#[test]
fn cleanup_must_finish_before_capacity_is_reused_and_handles_never_alias() {
    let (mut host, _, session) = setup();
    let mut pending = Vec::new();
    for _ in 0..16 {
        pending.push(opened(&mut host, &session));
    }
    error(
        host.open_surface(
            &session,
            &open("org.test.reader", "vault", "doc"),
            truth("vault", "doc"),
        ),
        ErrorCode::BudgetExceeded,
    );
    let old = &pending[0];
    host.close_surface(
        &session,
        &request(SurfaceOperation::Close {
            handle: old.descriptor.handle.clone(),
        }),
    )
    .unwrap();
    error(
        host.open_surface(
            &session,
            &open("org.test.reader", "vault", "doc"),
            truth("vault", "doc"),
        ),
        ErrorCode::BudgetExceeded,
    );
    let ticket = host.pending_surface_cleanup().remove(0);
    host.confirm_surface_closed(&ticket).unwrap();
    let next = opened(&mut host, &session);
    assert_ne!(old.descriptor.handle, next.descriptor.handle);
    host.confirm_surface_closed(&ticket).unwrap();
    error(
        host.native_frame_ready(&old.frame_ticket, frame(&old.frame_ticket)),
        ErrorCode::StaleContext,
    );
    host.native_frame_ready(&next.frame_ticket, frame(&next.frame_ticket))
        .unwrap();
}

#[test]
fn failed_frames_block_input_until_an_explicit_fresh_generation_is_ready() {
    let (mut host, _, session) = setup();
    let first = opened(&mut host, &session);
    let failed = host.native_frame_failed(&first.frame_ticket).unwrap();
    assert_eq!(failed.status, SurfaceStatus::Failed);
    assert!(failed.frame.is_none());
    error(
        host.native_input_ticket(&first.frame_ticket),
        ErrorCode::StaleContext,
    );
    error(
        host.native_frame_ready(&first.frame_ticket, frame(&first.frame_ticket)),
        ErrorCode::StaleContext,
    );
    let fresh = host
        .configure_surface(
            &session,
            &request(SurfaceOperation::Configure {
                handle: first.descriptor.handle,
                generation: first.descriptor.generation,
                page_index: 0,
                zoom: 1.0,
            }),
        )
        .unwrap();
    error(
        host.native_frame_failed(&first.frame_ticket),
        ErrorCode::StaleContext,
    );
    ready(&mut host, &fresh);
}

#[test]
fn invocation_preflight_uses_exact_live_scope_and_does_not_mutate_state() {
    let (mut host, _, session) = setup();
    host.preflight_surface_request(&session, &open("org.test.reader", "vault", "doc"))
        .unwrap();
    error(
        host.preflight_surface_request(&session, &open("org.test.other", "vault", "doc")),
        ErrorCode::PermissionDenied,
    );
    error(
        host.preflight_surface_request(&session, &open("org.test.reader", "vault", "other")),
        ErrorCode::PermissionDenied,
    );
    let transition = opened(&mut host, &session);
    let input = ready(&mut host, &transition);
    let lease = selected(&mut host, &input);
    let before = host.surface_state(&session, input.handle()).unwrap();
    let configure = request(SurfaceOperation::Configure {
        handle: input.handle().into(),
        generation: input.generation(),
        page_index: 1,
        zoom: 0.7,
    });
    host.preflight_surface_request(&session, &configure)
        .unwrap();
    host.preflight_surface_request(&session, &read(input.handle(), input.generation(), &lease))
        .unwrap();
    error(
        host.preflight_surface_request(
            &session,
            &read(input.handle(), input.generation(), "wrong-lease"),
        ),
        ErrorCode::StaleContext,
    );
    error(
        host.preflight_surface_request(
            &session,
            &request(SurfaceOperation::Configure {
                handle: input.handle().into(),
                generation: input.generation(),
                page_index: 2,
                zoom: 0.7,
            }),
        ),
        ErrorCode::StaleContext,
    );
    host.preflight_surface_request(
        &session,
        &request(SurfaceOperation::Close {
            handle: input.handle().into(),
        }),
    )
    .unwrap();
    assert_eq!(
        host.surface_state(&session, input.handle()).unwrap(),
        before
    );
    host.configure_surface(&session, &configure).unwrap();
    error(
        host.preflight_surface_request(&session, &configure),
        ErrorCode::StaleContext,
    );
}
#[test]
fn close_result_can_be_revalidated_without_reopening_a_closing_surface() {
    let (mut host, id, session) = setup();
    let transition = opened(&mut host, &session);
    let request = request(SurfaceOperation::Close {
        handle: transition.descriptor.handle.clone(),
    });
    let closed = host.close_surface(&session, &request).unwrap();
    assert_eq!(
        host.surface_result_state(&session, &closed.handle).unwrap(),
        closed
    );
    error(
        host.surface_state(&session, &closed.handle),
        ErrorCode::StaleContext,
    );
    error(
        host.preflight_surface_request(&session, &request),
        ErrorCode::StaleContext,
    );
    host.disable(id).unwrap();
    error(
        host.surface_result_state(&session, &closed.handle),
        ErrorCode::StaleContext,
    );
    let cleanup = host.pending_surface_cleanup();
    host.confirm_surface_closed(&cleanup[0]).unwrap();
}
