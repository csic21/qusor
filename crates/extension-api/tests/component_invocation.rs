use extension_api::{component_invocation::*, *};
fn request() -> ComponentInvocationRequest {
    ComponentInvocationRequest {
        api_version: HOST_API_V0_3,
        component_invocation_protocol: 1,
        request_id: "r".repeat(32),
        operation: ComponentInvocationOperation::ReadContext {},
    }
}
fn response(context: ComponentInvocationContext) -> ComponentInvocationResponse {
    ComponentInvocationResponse {
        api_version: HOST_API_V0_3,
        component_invocation_protocol: 1,
        request_id: "r".repeat(32),
        context,
    }
}
#[test]
fn purpose_only_roundtrips() {
    let req = request();
    assert_eq!(
        ComponentInvocationRequest::from_json(&req.to_json().unwrap()).unwrap(),
        req
    );
    for context in [
        ComponentInvocationContext::EditEvent {},
        ComponentInvocationContext::ParagraphProjection {},
    ] {
        let r = response(context);
        assert_eq!(
            ComponentInvocationResponse::from_json(&r.to_json().unwrap()).unwrap(),
            r
        );
    }
}
#[test]
fn unknown_fields_cannot_supply_identity_or_approval() {
    for key in ["resource", "mount_id", "sequence", "text", "approved"] {
        let mut v = serde_json::to_value(request()).unwrap();
        v[key] = serde_json::json!(true);
        assert!(ComponentInvocationRequest::from_json(&v.to_string()).is_err());
        let mut v = serde_json::to_value(request()).unwrap();
        v["operation"][key] = serde_json::json!(true);
        assert!(ComponentInvocationRequest::from_json(&v.to_string()).is_err());
        let mut v =
            serde_json::to_value(response(ComponentInvocationContext::EditEvent {})).unwrap();
        v[key] = serde_json::json!(true);
        assert!(ComponentInvocationResponse::from_json(&v.to_string()).is_err());
        let mut v =
            serde_json::to_value(response(ComponentInvocationContext::ParagraphProjection {}))
                .unwrap();
        v["context"][key] = serde_json::json!(true);
        assert!(ComponentInvocationResponse::from_json(&v.to_string()).is_err());
    }
}
#[test]
fn versions_tags_and_ascii_ids_are_closed() {
    for minor in [0, 1, 2, 8] {
        let mut r = request();
        r.api_version.minor = minor;
        assert_eq!(
            r.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
    let mut r_later = request();
    r_later.api_version = HOST_API_V0_4;
    r_later.validate().unwrap();
    let mut r = request();
    r.component_invocation_protocol = 2;
    assert!(r.validate().is_err());
    for id in [
        "".to_owned(),
        "r".repeat(33),
        "中文".into(),
        "r\0".into(),
        "r\"".into(),
    ] {
        let mut r = request();
        r.request_id = id;
        assert!(r.validate().is_err());
    }
    let mut v = serde_json::to_value(request()).unwrap();
    v["operation"] = serde_json::json!({"kind":"read_metadata"});
    assert!(ComponentInvocationRequest::from_json(&v.to_string()).is_err());
    let mut v = serde_json::to_value(response(ComponentInvocationContext::EditEvent {})).unwrap();
    v["context"] = serde_json::json!({"kind":"unknown"});
    assert!(ComponentInvocationResponse::from_json(&v.to_string()).is_err());
    v.as_object_mut().unwrap().remove("context");
    assert!(ComponentInvocationResponse::from_json(&v.to_string()).is_err());
}
#[test]
fn encoded_frame_bounds_are_enforced() {
    let req = request().to_json().unwrap();
    let exact = format!("{}{}", req, " ".repeat(65536 - req.len()));
    assert!(ComponentInvocationRequest::from_json(&exact).is_ok());
    assert!(ComponentInvocationRequest::from_json(&(exact + " ")).is_err());
    let res = response(ComponentInvocationContext::ParagraphProjection {})
        .to_json()
        .unwrap();
    let exact = format!("{}{}", res, " ".repeat(4096 - res.len()));
    assert!(ComponentInvocationResponse::from_json(&exact).is_ok());
    assert!(ComponentInvocationResponse::from_json(&(exact + " ")).is_err());
}
