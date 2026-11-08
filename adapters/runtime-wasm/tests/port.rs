use extension_runtime_api::{
    ExecutionLimits, MAX_RAW_REQUEST_BYTES, ModuleExecutor, RuntimeError, RuntimeErrorKind,
};
use runtime_wasm::WasmiRuntime;
use std::path::Path;

fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
 (import "qusor" "call" (func $call(param i32 i32 i32 i32)(result i32)))
 (memory(export "memory")1 2)(data(i32.const 0)"rq")
 (func(export "run")(result i32 i32)(local $n i32){body}))"#
    ))
    .unwrap()
}
fn call() -> &'static str {
    "i32.const 0 i32.const 2 i32.const 4096 i32.const 4096 call $call"
}
#[test]
fn actual_runtime_port_uses_only_external_authority_and_enforces_request_budget() {
    let executor: &dyn ModuleExecutor = &WasmiRuntime;
    let wasm = module(&format!(
        "{} local.set $n i32.const 4096 local.get $n",
        call()
    ));
    let mut requests = Vec::new();
    let mut broker = |bytes: &[u8]| {
        requests.push(bytes.to_vec());
        Ok("授权数据".as_bytes().to_vec())
    };
    let result = executor
        .execute(&wasm, &ExecutionLimits::default(), 2, &mut broker)
        .unwrap();
    assert_eq!(result.output, "授权数据");
    assert_eq!(result.calls, 1);
    assert_eq!(requests, [b"rq"]);
    let mut called = false;
    let error = executor
        .execute(&wasm, &ExecutionLimits::default(), 1, &mut |_: &[u8]| {
            called = true;
            Ok(vec![])
        })
        .unwrap_err();
    assert_eq!(error.code, "host_buffer_limit");
    assert_eq!(error.kind(), RuntimeErrorKind::BudgetExceeded);
    assert!(!called);
    let invalid_output = module(
        "i32.const 0 i32.const 2 i32.const 65530 i32.const 4096 call $call drop i32.const 0 i32.const 0",
    );
    let mut forwarded = 0;
    let error = executor
        .execute(
            &invalid_output,
            &ExecutionLimits::default(),
            2,
            &mut |_: &[u8]| {
                forwarded += 1;
                Ok(vec![])
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_pointer");
    assert_eq!(forwarded, 0);
    let error = executor
        .execute(&wasm, &ExecutionLimits::default(), 2, &mut |_: &[u8]| {
            Err(RuntimeError::new("permission_denied"))
        })
        .unwrap_err();
    assert_eq!(error.code, "permission_denied");
    assert_eq!(error.kind(), RuntimeErrorKind::BrokerRejected);
}
#[test]
fn trusted_port_limits_are_validated_and_actual_guest_fuel_is_bounded() {
    let executor: &dyn ModuleExecutor = &WasmiRuntime;
    let wasm = module("(loop $forever br $forever) i32.const 0 i32.const 0");
    let limits = ExecutionLimits {
        total_fuel: 100,
        slice_fuel: 10,
        ..ExecutionLimits::default()
    };
    let mut broker = |_: &[u8]| panic!("loop must not call a broker");
    assert_eq!(
        executor
            .execute(&wasm, &limits, 2, &mut broker)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::FuelExhausted
    );
    for (total, slice) in [(7, 10_000), (101, 13)] {
        let limits = ExecutionLimits {
            total_fuel: total,
            slice_fuel: slice,
            ..ExecutionLimits::default()
        };
        assert_eq!(
            executor
                .execute(&wasm, &limits, 2, &mut broker)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::FuelExhausted
        );
    }
    let invalid = ExecutionLimits {
        slice_fuel: 0,
        ..limits
    };
    assert_eq!(
        executor
            .execute(&wasm, &invalid, 2, &mut broker)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::InvalidConfiguration
    );
    assert_eq!(
        executor
            .execute(
                &wasm,
                &ExecutionLimits::default(),
                MAX_RAW_REQUEST_BYTES + 1,
                &mut broker
            )
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::InvalidConfiguration
    );
}
fn fixture(name: &str) -> Vec<u8> {
    wat::parse_file(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../experiments/extension-package/fixtures")
            .join(format!("private-{name}.wat")),
    )
    .unwrap()
}
#[test]
fn private_port_executes_migration_without_business_grants() {
    let executor: &dyn ModuleExecutor = &WasmiRuntime;
    let input = b"theme=dark";
    let output = executor
        .migrate(&fixture("migrate-good"), 1, 2, input)
        .unwrap();
    assert_eq!(input, b"theme=dark");
    assert_eq!(output, [input.as_slice(), b"\nlayout=wide"].concat());
    executor.preflight(&fixture("run-v2"), 2, &output).unwrap();
    assert_eq!(
        executor
            .preflight(&fixture("run-host-call"), 1, input)
            .unwrap_err()
            .code,
        "private_capability_denied"
    );
    assert_eq!(
        executor
            .migrate(&fixture("migrate-import"), 1, 2, input)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::InvalidModule
    );
}
