//! Actual long-lived parent and child processes; no mock result substitutes.
use extension_api::{ApiRange, Capability, Manifest, PackageVersion};
use qusor_package_probe::{
    package::{PackageFile, build_signed, publisher_key, verify},
    store::{GrantScope, Store},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-supervisor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> PathBuf {
        self.0.join("store")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn setup(path: &Path, packages: &[(&str, &str)]) {
    let mut store = Store::open(path).unwrap();
    for (id, fixture) in packages {
        let manifest = Manifest {
            manifest_version: 1,
            package_id: (*id).into(),
            publisher_id: "org.test".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 1,
                max_minor: 1,
            },
            required_capabilities: vec![Capability::DocumentMetadataRead],
            optional_capabilities: vec![],
            contributions: vec![],
        };
        let module = if *fixture == "invalid" {
            wat::parse_str("(module (memory (export \"memory\") 1))").unwrap()
        } else if matches!(*fixture, "calls8" | "calls9") {
            let source = fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/metadata.wat"),
            )
            .unwrap();
            let call = "    i32.const 0\n    i32.const 198\n    i32.const 1024\n    i32.const 4096\n    call $call\n    local.set $length";
            assert!(source.contains(call));
            let count = if *fixture == "calls8" { 8 } else { 9 };
            wat::parse_str(source.replace(call, &vec![call; count].join("\n"))).unwrap()
        } else {
            wat::parse_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("fixtures")
                    .join(fixture),
            )
            .unwrap()
        };
        let bytes = build_signed(
            manifest,
            "module.wasm",
            vec![PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: module,
            }],
            &[9; 32],
        )
        .unwrap();
        let package = verify(&bytes, &publisher_key(&[9; 32])).unwrap();
        store
            .install(
                &format!("install-{}", id.replace('.', "-")),
                0,
                &package,
                None,
            )
            .unwrap();
        store
            .grant(
                &format!("grant-{}", id.replace('.', "-")),
                1,
                id,
                GrantScope {
                    capability: Capability::DocumentMetadataRead,
                    library: "synthetic-library".into(),
                    document: "synthetic-document".into(),
                    version: "version-1".into(),
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + 3600,
                },
                None,
            )
            .unwrap();
    }
}
struct Client {
    child: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    pending: Vec<Value>,
    request: usize,
    closed: bool,
}
impl Client {
    fn start(path: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_qusor-package-probe"))
            .arg("supervise")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let value: Value = serde_json::from_str(&line).unwrap();
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        let ready = output
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| {
                use std::io::Read;
                let mut text = String::new();
                if child.try_wait().ok().flatten().is_some()
                    && let Some(mut stderr) = child.stderr.take()
                {
                    let _ = stderr.read_to_string(&mut text);
                }
                panic!("supervisor did not become ready: {error}; stderr={text}")
            });
        assert_eq!(ready["event"], "ready");
        assert_eq!(ready["pid"], child.id());
        Self {
            child,
            input,
            output,
            pending: Vec::new(),
            request: 0,
            closed: false,
        }
    }
    fn request(&mut self, mut command: Value) -> Value {
        self.request += 1;
        let id = format!("request-{}", self.request);
        command["request_id"] = json!(id);
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
        loop {
            let event = self.output.recv_timeout(Duration::from_secs(4)).unwrap();
            if event["request_id"] == id {
                return event;
            }
            self.pending.push(event);
        }
    }
    fn run(&mut self, package: &str, fault: &str) -> String {
        let response = self.request(json!({"op":"start","package_id":package,
            "library":"synthetic-library","document":"synthetic-document","version":"version-1","fault":fault}));
        assert_eq!(response["ok"], true, "{response}");
        response["run_id"].as_str().unwrap().into()
    }
    fn event(&mut self, run: &str, wanted: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(index) = self
                .pending
                .iter()
                .position(|event| event["run_id"] == run && event["event"] == wanted)
            {
                return self.pending.remove(index);
            }
            let value = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            self.pending.push(value);
        }
    }
    fn close(&mut self) {
        if self.closed {
            return;
        }
        assert_eq!(self.request(json!({"op":"shutdown"}))["ok"], true);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                self.closed = true;
                return;
            }
            assert!(Instant::now() < deadline, "supervisor failed to shut down");
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if !self.closed {
            // Also runs during assertion unwinding, to reclaim fault-injected children.
            let _ = writeln!(
                self.input,
                "{}",
                json!({"op":"shutdown","request_id":"cleanup"})
            );
            let _ = self.input.flush();
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(2));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn blocked_broker_does_not_block_other_installation_or_management_and_revoke_persists() {
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    let mut client = Client::start(&temp.store());
    let b = client.run("org.test.b", "block_host_call");
    let a = client.run("org.test.a", "none");
    let completed = client.event(&a, "completed");
    assert_eq!(completed["result"]["calls"], 1);
    let metadata: Value =
        serde_json::from_str(completed["result"]["output"].as_str().unwrap()).unwrap();
    assert_eq!(metadata["synthetic"], true);
    let status = client.request(json!({"op":"status"}));
    assert!(
        status["status"]["active"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["run_id"] == b)
    );
    assert_eq!(
        client.request(json!({"op":"cancel","run_id":b}))["ok"],
        true
    );
    assert_eq!(client.event(&b, "failed")["code"], "cancelled");
    let next = client.run("org.test.b", "none");
    assert_ne!(b, next);
    client.event(&next, "completed");
    let blocked = client.run("org.test.b", "block_host_call");
    let revoked = client.request(json!({"op":"revoke","package_id":"org.test.b","operation_id":"revoke-b","expected_revision":2}));
    assert_eq!(revoked["ok"], true);
    assert_eq!(client.event(&blocked, "failed")["code"], "revoked");
    client.close();
    let mut store = Store::open(&temp.store()).unwrap();
    assert!(
        store
            .resume(
                "org.test.b",
                "synthetic-library",
                "synthetic-document",
                "version-1"
            )
            .is_err()
    );
}

#[test]
fn uncertain_kill_or_wait_quarantines_only_that_installation_until_confirmed_reap() {
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    let mut client = Client::start(&temp.store());
    for (fault, code) in [
        ("kill_failure", "worker_kill_failed"),
        ("wait_failure", "worker_wait_failed"),
    ] {
        let b = client.run("org.test.b", fault);
        assert_eq!(client.event(&b, "quarantined")["code"], code);
        let denied = client.request(
            json!({"op":"start","package_id":"org.test.b","library":"synthetic-library",
            "document":"synthetic-document","version":"version-1"}),
        );
        assert_eq!(denied["error"], "installation_quarantined");
        let a = client.run("org.test.a", "none");
        client.event(&a, "completed");
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let response = client.request(json!({"op":"reap","package_id":"org.test.b"}));
            assert_eq!(response["ok"], true);
            if response["reaped"] == true {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        let replacement = client.run("org.test.b", "none");
        assert_ne!(replacement, b);
        client.event(&replacement, "completed");
    }
    client.close();
}

#[test]
fn real_delayed_worker_message_is_rejected_after_replacement_has_completed() {
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let mut client = Client::start(&temp.store());
    let old = client.run("org.test.b", "delay_result");
    assert_eq!(client.event(&old, "failed")["code"], "watchdog_timeout");
    let replacement = client.run("org.test.b", "none");
    client.event(&replacement, "completed");
    assert_ne!(replacement, old);
    assert_eq!(
        client.event(&old, "stale_event")["code"],
        "stale_worker_event"
    );
    assert!(
        !client
            .pending
            .iter()
            .any(|event| event["run_id"] == old && event["event"] == "completed")
    );
    client.close();
}

#[test]
fn real_invalid_entrypoint_failures_are_bounded_and_other_installations_continue() {
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[("org.test.a", "metadata.wat"), ("org.test.bad", "invalid")],
    );
    let mut client = Client::start(&temp.store());
    for _ in 0..3 {
        let run = client.run("org.test.bad", "none");
        assert_eq!(client.event(&run, "failed")["code"], "invalid_entrypoint");
    }
    let denied = client.request(
        json!({"op":"start","package_id":"org.test.bad","library":"synthetic-library",
        "document":"synthetic-document","version":"version-1"}),
    );
    assert_eq!(denied["error"], "retry_limit_reached");
    let status = client.request(json!({"op":"status"}));
    assert!(
        status["status"]["needs_repair"]
            .as_array()
            .unwrap()
            .contains(&json!("org.test.bad"))
    );
    let a = client.run("org.test.a", "none");
    client.event(&a, "completed");
    client.close();
    let mut store = Store::open(&temp.store()).unwrap();
    assert!(
        store
            .resume(
                "org.test.bad",
                "synthetic-library",
                "synthetic-document",
                "version-1"
            )
            .is_err()
    );
}

#[test]
fn eight_real_calls_and_result_fit_protocol_budget_while_ninth_is_a_runtime_limit() {
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[("org.test.eight", "calls8"), ("org.test.nine", "calls9")],
    );
    let mut client = Client::start(&temp.store());
    let eight = client.run("org.test.eight", "none");
    let completed = client.event(&eight, "completed");
    assert_eq!(completed["result"]["calls"], 8);
    let value: Value =
        serde_json::from_str(completed["result"]["output"].as_str().unwrap()).unwrap();
    assert_eq!(value["synthetic"], true);
    let nine = client.run("org.test.nine", "none");
    assert_eq!(client.event(&nine, "failed")["code"], "host_call_limit");
    let next = client.run("org.test.eight", "none");
    assert_eq!(client.event(&next, "completed")["result"]["calls"], 8);
    client.close();
}

#[test]
fn historical_revoke_receipt_does_not_cancel_a_newly_granted_generation() {
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let old;
    {
        let mut store = Store::open(&temp.store()).unwrap();
        old = store
            .stop("old-revoke", 2, "org.test.b", "revoke", None)
            .unwrap();
        store
            .grant(
                "new-grant",
                old.revision,
                "org.test.b",
                GrantScope {
                    capability: Capability::DocumentMetadataRead,
                    library: "synthetic-library".into(),
                    document: "synthetic-document".into(),
                    version: "version-1".into(),
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + 3600,
                },
                None,
            )
            .unwrap();
    }
    let mut client = Client::start(&temp.store());
    let run = client.run("org.test.b", "block_host_call");
    let replay = client.request(json!({"op":"revoke","package_id":"org.test.b","operation_id":"old-revoke","expected_revision":2}));
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["receipt"]["generation"], old.generation);
    let status = client.request(json!({"op":"status"}));
    assert!(
        status["status"]["active"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["run_id"] == run)
    );
    assert_eq!(
        client.request(json!({"op":"cancel","run_id":run}))["ok"],
        true
    );
    assert_eq!(client.event(&run, "failed")["code"], "cancelled");
    client.close();
}

fn adopt_orphans() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        rustix::process::set_child_subreaper(Some(rustix::process::getpid())).unwrap();
    });
}
fn reap_adopted(pid: u32) {
    use rustix::process::{Pid, WaitOptions, waitpid};
    let pid = Pid::from_raw(pid as i32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match waitpid(Some(pid), WaitOptions::NOHANG) {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => panic!("adopted worker reap failed: {error}"),
        }
        assert!(Instant::now() < deadline, "orphan worker did not terminate");
        thread::sleep(Duration::from_millis(2));
    }
}
fn wait_parent_exit(client: &mut Client, code: Option<i32>) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = client.child.try_wait().unwrap() {
            assert_eq!(status.code(), code);
            client.closed = true;
            return;
        }
        assert!(
            Instant::now() < deadline,
            "crashed supervisor remained alive"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn trigger_launch_crash(client: &mut Client, point: &str) -> Value {
    let command = json!({"request_id":"crash","op":"start","package_id":"org.test.b",
        "library":"synthetic-library","document":"synthetic-document","version":"version-1",
        "fault":"kill_failure","crash_point":point});
    writeln!(client.input, "{command}").unwrap();
    client.input.flush().unwrap();
    loop {
        let event = client.output.recv_timeout(Duration::from_secs(3)).unwrap();
        if event["event"] == "launch_checkpoint" {
            assert_eq!(event["point"], point);
            wait_parent_exit(client, Some(86));
            return event;
        }
    }
}
fn assert_completed(client: &mut Client, package: &str) {
    let run = client.run(package, "none");
    assert_eq!(client.event(&run, "completed")["result"]["calls"], 1);
}
fn ready_worker(client: &mut Client, package: &str) -> (String, u32) {
    ready_worker_with_fault(client, package, "kill_failure")
}
fn ready_worker_with_fault(client: &mut Client, package: &str, fault: &str) -> (String, u32) {
    let run = client.run(package, fault);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = client.request(json!({"op":"status"}));
        if let Some(active) = status["status"]["active"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["run_id"] == run && row["transport_ready"] == true)
        {
            return (run, active["pid"].as_u64().unwrap() as u32);
        }
        assert!(Instant::now() < deadline, "worker did not receive its boot");
        thread::sleep(Duration::from_millis(2));
    }
}

fn close_unconfirmed(client: &mut Client) {
    assert_eq!(client.request(json!({"op":"shutdown"}))["ok"], true);
    loop {
        let event = client.output.recv_timeout(Duration::from_secs(3)).unwrap();
        if event["event"] == "shutdown_complete" {
            assert_eq!(event["confirmed"], false);
            break;
        }
    }
    wait_parent_exit(client, Some(1));
}

#[test]
fn launch_checkpoints_recover_without_replaying_or_reusing_old_authority() {
    adopt_orphans();
    for point in [
        "before_lease",
        "after_lease",
        "after_spawn_intent",
        "after_spawn",
        "after_bind",
        "after_boot",
    ] {
        let temp = Scratch::new();
        setup(
            &temp.store(),
            &[
                ("org.test.a", "metadata.wat"),
                ("org.test.b", "metadata.wat"),
            ],
        );
        let mut old = Client::start(&temp.store());
        let checkpoint = trigger_launch_crash(&mut old, point);
        {
            let mut store = Store::open(&temp.store()).unwrap();
            let leases = store.worker_leases().unwrap();
            assert_eq!(leases.len(), usize::from(point != "before_lease"));
            if point != "before_lease" {
                assert_eq!(leases[0].spawn_attempted, point != "after_lease");
                assert_eq!(
                    leases[0].worker.is_some(),
                    matches!(point, "after_bind" | "after_boot")
                );
                assert!(
                    store
                        .resume(
                            "org.test.b",
                            "synthetic-library",
                            "synthetic-document",
                            "version-1"
                        )
                        .is_err()
                );
                assert!(
                    store
                        .stop("cannot-disable", 2, "org.test.b", "disable", None)
                        .is_err()
                );
                assert!(
                    store
                        .uninstall("cannot-remove", 2, "org.test.b", None)
                        .is_err()
                );
                assert!(
                    store
                        .grant(
                            "cannot-authorize",
                            2,
                            "org.test.b",
                            GrantScope {
                                capability: Capability::DocumentMetadataRead,
                                library: "synthetic-library".into(),
                                document: "synthetic-document".into(),
                                version: "version-1".into(),
                                expires_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                    + 3600,
                            },
                            None
                        )
                        .is_err()
                );
            }
        }
        let mut recovered = Client::start(&temp.store());
        let status = recovered.request(json!({"op":"status"}));
        if let Some(pid) = checkpoint["pid"].as_u64() {
            reap_adopted(pid as u32);
        }
        assert_completed(&mut recovered, "org.test.a");
        if matches!(point, "after_spawn_intent" | "after_spawn") {
            assert_eq!(
                status["status"]["worker_leases"].as_array().unwrap().len(),
                1
            );
            assert_eq!(
                status["status"]["orphaned"]["org.test.b"],
                "unbound_spawn_uncertain"
            );
            assert_eq!(
                recovered.request(json!({"op":"recover","package_id":"org.test.b"}))["recovered"],
                false
            );
            let denied=recovered.request(json!({"op":"start","package_id":"org.test.b","library":"synthetic-library","document":"synthetic-document","version":"version-1"}));
            assert_eq!(denied["ok"], false);
            close_unconfirmed(&mut recovered);
        } else {
            assert!(
                status["status"]["worker_leases"]
                    .as_array()
                    .unwrap()
                    .is_empty(),
                "{point}: {status}"
            );
            assert!(status["status"]["orphaned"].as_object().unwrap().is_empty());
            assert_completed(&mut recovered, "org.test.b");
            recovered.close();
        }
    }
}

#[test]
fn supervisor_sigkill_terminates_guarded_worker_then_new_boot_coordinates_it() {
    adopt_orphans();
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    let mut old = Client::start(&temp.store());
    let (old_run, pid) = ready_worker(&mut old, "org.test.b");
    old.child.kill().unwrap();
    wait_parent_exit(&mut old, None);
    let lease = Store::open(&temp.store())
        .unwrap()
        .worker_leases()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(lease.run_id, old_run);
    assert_eq!(lease.worker.as_ref().unwrap().pid, pid);
    let mut recovered = Client::start(&temp.store());
    let status = recovered.request(json!({"op":"status"}));
    assert!(
        status["status"]["worker_leases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    reap_adopted(pid);
    assert_completed(&mut recovered, "org.test.b");
    assert_completed(&mut recovered, "org.test.a");
    recovered.close();
}

#[test]
fn mismatched_live_pid_is_not_signaled_and_only_its_recorded_installation_is_blocked() {
    adopt_orphans();
    let target = Scratch::new();
    let foreign = Scratch::new();
    setup(
        &target.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    setup(&foreign.store(), &[("org.test.foreign", "metadata.wat")]);
    let mut old = Client::start(&target.store());
    let checkpoint = trigger_launch_crash(&mut old, "after_bind");
    let old_pid = checkpoint["pid"].as_u64().unwrap() as u32;
    let original = Store::open(&target.store())
        .unwrap()
        .worker_leases()
        .unwrap()
        .pop()
        .unwrap();
    let mut innocent = Client::start(&foreign.store());
    let (_, foreign_pid) = ready_worker(&mut innocent, "org.test.foreign");
    let foreign_status = innocent.request(json!({"op":"status"}));
    let foreign_identity = &foreign_status["status"]["worker_leases"][0]["worker"];
    let mut tampered = serde_json::to_value(&original).unwrap();
    tampered["worker"]["pid"] = json!(foreign_pid);
    tampered["worker"]["start_time_ticks"] = foreign_identity["start_time_ticks"].clone();
    {
        let db = rusqlite::Connection::open(target.store().join("state.sqlite3")).unwrap();
        let typed: qusor_package_probe::store::WorkerLease =
            serde_json::from_value(tampered).unwrap();
        let text = serde_json::to_string(&typed).unwrap();
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2",
            rusqlite::params![text, hash],
        )
        .unwrap();
    }
    let mut blocked = Client::start(&target.store());
    let status = blocked.request(json!({"op":"status"}));
    assert_eq!(
        status["status"]["orphaned"]["org.test.b"],
        "identity_mismatch"
    );
    let denied = blocked.request(
        json!({"op":"start","package_id":"org.test.b","library":"synthetic-library",
        "document":"synthetic-document","version":"version-1"}),
    );
    assert_eq!(denied["ok"], false);
    assert_eq!(
        blocked.request(json!({"op":"recover","package_id":"org.test.b"}))["recovered"],
        false
    );
    assert_completed(&mut blocked, "org.test.a");
    assert!(Path::new(&format!("/proc/{foreign_pid}")).exists());
    assert_eq!(blocked.request(json!({"op":"shutdown"}))["ok"], true);
    wait_parent_exit(&mut blocked, Some(1));
    {
        let db = rusqlite::Connection::open(target.store().join("state.sqlite3")).unwrap();
        use sha2::{Digest, Sha256};
        let text = serde_json::to_string(&original).unwrap();
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2",
            rusqlite::params![text, hash],
        )
        .unwrap();
    }
    let mut repaired = Client::start(&target.store());
    assert_completed(&mut repaired, "org.test.b");
    repaired.close();
    reap_adopted(old_pid);
    innocent.close();
}

#[test]
fn revoke_commit_crash_fences_the_lease_and_never_restores_old_grants() {
    adopt_orphans();
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    let mut old = Client::start(&temp.store());
    let (old_run, pid) = ready_worker(&mut old, "org.test.b");
    let command = json!({"op":"revoke","request_id":"crash-revoke","package_id":"org.test.b",
        "operation_id":"revoke-before-crash","expected_revision":2,"crash_point":"after_revoke_commit"});
    writeln!(old.input, "{command}").unwrap();
    old.input.flush().unwrap();
    loop {
        let event = old.output.recv_timeout(Duration::from_secs(3)).unwrap();
        if event["event"] == "launch_checkpoint" {
            assert_eq!(event["point"], "after_revoke_commit");
            wait_parent_exit(&mut old, Some(86));
            break;
        }
    }
    {
        let mut store = Store::open(&temp.store()).unwrap();
        let lease = store.worker_leases().unwrap().pop().unwrap();
        assert_eq!(lease.run_id, old_run);
        assert_eq!(lease.fenced.as_ref().unwrap().generation, 3);
        assert_eq!(
            store
                .stop("revoke-before-crash", 2, "org.test.b", "revoke", None)
                .unwrap()
                .generation,
            3
        );
    }
    let mut recovered = Client::start(&temp.store());
    let state = recovered.request(json!({"op":"status"}));
    assert!(
        state["status"]["worker_leases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let denied = recovered.request(
        json!({"op":"start","package_id":"org.test.b","library":"synthetic-library",
        "document":"synthetic-document","version":"version-1"}),
    );
    assert_eq!(denied["ok"], false);
    assert_completed(&mut recovered, "org.test.a");
    reap_adopted(pid);
    recovered.close();
}

#[test]
fn actual_live_orphan_is_signaled_through_pidfd_and_confirmed_before_replacement() {
    use rustix::{
        event::{PollFd, PollFlags, Timespec, poll},
        process::{Pid, PidfdFlags, pidfd_open},
    };
    adopt_orphans();
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let mut old = Client::start(&temp.store());
    let (_, pid) = ready_worker_with_fault(&mut old, "org.test.b", "orphan_survivor");
    let fd = pidfd_open(Pid::from_raw(pid as i32).unwrap(), PidfdFlags::NONBLOCK).unwrap();
    old.child.kill().unwrap();
    wait_parent_exit(&mut old, None);
    let mut watched = [PollFd::new(&fd, PollFlags::IN)];
    let zero = Timespec::try_from(Duration::ZERO).unwrap();
    poll(&mut watched, Some(&zero)).unwrap();
    assert!(
        !watched[0]
            .revents()
            .intersects(PollFlags::IN | PollFlags::HUP),
        "fault did not leave an actual live orphan"
    );
    let mut recovered = Client::start(&temp.store());
    let status = recovered.request(json!({"op":"status"}));
    assert!(
        status["status"]["worker_leases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let limit = Timespec::try_from(Duration::from_secs(1)).unwrap();
    poll(&mut watched, Some(&limit)).unwrap();
    assert!(
        watched[0]
            .revents()
            .intersects(PollFlags::IN | PollFlags::HUP)
    );
    reap_adopted(pid);
    assert_completed(&mut recovered, "org.test.b");
    recovered.close();
}

#[test]
fn prepared_lease_hash_and_canonical_identity_prevent_corruption_looking_like_a_reboot() {
    use sha2::{Digest, Sha256};
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let mut old = Client::start(&temp.store());
    trigger_launch_crash(&mut old, "after_spawn_intent");
    let original = Store::open(&temp.store())
        .unwrap()
        .worker_leases()
        .unwrap()
        .pop()
        .unwrap();
    let original_text = serde_json::to_string(&original).unwrap();
    let original_hash = format!("{:x}", Sha256::digest(original_text.as_bytes()));
    let other_boot = if original.owner.boot_id != "00000000-0000-0000-0000-000000000000" {
        "00000000-0000-0000-0000-000000000000"
    } else {
        "11111111-1111-1111-1111-111111111111"
    };
    let mut bitrot = original.clone();
    bitrot.owner.boot_id = other_boot.into();
    {
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET record=?1",
            [serde_json::to_string(&bitrot).unwrap()],
        )
        .unwrap();
    }
    let error = Store::open(&temp.store())
        .err()
        .expect("valid-UUID bitrot must not clear the exclusion");
    assert!(error.to_string().contains("integrity mismatch"));
    let mut flag_bitrot = original.clone();
    flag_bitrot.spawn_attempted = false;
    {
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET record=?1",
            [serde_json::to_string(&flag_bitrot).unwrap()],
        )
        .unwrap();
    }
    assert!(
        Store::open(&temp.store())
            .err()
            .unwrap()
            .to_string()
            .contains("integrity mismatch")
    );
    {
        let mut invalid = original.clone();
        invalid.spawn_attempted = false;
        let mut impossible_worker = invalid.owner.clone();
        impossible_worker.run_id = invalid.run_id.clone();
        invalid.worker = Some(impossible_worker);
        let text = serde_json::to_string(&invalid).unwrap();
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2",
            rusqlite::params![text, hash],
        )
        .unwrap();
        drop(db);
        assert!(
            Store::open(&temp.store())
                .err()
                .unwrap()
                .to_string()
                .contains("binding")
        );
    }
    // The production sequence now accepts canonical nonzero u64, beyond the
    // probe's 256-run budget. Keep corruption cases invalid under that contract.
    for suffix in ["0", "000", "18446744073709551616"] {
        let mut invalid = original.clone();
        invalid.run_id = format!("{}-{suffix}", invalid.supervisor_boot);
        let text = serde_json::to_string(&invalid).unwrap();
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET run_id=?1,record=?2,record_hash=?3",
            rusqlite::params![invalid.run_id, text, hash],
        )
        .unwrap();
        drop(db);
        assert!(Store::open(&temp.store()).is_err());
    }
    {
        let mut invalid = original.clone();
        invalid.owner.boot_id = "-".repeat(36);
        let text = serde_json::to_string(&invalid).unwrap();
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET run_id=?1,record=?2,record_hash=?3",
            rusqlite::params![invalid.run_id, text, hash],
        )
        .unwrap();
        drop(db);
        assert!(Store::open(&temp.store()).is_err());
    }
    {
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET run_id=?1,record=?2,record_hash=?3",
            rusqlite::params![original.run_id, original_text, original_hash],
        )
        .unwrap();
    }
    let mut blocked = Client::start(&temp.store());
    assert_eq!(
        blocked.request(json!({"op":"status"}))["status"]["orphaned"]["org.test.b"],
        "unbound_spawn_uncertain"
    );
    close_unconfirmed(&mut blocked);
    // Coherent old-boot metadata is a controlled test input, not an actual reboot.
    {
        let text = serde_json::to_string(&bitrot).unwrap();
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        let db = rusqlite::Connection::open(temp.store().join("state.sqlite3")).unwrap();
        db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2",
            rusqlite::params![text, hash],
        )
        .unwrap();
    }
    let mut recovered = Client::start(&temp.store());
    assert!(
        recovered.request(json!({"op":"status"}))["status"]["worker_leases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_completed(&mut recovered, "org.test.b");
    recovered.close();
}

fn held_starting_worker(client: &mut Client) -> (String, u32) {
    let run = client.run("org.test.b", "hold_guard");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = client.request(json!({"op":"status"}));
        if let Some(row) = status["status"]["active"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["run_id"] == run && row["guard_ready"] == true)
        {
            assert_eq!(row["identity_bound"], false);
            assert_eq!(row["transport_ready"], false);
            assert_eq!(row["state"], "starting");
            assert!(status["status"]["worker_leases"][0]["worker"].is_null());
            return (run, row["pid"].as_u64().unwrap() as u32);
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn starting_workers_can_be_cancelled_or_durably_revoked_without_receiving_boot() {
    for action in ["cancel", "revoke"] {
        let temp = Scratch::new();
        setup(
            &temp.store(),
            &[
                ("org.test.a", "metadata.wat"),
                ("org.test.b", "metadata.wat"),
            ],
        );
        let mut client = Client::start(&temp.store());
        let (run, _) = held_starting_worker(&mut client);
        let reply = if action == "cancel" {
            client.request(json!({"op":"cancel","run_id":run}))
        } else {
            client.request(json!({"op":"revoke","package_id":"org.test.b","operation_id":"starting-revoke","expected_revision":2}))
        };
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            client.event(&run, "failed")["code"],
            if action == "cancel" {
                "cancelled"
            } else {
                "revoked"
            }
        );
        let status = client.request(json!({"op":"status"}));
        assert!(
            status["status"]["worker_leases"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_completed(&mut client, "org.test.a");
        if action == "cancel" {
            assert_completed(&mut client, "org.test.b");
        } else {
            let denied=client.request(json!({"op":"start","package_id":"org.test.b","library":"synthetic-library","document":"synthetic-document","version":"version-1"}));
            assert_eq!(denied["ok"], false);
        }
        client.close();
    }
}

#[test]
fn starting_revoke_commit_crash_keeps_fence_and_unknown_spawn_exclusion() {
    adopt_orphans();
    let temp = Scratch::new();
    setup(
        &temp.store(),
        &[
            ("org.test.a", "metadata.wat"),
            ("org.test.b", "metadata.wat"),
        ],
    );
    let mut old = Client::start(&temp.store());
    let (run, pid) = held_starting_worker(&mut old);
    let command = json!({"op":"revoke","request_id":"starting-revoke-crash","package_id":"org.test.b",
        "operation_id":"starting-revoke","expected_revision":2,"crash_point":"after_revoke_commit"});
    writeln!(old.input, "{command}").unwrap();
    old.input.flush().unwrap();
    loop {
        let message = old.output.recv_timeout(Duration::from_secs(3)).unwrap();
        if message["event"] == "launch_checkpoint" {
            wait_parent_exit(&mut old, Some(86));
            break;
        }
    }
    let mut store = Store::open(&temp.store()).unwrap();
    let lease = store.worker_leases().unwrap().pop().unwrap();
    assert_eq!(lease.run_id, run);
    assert!(lease.spawn_attempted);
    assert!(lease.worker.is_none());
    assert_eq!(lease.fenced.as_ref().unwrap().generation, 3);
    assert!(
        store
            .resume(
                "org.test.b",
                "synthetic-library",
                "synthetic-document",
                "version-1"
            )
            .is_err()
    );
    drop(store);
    let mut recovered = Client::start(&temp.store());
    assert_eq!(
        recovered.request(json!({"op":"status"}))["status"]["orphaned"]["org.test.b"],
        "unbound_spawn_uncertain"
    );
    assert_completed(&mut recovered, "org.test.a");
    reap_adopted(pid);
    close_unconfirmed(&mut recovered);
}

#[test]
fn expired_authorization_is_rechecked_before_boot_is_queued_after_a_held_guard() {
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 2;
    {
        let mut store = Store::open(&temp.store()).unwrap();
        store
            .grant(
                "short-lived",
                2,
                "org.test.b",
                GrantScope {
                    capability: Capability::DocumentMetadataRead,
                    library: "synthetic-library".into(),
                    document: "synthetic-document".into(),
                    version: "version-1".into(),
                    expires_at: expiry,
                },
                None,
            )
            .unwrap();
    }
    let mut client = Client::start(&temp.store());
    let (run, _) = held_starting_worker(&mut client);
    let deadline = Instant::now() + Duration::from_secs(4);
    while SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        < expiry
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let response = client.request(json!({"op":"release_guard","run_id":run}));
    assert_eq!(response["ok"], true);
    assert_eq!(response["released"], false);
    assert_eq!(response["code"], "stale_authority");
    let failed = client.event(&run, "failed");
    assert_eq!(failed["code"], "stale_authority");
    assert_eq!(
        failed["boot_queued"], false,
        "expiration must prevent delivery, not merely reject a later broker call"
    );
    let status = client.request(json!({"op":"status"}));
    assert!(
        status["status"]["worker_leases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.close();
}

#[test]
fn valid_authorization_can_release_held_guard_and_execute_the_same_frozen_module() {
    let temp = Scratch::new();
    setup(&temp.store(), &[("org.test.b", "metadata.wat")]);
    let mut client = Client::start(&temp.store());
    let (run, _) = held_starting_worker(&mut client);
    let response = client.request(json!({"op":"release_guard","run_id":run}));
    assert_eq!(response["ok"], true);
    assert_eq!(response["released"], true);
    assert_eq!(response["code"], "ok");
    let completed = client.event(&run, "completed");
    assert_eq!(completed["boot_queued"], true);
    assert_eq!(completed["result"]["calls"], 1);
    let metadata: Value =
        serde_json::from_str(completed["result"]["output"].as_str().unwrap()).unwrap();
    assert_eq!(metadata["synthetic"], true);
    client.close();
}
