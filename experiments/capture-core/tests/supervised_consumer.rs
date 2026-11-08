//! Same-executable production process controller + persistent authority + actual Core.
mod supervised_support;
use extension_api::{ApiError, ErrorCode};
use extension_api::{HOST_API, HOST_API_V0_2};
use extension_host::durable::ManagedRun;
use extension_runtime_api::ExecutionLimits;
use runtime_process::{
    Controller, FrozenExecution, ProcessEvent, WorkerHandle, WorkerLaunch, WorkerOutcome,
    WorkerState,
};
use runtime_wasm::WasmiRuntime;
use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use supervised_support::*;

type Gate = Arc<Mutex<World>>;
struct Run {
    controller: Arc<Controller>,
    handle: WorkerHandle,
    managed: ManagedRun,
    package: String,
    frozen_module: Vec<u8>,
}
#[derive(Default, Debug)]
struct Observed {
    calls: usize,
    boot: bool,
    accepted: Option<String>,
    failure: Option<String>,
    rejected: bool,
    reaped: bool,
    call_microseconds: Vec<u128>,
    total_milliseconds: u128,
    cleanup_failure: Option<String>,
    cleanup_quarantined: bool,
}
#[derive(Default)]
struct Hooks {
    no_boot: bool,
    revoke_result: bool,
    raw: Option<Vec<u8>>,
    block: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    cleanup_lock: bool,
    raw_fast: bool,
}
fn start(
    gate: &Gate,
    package: &extension_package::package::VerifiedPackage,
    counter: u64,
    timeout: Duration,
) -> Run {
    start_on(
        gate,
        package,
        counter,
        timeout,
        Arc::new(Controller::new().unwrap()),
    )
}
fn start_on(
    gate: &Gate,
    package: &extension_package::package::VerifiedPackage,
    counter: u64,
    timeout: Duration,
    controller: Arc<Controller>,
) -> Run {
    let mut world = gate.lock().unwrap();
    let session = world.install(package);
    let owner = runtime_process::current().unwrap();
    let run = format!("0123456789abcdef0123456789abcdef-{counter}");
    let managed = ManagedRun::prepare(
        &mut world.host.lock().unwrap(),
        &world.store,
        &session,
        now(),
        &run,
    )
    .unwrap();
    let receipt = world.store.current(&package.manifest().package_id).unwrap();
    let frozen_module = world
        .store
        .current_package(&receipt)
        .unwrap()
        .module_bytes()
        .to_vec();
    world
        .store
        .prepare_worker_lease(&receipt, "0123456789abcdef0123456789abcdef", &run, &owner)
        .unwrap();
    world.store.mark_worker_spawn_attempted(&run).unwrap();
    let handle = controller
        .start(WorkerLaunch {
            run_id: run,
            startup_timeout: Duration::from_secs(2),
            execution_timeout: timeout,
        })
        .unwrap();
    Run {
        controller,
        handle,
        managed,
        package: package.manifest().package_id.clone(),
        frozen_module,
    }
}
fn revoke(gate: &Gate, run: &Run, operation: &str) {
    let mut world = gate.lock().unwrap();
    let current = world.store.current(&run.package).unwrap();
    world
        .store
        .stop_for_worker(
            operation,
            current.revision,
            &run.package,
            "revoke",
            Some(run.handle.run_id()),
        )
        .unwrap();
}
fn pump(
    gate: Gate,
    mut run: Run,
    module: Vec<u8>,
    limits: ExecutionLimits,
    mut hooks: Hooks,
) -> Observed {
    assert_eq!(
        module, run.frozen_module,
        "freeze must match current authenticated journal package"
    );
    eprintln!("supervised starting {}", run.package);
    let mut seen = Observed::default();
    let began = Instant::now();
    let end = began + Duration::from_secs(8);
    while Instant::now() < end {
        for event in run.controller.poll() {
            match event {
                ProcessEvent::Spawned { handle, .. } => {
                    assert_eq!(handle.run_id(), run.handle.run_id())
                }
                ProcessEvent::Guarded { handle, identity } => {
                    assert_eq!(handle.run_id(), run.handle.run_id());
                    {
                        let mut world = gate.lock().unwrap();
                        world
                            .store
                            .bind_worker_lease(handle.run_id(), identity)
                            .unwrap();
                    }
                    if hooks.no_boot {
                        revoke(&gate, &run, "revoke-before-boot");
                    }
                    let world = gate.lock().unwrap();
                    let controller = run.controller.clone();
                    let validation = run.managed.release_boot(
                        &mut world.host.lock().unwrap(),
                        &world.store,
                        now(),
                        || {
                            controller
                                .release_boot(
                                    &handle,
                                    FrozenExecution {
                                        module: module.clone(),
                                        limits: limits.clone(),
                                        request_limit: extension_runtime_api::MAX_RAW_REQUEST_BYTES,
                                    },
                                )
                                .map_err(|_| {
                                    ApiError::new(ErrorCode::InvalidState, "Boot queue rejected")
                                })
                        },
                    );
                    drop(world);
                    if validation.is_ok() {
                        seen.boot = true;
                    } else {
                        seen.rejected = true;
                        let _ = run.controller.terminate(&handle);
                    }
                }
                ProcessEvent::Ready { handle } => assert_eq!(handle.run_id(), run.handle.run_id()),
                ProcessEvent::Call {
                    handle,
                    call_id,
                    bytes,
                } => {
                    assert_eq!(handle.run_id(), run.handle.run_id());
                    seen.calls += 1;
                    if let Some((entered, release)) = hooks.block.take() {
                        entered.send(()).unwrap();
                        release.recv_timeout(Duration::from_secs(4)).unwrap();
                    }
                    let call_started = Instant::now();
                    let world = gate.lock().unwrap();
                    let response = if let Some(expected) = &hooks.raw {
                        assert_eq!(&bytes, expected);
                        if hooks.raw_fast {
                            Ok(vec![b'r'; 16])
                        } else {
                            world
                                .host
                                .lock()
                                .unwrap()
                                .validate_current(
                                    &world.store,
                                    run.managed.session(),
                                    now(),
                                    Some(handle.run_id()),
                                )
                                .map(|_| vec![b'r'; 4096])
                        }
                    } else {
                        world.broker(run.managed.session(), handle.run_id(), &bytes)
                    };
                    drop(world);
                    seen.call_microseconds
                        .push(call_started.elapsed().as_micros());
                    match response {
                        Ok(bytes) => {
                            let _ = run.controller.reply(&handle, call_id, bytes);
                        }
                        Err(_) => {
                            seen.rejected = true;
                            let _ = run.controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::ResultReceived { handle, outcome } => {
                    assert_eq!(handle.run_id(), run.handle.run_id());
                    if hooks.revoke_result {
                        revoke(&gate, &run, "revoke-before-result-accept");
                    }
                    let world = gate.lock().unwrap();
                    let accepted = run.managed.accept_result(
                        &mut world.host.lock().unwrap(),
                        &world.store,
                        now(),
                        || Ok(outcome),
                    );
                    drop(world);
                    match accepted {
                        Ok(WorkerOutcome::Completed(result)) => seen.accepted = Some(result.output),
                        Ok(WorkerOutcome::Failed(error)) => seen.failure = Some(error.code),
                        Err(_) => seen.rejected = true,
                    }
                }
                ProcessEvent::Failed { handle, error } => {
                    seen.failure = Some(format!("{error:?}"));
                    if run.controller.status(&handle).unwrap().state == WorkerState::NotSpawned {
                        run.managed.record_not_spawned().unwrap();
                        let mut world = gate.lock().unwrap();
                        run.managed
                            .finish(|| {
                                world
                                    .store
                                    .finish_worker_lease(handle.run_id())
                                    .map_err(|_| {
                                        ApiError::new(
                                            ErrorCode::InvalidState,
                                            "lease cleanup blocked",
                                        )
                                    })
                            })
                            .unwrap();
                        run.controller.forget(&handle).unwrap();
                        return seen;
                    }
                }
                ProcessEvent::Quarantined { error, .. } => {
                    panic!("unexpected quarantine for {}: {error:?}", run.package)
                }
                ProcessEvent::Reaped { handle, .. } => {
                    assert_eq!(handle.run_id(), run.handle.run_id());
                    seen.reaped = true;
                    run.managed.record_reaped().unwrap();
                    let mut world = gate.lock().unwrap();
                    if hooks.cleanup_lock {
                        let db = rusqlite::Connection::open(
                            world.scratch.0.join("packages/state.sqlite3"),
                        )
                        .unwrap();
                        db.execute_batch("BEGIN IMMEDIATE").unwrap();
                        let mut persistence_error = None;
                        let failed = run.managed.finish(|| {
                            world
                                .store
                                .finish_worker_lease(handle.run_id())
                                .map_err(|error| {
                                    persistence_error = Some(error.to_string());
                                    ApiError::new(ErrorCode::InvalidState, "lease cleanup blocked")
                                })
                        });
                        assert!(failed.is_err());
                        seen.cleanup_failure = persistence_error;
                        seen.cleanup_quarantined =
                            run.managed.phase() == extension_host::durable::RunPhase::Quarantined;
                        assert!(seen.cleanup_quarantined);
                        assert!(!world.store.worker_leases().unwrap().is_empty());
                        assert!(
                            world
                                .host
                                .lock()
                                .unwrap()
                                .activate(&world.store, &run.package, HOST_API, now())
                                .is_err()
                        );
                        assert_eq!(
                            run.controller.status(&handle).unwrap().state,
                            WorkerState::Reaped
                        );
                        db.execute_batch("ROLLBACK").unwrap();
                    }
                    run.managed
                        .finish(|| {
                            world
                                .store
                                .finish_worker_lease(handle.run_id())
                                .map_err(|_| {
                                    ApiError::new(ErrorCode::InvalidState, "lease cleanup blocked")
                                })
                        })
                        .unwrap();
                    run.controller.forget(&handle).unwrap();
                    assert!(world.store.worker_leases().unwrap().is_empty());
                    seen.total_milliseconds = began.elapsed().as_millis();
                    return seen;
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("production controller did not terminate within bounded test: {seen:?}");
}
fn core_metadata_and_call_limits() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let controller = Arc::new(Controller::new().unwrap());
    let mut results = vec![];
    for (counter, api, calls) in [
        (1, HOST_API, 1),
        (2, HOST_API_V0_2, 1),
        (3, HOST_API, 8),
        (4, HOST_API, 9),
    ] {
        let raw = request(api).to_json().unwrap().into_bytes();
        let wasm = module(&raw, calls);
        let package = signed(
            &format!("org.supervised.metadata{counter}"),
            api,
            wasm.clone(),
        );
        let run = start_on(
            &gate,
            &package,
            counter,
            Duration::from_secs(2),
            controller.clone(),
        );
        let seen = pump(
            gate.clone(),
            run,
            wasm,
            ExecutionLimits::default(),
            Hooks::default(),
        );
        eprintln!(
            "supervised metadata api={} calls={} observed={seen:?}",
            api.minor, calls
        );
        assert!(seen.reaped && seen.boot);
        if calls <= 8 {
            let value: serde_json::Value =
                serde_json::from_str(seen.accepted.as_ref().unwrap()).unwrap();
            assert_eq!(value["title"], "Actual supervised Core document");
            assert_eq!(value["version"], gate.lock().unwrap().version);
            assert_eq!(seen.calls, calls);
        } else {
            assert!(seen.accepted.is_none());
            assert_eq!(seen.calls, 8);
            assert_eq!(seen.failure.as_deref(), Some("host_call_limit"));
        }
        results.push(serde_json::json!({"api_minor":api.minor,"guest_calls":calls,"broker_calls":seen.calls,"reaped":seen.reaped,"failure":seen.failure,"call_microseconds":seen.call_microseconds,"total_milliseconds":seen.total_milliseconds,"vm_deadline_ms":250,"default_deadline_qualified":true}));
    }
    serde_json::json!({"case":"two_ordinary_signed_apis_and_8_9_calls","runs":results})
}
fn raw_transport() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let raw = vec![255u8; 65536];
    let wasm = module(&raw, 1);
    let package = signed("org.supervised.raw", HOST_API, wasm.clone());
    let run = start(&gate, &package, 5, Duration::from_secs(2));
    let seen = pump(
        gate,
        run,
        wasm,
        ExecutionLimits::default(),
        Hooks {
            raw: Some(raw),
            ..Hooks::default()
        },
    );
    let output = seen.accepted.as_ref().unwrap();
    assert_eq!(output.len(), 4096);
    assert!(output.bytes().all(|b| b == b'r'));
    assert_eq!(seen.calls, 1);
    assert!(seen.reaped);
    serde_json::json!({"case":"worst_json_64k_raw_request","request_bytes":65536,"response_bytes":4096,"note":"transport payload test; not a synthetic Core metadata service"})
}
fn default_call_budget() -> serde_json::Value {
    let mut records = vec![];
    for (counter, calls) in [(12, 8), (13, 9)] {
        let gate = Arc::new(Mutex::new(World::new()));
        let raw = b"budget transport probe".to_vec();
        let wasm = module(&raw, calls);
        let package = signed(
            &format!("org.supervised.budget{counter}"),
            HOST_API,
            wasm.clone(),
        );
        let run = start(&gate, &package, counter, Duration::from_secs(2));
        let seen = pump(
            gate,
            run,
            wasm,
            ExecutionLimits::default(),
            Hooks {
                raw: Some(raw),
                raw_fast: true,
                ..Hooks::default()
            },
        );
        assert_eq!(seen.calls, 8);
        if calls == 8 {
            assert_eq!(seen.accepted.as_ref().unwrap().len(), 16);
        } else {
            assert!(seen.accepted.is_none());
            assert_eq!(seen.failure.as_deref(), Some("host_call_limit"));
        }
        records.push(serde_json::json!({"requested_calls":calls,"calls":seen.calls,"vm_default_deadline_ms":250,"total_milliseconds":seen.total_milliseconds,"broker":"transport-only, no Core/capability service accessed","failure":seen.failure}));
    }
    serde_json::json!({"case":"default_250ms_raw_8_9_call_budget","runs":records})
}
fn revoked_boot_and_result() -> serde_json::Value {
    for (counter, no_boot) in [(6, true), (7, false)] {
        let gate = Arc::new(Mutex::new(World::new()));
        let wasm = module(request(HOST_API).to_json().unwrap().as_bytes(), 1);
        let package = signed("org.supervised.revoked", HOST_API, wasm.clone());
        let run = start(&gate, &package, counter, Duration::from_secs(2));
        let seen = pump(
            gate,
            run,
            wasm,
            ExecutionLimits::default(),
            Hooks {
                no_boot,
                revoke_result: !no_boot,
                ..Hooks::default()
            },
        );
        assert!(seen.rejected && seen.accepted.is_none() && seen.reaped);
        assert_eq!(seen.boot, !no_boot);
        assert_eq!(seen.calls, usize::from(!no_boot));
    }
    serde_json::json!({"case":"revocation_prevents_boot_and_rejects_received_result","passed":true})
}
fn blocked_broker_watchdog() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let wasm = module(request(HOST_API).to_json().unwrap().as_bytes(), 1);
    let package = signed("org.supervised.blocked", HOST_API, wasm.clone());
    let run = start(&gate, &package, 8, Duration::from_millis(300));
    let controller = run.controller.clone();
    let handle = run.handle.clone();
    let package_id = run.package.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker_gate = gate.clone();
    let pump_thread = thread::spawn(move || {
        pump(
            worker_gate,
            run,
            wasm,
            ExecutionLimits::default(),
            Hooks {
                block: Some((entered_tx, release_rx)),
                ..Hooks::default()
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let began = Instant::now();
    {
        let mut world = gate.lock().unwrap();
        let revision = world.store.current(&package_id).unwrap().revision;
        world
            .store
            .stop_for_worker(
                "management-during-blocked-broker",
                revision,
                &package_id,
                "revoke",
                Some(handle.run_id()),
            )
            .unwrap();
    }
    while controller.status(&handle).unwrap().state != WorkerState::Reaped {
        assert!(began.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !gate
            .lock()
            .unwrap()
            .store
            .worker_leases()
            .unwrap()
            .is_empty()
    );
    release_tx.send(()).unwrap();
    let seen = pump_thread.join().unwrap();
    assert!(seen.reaped && seen.rejected && seen.accepted.is_none());
    serde_json::json!({"case":"blocked_native_broker_independent_watchdog_and_management","milliseconds":began.elapsed().as_millis(),"management_committed_while_broker_blocked":true,"lease_retained_until_reaped_and_callback_returned":true})
}
fn cleanup_storage_failure() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let wasm = module(request(HOST_API).to_json().unwrap().as_bytes(), 1);
    let package = signed("org.supervised.cleanup", HOST_API, wasm.clone());
    let run = start(&gate, &package, 9, Duration::from_secs(2));
    let seen = pump(
        gate,
        run,
        wasm,
        ExecutionLimits::default(),
        Hooks {
            cleanup_lock: true,
            ..Hooks::default()
        },
    );
    assert!(seen.reaped && seen.accepted.is_some());
    serde_json::json!({"case":"real_sqlite_cleanup_busy_keeps_exclusion","passed":true,"sqlite_error":seen.cleanup_failure,"helper_quarantined":seen.cleanup_quarantined,"finished_after_rollback":true})
}
fn compute_watchdog_timeout() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let wasm=wat::parse_str(r#"(module(memory(export "memory")1 2)(func(export "run")(result i32 i32)(loop br 0) unreachable))"#).unwrap();
    let package = signed("org.supervised.compute", HOST_API, wasm.clone());
    let run = start(&gate, &package, 10, Duration::from_millis(100));
    let limits = ExecutionLimits {
        total_fuel: u64::MAX,
        slice_fuel: 10_000,
        deadline: Duration::from_secs(5),
        host_calls: 8,
    };
    let began = Instant::now();
    let seen = pump(gate, run, wasm, limits, Hooks::default());
    assert!(seen.reaped && seen.accepted.is_none() && seen.failure.is_some());
    assert!(began.elapsed() < Duration::from_secs(2));
    serde_json::json!({"case":"nonterminating_wasm_independent_process_deadline","milliseconds":began.elapsed().as_millis(),"failure":seen.failure})
}
fn crash_before_bind(observer: &std::path::Path) -> ! {
    let gate = Arc::new(Mutex::new(World::new()));
    let wasm = module(request(HOST_API).to_json().unwrap().as_bytes(), 1);
    let package = signed("org.supervised.orphan", HOST_API, wasm);
    let run = start(&gate, &package, 11, Duration::from_secs(2));
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        for event in run.controller.poll() {
            if let ProcessEvent::Spawned { pid, .. } = event {
                let observed = runtime_process::inspect(pid, run.handle.run_id()).unwrap();
                let world = gate.lock().unwrap();
                let value = serde_json::json!({"worker_observer_identity":observed,"store":world.scratch.0.join("packages"),"fixture_directory":world.scratch.0,"package":run.package,"run_id":run.handle.run_id()});
                let mut file = std::fs::File::create(observer).unwrap();
                use std::io::Write;
                file.write_all(value.to_string().as_bytes()).unwrap();
                file.sync_all().unwrap();
                // Real process exit deliberately skips Controller Drop and persistent bind.
                std::process::exit(73);
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("supervisor never reached actual Spawned window");
}
fn unknown_spawn_restart_exclusion() -> serde_json::Value {
    let scratch = Scratch::new("orphan-observer");
    let observer = scratch.0.join("observer.json");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--crash-before-bind")
        .arg(&observer)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(73));
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&observer).unwrap()).unwrap();
    let identity: extension_runtime_api::ProcessIdentity =
        serde_json::from_value(value["worker_observer_identity"].clone()).unwrap();
    let path = std::path::PathBuf::from(value["store"].as_str().unwrap());
    let mut store = extension_store::Store::open(&path).unwrap();
    let leases = store.worker_leases().unwrap();
    assert_eq!(leases.len(), 1);
    assert!(leases[0].spawn_attempted && leases[0].worker.is_none());
    assert!(
        store
            .execution_snapshot(value["package"].as_str().unwrap(), None)
            .is_err()
    );
    let owner = runtime_process::current().unwrap();
    let receipt = store.current(value["package"].as_str().unwrap()).unwrap();
    assert!(
        store
            .prepare_worker_lease(
                &receipt,
                "fedcba9876543210fedcba9876543210",
                "fedcba9876543210fedcba9876543210-1",
                &owner
            )
            .is_err()
    );
    let cleaned = runtime_process::cleanup(&identity).unwrap();
    assert!(matches!(
        cleaned,
        extension_runtime_api::CleanupOutcome::Terminated
            | extension_runtime_api::CleanupOutcome::Absent
    ));
    // Observer-only cleanup is not a production lease recovery proof; leave the ledger unresolved.
    assert_eq!(store.worker_leases().unwrap(), leases);
    drop(store);
    std::fs::remove_dir_all(value["fixture_directory"].as_str().unwrap()).unwrap();
    serde_json::json!({"case":"real_parent_exit_between_spawn_and_persistent_identity_bind","attempted_unknown_lease_survived_restart":true,"replacement_rejected":true,"observer_pidfd_cleanup":format!("{cleaned:?}"),"lease_not_automatically_cleared":true,"parent_exit_code":status.code(),"observer":value,"unknown_lease_before_and_after_cleanup":leases,"orphan_reaped_by_test":false})
}
fn real_source_version_mismatch() -> serde_json::Value {
    let gate = Arc::new(Mutex::new(World::new()));
    let (current, wrong) = {
        let mut world = gate.lock().unwrap();
        let current = world.version.clone();
        let draft:qusor_capture_probe::contract::CaptureDraft=serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"local","reference":"another-actual-source","title":"Other actual Core source","public_uri":null,"external_return":"none"},"snapshot_text":"Other immutable local source","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":5,"exact":"Other"}})).unwrap();
        let receipt = world
            .core
            .commit(
                "other-real-source",
                "vault",
                &draft,
                &qusor_capture_probe::core::Provenance {
                    package_id: "org.supervised.source".into(),
                    publisher_id: "org.supervised.author".into(),
                    package_digest: "b".repeat(64),
                    installation_id: 1,
                },
                "",
                None,
            )
            .unwrap();
        assert_ne!(current, receipt.snapshot_hash);
        world.version = receipt.snapshot_hash.clone();
        (current, receipt.snapshot_hash)
    };
    let wasm = module(request(HOST_API).to_json().unwrap().as_bytes(), 1);
    let package = signed("org.supervised.wrongversion", HOST_API, wasm.clone());
    let run = start(&gate, &package, 14, Duration::from_secs(2));
    let seen = pump(
        gate,
        run,
        wasm,
        ExecutionLimits::default(),
        Hooks::default(),
    );
    assert!(seen.boot && seen.reaped && seen.rejected && seen.accepted.is_none());
    assert_eq!(seen.calls, 1);
    serde_json::json!({"case":"real_core_source_version_mismatch_rejected","requested_document_current_version":current,"grant_other_actual_source_version":wrong,"returned_metadata":false})
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|a| a == "--crash-before-bind") {
        crash_before_bind(std::path::Path::new(&args[2]));
    }
    if args.get(1).is_some_and(|a| a == "--managed-worker")
        && args
            .get(4)
            .is_some_and(|run| run == "0123456789abcdef0123456789abcdef-11")
    {
        thread::sleep(Duration::from_secs(30));
        std::process::exit(74);
    }
    match runtime_process::worker_main(&WasmiRuntime) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("managed worker failed: {error}");
            std::process::exit(1);
        }
    }
    let selected = args
        .get(1)
        .filter(|s| s.as_str() == "--case")
        .and_then(|_| args.get(2))
        .map(String::as_str);
    let results = match selected {
        Some("metadata") => vec![core_metadata_and_call_limits()],
        Some("raw") => vec![raw_transport()],
        Some("budget") => vec![default_call_budget()],
        Some("revoke") => vec![revoked_boot_and_result()],
        Some("blocked") => vec![blocked_broker_watchdog()],
        Some("cleanup") => vec![cleanup_storage_failure()],
        Some("compute") => vec![compute_watchdog_timeout()],
        Some("unknown") => vec![unknown_spawn_restart_exclusion()],
        Some("version") => vec![real_source_version_mismatch()],
        Some(_) => panic!("unknown supervised case"),
        None => vec![
            core_metadata_and_call_limits(),
            raw_transport(),
            default_call_budget(),
            revoked_boot_and_result(),
            blocked_broker_watchdog(),
            cleanup_storage_failure(),
            compute_watchdog_timeout(),
            unknown_spawn_restart_exclusion(),
            real_source_version_mismatch(),
        ],
    };
    println!(
        "{}",
        serde_json::json!({"ok":true,"process_controller_consumer":results})
    );
}
