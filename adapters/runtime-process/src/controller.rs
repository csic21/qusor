use crate::{
    CleanupOutcome, ProcessIdentity,
    protocol::*,
    transport::{encode, read_frame},
};
use extension_runtime_api::{ExecutionLimits, MAX_RESPONSE_BYTES};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{BufReader, Write},
    process::{Child, Command as OsCommand, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_WORKERS: usize = 16;
const MAX_RETAINED: usize = 256;
const CONTROL_QUEUE: usize = 64;
const EVENT_QUEUE: usize = 128;
const TICK: Duration = Duration::from_millis(5);
const RETIRE_WAIT: Duration = Duration::from_millis(250);
static NEXT_CONTROLLER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessErrorKind {
    InvalidConfiguration,
    InvalidState,
    Capacity,
    Channel,
    Spawn,
    Identity,
    Protocol,
    Timeout,
    Cancelled,
    Cleanup,
    Unsupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessError {
    pub kind: ProcessErrorKind,
    pub code: &'static str,
}
impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for ProcessError {}
pub type ProcessResult<T> = Result<T, ProcessError>;
fn error(kind: ProcessErrorKind, code: &'static str) -> ProcessError {
    ProcessError { kind, code }
}

#[derive(Debug, Clone)]
pub struct WorkerLaunch {
    pub run_id: String,
    pub startup_timeout: Duration,
    pub execution_timeout: Duration,
}
impl Default for WorkerLaunch {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            startup_timeout: Duration::from_secs(3),
            execution_timeout: Duration::from_millis(600),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Queued,
    Starting,
    Guarded,
    BootQueued,
    Running,
    Retiring,
    Reaped,
    NotSpawned,
    Quarantined,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessExit {
    pub success: bool,
    pub code: Option<i32>,
}
#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub state: WorkerState,
    pub error: Option<ProcessError>,
    pub pid: Option<u32>,
    pub pending_call: Option<u64>,
    pub exit: Option<ProcessExit>,
}
struct Shared {
    owner: u64,
    next: AtomicU64,
    states: Mutex<BTreeMap<u64, WorkerStatus>>,
    namespace: Mutex<(Option<String>, u64)>,
    boots: Mutex<BTreeSet<u64>>,
    replies: Mutex<BTreeSet<(u64, u64)>>,
}
#[derive(Clone)]
pub struct WorkerHandle {
    owner: Arc<Shared>,
    key: u64,
    run_id: String,
}
impl fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerHandle")
            .field("run_id", &self.run_id)
            .finish()
    }
}
impl WorkerHandle {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    /// Also usable after Controller Drop to distinguish real reap from quarantine.
    pub fn status(&self) -> Option<WorkerStatus> {
        self.owner.states.lock().unwrap().get(&self.key).cloned()
    }
}
#[derive(Debug)]
pub enum ProcessEvent {
    Spawned {
        handle: WorkerHandle,
        pid: u32,
    },
    Guarded {
        handle: WorkerHandle,
        identity: ProcessIdentity,
    },
    Ready {
        handle: WorkerHandle,
    },
    Call {
        handle: WorkerHandle,
        call_id: u64,
        bytes: Vec<u8>,
    },
    ResultReceived {
        handle: WorkerHandle,
        outcome: WorkerOutcome,
    },
    Reaped {
        handle: WorkerHandle,
        exit: ProcessExit,
    },
    Failed {
        handle: WorkerHandle,
        error: ProcessError,
    },
    Quarantined {
        handle: WorkerHandle,
        error: ProcessError,
    },
}
enum Control {
    Start(WorkerHandle, WorkerLaunch),
    Boot(u64, FrozenExecution),
    Reply(u64, u64, Vec<u8>),
    Terminate(u64),
}
enum Incoming {
    Message(u64, Message),
    End(u64, bool),
    WriteFailed(u64),
    Cleanup(u64, std::io::Result<CleanupOutcome>),
}

/// Authority-free process control. Its dedicated thread owns timers and Child
/// handles, never calls a business broker and does not need the application to poll.
pub struct Controller {
    shared: Arc<Shared>,
    sender: SyncSender<Control>,
    events: Mutex<Receiver<ProcessEvent>>,
    stop: Arc<AtomicBool>,
    done: Mutex<Receiver<()>>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
}
impl Controller {
    pub fn new() -> ProcessResult<Self> {
        if !cfg!(target_os = "linux") {
            return Err(error(
                ProcessErrorKind::Unsupported,
                "process_platform_unsupported",
            ));
        }
        let owner = crate::current()
            .map_err(|_| error(ProcessErrorKind::Identity, "parent_identity_unavailable"))?;
        let executable = std::env::current_exe()
            .map_err(|_| error(ProcessErrorKind::Spawn, "current_executable_unavailable"))?;
        let identity = NEXT_CONTROLLER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| error(ProcessErrorKind::Capacity, "controller_identity_exhausted"))?;
        let shared = Arc::new(Shared {
            owner: identity,
            next: AtomicU64::new(1),
            states: Mutex::new(BTreeMap::new()),
            namespace: Mutex::new((None, 0)),
            boots: Mutex::new(BTreeSet::new()),
            replies: Mutex::new(BTreeSet::new()),
        });
        let (sender, commands) = mpsc::sync_channel(CONTROL_QUEUE);
        let (events_tx, events) = mpsc::sync_channel(EVENT_QUEUE);
        let (done_tx, done) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        // Explicitly preserve only the trusted loader path needed by a same-image
        // native host. Credentials and arbitrary ambient variables are not inherited.
        let loader_path = std::env::var_os("LD_LIBRARY_PATH");
        let join = thread::Builder::new()
            .name("qusor-process-control".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                control_loop(
                    commands,
                    events_tx,
                    signal,
                    owner,
                    executable,
                    loader_path,
                    done_tx,
                )
            })
            .map_err(|_| error(ProcessErrorKind::Spawn, "controller_thread_failed"))?;
        Ok(Self {
            shared,
            sender,
            events: Mutex::new(events),
            stop,
            done: Mutex::new(done),
            join: Mutex::new(Some(join)),
        })
    }
    pub fn start(&self, launch: WorkerLaunch) -> ProcessResult<WorkerHandle> {
        if self.stop.load(Ordering::Acquire) {
            return Err(error(ProcessErrorKind::InvalidState, "controller_stopping"));
        }
        let (prefix, counter) = run_identity(&launch.run_id).ok_or_else(|| {
            error(
                ProcessErrorKind::InvalidConfiguration,
                "invalid_run_identity",
            )
        })?;
        if [launch.startup_timeout, launch.execution_timeout]
            .iter()
            .any(|v| *v < Duration::from_millis(1) || *v > Duration::from_secs(10))
        {
            return Err(error(
                ProcessErrorKind::InvalidConfiguration,
                "invalid_process_deadline",
            ));
        }
        let mut namespace = self.shared.namespace.lock().unwrap();
        if namespace.0.as_ref().is_some_and(|v| v != prefix) || counter <= namespace.1 {
            return Err(error(
                ProcessErrorKind::InvalidConfiguration,
                "run_identity_reused_or_out_of_order",
            ));
        }
        let mut states = self.shared.states.lock().unwrap();
        if states.len() >= MAX_RETAINED {
            return Err(error(
                ProcessErrorKind::Capacity,
                "process_history_capacity",
            ));
        }
        namespace.0 = Some(prefix.to_owned());
        namespace.1 = counter;
        let key = self
            .shared
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| error(ProcessErrorKind::Capacity, "process_identity_exhausted"))?;
        let handle = WorkerHandle {
            owner: self.shared.clone(),
            key,
            run_id: launch.run_id.clone(),
        };
        states.insert(
            key,
            WorkerStatus {
                state: WorkerState::Queued,
                error: None,
                pid: None,
                pending_call: None,
                exit: None,
            },
        );
        if self
            .sender
            .try_send(Control::Start(handle.clone(), launch))
            .is_err()
        {
            states.remove(&key);
            return Err(error(
                ProcessErrorKind::Capacity,
                "process_command_backpressure",
            ));
        }
        Ok(handle)
    }
    pub fn poll(&self) -> Vec<ProcessEvent> {
        let events = self.events.lock().unwrap();
        (0..EVENT_QUEUE)
            .filter_map(|_| events.try_recv().ok())
            .collect()
    }
    pub fn status(&self, handle: &WorkerHandle) -> ProcessResult<WorkerStatus> {
        self.owns(handle)?;
        handle
            .status()
            .ok_or_else(|| error(ProcessErrorKind::InvalidState, "unknown_process_handle"))
    }
    pub fn release_boot(
        &self,
        handle: &WorkerHandle,
        execution: FrozenExecution,
    ) -> ProcessResult<()> {
        self.owns(handle)?;
        if !execution.valid()
            || !execution
                .limits
                .deadline
                .subsec_nanos()
                .is_multiple_of(1_000_000)
        {
            return Err(error(
                ProcessErrorKind::InvalidConfiguration,
                "invalid_frozen_execution",
            ));
        }
        if self.status(handle)?.state != WorkerState::Guarded {
            return Err(error(ProcessErrorKind::InvalidState, "worker_not_guarded"));
        }
        let mut boots = self.shared.boots.lock().unwrap();
        if !boots.insert(handle.key) {
            return Err(error(
                ProcessErrorKind::InvalidState,
                "boot_already_released",
            ));
        }
        if self
            .sender
            .try_send(Control::Boot(handle.key, execution))
            .is_err()
        {
            boots.remove(&handle.key);
            return Err(error(
                ProcessErrorKind::Capacity,
                "process_command_backpressure",
            ));
        }
        Ok(())
    }
    pub fn reply(&self, handle: &WorkerHandle, call_id: u64, bytes: Vec<u8>) -> ProcessResult<()> {
        self.owns(handle)?;
        let status = self.status(handle)?;
        if status.state != WorkerState::Running || status.pending_call != Some(call_id) {
            return Err(error(ProcessErrorKind::InvalidState, "stale_process_reply"));
        }
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(error(
                ProcessErrorKind::InvalidConfiguration,
                "process_reply_budget",
            ));
        }
        let mut replies = self.shared.replies.lock().unwrap();
        if !replies.insert((handle.key, call_id)) {
            return Err(error(
                ProcessErrorKind::InvalidState,
                "duplicate_process_reply",
            ));
        }
        if self
            .sender
            .try_send(Control::Reply(handle.key, call_id, bytes))
            .is_err()
        {
            replies.remove(&(handle.key, call_id));
            return Err(error(
                ProcessErrorKind::Capacity,
                "process_command_backpressure",
            ));
        }
        Ok(())
    }
    pub fn terminate(&self, handle: &WorkerHandle) -> ProcessResult<()> {
        self.owns(handle)?;
        if self.status(handle)?.state == WorkerState::Reaped {
            return Ok(());
        }
        self.sender
            .try_send(Control::Terminate(handle.key))
            .map_err(|_| error(ProcessErrorKind::Capacity, "process_command_backpressure"))
    }
    /// Call after durable lease release, following real Reaped or definite NotSpawned.
    pub fn forget(&self, handle: &WorkerHandle) -> ProcessResult<()> {
        self.owns(handle)?;
        let mut states = self.shared.states.lock().unwrap();
        if states
            .get(&handle.key)
            .is_none_or(|s| !matches!(s.state, WorkerState::Reaped | WorkerState::NotSpawned))
        {
            return Err(error(
                ProcessErrorKind::InvalidState,
                "process_exit_unconfirmed",
            ));
        }
        states.remove(&handle.key);
        self.shared.boots.lock().unwrap().remove(&handle.key);
        self.shared
            .replies
            .lock()
            .unwrap()
            .retain(|(k, _)| *k != handle.key);
        Ok(())
    }
    fn owns(&self, handle: &WorkerHandle) -> ProcessResult<()> {
        if Arc::ptr_eq(&self.shared, &handle.owner) && self.shared.owner == handle.owner.owner {
            Ok(())
        } else {
            Err(error(
                ProcessErrorKind::InvalidState,
                "foreign_process_handle",
            ))
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self
            .done
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(1))
            .is_ok()
            && let Some(join) = self.join.lock().unwrap().take()
        {
            let _ = join.join();
        }
        // If native termination cannot be confirmed, the control thread retains Child
        // ownership and quarantine; Drop never invents a reap or signals a naked PID.
    }
}
fn run_identity(run: &str) -> Option<(&str, u64)> {
    let (prefix, counter) = run.split_once('-')?;
    if prefix.len() != 32
        || !prefix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let value: u64 = counter.parse().ok()?;
    if value == 0 || value.to_string() != counter {
        return None;
    }
    Some((prefix, value))
}

struct Run {
    handle: WorkerHandle,
    child: Child,
    identity: Option<ProcessIdentity>,
    input: Option<SyncSender<Vec<u8>>>,
    launch: WorkerLaunch,
    started: Instant,
    booted: Option<Instant>,
    phase: WorkerState,
    calls: u64,
    pending: Option<u64>,
    limits: Option<ExecutionLimits>,
    request_limit: usize,
    result_seen: bool,
    outcome: Option<WorkerOutcome>,
    result_delivered: bool,
    stream_ended: bool,
    exit: Option<ProcessExit>,
    failure: Option<ProcessError>,
    retiring: Option<Instant>,
    cleanup_started: bool,
}
impl Run {
    fn sync(&self) {
        if let Some(status) = self
            .handle
            .owner
            .states
            .lock()
            .unwrap()
            .get_mut(&self.handle.key)
        {
            *status = WorkerStatus {
                state: self.phase,
                error: self.failure,
                pid: Some(self.child.id()),
                pending_call: self.pending,
                exit: self.exit,
            };
        }
    }
    fn emit(&self, tx: &SyncSender<ProcessEvent>, event: ProcessEvent) -> bool {
        tx.try_send(event).is_ok()
    }
    fn quarantine(&mut self, tx: &SyncSender<ProcessEvent>, failure: ProcessError) {
        self.phase = WorkerState::Quarantined;
        self.failure = Some(failure);
        self.input.take();
        self.sync();
        let _ = self.emit(
            tx,
            ProcessEvent::Quarantined {
                handle: self.handle.clone(),
                error: failure,
            },
        );
    }
    fn retire(
        &mut self,
        tx: &SyncSender<ProcessEvent>,
        incoming: &SyncSender<Incoming>,
        failure: ProcessError,
    ) {
        if self.retiring.is_none() {
            self.retiring = Some(Instant::now());
            self.failure = Some(failure);
            self.phase = WorkerState::Retiring;
            self.pending = None;
            self.input.take();
            self.sync();
            let _ = self.emit(
                tx,
                ProcessEvent::Failed {
                    handle: self.handle.clone(),
                    error: failure,
                },
            );
        }
        if !self.cleanup_started {
            if self.identity.is_none() {
                self.identity = crate::inspect(self.child.id(), self.handle.run_id()).ok();
            }
            if let Some(identity) = self.identity.clone() {
                self.cleanup_started = true;
                let sender = incoming.clone();
                let key = self.handle.key;
                if thread::Builder::new()
                    .name("qusor-process-cleanup".into())
                    .stack_size(128 * 1024)
                    .spawn(move || {
                        let result = crate::cleanup(&identity);
                        let _ = sender.try_send(Incoming::Cleanup(key, result));
                    })
                    .is_err()
                {
                    self.quarantine(
                        tx,
                        error(ProcessErrorKind::Cleanup, "cleanup_thread_failed"),
                    );
                }
            }
            // Missing identity can also mean an already-exiting worker. Never
            // signal it by PID; allow owned try_wait or the retire deadline to
            // establish the terminal fact before quarantine.
        }
    }
}

fn control_loop(
    commands: Receiver<Control>,
    events: SyncSender<ProcessEvent>,
    stop: Arc<AtomicBool>,
    owner: ProcessIdentity,
    executable: std::path::PathBuf,
    loader_path: Option<std::ffi::OsString>,
    done: SyncSender<()>,
) {
    let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE);
    let mut runs: BTreeMap<u64, Run> = BTreeMap::new();
    let mut stopping = false;
    loop {
        if stop.load(Ordering::Acquire) {
            stopping = true;
        }
        for _ in 0..CONTROL_QUEUE {
            match commands.try_recv() {
                Ok(Control::Start(handle, launch)) => {
                    if stopping || runs.len() >= MAX_WORKERS {
                        reject_start(
                            &handle,
                            &events,
                            error(ProcessErrorKind::Capacity, "process_worker_capacity"),
                        );
                        continue;
                    }
                    let mut command = OsCommand::new(&executable);
                    command
                        .arg("--managed-worker")
                        .arg(owner.pid.to_string())
                        .arg(owner.start_time_ticks.to_string())
                        .arg(handle.run_id())
                        .env_clear()
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null());
                    if let Some(path) = &loader_path {
                        command.env("LD_LIBRARY_PATH", path);
                    }
                    match command.spawn() {
                        Err(_) => reject_start(
                            &handle,
                            &events,
                            error(ProcessErrorKind::Spawn, "worker_spawn_failed"),
                        ),
                        Ok(mut child) => {
                            let key = handle.key;
                            let output = child.stdout.take();
                            let input = child.stdin.take();
                            let (writer, writes) = mpsc::sync_channel(2);
                            let mut run = Run {
                                identity: crate::inspect(child.id(), handle.run_id()).ok(),
                                handle,
                                child,
                                input: Some(writer),
                                launch,
                                started: Instant::now(),
                                booted: None,
                                phase: WorkerState::Starting,
                                calls: 0,
                                pending: None,
                                limits: None,
                                request_limit: 0,
                                result_seen: false,
                                outcome: None,
                                result_delivered: false,
                                stream_ended: false,
                                exit: None,
                                failure: None,
                                retiring: None,
                                cleanup_started: false,
                            };
                            run.sync();
                            let readers = if let (Some(output), Some(input)) = (output, input) {
                                let reader_sender = sender.clone();
                                let writer_sender = sender.clone();
                                let a = thread::Builder::new()
                                    .name("qusor-process-read".into())
                                    .stack_size(256 * 1024)
                                    .spawn(move || read_messages(output, key, reader_sender));
                                let b = thread::Builder::new()
                                    .name("qusor-process-write".into())
                                    .stack_size(128 * 1024)
                                    .spawn(move || {
                                        write_messages(input, key, writes, writer_sender)
                                    });
                                a.is_ok() && b.is_ok()
                            } else {
                                false
                            };
                            if !readers
                                || !run.emit(
                                    &events,
                                    ProcessEvent::Spawned {
                                        handle: run.handle.clone(),
                                        pid: run.child.id(),
                                    },
                                )
                            {
                                run.retire(
                                    &events,
                                    &sender,
                                    error(
                                        ProcessErrorKind::Channel,
                                        "process_transport_unavailable",
                                    ),
                                );
                            }
                            runs.insert(key, run);
                        }
                    }
                }
                Ok(Control::Boot(key, execution)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        if run.phase != WorkerState::Guarded || run.retiring.is_some() {
                            continue;
                        }
                        let boot = Boot {
                            protocol_version: PROCESS_PROTOCOL,
                            run_id: run.handle.run_id.clone(),
                            module: execution.module,
                            limits: WireLimits::from(&execution.limits),
                            request_limit: execution.request_limit,
                        };
                        let queued = encode(&boot, MAX_BOOT).ok().is_some_and(|bytes| {
                            run.input
                                .as_ref()
                                .is_some_and(|tx| tx.try_send(bytes).is_ok())
                        });
                        if queued {
                            run.limits = Some(execution.limits);
                            run.request_limit = execution.request_limit;
                            run.booted = Some(Instant::now());
                            run.phase = WorkerState::BootQueued;
                            run.sync();
                        } else {
                            run.retire(
                                &events,
                                &sender,
                                error(ProcessErrorKind::Channel, "worker_boot_failed"),
                            );
                        }
                    }
                }
                Ok(Control::Reply(key, call_id, bytes)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        if run.phase != WorkerState::Running
                            || run.pending != Some(call_id)
                            || run.retiring.is_some()
                        {
                            continue;
                        }
                        let reply = Reply {
                            run_id: run.handle.run_id.clone(),
                            call_id,
                            bytes,
                        };
                        let queued = encode(&reply, MAX_FRAME).ok().is_some_and(|bytes| {
                            run.input
                                .as_ref()
                                .is_some_and(|tx| tx.try_send(bytes).is_ok())
                        });
                        if queued {
                            run.pending = None;
                            run.sync();
                        } else {
                            run.retire(
                                &events,
                                &sender,
                                error(ProcessErrorKind::Channel, "worker_reply_failed"),
                            );
                        }
                    }
                }
                Ok(Control::Terminate(key)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        run.retire(
                            &events,
                            &sender,
                            error(ProcessErrorKind::Cancelled, "process_cancelled"),
                        );
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    stopping = true;
                    break;
                }
            }
        }
        for _ in 0..EVENT_QUEUE {
            match receiver.try_recv() {
                Ok(Incoming::Message(key, message)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        if run.retiring.is_some() || run.phase == WorkerState::Quarantined {
                            continue;
                        }
                        if message.run_id() != run.handle.run_id() {
                            run.retire(
                                &events,
                                &sender,
                                error(ProcessErrorKind::Protocol, "worker_identity_mismatch"),
                            );
                            continue;
                        }
                        let valid = match message {
                            Message::GuardReady {
                                protocol_version, ..
                            } if run.phase == WorkerState::Starting
                                && protocol_version == PROCESS_PROTOCOL =>
                            {
                                match crate::inspect(run.child.id(), run.handle.run_id()) {
                                    Ok(identity)
                                        if run
                                            .identity
                                            .as_ref()
                                            .is_none_or(|old| old == &identity) =>
                                    {
                                        run.identity = Some(identity.clone());
                                        run.phase = WorkerState::Guarded;
                                        run.sync();
                                        run.emit(
                                            &events,
                                            ProcessEvent::Guarded {
                                                handle: run.handle.clone(),
                                                identity,
                                            },
                                        )
                                    }
                                    _ => false,
                                }
                            }
                            Message::Ready { .. } if run.phase == WorkerState::BootQueued => {
                                run.phase = WorkerState::Running;
                                run.sync();
                                run.emit(
                                    &events,
                                    ProcessEvent::Ready {
                                        handle: run.handle.clone(),
                                    },
                                )
                            }
                            Message::Call { call_id, bytes, .. }
                                if run.phase == WorkerState::Running
                                    && run.pending.is_none()
                                    && !run.result_seen
                                    && run.calls
                                        < run
                                            .limits
                                            .as_ref()
                                            .map_or(0, |v| v.host_calls as u64)
                                    && call_id == run.calls + 1
                                    && bytes.len() <= run.request_limit =>
                            {
                                run.calls = call_id;
                                run.pending = Some(call_id);
                                run.sync();
                                run.emit(
                                    &events,
                                    ProcessEvent::Call {
                                        handle: run.handle.clone(),
                                        call_id,
                                        bytes,
                                    },
                                )
                            }
                            Message::Result { outcome, .. }
                                if run.phase == WorkerState::Running
                                    && run.pending.is_none()
                                    && !run.result_seen =>
                            {
                                let public = outcome.into_public();
                                let valid = public.as_ref().is_some_and(|value| match value {
                                    WorkerOutcome::Completed(result) => {
                                        result.calls as u64 == run.calls
                                            && result.fuel_consumed
                                                <= run.limits.as_ref().map_or(0, |v| v.total_fuel)
                                    }
                                    WorkerOutcome::Failed(_) => true,
                                });
                                if valid {
                                    run.result_seen = true;
                                    run.outcome = public;
                                    true
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };
                        if !valid {
                            run.retire(
                                &events,
                                &sender,
                                error(
                                    ProcessErrorKind::Protocol,
                                    "worker_protocol_or_event_backpressure",
                                ),
                            );
                        }
                    }
                }
                Ok(Incoming::End(key, malformed)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        run.stream_ended = true;
                        if malformed && run.retiring.is_none() {
                            run.retire(
                                &events,
                                &sender,
                                error(ProcessErrorKind::Protocol, "worker_protocol_failed"),
                            );
                        }
                    }
                }
                Ok(Incoming::WriteFailed(key)) => {
                    if let Some(run) = runs.get_mut(&key) {
                        run.retire(
                            &events,
                            &sender,
                            error(ProcessErrorKind::Channel, "worker_channel_failed"),
                        );
                    }
                }
                Ok(Incoming::Cleanup(key, _outcome)) => {
                    // Closing cmdline can race pidfd readiness during natural
                    // worker exit. Confirm via owned try_wait or retire timeout.
                    let _ = runs.get(&key);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let mut reaped = Vec::new();
        for (key, run) in &mut runs {
            if !run.handle.owner.states.lock().unwrap().contains_key(key) {
                // Forget is allowed only after actual reap/not-spawned. Do not
                // deliver deferred output after the caller retired its context.
                reaped.push(*key);
                continue;
            }

            match run.child.try_wait() {
                Ok(Some(exit)) => {
                    run.exit = Some(ProcessExit {
                        success: exit.success(),
                        code: exit.code(),
                    })
                }
                Ok(None) => {}
                Err(_) => {
                    run.quarantine(
                        &events,
                        error(ProcessErrorKind::Cleanup, "worker_wait_failed"),
                    );
                    continue;
                }
            }
            if stopping && run.retiring.is_none() {
                run.retire(
                    &events,
                    &sender,
                    error(ProcessErrorKind::Cancelled, "controller_shutdown"),
                );
            }
            if let Some(exit) = run.exit {
                // Require complete EOF and successful exit before exposing the
                // candidate. Trailing frames must not escape validation.
                if run.retiring.is_some() || run.stream_ended {
                    if run.retiring.is_none()
                        && (!exit.success || !run.result_seen)
                        && run.failure.is_none()
                    {
                        run.failure = Some(error(
                            ProcessErrorKind::Protocol,
                            if exit.success {
                                "worker_result_missing"
                            } else {
                                "worker_exit_nonzero"
                            },
                        ));
                        let _ = run.emit(
                            &events,
                            ProcessEvent::Failed {
                                handle: run.handle.clone(),
                                error: run.failure.unwrap(),
                            },
                        );
                        run.outcome = None;
                    }
                    run.phase = WorkerState::Reaped;
                    run.sync();
                    if !stopping
                        && run.retiring.is_none()
                        && !run.result_delivered
                        && let Some(outcome) = run.outcome.take()
                    {
                        match events.try_send(ProcessEvent::ResultReceived {
                            handle: run.handle.clone(),
                            outcome,
                        }) {
                            Ok(()) => run.result_delivered = true,
                            Err(mpsc::TrySendError::Full(ProcessEvent::ResultReceived {
                                outcome,
                                ..
                            })) => {
                                run.outcome = Some(outcome);
                                continue;
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => {
                                stopping = true;
                            }
                            Err(_) => unreachable!(),
                        }
                    }
                    let notified = run.emit(
                        &events,
                        ProcessEvent::Reaped {
                            handle: run.handle.clone(),
                            exit,
                        },
                    );
                    // Notification backpressure cannot undo an actual OS reap.
                    if notified || stopping {
                        reaped.push(*key);
                    }
                    continue;
                }
            }
            if let Some(since) = run.retiring {
                if since.elapsed() >= RETIRE_WAIT && run.phase != WorkerState::Quarantined {
                    run.quarantine(
                        &events,
                        error(ProcessErrorKind::Cleanup, "worker_exit_unconfirmed"),
                    );
                }
                continue;
            }
            let timeout = run
                .booted
                .map_or(run.launch.startup_timeout, |_| run.launch.execution_timeout);
            let since = run.booted.unwrap_or(run.started);
            if since.elapsed() >= timeout {
                let code = if run.booted.is_some() {
                    "watchdog_timeout"
                } else {
                    "worker_startup_timeout"
                };
                run.retire(&events, &sender, error(ProcessErrorKind::Timeout, code));
            }
        }
        for key in reaped {
            runs.remove(&key);
        }
        if stopping && runs.is_empty() {
            let _ = done.try_send(());
            break;
        }
        thread::sleep(if stopping {
            Duration::from_millis(10)
        } else {
            TICK
        });
    }
}
fn reject_start(handle: &WorkerHandle, events: &SyncSender<ProcessEvent>, failure: ProcessError) {
    if let Some(status) = handle.owner.states.lock().unwrap().get_mut(&handle.key) {
        status.state = WorkerState::NotSpawned;
        status.error = Some(failure);
    }
    let _ = events.try_send(ProcessEvent::Failed {
        handle: handle.clone(),
        error: failure,
    });
}
fn read_messages(stdout: impl std::io::Read, key: u64, sender: SyncSender<Incoming>) {
    let mut reader = BufReader::new(stdout);
    for _ in 0..MAX_CALLS + 4 {
        let bytes = match read_frame(&mut reader, MAX_FRAME) {
            Ok(Some(bytes)) => bytes,
            result => {
                let _ = sender.send(Incoming::End(key, result.is_err()));
                return;
            }
        };
        let message = match serde_json::from_slice::<Message>(&bytes) {
            Ok(value) => value,
            Err(_) => {
                let _ = sender.send(Incoming::End(key, true));
                return;
            }
        };
        if sender.send(Incoming::Message(key, message)).is_err() {
            return;
        }
    }
    let _ = sender.send(Incoming::End(key, true));
}
fn write_messages(
    mut stdin: impl Write,
    key: u64,
    receiver: Receiver<Vec<u8>>,
    sender: SyncSender<Incoming>,
) {
    while let Ok(bytes) = receiver.recv() {
        if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
            let _ = sender.try_send(Incoming::WriteFailed(key));
            return;
        }
    }
}
