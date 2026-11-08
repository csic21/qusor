use extension_api::{composition::*, draft::*, *};
fn resource() -> Resource {
    Resource::Draft {
        vault_id: "v".repeat(128),
        draft_id: "a".repeat(32),
    }
}
fn manifest() -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "ordinary.editor".into(),
        publisher_id: "ordinary.author".into(),
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
            id: "ordinary.editor.input".into(),
            title: "Editor".into(),
            kind: ContributionKind::ComponentProvider,
            required_capabilities: vec![Capability::NoteDraftEdit],
        }],
    }
}
fn request() -> DraftRequest {
    DraftRequest {
        api_version: HOST_API_V0_3,
        draft_protocol: 1,
        request_id: "request".into(),
        resource: resource(),
        mount_id: "b".repeat(32),
        operation: DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 512,
        },
    }
}
#[test]
fn explicit_version_vocabulary_never_extends_legacy() {
    assert_eq!(HOST_API.minor, 1);
    let m = manifest();
    m.validate().unwrap();
    assert!(m.supports_api(HOST_API_V0_3));
    assert!(!m.supports_api(HOST_API_V0_2));
    for v in [HOST_API, HOST_API_V0_2] {
        assert!(!capability_available_in(Capability::NoteDraftEdit, v));
        assert!(!capability_resource_valid(
            Capability::NoteDraftEdit,
            &resource(),
            v
        ));
    }
    for c in [
        Capability::SelectionRead,
        Capability::DocumentMetadataRead,
        Capability::CapturePropose,
        Capability::ReaderSurface,
    ] {
        assert!(!capability_available_in(c, HOST_API_V0_3));
    }
    let mut old = m.clone();
    old.host_api.min_minor = 2;
    assert!(old.validate().is_err());
    let mut mixed = m;
    mixed.contributions[0].kind = ContributionKind::Command;
    assert!(!mixed.supports_api(HOST_API_V0_3));
    let mut r = request();
    r.api_version = HOST_API_V0_2;
    assert!(r.validate().is_err());
}
#[test]
fn draft_wire_budgets_unknown_fields_and_exact_identities() {
    let r = request();
    assert_eq!(
        DraftRequest::from_slice(r.to_json().unwrap().as_bytes()).unwrap(),
        r
    );
    for bad in ["", "a", &"A".repeat(32)] {
        let mut r = request();
        r.mount_id = bad.into();
        assert!(r.validate().is_err());
    }
    let mut value = serde_json::to_value(request()).unwrap();
    value["approved"] = true.into();
    assert!(DraftRequest::from_slice(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(DraftRequest::from_slice(&vec![b' '; MAX_DRAFT_REQUEST_BYTES + 1]).is_err());
    for op in [
        DraftOperation::ReadChunk {
            revision: Some(0),
            offset: 0,
            max_bytes: 512,
        },
        DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 513,
        },
        DraftOperation::BeginReplace {
            expected_revision: 1,
            total_bytes: 1048577,
        },
        DraftOperation::AppendChunk {
            upload_id: "a".repeat(32),
            offset: 1048576,
            text: "x".into(),
        },
    ] {
        assert!(op.validate().is_err());
    }
    let response = DraftResponse {
        api_version: HOST_API_V0_3,
        draft_protocol: 1,
        request_id: "a".repeat(32),
        draft_id: "a".repeat(32),
        mount_id: "b".repeat(32),
        outcome: DraftOutcome::Snapshot {
            revision: u64::MAX,
            offset: 0,
            total_bytes: 1048576,
            text: "\0".repeat(512),
        },
    };
    let bytes = response.to_json().unwrap();
    assert!(bytes.len() <= 4096);
    assert_eq!(
        DraftResponse::from_slice(bytes.as_bytes()).unwrap(),
        response
    );
    assert!(DraftResponse::from_slice(&vec![b' '; 4097]).is_err());
}
#[test]
fn signed_descriptor_is_bounded_declarative_not_authorization() {
    for view in [
        NativeEditorView::TextEditor {
            label: "Thought".into(),
            placeholder: "Write".into(),
        },
        NativeEditorView::ParagraphEditor {
            protocols: None,
            label: "Paragraphs".into(),
            placeholder: "Write".into(),
        },
    ] {
        let d = CompositionDescriptor {
            composition_protocol: 1,
            api_version: HOST_API_V0_3,
            contribution_id: "ordinary.editor.input".into(),
            role: ComponentRole::AnnotationPlainTextEditor,
            contract: ANNOTATION_EDITOR_CONTRACT.into(),
            view,
        };
        d.validate_for_manifest(&manifest()).unwrap();
        assert_eq!(
            CompositionDescriptor::from_slice(d.to_json().unwrap().as_bytes()).unwrap(),
            d
        );
        let mut bad = serde_json::to_value(&d).unwrap();
        bad["view"]["url"] = "https://example.com".into();
        assert!(CompositionDescriptor::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
        let mut bad = d;
        bad.contribution_id = "other.editor.input".into();
        assert!(bad.validate_for_manifest(&manifest()).is_err());
    }
}
#[test]
fn own_invocation_context_transports_real_input_in_public_chunks() {
    let request = ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "request".into(),
        operation: ContextOperation::ReadContext {},
    };
    assert_eq!(
        ComponentContextRequest::from_slice(request.to_json().unwrap().as_bytes()).unwrap(),
        request
    );
    let response = ComponentContextResponse {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "a".repeat(32),
        resource: resource(),
        mount_id: "b".repeat(32),
        sequence: u64::MAX,
        expected_revision: u64::MAX,
        outcome: ContextOutcome::InputChunk {
            offset: 0,
            total_bytes: 1048576,
            text: "\0".repeat(512),
        },
    };
    let bytes = response.to_json().unwrap();
    assert!(bytes.len() <= 4096);
    assert_eq!(
        ComponentContextResponse::from_slice(bytes.as_bytes()).unwrap(),
        response
    );
    let mut bad = serde_json::to_value(request).unwrap();
    bad["mount_id"] = "c".repeat(32).into();
    assert!(ComponentContextRequest::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn every_closed_operation_and_outcome_roundtrips() {
    let id = "c".repeat(32);
    for operation in [
        DraftOperation::BeginReplace {
            expected_revision: 1,
            total_bytes: 1048576,
        },
        DraftOperation::AppendChunk {
            upload_id: id.clone(),
            offset: 0,
            text: "中😀\0".into(),
        },
        DraftOperation::CommitReplace {
            upload_id: id.clone(),
        },
        DraftOperation::CancelReplace {
            upload_id: id.clone(),
        },
    ] {
        let mut r = request();
        r.operation = operation;
        assert_eq!(
            DraftRequest::from_slice(r.to_json().unwrap().as_bytes()).unwrap(),
            r
        );
        let mut bad = serde_json::to_value(r).unwrap();
        bad["operation"]["approved"] = true.into();
        assert!(DraftRequest::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for outcome in [
        DraftOutcome::UploadStarted {
            upload_id: id.clone(),
        },
        DraftOutcome::ChunkAccepted {
            upload_id: id.clone(),
            next_offset: 512,
        },
        DraftOutcome::Replaced { revision: 2 },
        DraftOutcome::Cancelled { upload_id: id },
    ] {
        let r = DraftResponse {
            api_version: HOST_API_V0_3,
            draft_protocol: 1,
            request_id: "request".into(),
            draft_id: "a".repeat(32),
            mount_id: "b".repeat(32),
            outcome,
        };
        assert_eq!(
            DraftResponse::from_slice(r.to_json().unwrap().as_bytes()).unwrap(),
            r
        );
    }
    let mut r = request();
    r.draft_protocol = 2;
    assert!(r.to_json().is_err());
    r = request();
    r.resource = Resource::Vault {
        vault_id: "vault".into(),
    };
    assert!(r.validate().is_err());
}

#[test]
fn legacy_requests_reject_new_resource_vocabulary_before_host_policy() {
    for api_version in [HOST_API, HOST_API_V0_2] {
        let request = CapabilityRequest {
            api_version,
            request_id: "request".into(),
            capability: Capability::DocumentMetadataRead,
            resource: resource(),
        };
        let raw = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            CapabilityRequest::from_slice(&raw).unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
        assert_eq!(
            request.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
}

#[test]
fn paragraph_input_is_typed_bounded_and_cannot_claim_approval() {
    let mut response = ComponentContextResponse {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "replace".into(),
        resource: resource(),
        mount_id: "b".repeat(32),
        sequence: 1,
        expected_revision: 1,
        outcome: ContextOutcome::Context {
            action: ComponentAction::ReplaceParagraph {
                index: 1048576,
                input_bytes: 1048576,
            },
        },
    };
    assert_eq!(
        ComponentContextResponse::from_slice(response.to_json().unwrap().as_bytes()).unwrap(),
        response
    );
    let mut raw = serde_json::to_value(&response).unwrap();
    raw["outcome"]["action"]["approved"] = true.into();
    assert!(ComponentContextResponse::from_slice(&serde_json::to_vec(&raw).unwrap()).is_err());
    response.outcome = ContextOutcome::Context {
        action: ComponentAction::ReplaceParagraph {
            index: 0,
            input_bytes: 1048577,
        },
    };
    assert!(response.to_json().is_err());
    response.outcome = ContextOutcome::Context {
        action: ComponentAction::ReplaceParagraph {
            index: 1048577,
            input_bytes: 0,
        },
    };
    assert!(response.to_json().is_err());
    response.outcome = ContextOutcome::Context {
        action: ComponentAction::ReplaceParagraph {
            index: 0,
            input_bytes: 0,
        },
    };
    assert!(response.to_json().is_ok());
}

#[test]
fn paragraph_protocol_declaration_is_explicit_closed_and_legacy_compatible() {
    let legacy = r#"{"composition_protocol":1,"api_version":{"major":0,"minor":3},"contribution_id":"ordinary.editor.input","role":"annotation.plain-text-editor","contract":"annotation.plain-text-editor/v1","view":{"kind":"paragraph_editor","label":"Paragraphs","placeholder":"Write"}}"#;
    let old = CompositionDescriptor::from_slice(legacy.as_bytes()).unwrap();
    assert!(!old.supports_paragraph_projection());
    assert_eq!(old.to_json().unwrap(), legacy);
    let mut v: serde_json::Value = serde_json::from_str(legacy).unwrap();
    v["view"]["protocols"] = serde_json::Value::Null;
    let null = CompositionDescriptor::from_slice(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(null, old);
    assert_eq!(null.to_json().unwrap(), legacy);
    v["view"]["protocols"] = serde_json::json!({"component_invocation":1,"paragraph_projection":1});
    let declared = CompositionDescriptor::from_slice(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(declared.supports_paragraph_projection());
    assert_eq!(
        CompositionDescriptor::from_slice(declared.to_json().unwrap().as_bytes()).unwrap(),
        declared
    );
    for protocols in [
        serde_json::json!({"component_invocation":1}),
        serde_json::json!({"paragraph_projection":1}),
        serde_json::json!({"component_invocation":1,"paragraph_projection":1,"approved":true}),
    ] {
        v["view"]["protocols"] = protocols;
        assert!(CompositionDescriptor::from_slice(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    for key in ["component_invocation", "paragraph_projection"] {
        for version in [0, 2, 65535] {
            v["view"]["protocols"] =
                serde_json::json!({"component_invocation":1,"paragraph_projection":1});
            v["view"]["protocols"][key] = version.into();
            assert_eq!(
                CompositionDescriptor::from_slice(&serde_json::to_vec(&v).unwrap())
                    .unwrap_err()
                    .code,
                ErrorCode::UnsupportedVersion
            );
        }
    }
}
