//! Real same-executable processes with a trusted protocol executor. These tests
//! validate transport/control, not Wasm execution; Core consumers use Wasmi.
use extension_runtime_api::{
    CapabilityBroker, ExecutionLimits, ExecutionResult, MAX_RAW_REQUEST_BYTES, MAX_RESPONSE_BYTES,
    ModuleExecutor, RuntimeError,
};
use runtime_process::*;
use std::{
    io::{self, BufRead, Write},
    thread,
    time::{Duration, Instant},
};

struct Executor;
impl ModuleExecutor for Executor {
    fn execute(
        &self,
        module: &[u8],
        _: &ExecutionLimits,
        _: usize,
        broker: &mut dyn CapabilityBroker,
    ) -> extension_runtime_api::Result<ExecutionResult> {
        let mut calls = 0;
        let output = match module[0] {
            1 => {
                calls = 1;
                String::from_utf8(broker.call(&vec![255; MAX_RAW_REQUEST_BYTES])?)
                    .map_err(|_| RuntimeError::new("invalid_output"))?
            }
            2 => {
                for _ in 0..8 {
                    broker.call(b"rq")?;
                    calls += 1;
                }
                "ok".into()
            }
            3 => {
                calls = 1;
                broker.call(b"block")?;
                "done".into()
            }
            _ => "\0".repeat(MAX_RESPONSE_BYTES),
        };
        Ok(ExecutionResult {
            output,
            calls,
            fuel_consumed: 0,
        })
    }
    fn migrate(
        &self,
        _: &[u8],
        _: u32,
        _: u32,
        _: &[u8],
    ) -> extension_runtime_api::Result<Vec<u8>> {
        unreachable!()
    }
    fn preflight(&self, _: &[u8], _: u32, _: &[u8]) -> extension_runtime_api::Result<()> {
        unreachable!()
    }
}
fn write(value: serde_json::Value) {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &value).unwrap();
    out.write_all(b"\n").unwrap();
    out.flush().unwrap();
}
fn malicious(counter: u64, args: &[String]) {
    let parent = args[2].parse().unwrap();
    let start = args[3].parse().unwrap();
    let run = &args[4];
    guard_worker(parent, start, run).unwrap();
    write(
        serde_json::json!({"kind":"guard_ready","protocol_version":if counter==105{99}else{PROCESS_PROTOCOL},"run_id":run}),
    );
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).unwrap() == 0 {
        return;
    }
    write(serde_json::json!({"kind":"ready","run_id":run}));
    let result = serde_json::json!({"kind":"result","run_id":run,"outcome":{"status":"completed","result":{"output":"untrusted candidate","calls":0,"fuel_consumed":0}}});
    if counter != 104 {
        write(result.clone());
    }
    match counter {
        101 => write(serde_json::json!({"kind":"call","run_id":run,"call_id":1,"bytes":[]})),
        102 => write(result),
        103 => std::process::exit(9),
        106 => {
            println!("not-json");
            io::stdout().flush().unwrap();
        }
        107 => thread::sleep(Duration::from_secs(30)),
        _ => {}
    }
}
fn run_id(n: u64) -> String {
    format!("{}-{n}", "c".repeat(32))
}
fn launch(n: u64) -> WorkerLaunch {
    WorkerLaunch {
        run_id: run_id(n),
        startup_timeout: Duration::from_secs(3),
        execution_timeout: Duration::from_millis(250),
    }
}
fn frozen(mode: u8) -> FrozenExecution {
    FrozenExecution {
        module: vec![mode],
        limits: ExecutionLimits::default(),
        request_limit: MAX_RAW_REQUEST_BYTES,
    }
}
fn wait(mut f: impl FnMut() -> bool) {
    let now = Instant::now();
    while now.elapsed() < Duration::from_secs(5) {
        if f() {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("process test timed out");
}
fn pump(
    controller: &Controller,
    handle: &WorkerHandle,
    mode: u8,
    expect_result: bool,
) -> Vec<&'static str> {
    let mut events = Vec::new();
    let mut reaped = false;
    wait(|| {
        for event in controller.poll() {
            match event {
                ProcessEvent::Guarded { handle: h, .. } => {
                    controller.release_boot(&h, frozen(mode)).unwrap();
                }
                ProcessEvent::Call {
                    handle: h,
                    call_id,
                    bytes,
                } => {
                    if mode == 1 {
                        assert_eq!(bytes, vec![255; MAX_RAW_REQUEST_BYTES]);
                    }
                    controller
                        .reply(
                            &h,
                            call_id,
                            if mode == 1 {
                                vec![0; MAX_RESPONSE_BYTES]
                            } else {
                                b"ok".to_vec()
                            },
                        )
                        .unwrap();
                }
                ProcessEvent::ResultReceived { outcome, .. } => {
                    assert!(expect_result, "invalid candidate escaped EOF/exit checks");
                    if let WorkerOutcome::Completed(result) = outcome {
                        if mode == 1 {
                            assert_eq!(result.output.as_bytes(), vec![0; MAX_RESPONSE_BYTES]);
                        }
                    } else {
                        panic!("unexpected failure outcome");
                    }
                    events.push("result");
                }
                ProcessEvent::Reaped { exit, .. } => {
                    if expect_result {
                        assert!(exit.success);
                    }
                    events.push("reaped");
                    reaped = true;
                }
                ProcessEvent::Quarantined { error, .. } => {
                    panic!("unexpected quarantine {error:?}")
                }
                ProcessEvent::Failed { .. } => {
                    assert!(!expect_result);
                    events.push("failed");
                }
                _ => {}
            }
        }
        reaped
    });
    assert_eq!(
        controller.status(handle).unwrap().state,
        WorkerState::Reaped
    );
    events
}
fn raw_roundtrip_and_ordering() {
    let controller = Controller::new().unwrap();
    let handle = controller.start(launch(1)).unwrap();
    assert_eq!(pump(&controller, &handle, 1, true), ["result", "reaped"]);
    controller.forget(&handle).unwrap();
    assert!(controller.start(launch(1)).is_err());
}
fn invalid_candidate_streams_never_escape() {
    for n in [101, 102, 103, 104, 105, 106, 107] {
        let controller = Controller::new().unwrap();
        let handle = controller.start(launch(n)).unwrap();
        let events = pump(&controller, &handle, 0, false);
        assert!(events.contains(&"failed"));
        assert_eq!(events.last(), Some(&"reaped"));
        controller.forget(&handle).unwrap();
    }
}
fn watchdog_and_drop_need_no_poll() {
    let controller = Controller::new().unwrap();
    let mut config = launch(10);
    config.execution_timeout = Duration::from_millis(60);
    let handle = controller.start(config).unwrap();
    wait(|| controller.status(&handle).unwrap().state == WorkerState::Guarded);
    controller.release_boot(&handle, frozen(3)).unwrap();
    wait(|| controller.status(&handle).unwrap().state == WorkerState::Reaped);
    assert_eq!(
        controller.status(&handle).unwrap().error.unwrap().code,
        "watchdog_timeout"
    );
    let controller = Controller::new().unwrap();
    let handle = controller.start(launch(20)).unwrap();
    wait(|| controller.status(&handle).unwrap().state == WorkerState::Guarded);
    let now = Instant::now();
    drop(controller);
    assert!(now.elapsed() < Duration::from_secs(2));
    assert_eq!(handle.status().unwrap().state, WorkerState::Reaped);
}
fn drop_does_not_turn_full_event_queue_into_fake_cleanup_failure() {
    let controller = Controller::new().unwrap();
    let handles: Vec<_> = (200..216)
        .map(|n| controller.start(launch(n)).unwrap())
        .collect();
    let mut booted = std::collections::BTreeSet::new();
    wait(|| {
        for h in &handles {
            let s = controller.status(h).unwrap();
            if s.state == WorkerState::Guarded && booted.insert(h.run_id().to_owned()) {
                let _ = controller.release_boot(h, frozen(2));
            }
            if let Some(call) = s.pending_call {
                let _ = controller.reply(h, call, b"ok".to_vec());
            }
        }
        handles.iter().all(|h| {
            matches!(
                controller.status(h).unwrap().state,
                WorkerState::Reaped | WorkerState::NotSpawned
            )
        })
    });
    let now = Instant::now();
    drop(controller);
    assert!(now.elapsed() < Duration::from_secs(2));
    for h in handles {
        assert!(matches!(
            h.status().unwrap().state,
            WorkerState::Reaped | WorkerState::NotSpawned
        ));
    }
}
fn main() {
    if !cfg!(target_os = "linux") {
        assert!(matches!(
            Controller::new(),
            Err(ProcessError {
                kind: ProcessErrorKind::Unsupported,
                ..
            })
        ));
        println!("Linux process controller scenarios not executed; Unsupported confirmed");
        return;
    }
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--managed-worker") {
        let n = args[4].rsplit_once('-').unwrap().1.parse::<u64>().unwrap();
        if (101..=107).contains(&n) {
            malicious(n, &args);
        } else {
            worker_main(&Executor).unwrap();
        }
        return;
    }
    raw_roundtrip_and_ordering();
    invalid_candidate_streams_never_escape();
    watchdog_and_drop_need_no_poll();
    drop_does_not_turn_full_event_queue_into_fake_cleanup_failure();
    println!("4 real process controller scenarios passed");
}
