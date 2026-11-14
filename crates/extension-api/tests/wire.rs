use extension_api::{
    ApiError, ApiRange, ApiVersion, Capability, CapabilityRequest, ErrorCode, HOST_API,
    MAX_CONTRIBUTIONS, MAX_ERROR_MESSAGE_BYTES, MAX_ID_BYTES, MAX_MANIFEST_BYTES,
    MAX_REQUEST_BYTES, Manifest, PackageVersion, Resource,
};
use serde_json::{Value, json};

const MANIFEST: &str = include_str!("../fixtures/manifest.json");
const REQUEST: &str = include_str!("../fixtures/request.json");
const ERROR: &str = include_str!("../fixtures/error.json");

fn manifest() -> Manifest {
    Manifest::from_json(MANIFEST).unwrap()
}

fn request() -> CapabilityRequest {
    CapabilityRequest::from_json(REQUEST).unwrap()
}

fn manifest_value() -> Value {
    serde_json::from_str(MANIFEST).unwrap()
}

fn request_value() -> Value {
    serde_json::from_str(REQUEST).unwrap()
}

fn assert_invalid_manifest(value: &Value) {
    assert_eq!(
        Manifest::from_json(&value.to_string()).unwrap_err().code,
        ErrorCode::InvalidManifest,
        "accepted invalid manifest: {value}"
    );
}

fn assert_invalid_request(value: &Value) {
    assert_eq!(
        CapabilityRequest::from_json(&value.to_string())
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest,
        "accepted invalid request: {value}"
    );
}

#[test]
fn published_fixtures_round_trip_through_validated_wire_entries() {
    let manifest = manifest();
    let request = request();
    let error = ApiError::from_json(ERROR).unwrap();
    assert_eq!(
        Manifest::from_json(&manifest.to_json().unwrap()).unwrap(),
        manifest
    );
    assert_eq!(
        CapabilityRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        ApiError::from_json(&error.to_json().unwrap()).unwrap(),
        error
    );
    assert_eq!(request.capability, Capability::SelectionRead);
    assert!(manifest.host_api.supports(HOST_API));
    assert_eq!(request.resource.vault_id(), "vault-001");
}

#[test]
fn api_range_has_inclusive_minor_bounds_and_never_crosses_major_versions() {
    let range = ApiRange {
        major: 0,
        min_minor: 1,
        max_minor: 3,
    };
    assert!(range.supports(ApiVersion { major: 0, minor: 1 }));
    assert!(range.supports(ApiVersion { major: 0, minor: 3 }));
    assert!(!range.supports(ApiVersion { major: 0, minor: 0 }));
    assert!(!range.supports(ApiVersion { major: 0, minor: 4 }));
    assert!(!range.supports(ApiVersion { major: 1, minor: 2 }));
    let inverted = ApiRange {
        major: 0,
        min_minor: 3,
        max_minor: 1,
    };
    assert!(!inverted.supports(ApiVersion { major: 0, minor: 2 }));
}

#[test]
fn manifest_parsing_keeps_host_compatibility_separate_from_structure() {
    let mut manifest = manifest();
    manifest.host_api = ApiRange {
        major: 7,
        min_minor: 2,
        max_minor: 3,
    };
    assert!(manifest.validate().is_ok());
    assert!(!manifest.host_api.supports(HOST_API));
    manifest.host_api.min_minor = 4;
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest.manifest_version = 2;
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn request_requires_the_implemented_protocol_version() {
    for minor in 1..=7 {
        let mut request = request();
        request.api_version = ApiVersion { major: 0, minor };
        request.validate().unwrap();
    }
    for version in [
        ApiVersion { major: 0, minor: 0 },
        ApiVersion { major: 0, minor: 8 },
        ApiVersion { major: 1, minor: 1 },
    ] {
        let mut request = request();
        request.api_version = version;
        assert_eq!(
            request.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
        assert_eq!(
            request.to_json().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
}

#[test]
fn package_version_orders_numeric_components_without_loss() {
    let earlier = PackageVersion {
        major: 1,
        minor: 2,
        patch: u32::MAX,
    };
    let later = PackageVersion {
        major: 1,
        minor: 10,
        patch: 0,
    };
    assert!(earlier < later);
    let next_major = PackageVersion {
        major: 2,
        minor: 0,
        patch: 0,
    };
    assert!(later < next_major);
    let version = PackageVersion {
        major: u32::MAX,
        minor: u32::MAX,
        patch: u32::MAX,
    };
    assert_eq!(
        serde_json::from_str::<PackageVersion>(&serde_json::to_string(&version).unwrap()).unwrap(),
        version
    );
}

#[test]
fn manifest_rejects_unknown_fields_at_every_object_level() {
    for pointer in ["", "/package_version", "/host_api", "/contributions/0"] {
        let mut value = manifest_value();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert_invalid_manifest(&value);
    }
}

#[test]
fn request_rejects_unknown_fields_and_self_reported_identity() {
    for pointer in ["", "/api_version", "/resource"] {
        let mut value = request_value();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert_invalid_request(&value);
    }
    for identity in ["actor", "package_id", "runtime_session_id", "grant_epoch"] {
        let mut value = request_value();
        value[identity] = json!("self-reported");
        assert_invalid_request(&value);
    }
}

#[test]
fn unknown_capabilities_and_contribution_kinds_are_rejected() {
    for field in ["required_capabilities", "optional_capabilities"] {
        let mut value = manifest_value();
        value[field] = json!(["filesystem.all"]);
        assert_invalid_manifest(&value);
    }
    let mut value = manifest_value();
    value["contributions"][0]["required_capabilities"] = json!(["filesystem.all"]);
    assert_invalid_manifest(&value);
    value = manifest_value();
    value["contributions"][0]["kind"] = json!("native_window");
    assert_invalid_manifest(&value);
    let mut value = request_value();
    value["capability"] = json!("filesystem.all");
    assert_invalid_request(&value);
}

#[test]
fn duplicates_are_rejected_in_json_objects_and_declarations() {
    let duplicate = MANIFEST.replacen(
        "\"manifest_version\": 1",
        "\"manifest_version\": 1, \"manifest_version\": 1",
        1,
    );
    assert_eq!(
        Manifest::from_json(&duplicate).unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    let duplicate = REQUEST.replacen(
        "\"vault_id\": \"vault-001\"",
        "\"vault_id\": \"vault-001\", \"vault_id\": \"vault-002\"",
        1,
    );
    assert_eq!(
        CapabilityRequest::from_json(&duplicate).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let duplicate = REQUEST.replacen(
        "\"kind\": \"selection\"",
        "\"kind\": \"selection\", \"kind\": \"selection\"",
        1,
    );
    assert_eq!(
        CapabilityRequest::from_json(&duplicate).unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let mut manifest = manifest();
    manifest
        .required_capabilities
        .push(Capability::SelectionRead);
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest.required_capabilities.pop();
    manifest
        .optional_capabilities
        .push(Capability::SelectionRead);
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest.optional_capabilities.pop();
    manifest.contributions[0]
        .required_capabilities
        .push(Capability::SelectionRead);
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest.contributions[0].required_capabilities.pop();
    manifest
        .contributions
        .push(manifest.contributions[0].clone());
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}

#[test]
fn contributions_cannot_require_undeclared_capabilities() {
    let mut manifest = manifest();
    manifest.contributions[0].required_capabilities = vec![Capability::DocumentMetadataRead];
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest
        .optional_capabilities
        .push(Capability::DocumentMetadataRead);
    assert!(manifest.validate().is_ok());
}

#[test]
fn namespace_containment_uses_a_segment_boundary() {
    let mut manifest = manifest();
    for id in [
        "org.other.inspect",
        "org.example.selection-tools",
        "org.example.selection-tools-extra.inspect",
        "org.example.selection-tools..inspect",
        "org.example.selection-tools.0inspect",
        "org.example.selection-tools.Inspect",
        "org.example.selection-tools.inspect/../other",
    ] {
        manifest.contributions[0].id = id.into();
        assert_eq!(
            manifest.validate().unwrap_err().code,
            ErrorCode::InvalidManifest,
            "{id}"
        );
    }
    manifest.contributions[0].id = "org.example.selection-tools.inspect.detail-2".into();
    assert!(manifest.validate().is_ok());
}

#[test]
fn namespace_fields_are_bounded_ascii_and_not_display_names() {
    for bad in [
        "".into(),
        "org".into(),
        "Org.example".into(),
        "org.示例".into(),
        "org..example".into(),
        format!("org.{}", "x".repeat(MAX_ID_BYTES)),
    ] {
        let mut value = manifest_value();
        value["package_id"] = json!(bad);
        assert_invalid_manifest(&value);
        let mut value = manifest_value();
        value["publisher_id"] = json!(bad);
        assert_invalid_manifest(&value);
    }
    let mut manifest = manifest();
    manifest.publisher_id = format!("org.{}", "x".repeat(MAX_ID_BYTES - 4));
    assert!(manifest.validate().is_ok());
}

#[test]
fn display_title_limits_count_utf8_bytes_and_reject_blank_or_control_text() {
    let mut manifest = manifest();
    manifest.contributions[0].title = format!("{}a", "界".repeat(85));
    assert_eq!(manifest.contributions[0].title.len(), 256);
    assert!(manifest.validate().is_ok());
    manifest.contributions[0].title.push('b');
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    for title in [
        "",
        " \t",
        "\u{3000}",
        "line\nbreak",
        "nul\0",
        "delete\u{7f}",
    ] {
        manifest.contributions[0].title = title.into();
        assert_eq!(
            manifest.validate().unwrap_err().code,
            ErrorCode::InvalidManifest
        );
    }
}

#[test]
fn contribution_and_capability_list_limits_apply_to_in_memory_values() {
    let mut manifest = manifest();
    let template = manifest.contributions[0].clone();
    manifest.contributions = (0..MAX_CONTRIBUTIONS)
        .map(|index| {
            let mut contribution = template.clone();
            contribution.id = format!("{}.entry-{index}", manifest.package_id);
            contribution
        })
        .collect();
    assert!(manifest.validate().is_ok());
    let mut extra = template;
    extra.id = format!("{}.extra", manifest.package_id);
    manifest.contributions.push(extra);
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    manifest.contributions.clear();
    manifest.required_capabilities = vec![Capability::SelectionRead; 4];
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}

#[test]
fn resource_variants_are_closed_and_require_their_own_identifiers() {
    for resource in [
        json!({"kind":"other","vault_id":"v"}),
        json!({"kind":"document","vault_id":"v"}),
        json!({"kind":"vault","vault_id":"v","document_id":"d"}),
        json!({"kind":"selection","vault_id":"v","selection_id":"s","document_id":"d"}),
        json!({"vault_id":"v","selection_id":"s"}),
    ] {
        let mut value = request_value();
        value["resource"] = resource;
        assert_invalid_request(&value);
    }
    for resource in [
        Resource::Vault {
            vault_id: "v-1".into(),
        },
        Resource::Document {
            vault_id: "v-1".into(),
            document_id: "d:2".into(),
        },
        Resource::Selection {
            vault_id: "v-1".into(),
            selection_id: "s_3".into(),
        },
    ] {
        assert!(resource.validate().is_ok());
        assert_eq!(resource.vault_id(), "v-1");
        assert_eq!(
            serde_json::from_str::<Resource>(&serde_json::to_string(&resource).unwrap()).unwrap(),
            resource
        );
    }
}

#[test]
fn opaque_ids_are_bounded_and_never_accept_paths_or_whitespace() {
    for bad in [
        "".into(),
        " x".into(),
        "../x".into(),
        "/tmp/x".into(),
        "a/b".into(),
        "a\\b".into(),
        "a\n".into(),
        "资料".into(),
        "x".repeat(MAX_ID_BYTES + 1),
    ] {
        let mut request = request();
        request.request_id = bad.clone();
        assert_eq!(
            request.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        for resource in [
            Resource::Vault {
                vault_id: bad.clone(),
            },
            Resource::Document {
                vault_id: "v".into(),
                document_id: bad.clone(),
            },
            Resource::Selection {
                vault_id: "v".into(),
                selection_id: bad.clone(),
            },
        ] {
            assert_eq!(
                resource.validate().unwrap_err().code,
                ErrorCode::InvalidRequest
            );
        }
    }
    let mut request = request();
    request.request_id = "X".repeat(MAX_ID_BYTES);
    assert!(request.validate().is_ok());
}

#[test]
fn unsigned_numeric_types_reject_overflow_fractional_and_string_encodings() {
    for bad in [json!(-1), json!(65536), json!(0.1), json!("0"), Value::Null] {
        let mut value = request_value();
        value["api_version"]["major"] = bad;
        assert_invalid_request(&value);
    }
    for bad in [
        json!(-1),
        json!(4294967296_u64),
        json!(1.5),
        json!("1.0.0"),
        Value::Null,
    ] {
        let mut value = manifest_value();
        value["package_version"]["patch"] = bad;
        assert_invalid_manifest(&value);
    }
}

#[test]
fn wire_limits_count_all_bytes_before_deserialization() {
    let mut manifest_bytes = MANIFEST.as_bytes().to_vec();
    manifest_bytes.resize(MAX_MANIFEST_BYTES, b' ');
    assert!(Manifest::from_slice(&manifest_bytes).is_ok());
    manifest_bytes.push(b' ');
    assert_eq!(
        Manifest::from_slice(&manifest_bytes).unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
    let mut request_bytes = REQUEST.as_bytes().to_vec();
    request_bytes.resize(MAX_REQUEST_BYTES, b' ');
    assert!(CapabilityRequest::from_slice(&request_bytes).is_ok());
    request_bytes.push(b' ');
    assert_eq!(
        CapabilityRequest::from_slice(&request_bytes)
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
    assert_eq!(
        Manifest::from_slice(&vec![b'['; MAX_MANIFEST_BYTES + 1])
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn malformed_utf8_trailing_documents_and_missing_fields_are_rejected() {
    assert_eq!(
        Manifest::from_slice(b"\xff").unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    assert_eq!(
        CapabilityRequest::from_json(&(REQUEST.to_owned() + "{}"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    let mut value = manifest_value();
    value
        .as_object_mut()
        .unwrap()
        .remove("optional_capabilities");
    assert_invalid_manifest(&value);
    let mut value = request_value();
    value.as_object_mut().unwrap().remove("request_id");
    assert_invalid_request(&value);
}

#[test]
fn parser_errors_do_not_echo_untrusted_fields_or_payload_values() {
    let secret = "not-an-actual-secret-marker";
    let mut value = request_value();
    value[secret] = json!(secret);
    let error = CapabilityRequest::from_json(&value.to_string()).unwrap_err();
    assert!(!error.message.contains(secret));
    value = request_value();
    value["capability"] = json!(secret);
    let error = CapabilityRequest::from_json(&value.to_string()).unwrap_err();
    assert!(!error.message.contains(secret));
}

#[test]
fn wire_error_messages_are_bounded_and_codes_are_closed() {
    let error = ApiError::new(ErrorCode::PermissionDenied, "Permission denied");
    assert_eq!(error.to_string(), "PermissionDenied: Permission denied");
    let invalid = ApiError::new(
        ErrorCode::PermissionDenied,
        "x".repeat(MAX_ERROR_MESSAGE_BYTES + 1),
    );
    assert_eq!(
        invalid.to_json().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert!(ApiError::from_json(r#"{"code":"unknown","message":"error"}"#).is_err());
    assert!(
        ApiError::from_json(r#"{"code":"disabled","message":"error","stack":"private"}"#).is_err()
    );
    assert!(ApiError::from_json(r#"{"code":"disabled","message":" "}"#).is_err());
}
