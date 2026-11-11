use extension_api::{
    ApiRange, ApiVersion, Capability, CapabilityRequest, ContributionKind, ErrorCode, HOST_API,
    HOST_API_V0_2, HOST_API_V0_5, Manifest, Resource, SUPPORTED_HOST_APIS, capability_available_in,
    capability_resource_valid,
    invocation::{INVOCATION_PROTOCOL, InvocationRequest},
    is_supported_api,
    surface::*,
    validate_supported_apis,
};
use serde_json::{Value, json};

fn source() -> SourceVersion {
    SourceVersion {
        original_sha256: "a".repeat(64),
        extraction_revision: "b".repeat(64),
    }
}
fn document() -> Resource {
    Resource::Document {
        vault_id: "vault-1".into(),
        document_id: "doc-1".into(),
    }
}
fn request(operation: SurfaceOperation) -> SurfaceRequest {
    SurfaceRequest {
        surface_protocol: SURFACE_PROTOCOL,
        api_version: HOST_API_V0_2,
        request_id: "command-1".into(),
        operation,
    }
}
fn open() -> SurfaceRequest {
    request(SurfaceOperation::Open {
        resource: document(),
        expected: source(),
        contribution_id: "org.example.reader.open".into(),
    })
}
fn descriptor() -> SurfaceDescriptor {
    SurfaceDescriptor {
        handle: "surface-1".into(),
        resource: document(),
        source: source(),
        generation: 1,
        page_index: 0,
        page_count: 2,
        zoom: 0.6,
        status: SurfaceStatus::Ready,
        frame: Some(FrameMetadata {
            frame_id: "frame-1".into(),
            pixel_width: 1152,
            pixel_height: 1488,
        }),
        selection_lease: Some("lease-1".into()),
    }
}
fn response(result: SurfaceResult) -> SurfaceResponse {
    SurfaceResponse {
        surface_protocol: SURFACE_PROTOCOL,
        api_version: HOST_API_V0_2,
        request_id: "command-1".into(),
        result,
    }
}
fn manifest() -> Manifest {
    Manifest::from_json(include_str!("../fixtures/manifest.json")).unwrap()
}
#[test]
fn explicit_support_set_and_manifest_vocabulary_prevent_silent_legacy_upgrade() {
    assert_eq!(HOST_API, ApiVersion { major: 0, minor: 1 });
    validate_supported_apis(&SUPPORTED_HOST_APIS).unwrap();
    assert!(is_supported_api(HOST_API_V0_5));
    for bad in [
        vec![],
        vec![HOST_API, HOST_API],
        vec![ApiVersion { major: 0, minor: 6 }],
    ] {
        assert_eq!(
            validate_supported_apis(&bad).unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
    let mut legacy = manifest();
    assert_eq!(legacy.negotiate_api(&SUPPORTED_HOST_APIS), Some(HOST_API));
    legacy.host_api = ApiRange {
        major: 0,
        min_minor: 1,
        max_minor: 2,
    };
    assert_eq!(
        legacy.negotiate_api(&[HOST_API_V0_2, HOST_API]),
        Some(HOST_API_V0_2)
    );
    legacy.optional_capabilities.push(Capability::ReaderSurface);
    assert_eq!(
        legacy.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    legacy.host_api.min_minor = 2;
    legacy.validate().unwrap();
    assert!(!legacy.supports_api(HOST_API));
    assert!(legacy.supports_api(HOST_API_V0_2));
    assert_eq!(legacy.negotiate_api(&[HOST_API]), None);
    legacy.contributions[0].kind = ContributionKind::ReaderWorkspace;
    assert_eq!(
        legacy.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    legacy.contributions[0]
        .required_capabilities
        .push(Capability::ReaderSurface);
    legacy.validate().unwrap();
}
#[test]
fn capability_scope_extension_is_explicitly_versioned() {
    assert!(!capability_available_in(
        Capability::ReaderSurface,
        HOST_API
    ));
    assert!(capability_available_in(
        Capability::ReaderSurface,
        HOST_API_V0_2
    ));
    assert!(!capability_resource_valid(
        Capability::SelectionRead,
        &document(),
        HOST_API
    ));
    assert!(capability_resource_valid(
        Capability::SelectionRead,
        &document(),
        HOST_API_V0_2
    ));
    assert!(capability_resource_valid(
        Capability::SelectionRead,
        &Resource::Selection {
            vault_id: "vault-1".into(),
            selection_id: "selected-1".into()
        },
        HOST_API
    ));
    assert!(!capability_resource_valid(
        Capability::ReaderSurface,
        &Resource::Vault {
            vault_id: "vault-1".into()
        },
        HOST_API_V0_2
    ));
    let mut policy = CapabilityRequest {
        api_version: HOST_API,
        request_id: "read-1".into(),
        capability: Capability::ReaderSurface,
        resource: document(),
    };
    assert_eq!(
        policy.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
    policy.api_version = HOST_API_V0_2;
    CapabilityRequest::from_slice(policy.to_json().unwrap().as_bytes()).unwrap();
}
#[test]
fn only_bounded_guest_operations_round_trip_and_native_claims_are_rejected() {
    for original in [
        open(),
        request(SurfaceOperation::Configure {
            handle: "surface-1".into(),
            generation: 1,
            page_index: 1,
            zoom: 0.7,
        }),
        request(SurfaceOperation::ReadSelection {
            handle: "surface-1".into(),
            generation: 1,
            selection_lease: "lease-1".into(),
        }),
        request(SurfaceOperation::Close {
            handle: "surface-1".into(),
        }),
    ] {
        assert_eq!(
            SurfaceRequest::from_json(&original.to_json().unwrap()).unwrap(),
            original
        );
    }
    for kind in ["input", "down", "up", "frame_ready", "approve"] {
        let mut value = serde_json::to_value(open()).unwrap();
        value["operation"] = json!({"kind":kind,"handle":"surface-1","generation":1});
        assert_eq!(
            SurfaceRequest::from_json(&value.to_string())
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    for field in [
        "actor",
        "installation_id",
        "grant_epoch",
        "native_ticket",
        "approved",
    ] {
        let mut value = serde_json::to_value(open()).unwrap();
        value[field] = json!(true);
        assert_eq!(
            SurfaceRequest::from_json(&value.to_string())
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    let text = open().to_json().unwrap();
    let duplicate = text.replacen(
        "\"kind\":\"open\"",
        "\"kind\":\"open\",\"kind\":\"open\"",
        1,
    );
    assert!(SurfaceRequest::from_json(&duplicate).is_err());
    let duplicate = text.replacen(
        "\"original_sha256\":",
        "\"original_sha256\":\"bad\",\"original_sha256\":",
        1,
    );
    assert!(SurfaceRequest::from_json(&duplicate).is_err());
    assert_eq!(
        SurfaceRequest::from_slice(&vec![b' '; MAX_SURFACE_BYTES + 1])
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
}
#[test]
fn source_scope_status_generation_and_geometry_are_checked_before_use() {
    let mut value = open();
    value.api_version = HOST_API;
    assert_eq!(
        value.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
    value = open();
    value.surface_protocol = 2;
    assert_eq!(
        value.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
    let mut version = source();
    version.original_sha256 = "A".repeat(64);
    assert!(version.validate().is_err());
    for zoom in [0.0, 65.0, f64::NAN, f64::INFINITY] {
        assert!(
            request(SurfaceOperation::Configure {
                handle: "surface-1".into(),
                generation: 1,
                page_index: 0,
                zoom
            })
            .validate()
            .is_err()
        );
    }
    assert!(
        request(SurfaceOperation::Configure {
            handle: "../path".into(),
            generation: 1,
            page_index: 0,
            zoom: 1.0
        })
        .validate()
        .is_err()
    );
    assert!(
        request(SurfaceOperation::ReadSelection {
            handle: "surface-1".into(),
            generation: 0,
            selection_lease: "lease-1".into()
        })
        .validate()
        .is_err()
    );
    let mut descriptor = descriptor();
    descriptor.validate().unwrap();
    descriptor.status = SurfaceStatus::Pending;
    assert!(descriptor.validate().is_err());
    descriptor.frame = None;
    descriptor.selection_lease = None;
    descriptor.validate().unwrap();
    descriptor.status = SurfaceStatus::Ready;
    assert!(descriptor.validate().is_err());
    descriptor.status = SurfaceStatus::Closed;
    descriptor.page_index = 2;
    assert!(descriptor.validate().is_err());
    let frame = FrameMetadata {
        frame_id: "frame-1".into(),
        pixel_width: MAX_FRAME_EDGE + 1,
        pixel_height: 1,
    };
    assert!(frame.validate().is_err());
}
#[test]
fn whole_selection_payload_is_preserved_and_full_envelope_budget_is_enforced() {
    let draft =
        r#"{"protocol_version":2,"exact":"line1\nline2","segments":[{"start":1},{"start":8}]}"#;
    let selection = response(SurfaceResult::Selection {
        handle: "surface-1".into(),
        generation: 1,
        selection_lease: "lease-1".into(),
        capture_protocol: 2,
        draft_json: draft.into(),
    });
    assert_eq!(
        SurfaceResponse::from_json(&selection.to_json().unwrap()).unwrap(),
        selection
    );
    let state = response(SurfaceResult::State {
        surface: descriptor(),
    });
    assert_eq!(
        SurfaceResponse::from_json(&state.to_json().unwrap()).unwrap(),
        state
    );
    let too_large = response(SurfaceResult::Selection {
        handle: "surface-1".into(),
        generation: 1,
        selection_lease: "lease-1".into(),
        capture_protocol: 2,
        draft_json: "x".repeat(MAX_SURFACE_BYTES - 1),
    });
    assert_eq!(
        too_large.to_json().unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
    let mut json: Value = serde_json::from_str(&selection.to_json().unwrap()).unwrap();
    json["result"]["capture_protocol"] = json!(3);
    assert_eq!(
        SurfaceResponse::from_json(&json.to_string())
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );
}
#[test]
fn invocation_query_has_no_guest_selected_context_identity_or_authority() {
    let query = InvocationRequest {
        invocation_protocol: INVOCATION_PROTOCOL,
        api_version: HOST_API_V0_2,
        request_id: "parameters-1".into(),
    };
    assert_eq!(
        InvocationRequest::from_json(&query.to_json().unwrap()).unwrap(),
        query
    );
    for field in [
        "invocation_id",
        "actor",
        "approved",
        "surface",
        "resource",
        "coordinates",
    ] {
        let mut json = serde_json::to_value(&query).unwrap();
        json[field] = json!("claimed");
        assert_eq!(
            InvocationRequest::from_json(&json.to_string())
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    let duplicate = query.to_json().unwrap().replacen(
        "\"request_id\":",
        "\"request_id\":\"other\",\"request_id\":",
        1,
    );
    assert!(InvocationRequest::from_json(&duplicate).is_err());
    let mut legacy = query;
    legacy.api_version = HOST_API;
    assert_eq!(
        legacy.to_json().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
}
