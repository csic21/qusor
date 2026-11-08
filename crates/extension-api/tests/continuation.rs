use base64::{Engine, engine::general_purpose::STANDARD};
use extension_api::{continuation::*, *};
fn request(operation: ContinuationOperation) -> ContinuationRequest {
    ContinuationRequest {
        api_version: HOST_API_V0_3,
        continuation_protocol: 1,
        request_id: "r".repeat(32),
        resource: Resource::Draft {
            vault_id: "v".repeat(128),
            draft_id: "a".repeat(32),
        },
        mount_id: "b".repeat(32),
        sequence: u64::MAX,
        base_revision: u64::MAX,
        operation,
    }
}
fn response(outcome: ContinuationOutcome) -> ContinuationResponse {
    let r = request(ContinuationOperation::ReadMetadata {});
    ContinuationResponse {
        api_version: r.api_version,
        continuation_protocol: 1,
        request_id: r.request_id,
        resource: r.resource,
        mount_id: r.mount_id,
        sequence: r.sequence,
        base_revision: r.base_revision,
        outcome,
    }
}
#[test]
fn worst_payloads_roundtrip_with_real_encoded_budgets() {
    let r = request(ContinuationOperation::AdvanceEvent {
        expected_checkpoint_revision: u64::MAX,
        checkpoint_base64: encode_checkpoint(&vec![255; 8192]).unwrap(),
        complete: false,
        effect: Some(EventEffect::AppendChunk {
            offset: 0,
            text: "\0".repeat(512),
        }),
    });
    let encoded = r.to_json().unwrap();
    assert!(encoded.len() <= 65536);
    assert_eq!(ContinuationRequest::from_json(&encoded).unwrap(), r);
    for byte in [0, 255] {
        let r = response(ContinuationOutcome::CheckpointChunk {
            checkpoint_revision: u64::MAX,
            offset: 0,
            total_bytes: 8192,
            checkpoint_base64: encode_checkpoint(&vec![byte; 2048]).unwrap(),
        });
        let encoded = r.to_json().unwrap();
        assert!(encoded.len() <= 4096);
        assert_eq!(ContinuationResponse::from_json(&encoded).unwrap(), r);
    }
}
#[test]
fn canonical_base64_and_strict_resource_versions() {
    for bad in ["Zg", "Zg===", "Zh==", "Zg==\n", "-w==", "%%%%"] {
        assert!(decode_checkpoint(bad, 8192).is_err(), "{bad}");
    }
    assert_eq!(decode_checkpoint("Zg==", 8192).unwrap(), b"f");
    assert!(decode_checkpoint(&STANDARD.encode(vec![0; 8193]), 8192).is_err());
    assert_eq!(
        encode_checkpoint(&vec![0; 8193]).unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
    for version in [HOST_API, HOST_API_V0_2, ApiVersion { major: 0, minor: 4 }] {
        let mut r = request(ContinuationOperation::ReadMetadata {});
        r.api_version = version;
        assert_eq!(
            r.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
    let mut r = request(ContinuationOperation::ReadMetadata {});
    r.sequence = 0;
    assert!(r.validate().is_err());
    r = request(ContinuationOperation::ReadMetadata {});
    r.resource = Resource::Vault {
        vault_id: "vault".into(),
    };
    assert!(r.validate().is_err());
}
#[test]
fn all_operations_and_effect_receipts_are_closed_and_bounded() {
    for op in [
        ContinuationOperation::ReadMetadata {},
        ContinuationOperation::ReadCheckpointChunk {
            checkpoint_revision: 1,
            offset: 0,
            max_bytes: 2048,
        },
        ContinuationOperation::CancelEvent {
            expected_checkpoint_revision: 1,
        },
    ] {
        let r = request(op);
        assert_eq!(
            ContinuationRequest::from_json(&r.to_json().unwrap()).unwrap(),
            r
        );
        let mut v = serde_json::to_value(r).unwrap();
        v["operation"]["approved"] = true.into();
        assert!(ContinuationRequest::from_slice(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    for effect in [
        EventEffectOutcome::Started {},
        EventEffectOutcome::ChunkAccepted {
            next_offset: 1048576,
        },
        EventEffectOutcome::Replaced { revision: u64::MAX },
        EventEffectOutcome::Cancelled {},
    ] {
        let r = response(ContinuationOutcome::Metadata {
            metadata: ContinuationMetadata {
                checkpoint_revision: 1,
                checkpoint_bytes: 8192,
                state: ContinuationState::Active,
                last_effect: Some(effect),
            },
        });
        assert_eq!(
            ContinuationResponse::from_json(&r.to_json().unwrap()).unwrap(),
            r
        );
    }
    let mut bad = serde_json::to_value(request(ContinuationOperation::ReadMetadata {})).unwrap();
    bad["operation"] = serde_json::json!({"kind":"advance_event","expected_checkpoint_revision":1,"checkpoint_base64":"","complete":false,"effect":{"kind":"read_chunk","revision":null,"offset":0,"max_bytes":512}});
    assert!(ContinuationRequest::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
    assert!(ContinuationRequest::from_slice(&vec![b' '; 65537]).is_err());
    assert!(ContinuationResponse::from_slice(&vec![b' '; 4097]).is_err());
    let r = response(ContinuationOutcome::CheckpointChunk {
        checkpoint_revision: 1,
        offset: 2,
        total_bytes: 1,
        checkpoint_base64: String::new(),
    });
    assert!(r.validate().is_err());
}
