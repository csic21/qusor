use extension_runtime_api::{
    BoundedStreamCollector, ExecutionLimits, HARD_MAX_WASM_BYTES, MAX_RAW_REQUEST_BYTES,
    ModuleExecutor, RuntimeBudgetProfile, RuntimeBudgets, RuntimeError, RuntimeErrorKind,
    SessionExecutor,
};
use runtime_wasm::{WasmiRuntime, private_runtime};
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

#[test]
fn host_injected_mib_budgets_support_real_rust_scale_modules_memory_tables_and_responses() {
    let executor: &dyn ModuleExecutor = &WasmiRuntime;
    // Grow linear memory to 16 pages (1 MiB), declare a 64-slot function table,
    // embed a 300 KiB passive data payload (> legacy 256 KiB module limit), and
    // return a 16 KiB response (> legacy 4 KiB output limit).
    let padding = "A".repeat(300 * 1024);
    let wasm = wat::parse_str(format!(
        r#"(module
 (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
 (memory (export "memory") 1 32)
 (table 64 funcref)
 (data (i32.const 0) "rq")
 (data "{padding}")
 (func (export "run") (result i32 i32)
  (drop (memory.grow (i32.const 15)))
  (drop (call $call (i32.const 0) (i32.const 2) (i32.const 65536) (i32.const 65536)))
  (i32.const 65536)
  (i32.const 16384)))"#
    ))
    .unwrap();
    assert!(wasm.len() > 256 * 1024);

    let legacy_err = executor
        .execute_with_budgets(&wasm, &RuntimeBudgets::legacy_probe(), &mut |_: &[u8]| {
            Ok(vec![b'x'; 16 * 1024])
        })
        .unwrap_err();
    assert_eq!(legacy_err.code, "package_size_limit");
    assert_eq!(legacy_err.kind(), RuntimeErrorKind::BudgetExceeded);

    for profile in [
        RuntimeBudgetProfile::MobileStandard,
        RuntimeBudgetProfile::DesktopStandard,
    ] {
        let budgets = RuntimeBudgets::for_profile(profile);
        let result = executor
            .execute_with_budgets(&wasm, &budgets, &mut |req: &[u8]| {
                assert_eq!(req, b"rq");
                Ok(vec![b'z'; 16 * 1024])
            })
            .unwrap();
        assert_eq!(result.output.len(), 16 * 1024);
        assert_eq!(result.calls, 1);
    }

    let mut invalid = RuntimeBudgets::desktop_standard();
    invalid.max_wasm_bytes = HARD_MAX_WASM_BYTES + 1;
    let err = executor
        .execute_with_budgets(&wasm, &invalid, &mut |_: &[u8]| Ok(vec![]))
        .unwrap_err();
    assert_eq!(err.kind(), RuntimeErrorKind::InvalidConfiguration);
}

#[test]
fn streaming_chunks_deliver_multi_chunk_output_and_enforce_stream_budgets() {
    let executor: &dyn ModuleExecutor = &WasmiRuntime;
    let wasm = wat::parse_str(
        r#"(module
 (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
 (import "qusor" "emit_chunk" (func $emit (param i32 i32) (result i32)))
 (memory (export "memory") 1 4)
 (data (i32.const 0) "chunk-one|")
 (data (i32.const 16) "chunk-two|")
 (data (i32.const 32) "done")
 (func (export "run") (result i32 i32)
  (drop (call $emit (i32.const 0) (i32.const 10)))
  (drop (call $emit (i32.const 16) (i32.const 10)))
  (i32.const 32)
  (i32.const 4)))"#,
    )
    .unwrap();

    let budgets = RuntimeBudgets::mobile_standard();
    let mut collector = BoundedStreamCollector::from_budgets(&budgets).unwrap();
    let res = executor
        .execute_streaming(&wasm, &budgets, &mut |_: &[u8]| Ok(vec![]), &mut collector)
        .unwrap();
    assert_eq!(res.output, "done");
    assert_eq!(res.chunks_emitted, 2);
    assert_eq!(res.stream_bytes, 20);
    assert_eq!(collector.into_concatenated(), b"chunk-one|chunk-two|");

    // Exceeding per-chunk cap fails closed inside the VM before forwarding.
    let mut small_chunk = budgets.clone();
    small_chunk.max_stream_chunk_bytes = 8;
    let mut small_collector = BoundedStreamCollector::from_budgets(&small_chunk).unwrap();
    let err = executor
        .execute_streaming(
            &wasm,
            &small_chunk,
            &mut |_: &[u8]| Ok(vec![]),
            &mut small_collector,
        )
        .unwrap_err();
    assert_eq!(err.code, "stream_chunk_limit");
    assert_eq!(err.kind(), RuntimeErrorKind::BudgetExceeded);

    // Exceeding total stream cap fails closed on the second chunk.
    let mut small_total = budgets.clone();
    small_total.max_stream_chunk_bytes = 15;
    small_total.max_stream_total_bytes = 15;
    let mut total_collector = BoundedStreamCollector::from_budgets(&small_total).unwrap();
    let err = executor
        .execute_streaming(
            &wasm,
            &small_total,
            &mut |_: &[u8]| Ok(vec![]),
            &mut total_collector,
        )
        .unwrap_err();
    assert_eq!(err.code, "stream_total_limit");
    assert_eq!(total_collector.chunks().len(), 1);
}

#[test]
fn long_lived_session_preserves_guest_memory_across_turns_and_poisons_on_trap() {
    let sessions: &dyn SessionExecutor = &WasmiRuntime;
    // Guest maintains an in-memory counter at offset 0 and echoes request + count.
    let wasm = wat::parse_str(
        r#"(module
 (import "qusor" "call" (func $call (param i32 i32 i32 i32) (result i32)))
 (import "qusor" "emit_chunk" (func $emit (param i32 i32) (result i32)))
 (memory (export "memory") 1 4)
 (global $bump (mut i32) (i32.const 1024))
 (func (export "qusor_alloc") (param $len i32) (result i32)
  (local $ptr i32)
  (local.set $ptr (global.get $bump))
  (global.set $bump (i32.add (global.get $bump) (local.get $len)))
  (local.get $ptr))
 (func (export "handle") (param $ptr i32) (param $len i32) (result i32 i32)
  (local $count i32)
  (local.set $count (i32.add (i32.load (i32.const 0)) (i32.const 1)))
  (i32.store (i32.const 0) (local.get $count))
  (i32.store8 (i32.add (local.get $ptr) (local.get $len)) (i32.add (i32.const 48) (local.get $count)))
  (drop (call $call (local.get $ptr) (i32.add (local.get $len) (i32.const 1)) (i32.const 512) (i32.const 256)))
  (i32.const 512)
  (i32.add (local.get $len) (i32.const 1)))
 (func (export "trap_now") (param $ptr i32) (param $len i32) (result i32 i32)
  unreachable))"#,
    )
    .unwrap();

    let budgets = RuntimeBudgets::desktop_standard();
    let mut session = sessions.instantiate(&wasm, &budgets).unwrap();
    assert!(!session.is_poisoned());

    let mut seen = Vec::new();
    let r1 = session
        .invoke("handle", b"turn:", None, &mut |req: &[u8]| {
            seen.push(String::from_utf8_lossy(req).into_owned());
            Ok(req.to_vec())
        })
        .unwrap();
    assert_eq!(r1.output, "turn:1");
    assert_eq!(r1.calls, 1);

    let r2 = session
        .invoke("handle", b"turn:", None, &mut |req: &[u8]| {
            seen.push(String::from_utf8_lossy(req).into_owned());
            Ok(req.to_vec())
        })
        .unwrap();
    assert_eq!(r2.output, "turn:2");
    assert_eq!(seen, ["turn:1", "turn:2"]);
    assert_eq!(session.total_calls(), 2);
    assert!(session.total_fuel_consumed() > r1.fuel_consumed);

    // Calling emit_chunk during non-streaming invoke fails closed and poisons session.
    let err = session
        .invoke("trap_now", b"", None, &mut |_: &[u8]| Ok(vec![]))
        .unwrap_err();
    assert_eq!(err.kind(), RuntimeErrorKind::Trap);
    assert!(session.is_poisoned());

    let after = session
        .invoke("handle", b"turn:", None, &mut |_: &[u8]| Ok(vec![]))
        .unwrap_err();
    assert_eq!(after.code, "session_poisoned");
}

#[test]
fn private_runtime_honors_host_injected_mib_budgets() {
    let oversized_for_legacy = vec![b'x'; extension_runtime_api::MAX_PRIVATE_BYTES + 1];
    let err = private_runtime::run_migration_with_budgets(
        &fixture("migrate-good"),
        1,
        2,
        &oversized_for_legacy,
        &RuntimeBudgets::legacy_probe(),
    )
    .unwrap_err();
    assert_eq!(err.code, "private_state_size_limit");

    // Same 64 KiB + 1 byte state succeeds under MobileStandard/DesktopStandard
    // when the module has enough linear memory (2 pages = 128 KiB > 64 KiB).
    let wasm = wat::parse_str(
        r#"(module
 (memory (export "memory") 4 8)
 (global $bump (mut i32) (i32.const 1024))
 (func (export "alloc") (param $n i32) (result i32)
  (local $p i32)
  (local.set $p (global.get $bump))
  (global.set $bump (i32.add (global.get $bump) (local.get $n)))
  (local.get $p))
 (func (export "migrate") (param i32 i32 i32 i32) (result i64)
  (i64.or
   (i64.shl (i64.extend_i32_u (local.get 2)) (i64.const 32))
   (i64.extend_i32_u (local.get 3)))))"#,
    )
    .unwrap();
    let migrated = private_runtime::run_migration_with_budgets(
        &wasm,
        1,
        2,
        &oversized_for_legacy,
        &RuntimeBudgets::desktop_standard(),
    )
    .unwrap();
    assert_eq!(migrated.len(), oversized_for_legacy.len());
}
