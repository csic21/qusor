use extension_api::{Capability, CapabilityRequest, Resource};
use qusor_wasm_probe::{MAX_RESPONSE_BYTES, ProbeError, managed_runtime::run_managed};
use std::time::Duration;

fn request_bytes() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "api_version": {"major": 0, "minor": 1}, "request_id": "managed-request",
        "capability": "document.metadata.read",
        "resource": {"kind": "document", "vault_id": "vault", "document_id": "doc"}
    }))
    .unwrap()
}

fn module_with_request(request: &[u8], body: &str) -> Vec<u8> {
    let escaped: String = request.iter().map(|byte| format!("\\{byte:02x}")).collect();
    wat::parse_str(format!(
        r#"(module
            (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1 2)
            (data (i32.const 0) "{escaped}")
            (func (export "run") (result i32 i32) (local $length i32) {body}))"#
    ))
    .unwrap()
}

fn module(body: &str) -> Vec<u8> {
    module_with_request(&request_bytes(), body)
}

fn call() -> String {
    format!(
        "i32.const 0 i32.const {} i32.const 4096 i32.const 4096 call $call",
        request_bytes().len()
    )
}

#[test]
fn actual_guest_request_is_forwarded_and_only_broker_bytes_are_returned() {
    let wasm = module(&format!(
        "{} local.set $length i32.const 4096 local.get $length",
        call()
    ));
    let response = "父进程授权的数据".as_bytes();
    let mut received: Vec<CapabilityRequest> = Vec::new();
    let result = run_managed(&wasm, |request| {
        received.push(request);
        Ok(response.to_vec())
    })
    .unwrap();
    assert_eq!(result.output.as_bytes(), response);
    assert_eq!(result.calls, 1);
    assert!(result.fuel_consumed > 0);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].capability, Capability::DocumentMetadataRead);
    assert_eq!(
        received[0].resource,
        Resource::Document {
            vault_id: "vault".into(),
            document_id: "doc".into()
        }
    );
}

#[test]
fn broker_rejection_is_not_replaced_by_worker_grants_or_synthetic_data() {
    let wasm = module(&format!("{} drop i32.const 0 i32.const 0", call()));
    let error = run_managed(&wasm, |_| {
        Err(ProbeError {
            code: "permission_denied",
        })
    })
    .unwrap_err();
    assert_eq!(error.code, "permission_denied");
}

#[test]
fn invalid_guest_ranges_and_requests_are_rejected_before_forwarding() {
    for body in [
        format!(
            "i32.const -1 i32.const {} i32.const 4096 i32.const 4096 call $call drop i32.const 0 i32.const 0",
            request_bytes().len()
        ),
        format!(
            "i32.const 0 i32.const {} i32.const 65530 i32.const 4096 call $call drop i32.const 0 i32.const 0",
            request_bytes().len()
        ),
    ] {
        let mut forwarded = 0;
        let error = run_managed(&module(&body), |_| {
            forwarded += 1;
            Ok(vec![])
        })
        .unwrap_err();
        assert_eq!(error.code, "invalid_pointer");
        assert_eq!(forwarded, 0);
    }
    let mut forwarded = 0;
    let bad_request = module_with_request(
        b"{",
        "i32.const 0 i32.const 1 i32.const 4096 i32.const 4096 call $call drop i32.const 0 i32.const 0",
    );
    assert_eq!(
        run_managed(&bad_request, |_| {
            forwarded += 1;
            Ok(vec![])
        })
        .unwrap_err()
        .code,
        "invalid_request"
    );
    assert_eq!(forwarded, 0);
}

#[test]
fn call_and_response_budgets_remain_bounded_across_external_broker_calls() {
    let repeated = format!(
        "{} i32.const 0 i32.const 0",
        format!("{} drop ", call()).repeat(9)
    );
    let mut forwarded = 0;
    assert_eq!(
        run_managed(&module(&repeated), |_| {
            forwarded += 1;
            Ok(vec![])
        })
        .unwrap_err()
        .code,
        "host_call_limit"
    );
    assert_eq!(forwarded, 8);
    let wasm = module(&format!("{} drop i32.const 0 i32.const 0", call()));
    assert_eq!(
        run_managed(&wasm, |_| Ok(vec![0; MAX_RESPONSE_BYTES + 1]))
            .unwrap_err()
            .code,
        "host_response_limit"
    );
    let tiny_output = module(&format!(
        "i32.const 0 i32.const {} i32.const 4096 i32.const 1 call $call drop i32.const 0 i32.const 0",
        request_bytes().len()
    ));
    assert_eq!(
        run_managed(&tiny_output, |_| Ok(vec![0; 2]))
            .unwrap_err()
            .code,
        "output_capacity"
    );
}

#[test]
fn guest_instruction_memory_output_and_import_boundaries_are_enforced() {
    let cases = [
        (module("(loop $forever br $forever) i32.const 0 i32.const 0"), "fuel_exhausted"),
        (wat::parse_str(include_str!("../../fixtures/memory-grow.wat")).unwrap(), "vm_trap"),
        (module("i32.const 0 i32.const 4097"), "output_size_limit"),
        (wat::parse_str("(module (import \"ambient\" \"filesystem\" (func)) (memory (export \"memory\") 1) (func (export \"run\") (result i32 i32) i32.const 0 i32.const 0))").unwrap(), "unknown_import"),
        (wat::parse_str("(module (memory (export \"memory\") 1) (func $start) (start $start) (func (export \"run\") (result i32 i32) i32.const 0 i32.const 0))").unwrap(), "invalid_module"),
        (wat::parse_str("(module (memory (export \"memory\") 3) (func (export \"run\") (result i32 i32) i32.const 0 i32.const 0))").unwrap(), "instantiate_failed"),
    ];
    for (wasm, expected) in cases {
        let mut forwarded = 0;
        let error = run_managed(&wasm, |_| {
            forwarded += 1;
            Ok(vec![])
        })
        .unwrap_err();
        assert_eq!(error.code, expected);
        assert_eq!(forwarded, 0);
    }
}

#[test]
fn a_native_callback_that_returns_late_cannot_have_its_result_accepted() {
    let wasm = module(&format!("{} drop i32.const 0 i32.const 0", call()));
    assert_eq!(
        run_managed(&wasm, |_| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(b"late".to_vec())
        })
        .unwrap_err()
        .code,
        "deadline_exceeded"
    );
}

#[test]
fn raw_capture_budget_does_not_expand_legacy_policy_wire() {
    use qusor_wasm_probe::managed_runtime::run_managed_raw;
    let bytes = vec![b'x'; 5000];
    let wasm = module_with_request(
        &bytes,
        "i32.const 0 i32.const 5000 i32.const 8192 i32.const 4096 call $call drop i32.const 8192 i32.const 0",
    );
    let mut calls = 0;
    assert_eq!(
        run_managed(&wasm, |_| {
            calls += 1;
            Ok(vec![])
        })
        .unwrap_err()
        .code,
        "host_buffer_limit"
    );
    assert_eq!(calls, 0);
    run_managed_raw(&wasm, |received| {
        assert_eq!(received, &bytes);
        calls += 1;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(calls, 1);
    let too_large = module_with_request(
        &[],
        "i32.const 0 i32.const 65537 i32.const 8192 i32.const 4096 call $call drop i32.const 8192 i32.const 0",
    );
    assert_eq!(
        run_managed_raw(&too_large, |_| panic!("oversize must not forward"))
            .unwrap_err()
            .code,
        "host_buffer_limit"
    );
}
