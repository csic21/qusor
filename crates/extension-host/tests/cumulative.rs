use extension_api::{
    ApiRange, ApiVersion, Capability, Contribution, ContributionKind, ErrorCode, HOST_API_V0_2,
    HOST_API_V0_4, HOST_API_V0_5, HOST_API_V0_6, HOST_API_V0_7, Manifest, PackageVersion, Resource,
    SUPPORTED_HOST_APIS,
    composition::{
        ANNOTATION_EDITOR_CONTRACT, COMPOSITION_PROTOCOL, ComponentRole, CompositionDescriptor,
        NativeEditorView,
    },
    continuation::{CONTINUATION_PROTOCOL, ContinuationOperation, ContinuationRequest},
    draft::{DRAFT_PROTOCOL, DraftOperation, DraftRequest},
    library::{LIBRARY_PROTOCOL, LibraryOperation, LibraryRequest},
    notes::{NOTES_PROTOCOL, NotesOperation, NotesRequest},
    surface::{
        FrameMetadata, SURFACE_PROTOCOL, SourceVersion, SurfaceOperation, SurfaceRequest,
        SurfaceResult,
    },
    vocab::{VOCAB_PROTOCOL, VocabOperation, VocabRating, VocabRequest},
};
use extension_host::{
    Grant, Host,
    surface::{TrustedSurfaceDocument, TrustedSurfaceSelection},
};

fn vault() -> Resource {
    Resource::Vault {
        vault_id: "vault".into(),
    }
}

fn doc(id: &str) -> Resource {
    Resource::Document {
        vault_id: "vault".into(),
        document_id: id.into(),
    }
}

fn cross_domain_manifest() -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.example.suite".into(),
        publisher_id: "org.example".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 6,
            max_minor: 7,
        },
        required_capabilities: vec![
            Capability::NotesRead,
            Capability::VocabRead,
            Capability::LibraryRead,
            Capability::CapturePropose,
            Capability::ReaderSurface,
            Capability::SelectionRead,
        ],
        optional_capabilities: vec![Capability::VocabReview],
        contributions: vec![
            Contribution {
                id: "org.example.suite.notes".into(),
                kind: ContributionKind::NotesWorkspace,
                title: "Notes".into(),
                required_capabilities: vec![Capability::NotesRead],
            },
            Contribution {
                id: "org.example.suite.vocab".into(),
                kind: ContributionKind::VocabWorkspace,
                title: "Vocab".into(),
                required_capabilities: vec![Capability::VocabRead],
            },
            Contribution {
                id: "org.example.suite.library".into(),
                kind: ContributionKind::LibraryWorkspace,
                title: "Library".into(),
                required_capabilities: vec![Capability::LibraryRead],
            },
            Contribution {
                id: "org.example.suite.reader".into(),
                kind: ContributionKind::ReaderWorkspace,
                title: "Reader".into(),
                required_capabilities: vec![Capability::ReaderSurface, Capability::SelectionRead],
            },
        ],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}

#[test]
fn cross_domain_package_opens_negotiated_session_and_authorizes_across_domains() {
    let mut host = Host::new(
        "vault",
        [
            Capability::NotesRead,
            Capability::VocabRead,
            Capability::VocabReview,
            Capability::LibraryRead,
            Capability::CapturePropose,
            Capability::ReaderSurface,
            Capability::SelectionRead,
        ],
    )
    .unwrap();
    host.configure_surface_identity([9; 16]).unwrap();
    let id = host
        .register(cross_domain_manifest(), "a".repeat(64))
        .unwrap();

    // Grant required capabilities + SelectionRead on doc-1, intentionally omitting VocabReview.
    host.replace_grants(
        id,
        vec![
            Grant {
                capability: Capability::NotesRead,
                resource: vault(),
            },
            Grant {
                capability: Capability::VocabRead,
                resource: vault(),
            },
            Grant {
                capability: Capability::LibraryRead,
                resource: vault(),
            },
            Grant {
                capability: Capability::CapturePropose,
                resource: vault(),
            },
            Grant {
                capability: Capability::ReaderSurface,
                resource: doc("doc-1"),
            },
            Grant {
                capability: Capability::SelectionRead,
                resource: doc("doc-1"),
            },
        ],
    )
    .unwrap();
    host.enable(id).unwrap();

    let session = host
        .open_negotiated_session(id, &SUPPORTED_HOST_APIS)
        .unwrap();
    assert_eq!(host.session_api(&session).unwrap(), HOST_API_V0_7);

    // 1. Notes on the same 0.7 negotiated session.
    let notes_req = NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "notes-1".into(),
        operation: NotesOperation::Page {
            cursor: None,
            limit: 4,
        },
    };
    let notes_permit = host.authorize_notes(&session, &notes_req).unwrap();
    host.dispatch(notes_permit, |ctx| {
        assert_eq!(ctx.request.capability, Capability::NotesRead);
        Ok(())
    })
    .unwrap();

    // Mismatched wire version on a 0.7 session still fails closed.
    let mut wrong_notes = notes_req;
    wrong_notes.api_version = HOST_API_V0_4;
    assert_eq!(
        host.authorize_notes(&session, &wrong_notes)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );

    // 2. Vocab read succeeds; optional ungranted VocabReview fails closed.
    let vocab_decks = VocabRequest {
        vocab_protocol: VOCAB_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "vocab-1".into(),
        operation: VocabOperation::Decks {},
    };
    let vocab_permit = host.authorize_vocab(&session, &vocab_decks).unwrap();
    host.dispatch(vocab_permit, |ctx| {
        assert_eq!(ctx.request.capability, Capability::VocabRead);
        Ok(())
    })
    .unwrap();

    let vocab_review = VocabRequest {
        vocab_protocol: VOCAB_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "vocab-review-1".into(),
        operation: VocabOperation::Review {
            card_id: 1,
            rating: VocabRating::Good,
        },
    };
    assert_eq!(
        host.authorize_vocab(&session, &vocab_review)
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );

    // 3. Library 0.7 operation on the same 0.7 session.
    let library_req = LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "lib-1".into(),
        operation: LibraryOperation::SavedSelections {
            document_id: 1,
            page_index: 0,
        },
    };
    let lib_permit = host.authorize_library(&session, &library_req).unwrap();
    host.dispatch(lib_permit, |ctx| {
        assert_eq!(ctx.request.capability, Capability::LibraryRead);
        Ok(())
    })
    .unwrap();

    // 4. Reader Surface opened and read on the same 0.7 session.
    let source = SourceVersion {
        original_sha256: "b".repeat(64),
        extraction_revision: "c".repeat(64),
    };
    let open_req = SurfaceRequest {
        surface_protocol: SURFACE_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "surface-open".into(),
        operation: SurfaceOperation::Open {
            resource: doc("doc-1"),
            expected: source.clone(),
            contribution_id: "org.example.suite.reader".into(),
        },
    };
    let opened = host
        .open_surface(
            &session,
            &open_req,
            TrustedSurfaceDocument {
                resource: doc("doc-1"),
                source: source.clone(),
                page_count: 2,
            },
        )
        .unwrap();
    host.native_frame_ready(
        &opened.frame_ticket,
        FrameMetadata {
            frame_id: opened.frame_ticket.frame_id().into(),
            pixel_width: 800,
            pixel_height: 600,
        },
    )
    .unwrap();
    let input = host.native_input_ticket(&opened.frame_ticket).unwrap();
    let gesture = host.surface_begin_gesture(&input).unwrap();
    let lease = host
        .publish_surface_selection(
            &gesture,
            TrustedSurfaceSelection {
                source,
                capture_protocol: 1,
                draft_json: r#"{"protocol_version":1,"exact":"hello"}"#.into(),
            },
        )
        .unwrap();
    let read_req = SurfaceRequest {
        surface_protocol: SURFACE_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "surface-read".into(),
        operation: SurfaceOperation::ReadSelection {
            handle: opened.descriptor.handle,
            generation: opened.descriptor.generation,
            selection_lease: lease,
        },
    };
    let surface_resp = host.read_surface_selection(&session, &read_req).unwrap();
    assert_eq!(surface_resp.api_version, HOST_API_V0_7);
    assert!(matches!(
        surface_resp.result,
        SurfaceResult::Selection { .. }
    ));

    // Confirm surface read still fails closed if guest asks with wrong version.
    let mut wrong_surface = read_req;
    wrong_surface.api_version = HOST_API_V0_2;
    assert_eq!(
        host.read_surface_selection(&session, &wrong_surface)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn legacy_packages_keep_old_session_behavior_and_reject_unupgraded_versions() {
    let mut host = Host::new(
        "vault",
        [
            Capability::ReaderSurface,
            Capability::NotesRead,
            Capability::VocabRead,
            Capability::LibraryRead,
        ],
    )
    .unwrap();
    host.configure_surface_identity([7; 16]).unwrap();

    let notes_manifest = Manifest {
        manifest_version: 1,
        package_id: "com.qusor.notes".into(),
        publisher_id: "com.qusor".into(),
        package_version: PackageVersion {
            major: 0,
            minor: 1,
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
            id: "com.qusor.notes.workspace".into(),
            kind: ContributionKind::NotesWorkspace,
            title: "Notes".into(),
            required_capabilities: vec![Capability::NotesRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    };
    let notes_id = host.register(notes_manifest, "c".repeat(64)).unwrap();
    host.replace_grants(
        notes_id,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: vault(),
        }],
    )
    .unwrap();
    host.enable(notes_id).unwrap();

    let notes_session = host
        .open_negotiated_session(notes_id, &SUPPORTED_HOST_APIS)
        .unwrap();
    assert_eq!(host.session_api(&notes_session).unwrap(), HOST_API_V0_4);
    assert_eq!(
        host.open_session_for_api(notes_id, HOST_API_V0_5)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
    host.authorize_notes(
        &notes_session,
        &NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version: HOST_API_V0_4,
            request_id: "legacy-note".into(),
            operation: NotesOperation::Page {
                cursor: None,
                limit: 5,
            },
        },
    )
    .unwrap();

    let vocab_manifest = Manifest {
        manifest_version: 1,
        package_id: "com.qusor.vocab".into(),
        publisher_id: "com.qusor".into(),
        package_version: PackageVersion {
            major: 0,
            minor: 1,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 5,
            max_minor: 5,
        },
        required_capabilities: vec![Capability::VocabRead],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "com.qusor.vocab.workspace".into(),
            kind: ContributionKind::VocabWorkspace,
            title: "Vocab".into(),
            required_capabilities: vec![Capability::VocabRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    };
    let vocab_id = host.register(vocab_manifest, "d".repeat(64)).unwrap();
    host.replace_grants(
        vocab_id,
        vec![Grant {
            capability: Capability::VocabRead,
            resource: vault(),
        }],
    )
    .unwrap();
    host.enable(vocab_id).unwrap();
    let vocab_session = host
        .open_negotiated_session(vocab_id, &SUPPORTED_HOST_APIS)
        .unwrap();
    assert_eq!(host.session_api(&vocab_session).unwrap(), HOST_API_V0_5);
    host.authorize_vocab(
        &vocab_session,
        &VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: HOST_API_V0_5,
            request_id: "legacy-vocab".into(),
            operation: VocabOperation::Decks {},
        },
    )
    .unwrap();

    // Negotiating against an incompatible support slice fails closed.
    assert_eq!(
        host.open_negotiated_session(notes_id, &[HOST_API_V0_2, HOST_API_V0_6])
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn component_provider_works_above_api_0_3_and_rejects_unsupported() {
    let mut host = Host::new("vault", [Capability::NoteDraftEdit]).unwrap();
    host.configure_component_identity([5; 16]).unwrap();

    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.example.editor".into(),
        publisher_id: "org.example".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 3,
            max_minor: 7,
        },
        required_capabilities: vec![Capability::NoteDraftEdit],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.example.editor.mount".into(),
            kind: ContributionKind::ComponentProvider,
            title: "Editor".into(),
            required_capabilities: vec![Capability::NoteDraftEdit],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    };
    let id = host.register(manifest, "e".repeat(64)).unwrap();
    let descriptor = CompositionDescriptor {
        composition_protocol: COMPOSITION_PROTOCOL,
        api_version: HOST_API_V0_7,
        contribution_id: "org.example.editor.mount".into(),
        role: ComponentRole::AnnotationPlainTextEditor,
        contract: ANNOTATION_EDITOR_CONTRACT.into(),
        view: NativeEditorView::TextEditor {
            label: "Editor".into(),
            placeholder: "Write...".into(),
        },
    };
    let draft_resource = Resource::Draft {
        vault_id: "vault".into(),
        draft_id: "f".repeat(32),
    };
    let mount = host
        .open_component_mount(
            id,
            "org.example.editor.mount",
            draft_resource.clone(),
            &descriptor,
        )
        .unwrap();

    let draft_req = DraftRequest {
        api_version: HOST_API_V0_7,
        draft_protocol: DRAFT_PROTOCOL,
        request_id: "dr-1".into(),
        resource: draft_resource.clone(),
        mount_id: mount.mount_id().into(),
        operation: DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 64,
        },
    };
    host.dispatch_draft(&mount, draft_req, |_| Ok(())).unwrap();

    let cont_req = ContinuationRequest {
        api_version: HOST_API_V0_7,
        continuation_protocol: CONTINUATION_PROTOCOL,
        request_id: "ct-1".into(),
        resource: draft_resource,
        mount_id: mount.mount_id().into(),
        sequence: 1,
        base_revision: 1,
        operation: ContinuationOperation::ReadMetadata {},
    };
    host.dispatch_continuation(&mount, cont_req, |_| Ok(()))
        .unwrap();

    // Unsupported version fails closed.
    let bad_cont = ContinuationRequest {
        api_version: ApiVersion { major: 0, minor: 9 },
        continuation_protocol: CONTINUATION_PROTOCOL,
        request_id: "ct-bad".into(),
        resource: Resource::Draft {
            vault_id: "vault".into(),
            draft_id: "f".repeat(32),
        },
        mount_id: mount.mount_id().into(),
        sequence: 1,
        base_revision: 1,
        operation: ContinuationOperation::ReadMetadata {},
    };
    assert_eq!(
        host.dispatch_continuation(&mount, bad_cont, |_| Ok(()))
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
}
