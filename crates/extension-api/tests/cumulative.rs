use extension_api::{
    ApiRange, ApiVersion, Capability, CapabilityRequest, Contribution, ContributionKind, ErrorCode,
    HOST_API, HOST_API_V0_2, HOST_API_V0_3, HOST_API_V0_4, HOST_API_V0_5, HOST_API_V0_6,
    HOST_API_V0_7, HOST_API_V0_8, Manifest, PackageVersion, Resource, SUPPORTED_HOST_APIS,
    capability_available_in, capability_resource_valid,
    draft::{DRAFT_PROTOCOL, DraftOperation, DraftRequest},
    library::{LIBRARY_PROTOCOL, LibraryOperation, LibraryRequest},
    notes::{NOTES_PROTOCOL, NotesOperation, NotesRequest},
    surface::{SURFACE_PROTOCOL, SourceVersion, SurfaceOperation, SurfaceRequest},
    vocab::{VOCAB_PROTOCOL, VocabOperation, VocabRequest},
};

fn base_manifest(min_minor: u16, max_minor: u16) -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.example.cross".into(),
        publisher_id: "org.example".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor,
            max_minor,
        },
        required_capabilities: vec![],
        optional_capabilities: vec![],
        contributions: vec![],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}

#[test]
fn capabilities_and_contributions_are_cumulatively_available_from_intro_minor() {
    let intro_minor = [
        (Capability::SelectionRead, 1),
        (Capability::DocumentMetadataRead, 1),
        (Capability::CapturePropose, 1),
        (Capability::ReaderSurface, 2),
        (Capability::NoteDraftEdit, 3),
        (Capability::NotesRead, 4),
        (Capability::VocabRead, 5),
        (Capability::VocabReview, 5),
        (Capability::LibraryRead, 6),
        (Capability::UiSlot, 7),
    ];

    for minor in 1..=8 {
        let version = ApiVersion { major: 0, minor };
        for &(capability, min_intro) in &intro_minor {
            assert_eq!(
                capability_available_in(capability, version),
                minor >= min_intro,
                "capability {capability:?} availability mismatch at 0.{minor}"
            );
        }
    }

    for bad in [
        ApiVersion { major: 0, minor: 0 },
        ApiVersion { major: 0, minor: 9 },
        ApiVersion { major: 1, minor: 0 },
    ] {
        for &(capability, _) in &intro_minor {
            assert!(!capability_available_in(capability, bad));
        }
    }

    let doc = Resource::Document {
        vault_id: "vault".into(),
        document_id: "doc-1".into(),
    };
    assert!(!capability_resource_valid(
        Capability::SelectionRead,
        &doc,
        HOST_API
    ));
    for minor in 2..=8 {
        assert!(capability_resource_valid(
            Capability::SelectionRead,
            &doc,
            ApiVersion { major: 0, minor }
        ));
    }

    let kind_intro = [
        (ContributionKind::Command, Capability::SelectionRead, 1),
        (
            ContributionKind::SelectionAction,
            Capability::SelectionRead,
            1,
        ),
        (
            ContributionKind::ReaderWorkspace,
            Capability::ReaderSurface,
            2,
        ),
        (
            ContributionKind::ComponentProvider,
            Capability::NoteDraftEdit,
            3,
        ),
        (ContributionKind::NotesWorkspace, Capability::NotesRead, 4),
        (ContributionKind::VocabWorkspace, Capability::VocabRead, 5),
        (
            ContributionKind::LibraryWorkspace,
            Capability::LibraryRead,
            6,
        ),
        (ContributionKind::UiSlot, Capability::UiSlot, 7),
    ];

    for (index, (kind, cap, min_intro)) in kind_intro.iter().enumerate() {
        let mut manifest = base_manifest(*min_intro, 8);
        manifest.required_capabilities = vec![*cap];
        manifest.contributions = vec![Contribution {
            id: format!("org.example.cross.c{index}"),
            kind: *kind,
            title: "Contribution".into(),
            required_capabilities: vec![*cap],
        }];
        manifest.validate().unwrap();
        for minor in 1..=8 {
            let v = ApiVersion { major: 0, minor };
            assert_eq!(manifest.supports_api(v), minor >= *min_intro);
        }
    }
}

#[test]
fn cross_domain_manifest_validates_and_negotiates_highest_common_version() {
    let mut full = base_manifest(7, 8);
    full.required_capabilities = vec![
        Capability::SelectionRead,
        Capability::DocumentMetadataRead,
        Capability::CapturePropose,
        Capability::ReaderSurface,
        Capability::NoteDraftEdit,
        Capability::NotesRead,
        Capability::VocabRead,
        Capability::VocabReview,
        Capability::LibraryRead,
        Capability::UiSlot,
    ];
    full.contributions = vec![
        Contribution {
            id: "org.example.cross.reader".into(),
            kind: ContributionKind::ReaderWorkspace,
            title: "Reader".into(),
            required_capabilities: vec![Capability::ReaderSurface, Capability::SelectionRead],
        },
        Contribution {
            id: "org.example.cross.notes".into(),
            kind: ContributionKind::NotesWorkspace,
            title: "Notes".into(),
            required_capabilities: vec![Capability::NotesRead],
        },
        Contribution {
            id: "org.example.cross.vocab".into(),
            kind: ContributionKind::VocabWorkspace,
            title: "Vocab".into(),
            required_capabilities: vec![Capability::VocabRead, Capability::VocabReview],
        },
        Contribution {
            id: "org.example.cross.library".into(),
            kind: ContributionKind::LibraryWorkspace,
            title: "Library".into(),
            required_capabilities: vec![Capability::LibraryRead, Capability::CapturePropose],
        },
        Contribution {
            id: "org.example.cross.slot".into(),
            kind: ContributionKind::UiSlot,
            title: "Slot".into(),
            required_capabilities: vec![Capability::UiSlot],
        },
        Contribution {
            id: "org.example.cross.cmd".into(),
            kind: ContributionKind::Command,
            title: "Quick Command".into(),
            required_capabilities: vec![],
        },
    ];
    full.validate().unwrap();
    assert!(!full.supports_api(HOST_API_V0_6));
    assert!(full.supports_api(HOST_API_V0_7));
    assert!(full.supports_api(HOST_API_V0_8));
    assert_eq!(
        full.negotiate_api(&SUPPORTED_HOST_APIS),
        Some(HOST_API_V0_8)
    );
    assert_eq!(
        full.negotiate_api(&[HOST_API_V0_6, HOST_API_V0_7]),
        Some(HOST_API_V0_7)
    );

    // Notes + Vocab across 0.5..=0.8 without forcing library.read.
    let mut notes_and_vocab = base_manifest(5, 8);
    notes_and_vocab.required_capabilities = vec![Capability::NotesRead, Capability::VocabRead];
    notes_and_vocab.validate().unwrap();
    assert_eq!(
        notes_and_vocab.negotiate_api(&SUPPORTED_HOST_APIS),
        Some(HOST_API_V0_8)
    );

    // Declaring a capability whose introduction minor exceeds min_minor still fails closed.
    let mut too_low = base_manifest(4, 7);
    too_low.required_capabilities = vec![Capability::NotesRead, Capability::VocabRead];
    assert_eq!(
        too_low.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}

#[test]
fn legacy_manifests_keep_exact_bounds_and_compatibility() {
    let legacy_manifests = [
        (
            r#"{
              "manifest_version": 1,
              "package_id": "com.qusor.reader",
              "publisher_id": "com.qusor",
              "package_version": {"major": 0, "minor": 1, "patch": 0},
              "host_api": {"major": 0, "min_minor": 2, "max_minor": 2},
              "required_capabilities": ["reader.surface"],
              "optional_capabilities": ["selection.read"],
              "contributions": [{
                "id": "com.qusor.reader.workspace",
                "kind": "reader_workspace",
                "title": "Reader",
                "required_capabilities": ["reader.surface"]
              }]
            }"#,
            HOST_API_V0_2,
            HOST_API_V0_3,
        ),
        (
            r#"{
              "manifest_version": 1,
              "package_id": "com.qusor.notes",
              "publisher_id": "com.qusor",
              "package_version": {"major": 0, "minor": 1, "patch": 0},
              "host_api": {"major": 0, "min_minor": 4, "max_minor": 4},
              "required_capabilities": ["notes.read"],
              "optional_capabilities": [],
              "contributions": [{
                "id": "com.qusor.notes.workspace",
                "kind": "notes_workspace",
                "title": "Notes",
                "required_capabilities": ["notes.read"]
              }]
            }"#,
            HOST_API_V0_4,
            HOST_API_V0_5,
        ),
        (
            r#"{
              "manifest_version": 1,
              "package_id": "com.qusor.vocab",
              "publisher_id": "com.qusor",
              "package_version": {"major": 0, "minor": 1, "patch": 0},
              "host_api": {"major": 0, "min_minor": 5, "max_minor": 5},
              "required_capabilities": ["vocab.read", "vocab.review"],
              "optional_capabilities": [],
              "contributions": [{
                "id": "com.qusor.vocab.workspace",
                "kind": "vocab_workspace",
                "title": "Vocabulary",
                "required_capabilities": ["vocab.read"]
              }]
            }"#,
            HOST_API_V0_5,
            HOST_API_V0_6,
        ),
        (
            r#"{
              "manifest_version": 1,
              "package_id": "com.qusor.library",
              "publisher_id": "com.qusor",
              "package_version": {"major": 0, "minor": 1, "patch": 0},
              "host_api": {"major": 0, "min_minor": 7, "max_minor": 7},
              "required_capabilities": ["library.read", "capture.propose"],
              "optional_capabilities": [],
              "contributions": [{
                "id": "com.qusor.library.workspace",
                "kind": "library_workspace",
                "title": "Library",
                "required_capabilities": ["library.read"]
              }]
            }"#,
            HOST_API_V0_7,
            HOST_API_V0_6,
        ),
    ];

    for (raw, expected, outside) in legacy_manifests {
        let parsed = Manifest::from_json(raw).unwrap();
        assert_eq!(parsed.negotiate_api(&SUPPORTED_HOST_APIS), Some(expected));
        assert!(parsed.supports_api(expected));
        assert!(!parsed.supports_api(outside));
        assert_eq!(
            Manifest::from_json(&parsed.to_json().unwrap()).unwrap(),
            parsed
        );
    }
}

#[test]
fn unknown_capabilities_contributions_and_versions_fail_closed() {
    let unknown_cap = r#"{
      "manifest_version": 1,
      "package_id": "org.example.bad",
      "publisher_id": "org.example",
      "package_version": {"major": 1, "minor": 0, "patch": 0},
      "host_api": {"major": 0, "min_minor": 7, "max_minor": 7},
      "required_capabilities": ["ai.magic"],
      "optional_capabilities": [],
      "contributions": []
    }"#;
    assert_eq!(
        Manifest::from_json(unknown_cap).unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    let unknown_kind = r#"{
      "manifest_version": 1,
      "package_id": "org.example.bad",
      "publisher_id": "org.example",
      "package_version": {"major": 1, "minor": 0, "patch": 0},
      "host_api": {"major": 0, "min_minor": 7, "max_minor": 7},
      "required_capabilities": ["notes.read"],
      "optional_capabilities": [],
      "contributions": [{
        "id": "org.example.bad.panel",
        "kind": "custom_panel",
        "title": "Bad",
        "required_capabilities": ["notes.read"]
      }]
    }"#;
    assert_eq!(
        Manifest::from_json(unknown_kind).unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    for bad in [
        ApiVersion { major: 0, minor: 0 },
        ApiVersion { major: 0, minor: 9 },
        ApiVersion { major: 1, minor: 0 },
    ] {
        let req = CapabilityRequest {
            api_version: bad,
            request_id: "r1".into(),
            capability: Capability::NotesRead,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        };
        assert_eq!(
            req.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }

    // Typed sub-protocols accept cumulative versions >= intro and reject < intro or > 0.8.
    for minor in 4..=8 {
        NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version: ApiVersion { major: 0, minor },
            request_id: "n1".into(),
            operation: NotesOperation::Page {
                cursor: None,
                limit: 4,
            },
        }
        .validate()
        .unwrap();
    }
    assert_eq!(
        NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version: HOST_API_V0_3,
            request_id: "n0".into(),
            operation: NotesOperation::Page {
                cursor: None,
                limit: 4,
            },
        }
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::UnsupportedVersion
    );

    for minor in 5..=8 {
        VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: ApiVersion { major: 0, minor },
            request_id: "v1".into(),
            operation: VocabOperation::Decks {},
        }
        .validate()
        .unwrap();
    }
    assert_eq!(
        VocabRequest {
            vocab_protocol: VOCAB_PROTOCOL,
            api_version: HOST_API_V0_4,
            request_id: "v0".into(),
            operation: VocabOperation::Decks {},
        }
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::UnsupportedVersion
    );

    for minor in 6..=8 {
        LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: ApiVersion { major: 0, minor },
            request_id: "l1".into(),
            operation: LibraryOperation::Page {
                cursor: None,
                limit: 4,
            },
        }
        .validate()
        .unwrap();
    }
    assert_eq!(
        LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_6,
            request_id: "l_saved".into(),
            operation: LibraryOperation::SavedSelections {
                document_id: 1,
                page_index: 0,
            },
        }
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::UnsupportedVersion
    );

    for minor in 2..=8 {
        SurfaceRequest {
            surface_protocol: SURFACE_PROTOCOL,
            api_version: ApiVersion { major: 0, minor },
            request_id: "s1".into(),
            operation: SurfaceOperation::Open {
                resource: Resource::Document {
                    vault_id: "vault".into(),
                    document_id: "doc".into(),
                },
                expected: SourceVersion {
                    original_sha256: "a".repeat(64),
                    extraction_revision: "b".repeat(64),
                },
                contribution_id: "org.example.cross.reader".into(),
            },
        }
        .validate()
        .unwrap();
    }

    for minor in 3..=8 {
        DraftRequest {
            api_version: ApiVersion { major: 0, minor },
            draft_protocol: DRAFT_PROTOCOL,
            request_id: "d1".into(),
            resource: Resource::Draft {
                vault_id: "vault".into(),
                draft_id: "a".repeat(32),
            },
            mount_id: "b".repeat(32),
            operation: DraftOperation::ReadChunk {
                revision: None,
                offset: 0,
                max_bytes: 64,
            },
        }
        .validate()
        .unwrap();
    }
}
