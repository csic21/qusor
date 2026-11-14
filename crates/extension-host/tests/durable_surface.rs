use extension_api::{surface::*, *};
use extension_host::{Grant, durable::*, surface::*};
use std::{cell::RefCell, collections::BTreeMap};

struct Journal(RefCell<BTreeMap<String, AuthoritySnapshot>>);
struct Tombstone<'a>(&'a Journal);
impl AuthoritySource for Tombstone<'_> {
    fn authority_snapshot(
        &self,
        p: &str,
        r: AuthorityRead<'_>,
    ) -> Result<AuthoritySnapshot, ApiError> {
        self.0.authority_snapshot(p, r)
    }
    fn removed_installation(&self, p: &str) -> Result<RemovedInstallation, ApiError> {
        Ok(RemovedInstallation {
            installation_id: 1,
            package_id: p.into(),
            vault_id: "vault".into(),
            revision: 3,
            generation: 3,
        })
    }
}
impl AuthoritySource for Journal {
    fn authority_snapshot(
        &self,
        package: &str,
        _: AuthorityRead<'_>,
    ) -> Result<AuthoritySnapshot, ApiError> {
        self.0
            .borrow()
            .get(package)
            .cloned()
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "missing intent"))
    }
}
fn document() -> Resource {
    Resource::Document {
        vault_id: "vault".into(),
        document_id: "doc".into(),
    }
}
fn source() -> SourceVersion {
    SourceVersion {
        original_sha256: "a".repeat(64),
        extraction_revision: "b".repeat(64),
    }
}
fn snapshot(id: i64, package: &str) -> AuthoritySnapshot {
    AuthoritySnapshot {
        installation_id: id,
        revision: 2,
        generation: 2,
        digest: "a".repeat(64),
        vault_id: "vault".into(),
        enable_intent: true,
        required_api: Some(HOST_API_V0_2),
        manifest: Manifest {
            manifest_version: 1,
            package_id: package.into(),
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
            optional_capabilities: vec![Capability::SelectionRead, Capability::CapturePropose],
            contributions: vec![Contribution {
                id: format!("{package}.reader"),
                title: "Read".into(),
                kind: ContributionKind::ReaderWorkspace,
                required_capabilities: vec![Capability::ReaderSurface],
            }],
            orthogonal_capabilities: vec![],
            provides: vec![],
            requires: vec![],
        },
        grants: vec![Capability::ReaderSurface, Capability::SelectionRead]
            .into_iter()
            .map(|capability| DurableGrant {
                grant: Grant {
                    capability,
                    resource: document(),
                },
                expires_at: 100,
                resource_version: Some(source().extraction_revision),
            })
            .collect(),
    }
}
fn journal() -> Journal {
    Journal(RefCell::new(
        [
            ("org.test.a".into(), snapshot(1, "org.test.a")),
            ("org.test.b".into(), snapshot(2, "org.test.b")),
        ]
        .into(),
    ))
}
fn host() -> DurableHost {
    let mut h = DurableHost::new(
        "vault",
        [
            Capability::ReaderSurface,
            Capability::SelectionRead,
            Capability::CapturePropose,
        ],
    )
    .unwrap();
    h.configure_surface_identity([7; 16]).unwrap();
    h
}
fn request(operation: SurfaceOperation) -> SurfaceRequest {
    SurfaceRequest {
        surface_protocol: 1,
        api_version: HOST_API_V0_2,
        request_id: "reader".into(),
        operation,
    }
}
fn open(package: &str) -> SurfaceRequest {
    request(SurfaceOperation::Open {
        resource: document(),
        expected: source(),
        contribution_id: format!("{package}.reader"),
    })
}
fn truth() -> TrustedSurfaceDocument {
    TrustedSurfaceDocument {
        resource: document(),
        source: source(),
        page_count: 3,
    }
}
fn frame(ticket: &NativeFrameTicket) -> FrameMetadata {
    FrameMetadata {
        frame_id: ticket.frame_id().into(),
        pixel_width: 100,
        pixel_height: 100,
    }
}

#[test]
fn restored_surface_preserves_complete_selection_and_fences_native_results_after_revoke() {
    let j = journal();
    let mut h = host();
    let a = h.activate(&j, "org.test.a", HOST_API_V0_2, 1).unwrap();
    let b = h.activate(&j, "org.test.b", HOST_API_V0_2, 1).unwrap();
    let opened = h
        .with_surface(&j, &a, 2, None, |s| {
            s.open_surface(&open("org.test.a"), truth())
        })
        .unwrap();
    assert!(
        h.with_surface(&j, &b, 2, None, |s| s
            .native_frame_ready(&opened.frame_ticket, frame(&opened.frame_ticket)))
            .is_err(),
        "B cannot publish A frame"
    );
    let input = h
        .with_surface(&j, &a, 2, None, |s| {
            s.native_frame_ready(&opened.frame_ticket, frame(&opened.frame_ticket))?;
            s.native_input_ticket(&opened.frame_ticket)
        })
        .unwrap();
    let gesture = h
        .with_surface(&j, &a, 2, None, |s| s.surface_begin_gesture(&input))
        .unwrap();
    let complete = r#"{"protocol_version":2,"exact":"first\nsecond","segments":[{"exact":"first"},{"exact":"second"}]}"#;
    let lease = h
        .with_surface(&j, &a, 2, None, |s| {
            s.publish_surface_selection(
                &gesture,
                TrustedSurfaceSelection {
                    source: source(),
                    capture_protocol: 2,
                    draft_json: complete.into(),
                },
            )
        })
        .unwrap();
    let read = request(SurfaceOperation::ReadSelection {
        handle: opened.descriptor.handle.clone(),
        generation: opened.descriptor.generation,
        selection_lease: lease,
    });
    let response = h
        .with_surface(&j, &a, 2, None, |s| s.read_surface_selection(&read))
        .unwrap();
    let SurfaceResult::Selection {
        capture_protocol,
        draft_json,
        ..
    } = response.result
    else {
        panic!("complete selection missing")
    };
    assert_eq!(capture_protocol, 2);
    assert_eq!(draft_json, complete);
    j.0.borrow_mut().remove("org.test.a");
    assert!(
        h.with_surface(&j, &a, 3, None, |s| s.read_surface_selection(&read))
            .is_err()
    );
    assert_eq!(
        h.pending_surface_cleanup().len(),
        1,
        "logical revoke must not pretend native release"
    );
    assert!(
        h.with_surface(&j, &b, 3, None, |s| s.cancel_surface_gesture(&gesture))
            .is_err()
    );
    assert!(
        h.with_surface(&j, &b, 3, None, |s| s
            .open_surface(&open("org.test.b"), truth()))
            .is_ok()
    );
    j.0.borrow_mut()
        .insert("org.test.a".into(), snapshot(1, "org.test.a"));
    let fresh = h.activate(&j, "org.test.a", HOST_API_V0_2, 3).unwrap();
    let replacement = h
        .with_surface(&j, &fresh, 3, None, |s| {
            s.open_surface(&open("org.test.a"), truth())
        })
        .unwrap();
    assert_ne!(replacement.descriptor.handle, opened.descriptor.handle);
    assert!(
        h.with_surface(&j, &fresh, 3, None, |s| s
            .native_frame_ready(&opened.frame_ticket, frame(&opened.frame_ticket)))
            .is_err()
    );
    assert!(
        h.with_surface(&j, &fresh, 3, None, |s| s.read_surface_selection(&read))
            .is_err()
    );
    let cleanup = h.pending_surface_cleanup().remove(0);
    h.confirm_surface_closed(&cleanup).unwrap();
    assert!(h.pending_surface_cleanup().is_empty());
    assert!(
        h.with_surface(&j, &fresh, 3, None, |s| s
            .surface_state(&replacement.descriptor.handle))
            .is_ok(),
        "old cleanup must not remove the new mount"
    );
}

#[test]
fn durable_version_and_optional_read_are_checked_before_open_or_read() {
    let j = journal();
    let mut h = host();
    j.0.borrow_mut()
        .get_mut("org.test.a")
        .unwrap()
        .grants
        .retain(|g| g.grant.capability == Capability::ReaderSurface);
    let a = h.activate(&j, "org.test.a", HOST_API_V0_2, 1).unwrap();
    let mut wrong = open("org.test.a");
    if let SurfaceOperation::Open { expected, .. } = &mut wrong.operation {
        expected.extraction_revision = "c".repeat(64);
    }
    assert!(
        h.with_surface(&j, &a, 2, None, |s| s.open_surface(&wrong, truth()))
            .is_err()
    );
    let opened = h
        .with_surface(&j, &a, 2, None, |s| {
            s.open_surface(&open("org.test.a"), truth())
        })
        .unwrap();
    let input = h
        .with_surface(&j, &a, 2, None, |s| {
            s.native_frame_ready(&opened.frame_ticket, frame(&opened.frame_ticket))?;
            s.native_input_ticket(&opened.frame_ticket)
        })
        .unwrap();
    let lease = h
        .with_surface(&j, &a, 2, None, |s| {
            let gesture = s.surface_begin_gesture(&input)?;
            s.publish_surface_selection(
                &gesture,
                TrustedSurfaceSelection {
                    source: source(),
                    capture_protocol: 1,
                    draft_json: "{}".into(),
                },
            )
        })
        .unwrap();
    let read = request(SurfaceOperation::ReadSelection {
        handle: opened.descriptor.handle,
        generation: opened.descriptor.generation,
        selection_lease: lease,
    });
    assert!(
        h.with_surface(&j, &a, 2, None, |s| s.read_surface_selection(&read))
            .is_err(),
        "presenting a page must not grant selection read"
    );
}

#[test]
fn repeated_activation_keeps_unreleased_resources_in_the_same_installation_quota() {
    let j = journal();
    let mut h = host();
    let mut previous = None;
    for _ in 0..16 {
        let session = h.activate(&j, "org.test.a", HOST_API_V0_2, 1).unwrap();
        if let Some(old) = previous.as_ref() {
            assert!(
                h.close(old).is_err(),
                "old close must not close its successor"
            );
        }
        h.with_surface(&j, &session, 1, None, |s| {
            s.open_surface(&open("org.test.a"), truth())
        })
        .unwrap();
        previous = Some(session);
    }
    let a = h.activate(&j, "org.test.a", HOST_API_V0_2, 1).unwrap();
    let error = h
        .with_surface(&j, &a, 1, None, |s| {
            s.open_surface(&open("org.test.a"), truth())
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BudgetExceeded);
    assert_eq!(h.pending_surface_cleanup().len(), 16);
    let b = h.activate(&j, "org.test.b", HOST_API_V0_2, 1).unwrap();
    assert!(
        h.with_surface(&j, &b, 1, None, |s| s
            .open_surface(&open("org.test.b"), truth()))
            .is_ok()
    );
    let cleanup = h.pending_surface_cleanup().remove(0);
    h.confirm_surface_closed(&cleanup).unwrap();
    assert!(
        h.with_surface(&j, &a, 1, None, |s| s
            .open_surface(&open("org.test.a"), truth()))
            .is_ok()
    );
}

#[test]
fn durable_uninstall_cannot_retire_before_the_native_release_ack() {
    let j = journal();
    let mut h = host();
    let a = h.activate(&j, "org.test.a", HOST_API_V0_2, 1).unwrap();
    h.with_surface(&j, &a, 1, None, |s| {
        s.open_surface(&open("org.test.a"), truth())
    })
    .unwrap();
    assert!(
        h.retire_removed(&j, 1, "org.test.a").is_err(),
        "closed resources alone are not an uninstall proof"
    );
    j.0.borrow_mut().remove("org.test.a");
    let removed = Tombstone(&j);
    assert!(h.retire_removed(&removed, 1, "org.test.a").is_err());
    assert_eq!(h.pending_surface_cleanup().len(), 1);
    let ticket = h.pending_surface_cleanup().remove(0);
    h.confirm_surface_closed(&ticket).unwrap();
    h.retire_removed(&removed, 1, "org.test.a").unwrap();
    assert!(
        h.with_surface(&removed, &a, 1, None, |s| s.surface_state(ticket.handle()))
            .is_err()
    );
}
