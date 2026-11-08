use qusor_wasm_probe::private_runtime::{MAX_PRIVATE_BYTES, preflight, run_migration};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extension-package/fixtures")
        .join(format!("private-{name}.wat"));
    wat::parse_file(path).unwrap()
}

#[test]
fn real_author_migration_preserves_input_and_matches_new_activation_schema() {
    let input = "theme=dark\nfont=中文".as_bytes();
    preflight(&fixture("run-v1"), 1, input).unwrap();
    let output = run_migration(&fixture("migrate-good"), 1, 2, input).unwrap();
    assert_eq!(output, [input, b"\nlayout=wide"].concat());
    preflight(&fixture("run-v2"), 2, &output).unwrap();
    assert_eq!(input, "theme=dark\nfont=中文".as_bytes());
    assert_eq!(
        preflight(&fixture("run-v2"), 2, input).unwrap_err().code,
        "private_state_rejected"
    );
    let mut damaged = output;
    *damaged.last_mut().unwrap() = b'x';
    assert_eq!(
        preflight(&fixture("run-v2"), 2, &damaged).unwrap_err().code,
        "private_state_rejected"
    );
    assert_eq!(
        preflight(&fixture("run-v1"), 2, input).unwrap_err().code,
        "private_state_rejected"
    );
}

#[test]
fn migration_guest_failures_are_rejected_at_expected_boundary() {
    for (name, code) in [
        ("migrate-trap", "private_vm_trap"),
        ("migrate-loop", "private_fuel_exhausted"),
        ("migrate-alloc-loop", "private_fuel_exhausted"),
        ("migrate-bad-pointer", "private_invalid_pointer"),
        ("migrate-oversize", "private_state_size_limit"),
        ("migrate-import", "private_import_denied"),
        ("migrate-wrong-schema", "private_vm_trap"),
        ("migrate-start", "private_invalid_module"),
    ] {
        assert_eq!(
            run_migration(&fixture(name), 1, 2, b"theme=dark")
                .unwrap_err()
                .code,
            code,
            "fixture {name}"
        );
    }
}

#[test]
fn preflight_checks_real_abi_and_denies_business_calls_without_grants() {
    assert_eq!(
        preflight(&fixture("run-bad-abi"), 1, b"theme=dark")
            .unwrap_err()
            .code,
        "private_invalid_abi"
    );
    assert_eq!(
        preflight(&fixture("run-host-call"), 1, b"theme=dark")
            .unwrap_err()
            .code,
        "private_capability_denied"
    );
    let missing_run = wat::parse_str("(module (memory (export \"memory\") 1))").unwrap();
    assert_eq!(
        preflight(&missing_run, 1, b"").unwrap_err().code,
        "private_invalid_abi"
    );
    let v1 = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../extension-package/fixtures/private-run-v1.wat"),
    )
    .unwrap();
    let loop_check =
        wat::parse_str(v1.replace("i32.const 0))", "(loop $again br $again) i32.const 0))"))
            .unwrap();
    assert_eq!(
        preflight(&loop_check, 1, b"theme=dark").unwrap_err().code,
        "private_fuel_exhausted"
    );
}

#[test]
fn all_input_output_and_memory_boundaries_are_enforced() {
    let module = fixture("migrate-good");
    let maximum = vec![b'x'; MAX_PRIVATE_BYTES];
    assert_eq!(
        run_migration(&module, 1, 2, &maximum).unwrap_err().code,
        "private_state_size_limit"
    );
    preflight(&fixture("run-v1"), 1, &maximum).unwrap();
    let oversized = vec![b'x'; MAX_PRIVATE_BYTES + 1];
    assert_eq!(
        run_migration(&module, 1, 2, &oversized).unwrap_err().code,
        "private_state_size_limit"
    );
    assert_eq!(
        preflight(&fixture("run-v1"), 1, &oversized)
            .unwrap_err()
            .code,
        "private_state_size_limit"
    );
    assert_eq!(
        run_migration(&module, 0, 2, b"").unwrap_err().code,
        "private_invalid_schema"
    );
    assert_eq!(
        preflight(&fixture("run-v1"), u32::MAX, b"")
            .unwrap_err()
            .code,
        "private_invalid_schema"
    );
    let allocate_bad = wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"alloc\") (param i32) (result i32) i32.const -1) (func (export \"migrate\") (param i32 i32 i32 i32) (result i64) i64.const 0))").unwrap();
    assert_eq!(
        run_migration(&allocate_bad, 1, 2, b"x").unwrap_err().code,
        "private_invalid_pointer"
    );
    let big_memory = wat::parse_str("(module (memory (export \"memory\") 3))").unwrap();
    assert_eq!(
        run_migration(&big_memory, 1, 2, b"").unwrap_err().code,
        "private_instantiate_failed"
    );
    let big_module = vec![0; qusor_wasm_probe::MAX_WASM_BYTES + 1];
    assert_eq!(
        run_migration(&big_module, 1, 2, b"").unwrap_err().code,
        "private_module_size_limit"
    );
}

#[test]
fn allocation_and_migration_share_one_cumulative_instruction_budget() {
    let work = "(local $n i32) i32.const 100000 local.set $n (loop $again local.get $n i32.const 1 i32.sub local.tee $n br_if $again)";
    let module = |alloc_work: &str, migration_work: &str| {
        wat::parse_str(format!("(module (memory (export \"memory\") 1 2) (func (export \"alloc\") (param i32) (result i32) {alloc_work} i32.const 1024) (func (export \"migrate\") (param i32 i32 i32 i32) (result i64) {migration_work} i64.const 0))")).unwrap()
    };
    run_migration(&module(work, ""), 1, 2, b"x").unwrap();
    run_migration(&module("", work), 1, 2, b"x").unwrap();
    assert_eq!(
        run_migration(&module(work, work), 1, 2, b"x")
            .unwrap_err()
            .code,
        "private_fuel_exhausted"
    );
}
