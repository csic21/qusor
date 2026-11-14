use extension_api::{composition::*, draft::*, *};
use extension_host::{Host, InstallationId, Status, component::*};
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
            min_minor: 3,
            max_minor: 3,
        },
        required_capabilities: vec![Capability::NoteDraftEdit],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: format!("{name}.editor"),
            title: "Thought editor".into(),
            kind: ContributionKind::ComponentProvider,
            required_capabilities: vec![Capability::NoteDraftEdit],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}
fn descriptor(name: &str) -> CompositionDescriptor {
    CompositionDescriptor {
        composition_protocol: COMPOSITION_PROTOCOL,
        api_version: HOST_API_V0_3,
        contribution_id: format!("{name}.editor"),
        role: ComponentRole::AnnotationPlainTextEditor,
        contract: ANNOTATION_EDITOR_CONTRACT.into(),
        view: NativeEditorView::TextEditor {
            label: "Thought".into(),
            placeholder: "Write here".into(),
        },
    }
}
fn resource(vault: &str, n: u8) -> Resource {
    Resource::Draft {
        vault_id: vault.into(),
        draft_id: format!("{n:032x}"),
    }
}
fn setup() -> (Host, InstallationId) {
    let mut host = Host::new("vault", [Capability::NoteDraftEdit]).unwrap();
    host.configure_component_identity([17; 16]).unwrap();
    let id = host
        .register(manifest("org.test.a"), "a".repeat(64))
        .unwrap();
    (host, id)
}
fn open(host: &mut Host, id: InstallationId, n: u8) -> MountHandle {
    host.open_component_mount(
        id,
        "org.test.a.editor",
        resource("vault", n),
        &descriptor("org.test.a"),
    )
    .unwrap()
}
fn request(handle: &MountHandle) -> DraftRequest {
    DraftRequest {
        api_version: HOST_API_V0_3,
        draft_protocol: DRAFT_PROTOCOL,
        request_id: "read".into(),
        resource: handle.resource().clone(),
        mount_id: handle.mount_id().into(),
        operation: DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 512,
        },
    }
}
#[test]
fn temporary_mount_does_not_enable_installation_or_create_session_grants() {
    let (mut host, id) = setup();
    assert!(host.open_session(id).is_err());
    let mount = open(&mut host, id, 1);
    assert!(host.open_session(id).is_err());
    assert!(host.contributions().is_empty());
    assert_eq!(mount.mount_id().len(), 32);
    assert!(
        mount
            .mount_id()
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let value = host
        .dispatch_draft(&mount, request(&mount), |ctx| {
            assert_eq!(ctx.installation_id, id);
            assert_eq!(ctx.resource, resource("vault", 1));
            assert_eq!(ctx.package_digest, "a".repeat(64));
            Ok(ctx.package_id.clone())
        })
        .unwrap();
    assert_eq!(value, "org.test.a");
    // Existing public status remains Disabled; this mount is not an installation enable.
    assert_eq!(host.status(id).unwrap(), Status::Disabled);
}
#[test]
fn owner_exact_scope_api_and_descriptor_are_independent_checks() {
    let (mut host, id) = setup();
    let a = open(&mut host, id, 1);
    let b = open(&mut host, id, 2);
    let (other, _) = setup();
    assert!(other.validate_component_mount(&a).is_err());
    for bad in [
        Resource::Vault {
            vault_id: "vault".into(),
        },
        resource("other", 1),
        resource("vault", 2),
    ] {
        let mut r = request(&a);
        r.resource = bad;
        assert!(host.dispatch_draft(&a, r, |_| Ok(())).is_err());
    }
    let mut r = request(&a);
    r.mount_id = b.mount_id().into();
    assert!(host.dispatch_draft(&a, r, |_| Ok(())).is_err());
    let mut r = request(&a);
    r.api_version = HOST_API_V0_2;
    assert!(host.dispatch_draft(&a, r, |_| Ok(())).is_err());
    let mut wrong = descriptor("org.test.a");
    wrong.contribution_id = "org.test.a.missing".into();
    assert!(
        host.open_component_mount(id, "org.test.a.editor", resource("vault", 3), &wrong)
            .is_err()
    );
    assert!(
        host.open_component_mount(
            id,
            "org.test.a.editor",
            resource("foreign", 3),
            &descriptor("org.test.a")
        )
        .is_err()
    );
}
#[test]
fn closing_one_mount_cannot_close_its_successor_and_ack_is_host_owned() {
    let (mut host, id) = setup();
    let a = open(&mut host, id, 1);
    let b = open(&mut host, id, 1);
    assert_ne!(a.mount_id(), b.mount_id());
    host.close_component_mount(&a).unwrap();
    assert!(host.validate_component_mount(&a).is_err());
    host.validate_component_mount(&b).unwrap();
    let ticket = host.pending_component_cleanup().pop().unwrap();
    let (mut other, _) = setup();
    assert!(other.confirm_component_released(&ticket).is_err());
    host.confirm_component_released(&ticket).unwrap();
    host.confirm_component_released(&ticket).unwrap();
    host.close_component_mount(&a).unwrap();
    host.validate_component_mount(&b).unwrap();
}
#[test]
fn invalidation_keeps_per_installation_closing_quota_until_actual_ack() {
    let (mut host, id) = setup();
    let mounts = (0..16).map(|n| open(&mut host, id, n)).collect::<Vec<_>>();
    assert!(
        host.open_component_mount(
            id,
            "org.test.a.editor",
            resource("vault", 31),
            &descriptor("org.test.a")
        )
        .is_err()
    );
    host.disable(id).unwrap();
    assert_eq!(host.pending_component_cleanup().len(), 16);
    for mount in &mounts {
        assert!(
            host.dispatch_draft(mount, request(mount), |_| Ok(()))
                .is_err()
        );
    }
    assert!(
        host.open_component_mount(
            id,
            "org.test.a.editor",
            resource("vault", 32),
            &descriptor("org.test.a")
        )
        .is_err()
    );
    let ticket = host.pending_component_cleanup().pop().unwrap();
    host.confirm_component_released(&ticket).unwrap();
    let successor = open(&mut host, id, 33);
    host.close_component_mount(&mounts[0]).unwrap();
    host.validate_component_mount(&successor).unwrap();
    assert!(host.configure_component_identity([18; 16]).is_err());
}
#[test]
fn global_limit_counts_closing_resources_across_installations() {
    let (mut host, id) = setup();
    for n in 0..16 {
        open(&mut host, id, n);
    }
    for index in 1..4 {
        let name = format!("org.test.p{index}");
        let id = host.register(manifest(&name), "b".repeat(64)).unwrap();
        for n in 0..16 {
            host.open_component_mount(
                id,
                &format!("{name}.editor"),
                resource("vault", n),
                &descriptor(&name),
            )
            .unwrap();
        }
    }
    assert!(
        host.open_component_mount(
            id,
            "org.test.a.editor",
            resource("vault", 44),
            &descriptor("org.test.a")
        )
        .is_err()
    );
    let name = "org.test.extra";
    let extra = host.register(manifest(name), "c".repeat(64)).unwrap();
    assert!(
        host.open_component_mount(
            extra,
            &format!("{name}.editor"),
            resource("vault", 1),
            &descriptor(name)
        )
        .is_err()
    );
    host.disable(id).unwrap();
    assert!(
        host.open_component_mount(
            extra,
            &format!("{name}.editor"),
            resource("vault", 1),
            &descriptor(name)
        )
        .is_err()
    );
    let ticket = host.pending_component_cleanup().pop().unwrap();
    host.confirm_component_released(&ticket).unwrap();
    host.open_component_mount(
        extra,
        &format!("{name}.editor"),
        resource("vault", 1),
        &descriptor(name),
    )
    .unwrap();
}
#[test]
fn entropy_is_required_and_cannot_be_reset_after_release() {
    let mut host = Host::new("vault", [Capability::NoteDraftEdit]).unwrap();
    let id = host
        .register(manifest("org.test.a"), "a".repeat(64))
        .unwrap();
    assert!(host.configure_component_identity([0; 16]).is_err());
    assert!(
        host.open_component_mount(
            id,
            "org.test.a.editor",
            resource("vault", 1),
            &descriptor("org.test.a")
        )
        .is_err()
    );
    host.configure_component_identity([1; 16]).unwrap();
    let mount = open(&mut host, id, 1);
    host.close_component_mount(&mount).unwrap();
    let ticket = host.pending_component_cleanup().pop().unwrap();
    host.confirm_component_released(&ticket).unwrap();
    assert!(host.configure_component_identity([1; 16]).is_err());
    assert!(host.configure_component_identity([2; 16]).is_err());
}

#[test]
fn old_grant_session_path_cannot_create_draft_permission() {
    let (mut host, id) = setup();
    assert!(
        host.replace_grants(
            id,
            vec![extension_host::Grant {
                capability: Capability::NoteDraftEdit,
                resource: resource("vault", 1)
            }]
        )
        .is_err()
    );
    assert!(host.enable(id).is_err());
    assert!(host.open_session(id).is_err());
    let mount = open(&mut host, id, 1);
    host.dispatch_draft(&mount, request(&mount), |_| Ok(()))
        .unwrap();
    assert!(host.open_session(id).is_err());
}
#[test]
fn ordinary_session_remains_independent_of_component_mounts() {
    let mut host = Host::new(
        "vault",
        [Capability::NoteDraftEdit, Capability::DocumentMetadataRead],
    )
    .unwrap();
    host.configure_component_identity([3; 16]).unwrap();
    let mut ordinary = manifest("org.test.reader");
    ordinary.host_api = ApiRange {
        major: 0,
        min_minor: 1,
        max_minor: 1,
    };
    ordinary.required_capabilities = vec![Capability::DocumentMetadataRead];
    ordinary.contributions = vec![];
    let reader = host.register(ordinary, "b".repeat(64)).unwrap();
    let document = Resource::Document {
        vault_id: "vault".into(),
        document_id: "1".into(),
    };
    host.replace_grants(
        reader,
        vec![extension_host::Grant {
            capability: Capability::DocumentMetadataRead,
            resource: document.clone(),
        }],
    )
    .unwrap();
    host.enable(reader).unwrap();
    let session = host.open_session(reader).unwrap();
    let editor = host
        .register(manifest("org.test.a"), "a".repeat(64))
        .unwrap();
    let mount = open(&mut host, editor, 1);
    let read = CapabilityRequest {
        api_version: HOST_API,
        request_id: "metadata".into(),
        capability: Capability::DocumentMetadataRead,
        resource: document,
    };
    host.authorize(&session, read.clone()).unwrap();
    host.close_component_mount(&mount).unwrap();
    host.authorize(&session, read).unwrap();
    assert!(
        host.authorize(
            &session,
            CapabilityRequest {
                api_version: HOST_API_V0_3,
                request_id: "draft".into(),
                capability: Capability::NoteDraftEdit,
                resource: resource("vault", 1)
            }
        )
        .is_err()
    );
}
