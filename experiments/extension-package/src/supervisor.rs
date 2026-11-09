//! Linux managed-process experiment: the long-lived parent owns all authority.
//! Workers only receive frozen Wasm bytes and use a bounded capability IPC channel.
//! Processes isolate faults; this is not an OS permission sandbox.
use crate::process_identity::{self, CleanupOutcome, ProcessIdentity};
use crate::store::{Receipt, Result, Store};
use extension_api::CapabilityRequest;
use qusor_wasm_probe::{MAX_RESPONSE_BYTES, MAX_WASM_BYTES, ProbeError, Report};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{self, Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

const WATCHDOG: Duration = Duration::from_millis(600);
const HELD_GUARD_WATCHDOG: Duration = Duration::from_secs(5);
const DELIVERY_DELAY: Duration = Duration::from_millis(1000);
const MAX_FRAME: usize = 64 * 1024;
const MAX_MANAGEMENT_REPORT: usize = 1024 * 1024;
const MAX_BOOT: usize = 4 * MAX_WASM_BYTES + 4096;
const MAX_WORKERS: usize = 16;
const MAX_RUNS: u64 = 256;
const MAX_CALLS: u64 = 8;
const MAX_LAUNCH_FAILURES: u32 = 3;

/// Faults are trusted management/test inputs, never read from a package manifest.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    #[default]
    None,
    BlockHostCall,
    Crash,
    DelayResult,
    KillFailure,
    WaitFailure,
    LaunchFailure,
    OrphanSurvivor,
    HoldGuard,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Management {
    Start {
        request_id: String,
        package_id: String,
        library: String,
        document: String,
        version: String,
        #[serde(default)]
        fault: Fault,
        #[serde(default)]
        crash_point: Option<String>,
    },
    Status {
        request_id: String,
    },
    Cancel {
        request_id: String,
        run_id: String,
    },
    Revoke {
        request_id: String,
        package_id: String,
        operation_id: String,
        expected_revision: i64,
        #[serde(default)]
        crash_point: Option<String>,
    },
    Reap {
        request_id: String,
        package_id: String,
    },
    Recover {
        request_id: String,
        package_id: String,
    },
    ReleaseGuard {
        request_id: String,
        run_id: String,
    },
    Shutdown {
        request_id: String,
    },
}
impl Management {
    fn request_id(&self) -> &str {
        match self {
            Self::Start { request_id, .. }
            | Self::Status { request_id }
            | Self::Cancel { request_id, .. }
            | Self::Revoke { request_id, .. }
            | Self::Reap { request_id, .. }
            | Self::Recover { request_id, .. }
            | Self::ReleaseGuard { request_id, .. }
            | Self::Shutdown { request_id } => request_id,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Boot {
    run_id: String,
    module: Vec<u8>,
    fault: Fault,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WorkerMessage {
    GuardReady {
        run_id: String,
    },
    Ready {
        run_id: String,
    },
    Call {
        run_id: String,
        call_id: u64,
        request: CapabilityRequest,
    },
    Result {
        run_id: String,
        report: Report,
    },
}
impl WorkerMessage {
    fn run_id(&self) -> &str {
        match self {
            Self::GuardReady { run_id }
            | Self::Ready { run_id }
            | Self::Call { run_id, .. }
            | Self::Result { run_id, .. } => run_id,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    run_id: String,
    call_id: u64,
    bytes: Vec<u8>,
}
enum Transport {
    WriteFailed {
        expected: String,
    },
    Message {
        expected: String,
        message: WorkerMessage,
    },
    End {
        expected: String,
        malformed: bool,
    },
}

fn read_frame(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete IPC frame",
                ))
            };
        }
        let length = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if frame.len() + length > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC frame budget exceeded",
            ));
        }
        frame.extend_from_slice(&available[..length]);
        reader.consume(length);
        if frame.last() == Some(&b'\n') {
            return Ok(Some(frame));
        }
    }
}
fn write_frame(writer: &mut impl Write, value: &impl Serialize, limit: usize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame budget exceeded",
        ));
    }
    writer.write_all(&bytes)?;
    writer.flush()
}
fn read_worker(stdout: impl Read, expected: String, fault: Fault, sender: SyncSender<Transport>) {
    let mut reader = BufReader::new(stdout);
    for _ in 0..MAX_CALLS + 4 {
        let frame = match read_frame(&mut reader, MAX_FRAME) {
            Ok(Some(frame)) => frame,
            result => {
                let _ = sender.send(Transport::End {
                    expected,
                    malformed: result.is_err(),
                });
                return;
            }
        };
        let message = match serde_json::from_slice::<WorkerMessage>(&frame) {
            Ok(message) => message,
            Err(_) => {
                let _ = sender.send(Transport::End {
                    expected,
                    malformed: true,
                });
                return;
            }
        };
        if fault == Fault::DelayResult && matches!(message, WorkerMessage::Result { .. }) {
            // Real bytes have already arrived from this worker. Delay delivery to
            // exercise an old transport event arriving after that run is retired.
            thread::sleep(DELIVERY_DELAY);
        }
        if sender
            .send(Transport::Message {
                expected: expected.clone(),
                message,
            })
            .is_err()
        {
            return;
        }
    }
    let _ = sender.send(Transport::End {
        expected,
        malformed: true,
    });
}

fn write_worker(
    mut input: impl Write,
    frames: Receiver<Vec<u8>>,
    expected: String,
    sender: SyncSender<Transport>,
) {
    while let Ok(frame) = frames.recv() {
        if input
            .write_all(&frame)
            .and_then(|()| input.flush())
            .is_err()
        {
            let _ = sender.send(Transport::WriteFailed { expected });
            return;
        }
    }
}
fn queue_frame(sender: &SyncSender<Vec<u8>>, value: &impl Serialize, limit: usize) -> bool {
    let Ok(mut bytes) = serde_json::to_vec(value) else {
        return false;
    };
    bytes.push(b'\n');
    bytes.len() <= limit && sender.try_send(bytes).is_ok()
}

/// Trusted worker executable mode. It has no Store, Host, grant or file path.
pub fn worker_main() -> Result<bool> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err("managed worker requires parent identity and run nonce".into());
    }
    let parent_pid = args[2].parse()?;
    let parent_start = args[3].parse()?;
    let expected_run = args[4].clone();
    process_identity::guard_worker(parent_pid, parent_start, &expected_run)?;
    let mut output = io::stdout().lock();
    write_frame(
        &mut output,
        &WorkerMessage::GuardReady {
            run_id: expected_run.clone(),
        },
        MAX_FRAME,
    )?;
    let mut input = BufReader::new(io::stdin().lock());
    let boot: Boot =
        serde_json::from_slice(&read_frame(&mut input, MAX_BOOT)?.ok_or("missing boot")?)?;
    if boot.module.len() > MAX_WASM_BYTES || boot.run_id.len() > 96 || boot.run_id != expected_run {
        return Err("invalid worker boot".into());
    }
    if boot.fault == Fault::OrphanSurvivor {
        // Explicit trusted crash experiment only: exercise actual pidfd cleanup,
        // rather than only observing children killed by the default guard.
        #[cfg(target_os = "linux")]
        rustix::process::set_parent_process_death_signal(None)?;
        #[cfg(not(target_os = "linux"))]
        return Err("orphan-survivor fault profile is qualified only on Linux".into());
    }
    write_frame(
        &mut output,
        &WorkerMessage::Ready {
            run_id: boot.run_id.clone(),
        },
        MAX_FRAME,
    )?;
    if boot.fault == Fault::Crash {
        process::exit(87);
    }
    if matches!(
        boot.fault,
        Fault::KillFailure | Fault::WaitFailure | Fault::OrphanSurvivor
    ) {
        // Deliberate uncooperative native worker for process-control injection.
        thread::sleep(Duration::from_secs(60));
    }
    let mut next_call = 0;
    let result = qusor_wasm_probe::managed_runtime::run_managed(&boot.module, |request| {
        next_call += 1;
        let message = WorkerMessage::Call {
            run_id: boot.run_id.clone(),
            call_id: next_call,
            request,
        };
        write_frame(&mut output, &message, MAX_FRAME).map_err(|_| ProbeError {
            code: "managed_channel_failed",
        })?;
        let bytes = read_frame(&mut input, MAX_FRAME)
            .map_err(|_| ProbeError {
                code: "managed_channel_failed",
            })?
            .ok_or(ProbeError {
                code: "managed_channel_closed",
            })?;
        let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| ProbeError {
            code: "managed_channel_failed",
        })?;
        if reply.run_id != boot.run_id
            || reply.call_id != next_call
            || reply.bytes.len() > MAX_RESPONSE_BYTES
        {
            return Err(ProbeError {
                code: "managed_channel_failed",
            });
        }
        Ok(reply.bytes)
    });
    let report = Report::from_result(result);
    let ok = report.ok;
    write_frame(
        &mut output,
        &WorkerMessage::Result {
            run_id: boot.run_id,
            report,
        },
        MAX_FRAME,
    )?;
    Ok(ok)
}

#[derive(Serialize)]
struct RunStatus {
    run_id: String,
    package_id: String,
    pid: u32,
    state: &'static str,
    transport_ready: bool,
    guard_ready: bool,
    identity_bound: bool,
    boot_queued: bool,
}
struct Run {
    id: String,
    package_id: String,
    token: String,
    library: String,
    document: String,
    version: String,
    installation: Receipt,
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    fault: Fault,
    started: Instant,
    report: Option<Report>,
    exit: Option<ExitStatus>,
    stream_ended: bool,
    calls: u64,
    ready: bool,
    activated: bool,
    quarantined: bool,
    retiring: Option<(&'static str, Instant)>,
    guard_seen: bool,
    bound: bool,
    module: Option<Vec<u8>>,
    launch_crash: Option<String>,
    boot_queued: bool,
}
impl Run {
    fn status(&self) -> RunStatus {
        RunStatus {
            run_id: self.id.clone(),
            package_id: self.package_id.clone(),
            pid: self.child.id(),
            transport_ready: self.ready,
            guard_ready: self.guard_seen,
            identity_bound: self.bound,
            boot_queued: self.boot_queued,
            state: if self.quarantined {
                "quarantined"
            } else if self.retiring.is_some() {
                "retiring"
            } else if !self.activated {
                "starting"
            } else {
                "running"
            },
        }
    }
}

fn policy_code(error: &(dyn std::error::Error + 'static)) -> &'static str {
    match error
        .downcast_ref::<extension_api::ApiError>()
        .map(|error| error.code)
    {
        Some(extension_api::ErrorCode::PermissionDenied) => "permission_denied",
        Some(extension_api::ErrorCode::StaleContext) => "stale_authority",
        Some(extension_api::ErrorCode::UnsupportedVersion) => "unsupported_version",
        Some(extension_api::ErrorCode::CapabilityUnavailable) => "capability_unavailable",
        Some(_) => "policy_rejected",
        None => "authority_unavailable",
    }
}

/// One persistent owner of management state and multiple independently reaped workers.
pub struct Supervisor {
    store: Store,
    executable: PathBuf,
    boot: String,
    next_run: u64,
    runs: BTreeMap<String, Run>,
    sender: SyncSender<Transport>,
    receiver: Receiver<Transport>,
    events: Vec<Value>,
    launch_failures: BTreeMap<String, u32>,
    needs_repair: BTreeSet<String>,
    owner: ProcessIdentity,
    orphaned: BTreeMap<String, String>,
}
impl Supervisor {
    pub fn open(path: &Path, executable: PathBuf) -> Result<Self> {
        let store = Store::open(path)?;
        let mut entropy = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut entropy)?;
        let boot: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
        let (sender, receiver) = mpsc::sync_channel(64);
        let owner = process_identity::current()?;
        let mut supervisor = Self {
            store,
            executable,
            boot,
            next_run: 0,
            runs: BTreeMap::new(),
            sender,
            receiver,
            events: Vec::new(),
            launch_failures: BTreeMap::new(),
            needs_repair: BTreeSet::new(),
            owner,
            orphaned: BTreeMap::new(),
        };
        let packages: Vec<_> = supervisor
            .store
            .worker_leases()?
            .into_iter()
            .map(|lease| lease.installation.package_id)
            .collect();
        for package in packages {
            let _ = supervisor.recover_orphan(&package);
        }
        Ok(supervisor)
    }
    fn current(&self, package: &str) -> Result<Receipt> {
        self.store
            .status()?
            .into_iter()
            .find(|item| item.package_id == package)
            .ok_or_else(|| "installation not found".into())
    }
    fn recover_orphan(&mut self, package: &str) -> Result<bool> {
        if self.runs.contains_key(package) {
            return Err("installation_is_managed".into());
        }
        let lease = self
            .store
            .worker_leases()?
            .into_iter()
            .find(|lease| lease.installation.package_id == package);
        let Some(lease) = lease else {
            self.orphaned.remove(package);
            return Ok(true);
        };
        let outcome = if lease.owner.boot_id != self.owner.boot_id {
            "prior_kernel_boot"
        } else if let Some(worker) = &lease.worker {
            match process_identity::cleanup(worker) {
                Ok(CleanupOutcome::Terminated) => "terminated",
                Ok(CleanupOutcome::Absent) => "absent",
                Ok(CleanupOutcome::Mismatch) => {
                    self.orphaned
                        .insert(package.into(), "identity_mismatch".into());
                    return Ok(false);
                }
                Ok(CleanupOutcome::Uncertain) => {
                    self.orphaned
                        .insert(package.into(), "termination_unconfirmed".into());
                    return Ok(false);
                }
                Err(_) => {
                    self.orphaned
                        .insert(package.into(), "process_identity_unavailable".into());
                    return Ok(false);
                }
            }
        } else if !lease.spawn_attempted {
            "unstarted_intent"
        } else {
            self.orphaned
                .insert(package.into(), "unbound_spawn_uncertain".into());
            return Ok(false);
        };
        if self.store.finish_worker_lease(&lease.run_id).is_err() {
            self.orphaned
                .insert(package.into(), "recovery_commit_unconfirmed".into());
            return Ok(false);
        }
        self.orphaned.remove(package);
        self.events.push(json!({"event":"orphan_recovered","package_id":package,"run_id":lease.run_id,"code":outcome}));
        Ok(true)
    }
    fn start(
        &mut self,
        package: &str,
        scope: (&str, &str, &str),
        fault: Fault,
        crash: Option<&str>,
    ) -> Result<Value> {
        validate_launch_crash(crash)?;
        if self.orphaned.contains_key(package) {
            return Err("installation_has_unresolved_orphan".into());
        }
        if self.needs_repair.contains(package) {
            return Err("retry_limit_reached".into());
        }
        if let Some(existing) = self.runs.get(package) {
            return Err(if existing.quarantined {
                "installation_quarantined"
            } else {
                "installation_busy"
            }
            .into());
        }
        if self.runs.len() >= MAX_WORKERS || self.next_run >= MAX_RUNS {
            return Err("supervisor_budget_exhausted".into());
        }
        let token = self.store.resume(package, scope.0, scope.1, scope.2)?;
        let (installation, module) =
            match self.store.managed_module(&token, scope.0, scope.1, scope.2) {
                Ok(value) => value,
                Err(error) => {
                    self.store.managed_invalidate(&token);
                    return Err(error);
                }
            };
        self.next_run += 1;
        let id = format!("{}-{}", self.boot, self.next_run);
        launch_crash(crash, "before_lease", &id, None);
        self.store
            .prepare_worker_lease(&installation, &self.boot, &id, &self.owner)?;
        launch_crash(crash, "after_lease", &id, None);
        self.store.mark_worker_spawn_attempted(&id)?;
        launch_crash(crash, "after_spawn_intent", &id, None);
        let spawned = if fault == Fault::LaunchFailure {
            Err(io::Error::other("injected launch failure"))
        } else {
            Command::new(&self.executable)
                .arg("--managed-worker")
                .arg(self.owner.pid.to_string())
                .arg(self.owner.start_time_ticks.to_string())
                .arg(&id)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        };
        let mut child = match spawned {
            Ok(child) => child,
            Err(_) => {
                self.store.managed_invalidate(&token);
                self.store.finish_worker_lease(&id)?;
                self.note_launch_failure(package)?;
                return Err("worker_launch_failed".into());
            }
        };
        launch_crash(crash, "after_spawn", &id, Some(child.id()));
        let output = child.stdout.take().ok_or("worker_output_failed")?;
        let input = child.stdin.take().ok_or("worker_input_failed")?;
        let (write_sender, write_receiver) = mpsc::sync_channel(2);
        let write_expected = id.clone();
        let writer_events = self.sender.clone();
        thread::spawn(move || write_worker(input, write_receiver, write_expected, writer_events));
        let reader_sender = self.sender.clone();
        let expected = id.clone();
        thread::spawn(move || read_worker(output, expected, fault, reader_sender));
        let pid = child.id();
        let run = Run {
            id: id.clone(),
            package_id: package.into(),
            token,
            library: scope.0.into(),
            document: scope.1.into(),
            version: scope.2.into(),
            installation,
            child,
            input: Some(write_sender),
            fault,
            started: Instant::now(),
            report: None,
            exit: None,
            stream_ended: false,
            calls: 0,
            ready: false,
            activated: false,
            quarantined: false,
            retiring: None,
            guard_seen: false,
            bound: false,
            module: Some(module),
            launch_crash: crash.map(str::to_owned),
            boot_queued: false,
        };
        self.runs.insert(package.into(), run);
        Ok(json!({"run_id":id,"pid":pid}))
    }
    fn note_launch_failure(&mut self, package: &str) -> Result<()> {
        let failures = self.launch_failures.entry(package.into()).or_default();
        *failures += 1;
        if *failures >= MAX_LAUNCH_FAILURES {
            let current = self.current(package)?;
            let operation = format!("supervisor-{}-{}", self.boot, self.next_run);
            self.needs_repair.insert(package.into());
            let lease = self.store.worker_leases()?.into_iter().find(|lease| {
                lease.installation.package_id == package
                    && lease.supervisor_boot == self.boot
                    && lease.spawn_attempted
            });
            self.store.stop_for_worker(
                &operation,
                current.revision,
                package,
                "disable",
                None,
                lease.as_ref().map(|lease| lease.run_id.as_str()),
            )?;
        }
        Ok(())
    }
    fn status(&self) -> Result<Value> {
        let active: Vec<_> = self
            .runs
            .values()
            .filter(|run| !run.quarantined)
            .map(Run::status)
            .collect();
        let quarantined: Vec<_> = self
            .runs
            .values()
            .filter(|run| run.quarantined)
            .map(Run::status)
            .collect();
        Ok(
            json!({"active":active,"quarantined":quarantined,"launch_failures":self.launch_failures,
            "needs_repair":self.needs_repair,"installations":self.store.status()?,"orphaned":self.orphaned,"worker_leases":self.store.worker_leases()?}),
        )
    }
    fn terminal(&mut self, mut run: Run, code: &str, report: Option<Report>) {
        if self.store.finish_worker_lease(&run.id).is_err() {
            self.quarantine(run, "worker_lease_commit_failed");
            return;
        }
        self.store.managed_invalidate(&run.token);
        run.input.take();
        if let Some(report) = report.filter(|report| report.ok) {
            self.events.push(
                json!({"event":"completed","run_id":run.id,"package_id":run.package_id,
                "code":"ok","result":report.result,"boot_queued":run.boot_queued}),
            );
        } else {
            self.events.push(
                json!({"event":"failed","run_id":run.id,"package_id":run.package_id,"code":code,"boot_queued":run.boot_queued}),
            );
        }
    }
    fn quarantine(&mut self, mut run: Run, code: &str) {
        self.store.managed_invalidate(&run.token);
        run.input.take();
        run.quarantined = true;
        run.report = None;
        self.events.push(
            json!({"event":"quarantined","run_id":run.id,"package_id":run.package_id,"code":code}),
        );
        self.runs.insert(run.package_id.clone(), run);
    }
    fn terminate(&mut self, package: &str, code: &'static str) {
        let Some(mut run) = self.runs.remove(package) else {
            return;
        };
        self.store.managed_invalidate(&run.token);
        run.input.take();
        if run.fault == Fault::KillFailure {
            self.quarantine(run, "worker_kill_failed");
            return;
        }
        if run.child.kill().is_err() {
            match run.child.try_wait() {
                Ok(Some(_)) if run.fault != Fault::WaitFailure => {
                    self.terminal(run, code, None);
                    return;
                }
                _ => {
                    self.quarantine(run, "worker_kill_failed");
                    return;
                }
            }
        }
        if run.fault == Fault::WaitFailure {
            self.quarantine(run, "worker_wait_failed");
            return;
        }
        match run.child.try_wait() {
            Ok(Some(_)) => self.terminal(run, code, None),
            Ok(None) => {
                // Keep ownership until a later poll confirms exit. Logical authority
                // is already revoked, so no result can be accepted in this interval.
                run.report = None;
                run.retiring = Some((code, Instant::now()));
                self.runs.insert(package.into(), run);
            }
            Err(_) => self.quarantine(run, "worker_wait_failed"),
        }
    }
    fn reap(&mut self, package: &str) -> Result<bool> {
        let Some(mut run) = self.runs.remove(package) else {
            return Ok(true);
        };
        if !run.quarantined {
            self.runs.insert(package.into(), run);
            return Err("installation_not_quarantined".into());
        }
        // Explicit recovery clears only the injected control fault, not authority.
        run.fault = Fault::None;
        let _ = run.child.kill();
        match run.child.try_wait() {
            Ok(Some(_)) => {
                if let Err(error) = self.store.finish_worker_lease(&run.id) {
                    self.runs.insert(package.into(), run);
                    return Err(error);
                }
                Ok(true)
            }
            Ok(None) => {
                self.runs.insert(package.into(), run);
                Ok(false)
            }
            Err(_) => {
                self.runs.insert(package.into(), run);
                Err("worker_wait_failed".into())
            }
        }
    }
    fn identity_valid(&mut self, run: &Run) -> bool {
        self.store
            .managed_validate(
                &run.token,
                &run.id,
                &run.library,
                &run.document,
                &run.version,
            )
            .is_ok()
            && self.current(&run.package_id).is_ok_and(|current| {
                current.installation_id == run.installation.installation_id
                    && current.digest == run.installation.digest
                    && current.generation == run.installation.generation
            })
    }
    fn finish_guard(&mut self, run: &mut Run) -> Option<&'static str> {
        let binding = process_identity::inspect(run.child.id(), &run.id)
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
            .and_then(|identity| self.store.bind_worker_lease(&run.id, identity));
        if binding.is_err() {
            return Some("worker_identity_failed");
        }
        run.bound = true;
        launch_crash(
            run.launch_crash.as_deref(),
            "after_bind",
            &run.id,
            Some(run.child.id()),
        );
        // Authorization may expire or be fenced while a native guard handshake is pending.
        // No executable bytes cross this boundary until the current pure Host gate passes.
        if !self.identity_valid(run) {
            return Some("stale_authority");
        }
        let boot = Boot {
            run_id: run.id.clone(),
            module: run.module.take().expect("gated module"),
            fault: run.fault,
        };
        if run
            .input
            .as_ref()
            .is_none_or(|input| !queue_frame(input, &boot, MAX_BOOT))
        {
            return Some("worker_boot_failed");
        }
        run.boot_queued = true;
        launch_crash(
            run.launch_crash.as_deref(),
            "after_boot",
            &run.id,
            Some(run.child.id()),
        );
        None
    }
    fn release_guard(&mut self, run_id: &str) -> Result<Value> {
        let package = self
            .runs
            .iter()
            .find(|(_, run)| run.id == run_id)
            .map(|(package, _)| package.clone())
            .ok_or("run not found")?;
        let mut run = self.runs.remove(&package).expect("located run");
        if run.fault != Fault::HoldGuard
            || !run.guard_seen
            || run.bound
            || run.quarantined
            || run.retiring.is_some()
        {
            self.runs.insert(package, run);
            return Err("guard_release_not_available".into());
        }
        let failure = self.finish_guard(&mut run);
        self.runs.insert(package.clone(), run);
        if let Some(code) = failure {
            if code != "stale_authority" {
                let _ = self.note_launch_failure(&package);
            }
            self.terminate(&package, code);
        }
        Ok(json!({"released":failure.is_none(),"code":failure.unwrap_or("ok")}))
    }
    fn message(&mut self, expected: String, message: WorkerMessage) {
        let package = self
            .runs
            .iter()
            .find(|(_, run)| run.id == expected)
            .map(|(package, _)| package.clone());
        let Some(package) = package else {
            self.events
                .push(json!({"event":"stale_event","run_id":expected,"code":"stale_worker_event"}));
            return;
        };
        let mut run = self.runs.remove(&package).expect("located run");
        if run.quarantined || run.retiring.is_some() {
            self.events
                .push(json!({"event":"stale_event","run_id":expected,"code":"stale_worker_event"}));
            self.runs.insert(package, run);
            return;
        }
        if message.run_id() != run.id {
            self.runs.insert(package.clone(), run);
            self.terminate(&package, "stale_authority");
            return;
        }
        if matches!(&message, WorkerMessage::GuardReady { .. }) {
            let failure = if run.guard_seen || run.bound {
                Some("worker_protocol_failed")
            } else {
                run.guard_seen = true;
                if run.fault == Fault::HoldGuard {
                    None
                } else {
                    self.finish_guard(&mut run)
                }
            };
            self.runs.insert(package.clone(), run);
            if let Some(code) = failure {
                if code != "stale_authority" {
                    let _ = self.note_launch_failure(&package);
                }
                self.terminate(&package, code);
            }
            return;
        }
        if !run.bound || !self.identity_valid(&run) {
            self.runs.insert(package.clone(), run);
            self.terminate(&package, "stale_authority");
            return;
        }
        let failure = match message {
            WorkerMessage::GuardReady { .. } => unreachable!("guard handshake already handled"),
            WorkerMessage::Ready { .. } => {
                if run.ready || run.calls != 0 || run.report.is_some() {
                    Some("worker_protocol_failed")
                } else {
                    run.ready = true;
                    None
                }
            }
            WorkerMessage::Call {
                call_id, request, ..
            } => {
                if !run.ready
                    || call_id != run.calls + 1
                    || run.calls >= MAX_CALLS
                    || run.report.is_some()
                {
                    Some("worker_protocol_failed")
                } else {
                    run.calls += 1;
                    self.launch_failures.remove(&package);
                    match self.store.managed_call(
                        &run.token,
                        &run.id,
                        &run.library,
                        &run.document,
                        &run.version,
                        request,
                    ) {
                        Ok(bytes) => {
                            run.activated = true;
                            if run.fault == Fault::BlockHostCall {
                                None // Simulated pending broker I/O after actual authorization.
                            } else {
                                let reply = Reply {
                                    run_id: run.id.clone(),
                                    call_id,
                                    bytes,
                                };
                                if run
                                    .input
                                    .as_ref()
                                    .is_none_or(|input| !queue_frame(input, &reply, MAX_FRAME))
                                {
                                    Some("worker_channel_failed")
                                } else {
                                    None
                                }
                            }
                        }
                        Err(error) => Some(policy_code(error.as_ref())),
                    }
                }
            }
            WorkerMessage::Result { report, .. } => {
                let valid = report.ok == report.result.is_some()
                    && report.result.as_ref().is_none_or(|result| {
                        result.output.len() <= MAX_RESPONSE_BYTES
                            && result.calls as u64 == run.calls
                            && result.fuel_consumed <= 1_000_000
                    })
                    && report.code.len() <= 128;
                if !run.ready || run.report.is_some() || !valid {
                    Some("worker_protocol_failed")
                } else {
                    run.report = Some(report);
                    None
                }
            }
        };
        self.runs.insert(package.clone(), run);
        if let Some(code) = failure {
            self.terminate(&package, code);
        }
    }
    pub fn poll(&mut self) {
        for _ in 0..64 {
            match self.receiver.try_recv() {
                Ok(Transport::WriteFailed { expected }) => {
                    let pending = self
                        .runs
                        .iter()
                        .find(|(_, run)| {
                            run.id == expected && !run.quarantined && run.retiring.is_none()
                        })
                        .map(|(package, run)| (package.clone(), run.ready));
                    if let Some((package, ready)) = pending {
                        if !ready {
                            let _ = self.note_launch_failure(&package);
                        }
                        self.terminate(
                            &package,
                            if ready {
                                "worker_channel_failed"
                            } else {
                                "worker_boot_failed"
                            },
                        );
                    }
                }
                Ok(Transport::Message { expected, message }) => self.message(expected, message),
                Ok(Transport::End {
                    expected,
                    malformed,
                }) => {
                    if let Some((package, run)) =
                        self.runs.iter_mut().find(|(_, run)| run.id == expected)
                    {
                        run.stream_ended = true;
                        if malformed && !run.quarantined && run.retiring.is_none() {
                            let package = package.clone();
                            self.terminate(&package, "worker_protocol_failed");
                        }
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let packages: Vec<_> = self.runs.keys().cloned().collect();
        for package in packages {
            let mut run = self.runs.remove(&package).expect("current run");
            if run.quarantined {
                self.runs.insert(package, run);
                continue;
            }
            match run.child.try_wait() {
                Ok(Some(exit)) => run.exit = Some(exit),
                Ok(None) => {}
                Err(_) => {
                    self.quarantine(run, "worker_wait_failed");
                    continue;
                }
            }
            if let Some((code, since)) = run.retiring {
                if run.exit.is_some() {
                    self.terminal(run, code, None);
                } else if since.elapsed() >= Duration::from_millis(100) {
                    self.quarantine(run, "worker_wait_failed");
                } else {
                    self.runs.insert(package, run);
                }
                continue;
            }
            if let Some(exit) = run.exit {
                if let Some(report) = run.report.take() {
                    // Termination paths already invalidated this token; their stored
                    // failure is accepted only as a diagnostic, never an output.
                    let authorized = !report.ok || self.identity_valid(&run);
                    if report.ok {
                        self.launch_failures.remove(&package);
                    } else if matches!(
                        report.code.as_str(),
                        "invalid_module"
                            | "unknown_import"
                            | "instantiate_failed"
                            | "missing_memory"
                            | "invalid_entrypoint"
                            | "package_size_limit"
                            | "runtime_configuration"
                    ) {
                        let _ = self.note_launch_failure(&package);
                    }
                    if !authorized {
                        self.terminal(run, "stale_authority", None);
                    } else if report.ok && !exit.success() {
                        self.terminal(run, "worker_exit", None);
                    } else {
                        let code = report.code.clone();
                        self.terminal(run, &code, Some(report));
                    }
                    continue;
                }
                if run.stream_ended {
                    if !run.ready {
                        let _ = self.note_launch_failure(&package);
                    }
                    self.terminal(run, "worker_exit", None);
                    continue;
                }
            }
            let watchdog = if run.fault == Fault::HoldGuard {
                HELD_GUARD_WATCHDOG
            } else {
                WATCHDOG
            };
            if run.started.elapsed() >= watchdog {
                let ready = run.ready;
                self.runs.insert(package.clone(), run);
                if !ready {
                    let _ = self.note_launch_failure(&package);
                }
                self.terminate(
                    &package,
                    if ready {
                        "watchdog_timeout"
                    } else {
                        "worker_startup_timeout"
                    },
                );
            } else {
                self.runs.insert(package, run);
            }
        }
    }
    fn manage(&mut self, command: Management) -> Result<Value> {
        match command {
            Management::Start {
                package_id,
                library,
                document,
                version,
                fault,
                crash_point,
                ..
            } => self.start(
                &package_id,
                (&library, &document, &version),
                fault,
                crash_point.as_deref(),
            ),
            Management::Status { .. } => Ok(json!({"status":self.status()?})),
            Management::Cancel { run_id, .. } => {
                let package = self
                    .runs
                    .iter()
                    .find(|(_, run)| run.id == run_id)
                    .map(|(package, _)| package.clone())
                    .ok_or("run not found")?;
                self.terminate(&package, "cancelled");
                Ok(json!({"cancelled":true}))
            }
            Management::Revoke {
                package_id,
                operation_id,
                expected_revision,
                crash_point,
                ..
            } => {
                if crash_point
                    .as_deref()
                    .is_some_and(|point| point != "after_revoke_commit")
                {
                    return Err("unknown revoke crash point".into());
                }
                let run_id = self.runs.get(&package_id).map(|run| run.id.clone());
                let receipt = self.store.stop_for_worker(
                    &operation_id,
                    expected_revision,
                    &package_id,
                    "revoke",
                    None,
                    run_id.as_deref(),
                )?;
                if let Some(point) = crash_point.as_deref() {
                    let run = self.runs.get(&package_id);
                    launch_crash(
                        Some(point),
                        point,
                        run_id.as_deref().unwrap_or("no-active-run"),
                        run.map(|run| run.child.id()),
                    );
                }
                let current = self.current(&package_id)?;
                // A replay returns historical facts. It must not cancel a newer
                // authorized generation after restart/regrant or reinstall.
                if current.installation_id == receipt.installation_id
                    && current.digest == receipt.digest
                    && current.generation == receipt.generation
                {
                    self.terminate(&package_id, "revoked");
                }
                Ok(json!({"receipt":receipt}))
            }
            Management::Reap { package_id, .. } => Ok(json!({"reaped":self.reap(&package_id)?})),
            Management::ReleaseGuard { run_id, .. } => self.release_guard(&run_id),
            Management::Recover { package_id, .. } => {
                Ok(json!({"recovered":self.recover_orphan(&package_id)?}))
            }
            Management::Shutdown { .. } => Ok(json!({"shutdown":true})),
        }
    }
    fn drain_events(&mut self, writer: &mut impl Write) -> Result<()> {
        for event in self.events.drain(..) {
            write_frame(writer, &event, MAX_FRAME)?;
        }
        Ok(())
    }
    fn shutdown(&mut self) -> bool {
        let packages: Vec<_> = self.runs.keys().cloned().collect();
        for package in packages {
            if let Some(run) = self.runs.get_mut(&package) {
                run.fault = Fault::None;
            }
            self.terminate(&package, "shutdown");
        }
        let deadline = Instant::now() + Duration::from_millis(300);
        while !self.runs.is_empty() && Instant::now() < deadline {
            self.poll();
            let quarantined: Vec<_> = self
                .runs
                .iter()
                .filter(|(_, run)| run.quarantined)
                .map(|(id, _)| id.clone())
                .collect();
            for package in quarantined {
                let _ = self.reap(&package);
            }
            thread::sleep(Duration::from_millis(2));
        }
        self.runs.is_empty() && self.orphaned.is_empty()
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        for run in self.runs.values_mut() {
            self.store.managed_invalidate(&run.token);
            run.input.take();
            let _ = run.child.kill();
            // Never wait indefinitely in Drop or claim uncertain reclamation succeeded.
            let _ = run.child.try_wait();
        }
    }
}

/// Bounded JSONL management service. Only the main thread touches Store/Host state.
pub fn run_cli(path: &Path) -> Result<()> {
    let mut supervisor = Supervisor::open(path, env::current_exe()?)?;
    let (commands, incoming) = mpsc::sync_channel(32);
    thread::spawn(move || {
        let mut input = BufReader::new(io::stdin().lock());
        loop {
            match read_frame(&mut input, MAX_FRAME) {
                Ok(Some(frame)) => {
                    if commands.send(Some(frame)).is_err() {
                        return;
                    }
                }
                _ => {
                    let _ = commands.send(None);
                    return;
                }
            }
        }
    });
    let mut output = io::stdout().lock();
    write_frame(
        &mut output,
        &json!({"event":"ready","pid":process::id(),"boot":supervisor.boot,
        "watchdog_ms":WATCHDOG.as_millis(),"delay_result_ms":DELIVERY_DELAY.as_millis(),"hold_guard_watchdog_ms":HELD_GUARD_WATCHDOG.as_millis()}),
        MAX_FRAME,
    )?;
    let mut closing = false;
    while !closing {
        for _ in 0..8 {
            let frame = match incoming.try_recv() {
                Ok(Some(frame)) => frame,
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    closing = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            };
            let command = match serde_json::from_slice::<Management>(&frame) {
                Ok(command) => command,
                Err(_) => {
                    write_frame(
                        &mut output,
                        &json!({"ok":false,"request_id":null,"error":"invalid_management_command"}),
                        MAX_FRAME,
                    )?;
                    continue;
                }
            };
            let request_id = command.request_id().to_owned();
            if request_id.is_empty() || request_id.len() > 128 {
                write_frame(
                    &mut output,
                    &json!({"ok":false,"request_id":null,"error":"invalid_request_id"}),
                    MAX_FRAME,
                )?;
                continue;
            }
            let shutdown = matches!(command, Management::Shutdown { .. });
            let response = match supervisor.manage(command) {
                Ok(mut value) => {
                    value["ok"] = json!(true);
                    value["request_id"] = json!(request_id);
                    value
                }
                Err(error) => json!({"ok":false,"request_id":request_id,"error":error.to_string()}),
            };
            write_frame(&mut output, &response, MAX_MANAGEMENT_REPORT)?;
            if shutdown {
                closing = true;
                break;
            }
        }
        supervisor.poll();
        supervisor.drain_events(&mut output)?;
        if !closing {
            thread::sleep(Duration::from_millis(2));
        }
    }
    let confirmed = supervisor.shutdown();
    supervisor.drain_events(&mut output)?;
    write_frame(
        &mut output,
        &json!({"event":"shutdown_complete","confirmed":confirmed}),
        MAX_FRAME,
    )?;
    if !confirmed {
        return Err("shutdown_has_unconfirmed_workers".into());
    }
    Ok(())
}

fn validate_launch_crash(point: Option<&str>) -> Result<()> {
    if point.is_some_and(|point| {
        ![
            "before_lease",
            "after_lease",
            "after_spawn_intent",
            "after_spawn",
            "after_bind",
            "after_boot",
        ]
        .contains(&point)
    }) {
        return Err("unknown launch crash point".into());
    }
    Ok(())
}
fn launch_crash(requested: Option<&str>, point: &str, run_id: &str, pid: Option<u32>) {
    if requested == Some(point) {
        let mut output = io::stdout().lock();
        let _ = write_frame(
            &mut output,
            &json!({"event":"launch_checkpoint","point":point,"run_id":run_id,"pid":pid}),
            MAX_FRAME,
        );
        process::exit(86);
    }
}
