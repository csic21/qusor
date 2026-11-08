use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use qusor_wasm_probe::{
    MAX_WASM_BYTES, Options, Package, Probe, ProbeError, RunResult, SYNTHETIC_SELECTION,
};

fn package_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../packages")
        .join(name)
}

fn load(name: &str) -> Package {
    Package::load(&package_path(name)).unwrap_or_else(|error| {
        panic!("fixture package {name} is missing or invalid ({error}); build all author fixtures before running these tests")
    })
}

fn probe(name: &str, options: Options) -> Probe {
    Probe::new(load(name), options).unwrap()
}

#[test]
fn normal_package_and_independently_built_update_use_host_returned_text() {
    for (name, prefix) in [("valid", "[excerpt] "), ("valid-v2", "[excerpt v2] ")] {
        let result = probe(name, Options::default()).run().unwrap();
        assert_eq!(result.output, format!("{prefix}{SYNTHETIC_SELECTION}"));
        assert_eq!(result.calls, 1);
        assert!(result.fuel_consumed > 0);
    }
    assert_ne!(load("valid").digest, load("valid-v2").digest);
    assert!(load("valid-v2").manifest.package_version > load("valid").manifest.package_version);
}

#[test]
fn maximum_raw_output_accepts_nul_bytes_with_worst_case_json_expansion() {
    let result = probe("max-escaped-output", Options::default())
        .run()
        .unwrap();
    assert_eq!(result.output.len(), qusor_wasm_probe::MAX_RESPONSE_BYTES);
    assert!(result.output.bytes().all(|byte| byte == 0));
    assert_eq!(result.calls, 0);
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(encoded.len() > 6 * qusor_wasm_probe::MAX_RESPONSE_BYTES);
    let decoded: RunResult = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.output, result.output);
}

#[test]
fn adversarial_packages_fail_at_their_expected_boundary() {
    for (name, expected) in [
        ("cross-vault", "permission_denied"),
        ("cross-selection", "permission_denied"),
        ("unauthorized-capture", "permission_denied"),
        ("memory-grow", "vm_trap"),
        ("initial-memory", "instantiate_failed"),
        ("table-grow", "vm_trap"),
        ("ambient-import", "unknown_import"),
        ("invalid-pointer", "invalid_pointer"),
        ("invalid-output-pointer", "invalid_pointer"),
        ("oversized-request", "host_buffer_limit"),
        ("oversized-output", "output_size_limit"),
        ("start-function", "invalid_module"),
        ("deep-recursion", "vm_trap"),
    ] {
        let result = probe(name, Options::default()).run();
        assert_eq!(result.unwrap_err().code, expected, "fixture {name}");
    }
}

#[test]
fn infinite_guest_loop_is_stopped_by_total_instruction_budget() {
    let options = Options {
        total_fuel: 5_000,
        slice_fuel: 100,
        deadline: Duration::from_secs(2),
        ..Options::default()
    };
    let probe = probe("infinite-loop", options);
    let control = probe.control();
    assert_eq!(probe.run().unwrap_err().code, "fuel_exhausted");
    assert!(control.slices() > 1);
    assert_eq!(control.calls(), 0);
}

#[test]
fn guest_deadline_is_independent_of_a_very_large_total_fuel_budget() {
    let options = Options {
        total_fuel: u64::MAX,
        slice_fuel: 1_000,
        deadline: Duration::from_millis(250),
        ..Options::default()
    };
    let probe = probe("infinite-loop", options);
    let control = probe.control();
    let started = Instant::now();
    assert_eq!(probe.run().unwrap_err().code, "deadline_exceeded");
    assert!(control.slices() > 0);
    assert_eq!(control.calls(), 0);
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn interrupt_running_guest(revoke: bool) -> Result<RunResult, ProbeError> {
    let options = Options {
        total_fuel: u64::MAX,
        slice_fuel: 1_000,
        deadline: Duration::from_secs(2),
        ..Options::default()
    };
    let probe = probe("infinite-loop", options);
    let control = probe.control();
    let worker = thread::spawn(move || probe.run());
    let timeout = Instant::now() + Duration::from_secs(1);
    while control.slices() == 0 && !worker.is_finished() && Instant::now() < timeout {
        thread::sleep(Duration::from_millis(1));
    }
    let observed_running = control.slices() > 0;
    let revoke_result = if observed_running && revoke {
        control.revoke()
    } else {
        control.cancel();
        Ok(())
    };
    // On timeout or failed revocation, stop the worker before any assertion. This
    // prevents a failed synchronization assertion from abandoning a live guest.
    if !observed_running || revoke_result.is_err() {
        control.cancel();
    }
    let outcome = worker.join();
    assert!(
        observed_running,
        "guest did not reach a fuel slice before interruption timeout"
    );
    assert!(
        revoke_result.is_ok(),
        "revocation failed: {revoke_result:?}"
    );
    assert_eq!(
        control.calls(),
        0,
        "interruption must not depend on guest broker calls"
    );
    outcome.expect("guest worker panicked")
}

#[test]
fn cancelling_an_already_running_loop_stops_it_between_fuel_slices() {
    assert_eq!(
        interrupt_running_guest(false).unwrap_err().code,
        "cancelled"
    );
}

#[test]
fn revoking_an_already_running_loop_stops_it_without_a_broker_call() {
    assert_eq!(
        interrupt_running_guest(true).unwrap_err().code,
        "stale_context"
    );
}

#[test]
fn cancellation_and_revocation_are_also_checked_before_guest_execution() {
    let cancelled = probe("valid", Options::default());
    let control = cancelled.control();
    control.cancel();
    assert_eq!(cancelled.run().unwrap_err().code, "cancelled");
    assert_eq!(control.calls(), 0);
    let revoked = probe("valid", Options::default());
    let control = revoked.control();
    control.revoke().unwrap();
    assert_eq!(revoked.run().unwrap_err().code, "stale_context");
    assert_eq!(control.calls(), 0);
}

#[test]
fn broker_call_budget_is_enforced_on_the_second_attempt() {
    let options = Options {
        host_calls: 1,
        ..Options::default()
    };
    let probe = probe("multiple-calls", options);
    let control = probe.control();
    assert_eq!(probe.run().unwrap_err().code, "host_call_limit");
    assert_eq!(control.calls(), 2, "counter includes the rejected attempt");
    let allowed = probe_fixture_with_two_calls();
    assert_eq!(allowed.calls, 2);
    assert_eq!(allowed.output, format!("[excerpt] {SYNTHETIC_SELECTION}"));
}

fn probe_fixture_with_two_calls() -> RunResult {
    probe("multiple-calls", Options::default()).run().unwrap()
}

#[test]
fn zero_execution_budgets_are_rejected_before_execution() {
    for options in [
        Options {
            total_fuel: 0,
            ..Options::default()
        },
        Options {
            slice_fuel: 0,
            ..Options::default()
        },
        Options {
            host_calls: 0,
            ..Options::default()
        },
    ] {
        let error = Probe::new(load("valid"), options)
            .err()
            .expect("zero budget accepted");
        assert_eq!(error.code, "invalid_options");
    }
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct ScratchPackage(PathBuf);

impl ScratchPackage {
    fn copy_valid() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "qusor-wasm-package-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let scratch = Self(directory);
        for name in ["package.json", "manifest.json", "module.wasm"] {
            fs::copy(package_path("valid").join(name), scratch.0.join(name)).unwrap();
        }
        scratch
    }

    fn edit_descriptor(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = self.0.join("package.json");
        let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        edit(&mut value);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn error_code(&self) -> &'static str {
        Package::load(&self.0)
            .err()
            .expect("invalid package accepted")
            .code
    }
}

impl Drop for ScratchPackage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn module_tampering_is_rejected_before_compilation() {
    let package = ScratchPackage::copy_valid();
    let path = package.0.join("module.wasm");
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(0);
    fs::write(path, bytes).unwrap();
    assert_eq!(package.error_code(), "integrity_mismatch");
}

#[test]
fn descriptor_rejects_path_traversal_unknown_fields_versions_and_invalid_digest() {
    for (key, value) in [
        ("entry", serde_json::json!("../module.wasm")),
        ("entry", serde_json::json!("/tmp/module.wasm")),
        ("format_version", serde_json::json!(2)),
        ("sha256", serde_json::json!("A".repeat(64))),
        ("sha256", serde_json::json!("0".repeat(63))),
        ("unknown", serde_json::json!(true)),
    ] {
        let package = ScratchPackage::copy_valid();
        package.edit_descriptor(|descriptor| descriptor[key] = value);
        assert_eq!(package.error_code(), "invalid_package", "field {key}");
    }
}

#[test]
fn loader_limits_all_package_files_before_parsing_or_compilation() {
    for (file, size) in [
        ("package.json", 4097),
        ("manifest.json", 65537),
        ("module.wasm", MAX_WASM_BYTES + 1),
    ] {
        let package = ScratchPackage::copy_valid();
        fs::write(package.0.join(file), vec![b' '; size]).unwrap();
        assert_eq!(package.error_code(), "package_size_limit", "file {file}");
    }
}

#[test]
fn loader_rejects_invalid_manifest_and_non_wasm_input() {
    let package = ScratchPackage::copy_valid();
    fs::write(package.0.join("manifest.json"), b"{}").unwrap();
    assert_eq!(package.error_code(), "invalid_manifest");
    let package = ScratchPackage::copy_valid();
    fs::write(package.0.join("module.wasm"), b"not wasm").unwrap();
    assert_eq!(package.error_code(), "invalid_wasm");
}

#[test]
fn loader_rejects_directory_instead_of_package_file() {
    let package = ScratchPackage::copy_valid();
    let path = package.0.join("module.wasm");
    fs::remove_file(&path).unwrap();
    fs::create_dir(path).unwrap();
    assert_eq!(package.error_code(), "invalid_package_file");
}

#[cfg(unix)]
#[test]
fn loader_rejects_symlink_instead_of_package_file() {
    let package = ScratchPackage::copy_valid();
    let path = package.0.join("module.wasm");
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(package_path("valid").join("module.wasm"), path).unwrap();
    assert_eq!(package.error_code(), "invalid_package_file");
}

fn existing_authority(package: &Package, response: &[u8]) -> qusor_wasm_probe::ExistingAuthority {
    use extension_api::{Capability, CapabilityRequest, HOST_API, Resource};
    use extension_host::{Grant, Host};
    let resource = Resource::Selection {
        vault_id: "probe-vault".into(),
        selection_id: "selection-1".into(),
    };
    let mut host = Host::new(
        "probe-vault",
        [Capability::SelectionRead, Capability::CapturePropose],
    )
    .unwrap();
    let installation = host
        .register(package.manifest.clone(), package.digest.clone())
        .unwrap();
    host.replace_grants(
        installation,
        vec![Grant {
            capability: Capability::SelectionRead,
            resource: resource.clone(),
        }],
    )
    .unwrap();
    host.enable(installation).unwrap();
    let session = host.open_session(installation).unwrap();
    qusor_wasm_probe::ExistingAuthority {
        host,
        installation,
        session,
        request: CapabilityRequest {
            api_version: HOST_API,
            request_id: "external-adapter".into(),
            capability: Capability::SelectionRead,
            resource,
        },
        response: response.to_vec(),
        expires_at: None,
    }
}

#[test]
fn trusted_existing_authority_supplies_actual_service_bytes_without_new_grants() {
    let package = load("valid");
    let context = existing_authority(&package, b"External bounded response");
    let result = Probe::from_authority(package, Options::default(), context)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(result.output, "[excerpt] External bounded response");
    assert_eq!(result.calls, 1);
}

#[test]
fn existing_authority_rejects_stale_session_digest_mismatch_and_expiry() {
    let package = load("valid");
    let mut context = existing_authority(&package, b"must not be delivered");
    context
        .host
        .replace_grants(context.installation, vec![])
        .unwrap();
    assert!(Probe::from_authority(package, Options::default(), context).is_err());

    let mut package = load("valid");
    let context = existing_authority(&package, b"must not be delivered");
    package.digest = "f".repeat(64);
    assert!(Probe::from_authority(package, Options::default(), context).is_err());

    let package = load("valid");
    let mut context = existing_authority(&package, b"must not be delivered");
    context.expires_at = Some(1);
    let error = Probe::from_authority(package, Options::default(), context)
        .unwrap()
        .run()
        .unwrap_err();
    assert_eq!(error.code, "grant_expired");
}

#[test]
fn adapter_response_is_only_delivered_for_its_exact_request_scope() {
    let package = load("valid");
    let mut context = existing_authority(&package, b"must not be delivered");
    // A host may authorize a larger scope than one adapter response. The runner
    // must not silently send one resource's bytes to a request for another.
    context
        .host
        .replace_grants(
            context.installation,
            vec![
                extension_host::Grant {
                    capability: extension_api::Capability::SelectionRead,
                    resource: extension_api::Resource::Selection {
                        vault_id: "probe-vault".into(),
                        selection_id: "selection-1".into(),
                    },
                },
                extension_host::Grant {
                    capability: extension_api::Capability::SelectionRead,
                    resource: extension_api::Resource::Selection {
                        vault_id: "probe-vault".into(),
                        selection_id: "selection-2".into(),
                    },
                },
            ],
        )
        .unwrap();
    context.request.resource = extension_api::Resource::Selection {
        vault_id: "probe-vault".into(),
        selection_id: "selection-2".into(),
    };
    context.session = context.host.open_session(context.installation).unwrap();
    let error = Probe::from_authority(package, Options::default(), context)
        .unwrap()
        .run()
        .unwrap_err();
    assert_eq!(error.code, "capability_unavailable");
}

#[test]
fn existing_authority_cannot_mix_another_host_session_or_installation() {
    let package = load("valid");
    let mut context = existing_authority(&package, b"never delivered");
    let foreign = existing_authority(&package, b"foreign");
    context.session = foreign.session;
    assert!(Probe::from_authority(package, Options::default(), context).is_err());

    let package = load("valid");
    let mut context = existing_authority(&package, b"never delivered");
    let foreign = existing_authority(&package, b"foreign");
    context.installation = foreign.installation;
    assert!(Probe::from_authority(package, Options::default(), context).is_err());
}
