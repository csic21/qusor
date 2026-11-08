//! Trusted native UI assembly for runtime-selected documents. Guest sees only Surface wire.
use crate::{
    dynamic_worker::{self, Artifact, WorkerConfig},
    plugin_surface::{self, ReaderSurface},
};
use nsh_core::{
    ImageData,
    element::{JsHost, JsIn, JsOwned, PropError, TsTypes},
    file_dialog::{FileFilter, OpenDialog},
    prelude::*,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};
#[path = "encoded_bytes.rs"]
mod encoded_bytes;
#[cfg(test)]
#[path = "frame_caller_tests.rs"]
mod frame_caller_tests;
#[path = "frame_owner_map.rs"]
mod frame_owner_map;
#[path = "frame_scheduler.rs"]
mod frame_scheduler;
use frame_owner_map::{FrameCaller, FrameKey};
type Result<T> = std::result::Result<T, String>;
const IDLE: u8 = 0;
const PICKING: u8 = 1;
const PARSING: u8 = 2;
const COMMITTING: u8 = 3;
const READY: u8 = 4;
struct Config {
    resources: PathBuf,
    data: PathBuf,
    worker: WorkerConfig,
    nonce: String,
    picker_dir: Option<PathBuf>,
}
struct State {
    config: Config,
    generation: AtomicU64,
    phase: AtomicU8,
    has_document: AtomicBool,
    operation_running: Arc<AtomicBool>,
    closing: AtomicBool,
    lifecycle: Mutex<()>,
    frames: FrameCaller<FrameFile>,
    artifact: Mutex<Option<Arc<Artifact>>>,
}
static STATE: Mutex<Option<Arc<State>>> = Mutex::new(None);
fn state() -> Result<Arc<State>> {
    STATE
        .lock()
        .map_err(|_| "dynamic state unavailable")?
        .clone()
        .ok_or("dynamic state unavailable".into())
}
fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn initialize(resources: &Path, data: &Path, worker: &Path, library: &Path) -> Result<()> {
    plugin_surface::native_editor::initialize(resources)?;
    let mut nonce = [0; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(text)?;
    let config = Config {
        resources: resources.into(),
        data: data.into(),
        worker: WorkerConfig {
            worker: worker.into(),
            library: library.into(),
            jobs: data.join("native-jobs"),
        },
        nonce: nonce.iter().map(|b| format!("{b:02x}")).collect(),
        picker_dir: std::env::var_os("QUSOR_DYNAMIC_PICKER_DIR").map(PathBuf::from),
    };
    let next = Arc::new(State {
        config,
        generation: AtomicU64::new(0),
        phase: AtomicU8::new(IDLE),
        has_document: AtomicBool::new(false),
        operation_running: Arc::new(AtomicBool::new(false)),
        closing: AtomicBool::new(false),
        lifecycle: Mutex::new(()),
        frames: FrameCaller::new()?,
        artifact: Mutex::new(None),
    });
    let previous = {
        STATE
            .lock()
            .map_err(|_| "dynamic state unavailable")?
            .replace(next)
    };
    if let Some(previous) = previous {
        let admission = previous
            .lifecycle
            .lock()
            .map_err(|_| "dynamic lifecycle unavailable")?;
        previous.closing.store(true, Ordering::SeqCst);
        previous
            .generation
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
            .ok();
        // Outside STATE lock. Drop is also nonblocking; the independent
        // retirement service owns its actual thread handle before tasks start.
        drop(admission);
        previous.frames.close()?;
    }
    Ok(())
}
pub fn shutdown() -> Result<()> {
    plugin_surface::native_editor::begin_shutdown();
    // Flags only here. Main later performs one full editor retirement: via
    // plugin_surface.shutdown on success, or without taking SERVICE on error.
    let previous = {
        STATE
            .lock()
            .map_err(|_| "dynamic state unavailable")?
            .take()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    if let Some(previous) = previous {
        {
            let _admission = previous
                .lifecycle
                .lock()
                .map_err(|_| "dynamic lifecycle unavailable")?;
            previous.closing.store(true, Ordering::SeqCst);
            previous
                .generation
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                .ok();
        }
        let ticket = previous.frames.retirement_ticket();
        previous.frames.close()?;
        let operations = previous.wait_operations_until(deadline);
        let owners = previous.frames.snapshot_owners()?;
        eprintln!(
            "FRAME_RUST_RETIREMENT {}",
            json!({"phase":format!("{:?}",ticket.phase()),"owners":owners.iter().map(|(key,s)|json!({"mount_id":key.mount_id,"generation":key.generation,"frame_id":key.frame_id,"pending":s.pending,"running":s.running,"undelivered":s.undelivered,"local_delivery":s.delivered_local,"encoded_containers":s.encoded_containers,"encoded_referenced_bytes":s.encoded_referenced_bytes,"bridge_attempts":s.handed_to_bridge})).collect::<Vec<_>>(),"native_release_ack":false})
        );
        if !ticket.wait_until(deadline)? {
            return Err("Rust frame retirement still pending; handles retained".into());
        }
        operations?;
    }
    if !frame_scheduler::shutdown_retirement_until(deadline)? {
        return Err("frame retirement service still pending; handles retained".into());
    }
    let encoded = encoded_bytes::snapshot()?;
    eprintln!(
        "FRAME_ENCODED_SCOPE {}",
        json!({"containers":encoded.containers,"referenced_bytes":encoded.referenced_bytes,"created":encoded.created,"released":encoded.released,"uncertain":encoded.uncertain,"retainer_class":"unknown_ImageData_Blob_ImageSource_cache_or_loader","allocation_may_remain_in_native_png_cache":true,"native_release_ack":false})
    );
    Ok(())
}
struct Completion<T> {
    value: Option<Result<T>>,
    waker: Option<Waker>,
}
pub(crate) struct Job<T> {
    state: Arc<Mutex<Completion<T>>>,
}
impl<T: Send + 'static> Job<T> {
    pub(crate) fn start(work: impl FnOnce() -> Result<T> + Send + 'static) -> Self {
        let state = Arc::new(Mutex::new(Completion {
            value: None,
            waker: None,
        }));
        let done = state.clone();
        let spawned = std::thread::Builder::new()
            .name("qusor-native-job".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                    .unwrap_or_else(|_| Err("native background job failed".into()));
                let wake = if let Ok(mut done) = done.lock() {
                    done.value = Some(result);
                    done.waker.take()
                } else {
                    None
                };
                if let Some(wake) = wake {
                    wake.wake();
                }
            });
        if let Err(error) = spawned
            && let Ok(mut done) = state.lock()
        {
            done.value = Some(Err(format!("native job could not start: {error}")));
        }
        Self { state }
    }
}
impl<T> Future for Job<T> {
    type Output = Result<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state.lock() {
            Ok(mut state) => match state.value.take() {
                Some(value) => Poll::Ready(value),
                None => {
                    state.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            },
            Err(_) => Poll::Ready(Err("native completion unavailable".into())),
        }
    }
}
struct OperationSlot(Arc<AtomicBool>);
impl Drop for OperationSlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
fn envelope(value: Value) -> String {
    json!({"ok":true,"value":value}).to_string()
}
fn failure(error: String) -> String {
    eprintln!("DYNAMIC_FAILURE {error}");
    json!({"ok":false,"error":error}).to_string()
}
impl State {
    /// Admission is charged before spawn; no lifecycle/Gate lock spans work.
    fn ready_job<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<Job<T>> {
        let slot = {
            let _admission = self
                .lifecycle
                .lock()
                .map_err(|_| "dynamic lifecycle unavailable")?;
            if self.closing.load(Ordering::SeqCst) || self.operation_running.load(Ordering::SeqCst)
            {
                return Err("dynamic host is closing or an operation is retiring".into());
            }
            self.phase
                .compare_exchange(READY, COMMITTING, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| "another operation is active")?;
            self.operation_running.store(true, Ordering::SeqCst);
            OperationSlot(self.operation_running.clone())
        };
        Ok(Job::start(move || {
            let _slot = slot;
            work()
        }))
    }
    fn wait_operations_until(&self, deadline: std::time::Instant) -> Result<()> {
        while self.operation_running.load(Ordering::SeqCst) && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if self.operation_running.load(Ordering::SeqCst) {
            Err("dynamic operation still pending; Service must remain retained".into())
        } else {
            Ok(())
        }
    }
    fn settled(&self) -> u8 {
        if self.has_document.load(Ordering::SeqCst) {
            READY
        } else {
            IDLE
        }
    }
    fn begin(&self) -> Result<u64> {
        let _admission = self
            .lifecycle
            .lock()
            .map_err(|_| "dynamic lifecycle unavailable")?;
        if self.closing.load(Ordering::SeqCst) || self.operation_running.load(Ordering::SeqCst) {
            return Err("dynamic host is closing or an operation is retiring".into());
        }
        loop {
            let phase = self.phase.load(Ordering::SeqCst);
            if phase != IDLE && phase != READY {
                return Err("an import or save is still active".into());
            }
            if self
                .phase
                .compare_exchange(phase, PICKING, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }
        match self
            .generation
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(1).filter(|next| *next < u64::MAX)
            }) {
            Ok(n) => Ok(n + 1),
            Err(_) => {
                self.phase.store(self.settled(), Ordering::SeqCst);
                Err("native job sequence exhausted".into())
            }
        }
    }
    fn current(&self, job: u64) -> Result<()> {
        if !self.closing.load(Ordering::SeqCst) && self.generation.load(Ordering::SeqCst) == job {
            Ok(())
        } else {
            Err("import cancelled or superseded".into())
        }
    }
}
#[derive(Clone)]
pub struct FrameFile {
    info: Value,
    image: ImageData,
    // Last Rust clone releases the local-result charge only after image drops.
    local_lease: Option<frame_scheduler::LocalResultLease>,
}
impl JsValue for FrameFile {
    fn ts_type(types: &mut TsTypes) -> String {
        types.declare("DynamicPdfFrame", |_| {
            "export interface DynamicPdfFrame { info: string; file: File; }".into()
        })
    }
    fn from_js(value: JsIn<'_>) -> std::result::Result<Self, PropError> {
        Err(value.expected("native frame output only"))
    }
    fn to_js<'a>(&self, host: &'a dyn JsHost) -> std::result::Result<JsOwned<'a>, PropError> {
        let handle = self.info["handle"]
            .as_str()
            .ok_or_else(|| PropError("frame handle missing".into()))?;
        let generation = self.info["generation"]
            .as_u64()
            .ok_or_else(|| PropError("frame generation missing".into()))?;
        let frame = self.info["frame_id"]
            .as_str()
            .ok_or_else(|| PropError("frame id missing".into()))?;
        plugin_surface::native_frame_context(handle, generation, frame).map_err(PropError)?;
        let lease = self
            .local_lease
            .as_ref()
            .ok_or_else(|| PropError("untracked frame output".into()))?;
        let owned = JsOwned::new(host, host.new_object()?);
        let object = owned.get().raw();
        host.set_field(object, "info", host.new_string(&self.info.to_string())?)?;
        // Count the attempt before any ImageData can escape into File/Blob.
        // Encoded guard tracks actual shared-container lifetime independently.
        lease.record_bridge_attempt().map_err(PropError)?;
        host.set_field(
            object,
            "file",
            host.new_file(&self.image, "image/png", "native-page.png", None)?,
        )?;
        Ok(owned)
    }
}
#[module]
pub struct DynamicReader;
#[module]
impl DynamicReader {
    pub fn choose() -> Later<String> {
        Later::new(async {
            let result = async {
                let s = state()?;
                let job = s.begin()?;
                let mut dialog = OpenDialog::files()
                    .filter(FileFilter::new("PDF", ["pdf"]))
                    .title("打开本地 PDF")
                    .button_label("打开");
                if let Some(directory) = &s.config.picker_dir {
                    dialog.directory = Some(directory.clone());
                }
                let picked = match pick_paths(dialog).await {
                    Ok(value) => value,
                    Err(error) => {
                        if s.generation.load(Ordering::SeqCst) == job {
                            s.phase.store(s.settled(), Ordering::SeqCst);
                        }
                        return Err(text(error));
                    }
                };
                s.current(job)?;
                let Some(paths) = picked else {
                    s.phase.store(s.settled(), Ordering::SeqCst);
                    eprintln!("DYNAMIC_PICK_CANCELLED {job}");
                    return Ok(envelope(json!({"cancelled":true})));
                };
                let path = match paths.into_iter().next() {
                    Some(path) => path,
                    None => {
                        s.phase.store(s.settled(), Ordering::SeqCst);
                        return Err("no file was chosen".into());
                    }
                };
                s.phase.store(PARSING, Ordering::SeqCst);
                let background = s.clone();
                let parser_admission = {
                    let _admission = s
                        .lifecycle
                        .lock()
                        .map_err(|_| "dynamic lifecycle unavailable")?;
                    s.current(job)?;
                    s.operation_running.compare_exchange(
                        false,
                        true,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                };
                let outcome = if parser_admission.is_err() {
                    Err(
                        "previous native read has not finished; no replacement parser started"
                            .into(),
                    )
                } else {
                    let slot = OperationSlot(background.operation_running.clone());
                    Job::start(move || {
                        let _slot = slot;
                        let (prepared, artifact) = dynamic_worker::inspect(
                            &background.config.worker,
                            &path,
                            job,
                            &background.generation,
                            &background.config.nonce,
                        )?;
                        background.current(job)?;
                        background
                            .phase
                            .compare_exchange(
                                PARSING,
                                COMMITTING,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            )
                            .map_err(|_| "import was cancelled before commit")?;
                        let initialization = plugin_surface::initialize_document_checked(
                            &background.config.resources,
                            &background.config.data,
                            prepared,
                        )?;
                        let digest = artifact.report_digest.clone();
                        *background
                            .artifact
                            .lock()
                            .map_err(|_| "native artifact state unavailable")? = Some(artifact);
                        background.has_document.store(true, Ordering::SeqCst);
                        if let plugin_surface::DocumentInitialization::ImportedUnavailable {
                            message,
                        } = initialization
                        {
                            eprintln!("DYNAMIC_IMPORTED_AUTHORITY_UNAVAILABLE {message}");
                            return Ok(envelope(
                                json!({"imported":true,"opened":false,"error":message}),
                            ));
                        }
                        let opened: Value =
                            serde_json::from_str(&ReaderSurface::open(digest)).map_err(text)?;
                        if opened["ok"] != true {
                            eprintln!("DYNAMIC_IMPORTED_READER_UNAVAILABLE {}", opened);
                            return Ok(envelope(
                                json!({"imported":true,"opened":false,"error":opened["error"]}),
                            ));
                        }
                        eprintln!("DYNAMIC_IMPORTED {}", opened["value"]);
                        Ok(envelope(opened["value"].clone()))
                    })
                    .await
                };
                if s.generation.load(Ordering::SeqCst) == job {
                    s.phase.store(s.settled(), Ordering::SeqCst);
                }
                outcome
            }
            .await;
            Ok::<String, String>(match result {
                Ok(value) => value,
                Err(error) => failure(error),
            })
        })
    }
    pub fn reopen() -> Later<String> {
        Later::new(async {
            let s = state()?;
            let outcome = s
                .ready_job(move || {
                    Ok(ReaderSurface::open(plugin_surface::current_report_digest()?))
                })?
                .await;
            s.phase.store(s.settled(), Ordering::SeqCst);
            outcome
        })
    }
    pub fn cancel() -> String {
        match state() {
            Ok(s) => {
                if s.phase
                    .compare_exchange(PARSING, s.settled(), Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    if s.generation
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                        .is_err()
                    {
                        return failure("native job sequence exhausted".into());
                    }
                    envelope(json!({"cancelled":true}))
                } else {
                    envelope(
                        json!({"cancelled":false,"committing":s.phase.load(Ordering::SeqCst)==COMMITTING}),
                    )
                }
            }
            Err(error) => failure(error),
        }
    }
    pub fn frame(handle: String, generation: u64, frame_id: String) -> Later<FrameFile> {
        Later::new(async move {
            let context = plugin_surface::native_frame_context(&handle, generation, &frame_id)?;
            if context.config["handle"] != handle
                || context.config["generation"] != generation
                || context.config["frame_id"] != frame_id
            {
                return Err("stale native frame metadata".into());
            }
            let s = state()?;
            let artifact = s
                .artifact
                .lock()
                .map_err(|_| "native artifact unavailable")?
                .clone()
                .ok_or("native artifact unavailable")?;
            if artifact.report_digest != context.report_digest
                || artifact.report.original_sha256 != context.report.original_sha256
            {
                return Err("native frame source mismatch".into());
            }
            let key = FrameKey {
                mount_id: context.ticket.mount_id().into(),
                generation: context.ticket.generation(),
                frame_id: context.ticket.frame_id().into(),
            };
            let worker_state = s.clone();
            let delivery = s
                .frames
                .submit(key, move |cancel| {
                    let _state_keepalive = worker_state;
                    if cancel.cancelled() {
                        return Err("frame cancelled before context read".into());
                    }
                    let start =
                        plugin_surface::native_frame_context(&handle, generation, &frame_id)?;
                    if start.report_digest != context.report_digest
                        || start.config["frame_id"] != frame_id
                    {
                        return Err("native image request superseded".into());
                    }
                    if cancel.cancelled() {
                        return Err("frame cancelled before PNG".into());
                    }
                    let bytes = dynamic_worker::png(&artifact, context.page_index)?;
                    if cancel.cancelled() {
                        return Err("frame cancelled after PNG".into());
                    }
                    let fresh =
                        plugin_surface::native_frame_context(&handle, generation, &frame_id)?;
                    if fresh.report_digest != context.report_digest
                        || fresh.config["document_id"] != context.config["document_id"]
                        || fresh.config["handle"] != handle
                        || fresh.config["generation"] != generation
                        || fresh.config["frame_id"] != frame_id
                    {
                        return Err("late native image belongs to an old document".into());
                    }
                    eprintln!("DYNAMIC_FRAME {}", context.config);
                    let mut info = context.config;
                    info["text_status"] =
                        json!(artifact.report.pages[context.page_index].text_status);
                    Ok(FrameFile {
                        info,
                        image: ImageData::new(encoded_bytes::TrackedEncodedBytes::new(
                            bytes,
                            cancel.owner(),
                        )?),
                        local_lease: None,
                    })
                })?
                .await?;
            let (mut file, lease) = delivery.into_local_value();
            file.local_lease = Some(lease);
            Ok::<FrameFile, String>(file)
        })
    }
    pub fn save(candidate: String, note: String) -> Later<String> {
        Later::new(async move {
            let s = state()?;
            let value = s
                .ready_job(move || Ok(ReaderSurface::save(candidate, note)))?
                .await;
            s.phase.store(s.settled(), Ordering::SeqCst);
            value
        })
    }
}

#[cfg(test)]
mod dynamic_tests {
    use super::*;

    fn fixture_state() -> State {
        State {
            config: Config {
                resources: PathBuf::new(),
                data: PathBuf::new(),
                worker: WorkerConfig {
                    worker: PathBuf::new(),
                    library: PathBuf::new(),
                    jobs: PathBuf::new(),
                },
                nonce: "fixture".into(),
                picker_dir: None,
            },
            generation: AtomicU64::new(1),
            phase: AtomicU8::new(READY),
            has_document: AtomicBool::new(true),
            operation_running: Arc::new(AtomicBool::new(false)),
            closing: AtomicBool::new(false),
            lifecycle: Mutex::new(()),
            frames: FrameCaller::new().unwrap(),
            artifact: Mutex::new(None),
        }
    }
    #[test]
    fn legacy_ready_jobs_are_charged_before_spawn_and_shutdown_cannot_pass_them() {
        // reopen and legacy save both route through this exact ready_job gate.
        // Barrier work is an actual Job/thread, not a simulated running counter.
        for kind in ["reopen", "save"] {
            let state = fixture_state();
            let (entered, started) = std::sync::mpsc::channel();
            let (release, wait) = std::sync::mpsc::channel();
            let work_running = state.operation_running.clone();
            let job = state
                .ready_job(move || {
                    assert!(work_running.load(Ordering::SeqCst));
                    entered.send(()).unwrap();
                    wait.recv().unwrap();
                    Ok(())
                })
                .unwrap();
            assert!(
                state.operation_running.load(Ordering::SeqCst),
                "{kind} not charged before queue"
            );
            started
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap();
            {
                let _admission = state.lifecycle.lock().unwrap();
                state.closing.store(true, Ordering::SeqCst);
                state.generation.fetch_add(1, Ordering::SeqCst);
            }
            let completed = state.wait_operations_until(
                std::time::Instant::now() + std::time::Duration::from_millis(15),
            );
            assert!(
                completed.is_err(),
                "State shutdown wait passed before {kind} returned"
            );
            assert!(
                state
                    .ready_job(|| -> Result<()> { panic!("closed State must not run business") })
                    .is_err()
            );
            release.send(()).unwrap();
            assert!(
                state
                    .wait_operations_until(
                        std::time::Instant::now() + std::time::Duration::from_secs(1)
                    )
                    .is_ok()
            );
            drop(job);
            let ticket = state.frames.retirement_ticket();
            state.frames.close().unwrap();
            assert!(
                ticket
                    .wait_until(std::time::Instant::now() + std::time::Duration::from_secs(1))
                    .unwrap()
            );
        }
    }
    #[test]
    fn frame_queue_keeps_only_latest_pending_request() {
        let scheduler = FrameCaller::<FrameFile>::new().unwrap();
        let key = FrameKey {
            mount_id: "trusted-test-mount".into(),
            generation: 1,
            frame_id: "frame".into(),
        };
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = scheduler
            .submit(key.clone(), move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Err("first completed".into())
            })
            .unwrap();
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let executed = Arc::new(AtomicU64::new(0));
        let stale_count = executed.clone();
        let mut stale = scheduler
            .submit(key.clone(), move |_| {
                stale_count.fetch_add(100, Ordering::SeqCst);
                Err("stale ran".into())
            })
            .unwrap();
        let latest_count = executed.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let latest = scheduler
            .submit(key, move |_| {
                latest_count.fetch_add(1, Ordering::SeqCst);
                done_tx.send(()).unwrap();
                Err("latest completed".into())
            })
            .unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            Pin::new(&mut stale).poll(&mut cx),
            Poll::Ready(Err(_))
        ));
        release_tx.send(()).unwrap();
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        let ticket = scheduler.retirement_ticket();
        scheduler.close().unwrap();
        drop((first, latest, stale));
        assert!(
            ticket
                .wait_until(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .unwrap()
        );
    }
}
