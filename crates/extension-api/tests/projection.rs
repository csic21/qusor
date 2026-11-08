use extension_api::{continuation::encode_checkpoint, projection::*, *};
fn context() -> ProjectionContext {
    ProjectionContext {
        resource: Resource::Draft {
            vault_id: "v".repeat(128),
            draft_id: "a".repeat(32),
        },
        mount_id: "b".repeat(32),
        query_id: "c".repeat(32),
        base_revision: u64::MAX,
        query_generation: u64::MAX,
        first_paragraph: 0,
        max_rows: 8,
        total_bytes: 1048576,
    }
}
fn query(operation: ProjectionOperation) -> ProjectionRequest {
    let c = context();
    ProjectionRequest::Query {
        api_version: HOST_API_V0_3,
        projection_protocol: 1,
        request_id: "r".repeat(32),
        resource: c.resource,
        mount_id: c.mount_id,
        query_id: c.query_id,
        base_revision: c.base_revision,
        operation,
    }
}
#[test]
fn bounded_checkpoint_roundtrip_and_worst_escaped_identity() {
    // Control characters in an otherwise valid opaque vault identifier are not
    // assumed valid; use maximum permitted identity and worst binary base64.
    let req = query(ProjectionOperation::AdvanceProjection {
        expected_checkpoint_revision: u64::MAX,
        checkpoint_base64: encode_checkpoint(&vec![255; 8192]).unwrap(),
        window: None,
        complete: false,
    });
    let json = req.to_json().unwrap();
    assert!(json.len() <= 65536);
    assert_eq!(ProjectionRequest::from_json(&json).unwrap(), req);
    for byte in [0, 255] {
        let c = context();
        let r = ProjectionResponse::Query {
            api_version: HOST_API_V0_3,
            projection_protocol: 1,
            request_id: "r".repeat(32),
            resource: c.resource,
            mount_id: c.mount_id,
            query_id: c.query_id,
            base_revision: c.base_revision,
            outcome: ProjectionOutcome::CheckpointChunk {
                checkpoint_revision: u64::MAX,
                offset: 6144,
                total_bytes: 8192,
                checkpoint_base64: encode_checkpoint(&vec![byte; 2048]).unwrap(),
            },
        };
        let json = r.to_json().unwrap();
        assert!(json.len() <= 4096);
        assert_eq!(ProjectionResponse::from_json(&json).unwrap(), r);
    }
}
#[test]
fn closed_identity_versions_and_no_effect_vocabulary() {
    let r = ProjectionRequest::ReadContext {
        api_version: HOST_API_V0_3,
        projection_protocol: 1,
        request_id: "r".into(),
    };
    let mut v: serde_json::Value = serde_json::from_str(&r.to_json().unwrap()).unwrap();
    v["mount_id"] = "b".repeat(32).into();
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    v.as_object_mut().unwrap().remove("mount_id");
    v["api_version"]["minor"] = 2.into();
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    let mut v: serde_json::Value = serde_json::from_str(
        &query(ProjectionOperation::ReadMetadata {})
            .to_json()
            .unwrap(),
    )
    .unwrap();
    v["operation"]["effect"] = serde_json::json!({"kind":"commit_replace"});
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    v["operation"] = serde_json::json!({"kind":"advance_projection","expected_checkpoint_revision":1,"checkpoint_base64":""});
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    assert!(
        query(ProjectionOperation::AdvanceProjection {
            expected_checkpoint_revision: 1,
            checkpoint_base64: "Zh==".into(),
            window: None,
            complete: false
        })
        .validate()
        .is_err()
    );
}
#[test]
fn partial_eof_and_window_completion_are_distinct() {
    let c = context();
    let mut w = ProjectionWindow {
        rows: vec![RowSpan {
            ordinal: 0,
            start: 0,
            end: 512,
            complete: false,
        }],
        reached_eof: false,
    };
    assert!(!w.validate_for_context(&c).unwrap());
    w.reached_eof = true;
    assert!(w.validate_for_context(&c).is_err());
    w.reached_eof = false;
    w.rows[0].complete = true;
    assert!(!w.validate_for_context(&c).unwrap());
    w.rows = (0..8)
        .map(|i| RowSpan {
            ordinal: i,
            start: i * 2,
            end: i * 2 + 1,
            complete: true,
        })
        .collect();
    assert!(w.validate_for_context(&c).unwrap());
    let mut m = ProjectionMetadata {
        checkpoint_revision: 1,
        checkpoint_bytes: 0,
        state: ProjectionState::Completed,
        window: Some(w),
        window_ready: true,
    };
    assert!(m.validate_for_context(&c).is_ok());
    m.window_ready = false;
    assert!(m.validate_for_context(&c).is_err());
    m.state = ProjectionState::Active;
    assert!(m.validate_for_context(&c).is_err());
}
#[test]
fn empty_draft_out_of_range_and_span_cross_checks() {
    let mut c = context();
    c.total_bytes = 0;
    let empty = ProjectionWindow {
        rows: vec![],
        reached_eof: true,
    };
    assert!(empty.validate_for_context(&c).is_err());
    let w = ProjectionWindow {
        rows: vec![RowSpan {
            ordinal: 0,
            start: 0,
            end: 0,
            complete: true,
        }],
        reached_eof: true,
    };
    assert!(w.validate_for_context(&c).unwrap());
    c.first_paragraph = 1;
    assert!(empty.validate_for_context(&c).unwrap());
    assert!(w.validate_for_context(&c).is_err());
    c = context();
    let mut w = ProjectionWindow {
        rows: vec![
            RowSpan {
                ordinal: 0,
                start: 0,
                end: 3,
                complete: true,
            },
            RowSpan {
                ordinal: 1,
                start: 2,
                end: 4,
                complete: true,
            },
        ],
        reached_eof: false,
    };
    assert!(w.validate_for_context(&c).is_err());
    w.rows[1].start = 3;
    w.rows[1].ordinal = 2;
    assert!(w.validate_for_context(&c).is_err());
    w.rows[1].ordinal = 1;
    w.rows[0].complete = false;
    assert!(w.validate_for_context(&c).is_err());
    w.rows[0].complete = true;
    w.rows[1].end = 1048577;
    assert!(w.validate_for_context(&c).is_err());
}

#[test]
fn checkpoint_and_raw_frame_limits_are_closed() {
    for operation in [
        ProjectionOperation::ReadCheckpointChunk {
            checkpoint_revision: 0,
            offset: 0,
            max_bytes: 1,
        },
        ProjectionOperation::ReadCheckpointChunk {
            checkpoint_revision: 1,
            offset: 8193,
            max_bytes: 1,
        },
        ProjectionOperation::ReadCheckpointChunk {
            checkpoint_revision: 1,
            offset: 0,
            max_bytes: 2049,
        },
        ProjectionOperation::Cancel {
            expected_checkpoint_revision: 0,
        },
    ] {
        assert!(query(operation).validate().is_err());
    }
    assert!(ProjectionRequest::from_slice(&vec![b' '; 65537]).is_err());
    assert!(ProjectionResponse::from_slice(&vec![b' '; 4097]).is_err());
    let mut v: serde_json::Value = serde_json::from_str(
        &query(ProjectionOperation::ReadMetadata {})
            .to_json()
            .unwrap(),
    )
    .unwrap();
    v["projection_protocol"] = 2.into();
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    let outcome = ProjectionOutcome::CheckpointChunk {
        checkpoint_revision: 1,
        offset: 8192,
        total_bytes: 8192,
        checkpoint_base64: encode_checkpoint(&[255]).unwrap(),
    };
    assert!(outcome.validate().is_err());
}

#[test]
fn nullable_windows_are_required_not_implicitly_defaulted() {
    let r = query(ProjectionOperation::AdvanceProjection {
        expected_checkpoint_revision: 1,
        checkpoint_base64: String::new(),
        window: None,
        complete: false,
    });
    let mut v: serde_json::Value = serde_json::from_str(&r.to_json().unwrap()).unwrap();
    assert!(v["operation"]["window"].is_null());
    assert_eq!(ProjectionRequest::from_json(&v.to_string()).unwrap(), r);
    v["operation"].as_object_mut().unwrap().remove("window");
    assert!(ProjectionRequest::from_json(&v.to_string()).is_err());
    let m = ProjectionMetadata {
        checkpoint_revision: 1,
        checkpoint_bytes: 0,
        state: ProjectionState::Active,
        window: None,
        window_ready: false,
    };
    let mut v = serde_json::to_value(&m).unwrap();
    assert_eq!(
        serde_json::from_value::<ProjectionMetadata>(v.clone()).unwrap(),
        m
    );
    v.as_object_mut().unwrap().remove("window");
    assert!(serde_json::from_value::<ProjectionMetadata>(v).is_err());
}
