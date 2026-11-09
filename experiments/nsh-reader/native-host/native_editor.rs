//! Trusted temporary native assembly. Gate/Core/production authority are shared
//! with the reader; provider execution uses the shared public edit broker.
//! Lock order is EDITOR -> reader Gate -> authority. Never hold Gate over wait.
use super::*;
use extension_api::composition::CompositionDescriptor;
use extension_host::{
    component::MountHandle,
    durable::{AuthorityRead, ComponentAuthoritySource, ManagedRun, UserMountIntent},
};
use learning_application::{
    annotation_draft::{AnnotationDrafts, FrozenAnnotationSave},
    draft_text::DraftId,
    provider_events::{EventAction, EventCursor, EventState, ProviderEvents},
};
use qusor_capture_probe::component_edit::{EditInvocation, EditServices};
use runtime_process::{
    Controller, FrozenExecution, ProcessEvent, WorkerHandle, WorkerLaunch, WorkerOutcome,
    WorkerState,
};
use runtime_wasm::{ExecutionLimits, RuntimeErrorKind};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static RESOURCES: Mutex<Option<PathBuf>> = Mutex::new(None);
static EDITOR: Mutex<Option<Editor>> = Mutex::new(None);
static STATUS: Mutex<Value> = Mutex::new(Value::Null);
static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
// Survives a shutdown attempt: live-host callbacks cannot reopen admission
// between retirement returning and the reader Service being taken.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
struct Apply {
    base: u64,
    text: String,
    cursor: EventCursor,
    completed: Option<u64>,
}
// Retained until confirmed OS retirement, durable finish and Controller forget.
// An unknown cleanup result must never discard this owner or clear its lease.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartFact {
    NotCalled,
    InProgress,
    Started,
    Rejected,
}
#[derive(Clone, Copy)]
enum LeaseObservation {
    Unknown,
    Absent,
    Present,
}
struct PendingRun {
    run_id: String,
    handle: Option<WorkerHandle>,
    start_fact: StartFact,
    lease_observation: LeaseObservation,
    poisoned: bool,
    events: VecDeque<ProcessEvent>,
    managed: ManagedRun<MountHandle>,
    invocation: Option<EditInvocation>,
    last: Vec<u8>,
    calls: usize,
    accepted: bool,
    retry: Option<String>,
    fatal: Option<String>,
    reaped: bool,
    not_spawned: bool,
    retirement_recorded: bool,
    lease_finished: bool,
    finished: bool,
}
struct Editor {
    binding: String,
    candidate: String,
    operation: String,
    document: i64,
    generation: i64,
    origin: Provenance,
    source_metadata: Value,
    drafts: AnnotationDrafts,
    draft: DraftId,
    events: ProviderEvents,
    service: EditServices,
    mount: MountHandle,
    package: extension_package::package::VerifiedPackage,
    controller: Controller,
    boot: String,
    next: u64,
    pending_run: Option<Arc<Mutex<PendingRun>>>,
    installation_id: i64,
    last: Option<Apply>,
    frozen: Option<FrozenAnnotationSave>,
    receipt: Option<PdfReceipt>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn api(e: impl std::fmt::Display) -> extension_api::ApiError {
    extension_api::ApiError::new(extension_api::ErrorCode::InvalidState, e.to_string())
}
pub fn initialize(resources: &Path) -> Result<()> {
    // Reset only at a controlled new lifecycle, never over a live editor/job.
    if BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("cannot initialize over active editor job".into());
    }
    let _admission = Busy;
    if editor_for_retirement()?.is_some() {
        return Err("cannot initialize over active editor lifecycle".into());
    }
    *mutex(&RESOURCES)? = Some(resources.into());
    SHUTTING_DOWN.store(false, Ordering::SeqCst);
    Ok(())
}
fn status(e: &Editor, state: &str) -> Result<Value> {
    let chunk = e.drafts.read_chunk(&e.draft, None, 0, 512).map_err(text)?;
    Ok(
        json!({"binding":e.binding,"provider":"a","revision":chunk.revision,"state":state,"pending":e.last.as_ref().is_some_and(|a|a.completed.is_none()),"last_applied_base":e.last.as_ref().filter(|a|a.completed.is_some()).map(|a|a.base),"candidate_id":e.candidate,"receipt":e.receipt}),
    )
}
fn cache(e: &Editor, state: &str) -> Result<Value> {
    let value = status(e, state)?;
    *mutex(&STATUS)? = value.clone();
    Ok(value)
}
fn validate_binding(e: &Editor, binding: &str) -> Result<()> {
    if e.binding != binding {
        return Err("stale editor binding".into());
    }
    Ok(())
}
fn validate_source(e: &Editor, g: &mut Gate) -> Result<()> {
    let (source, generation) = e.drafts.source(&e.draft).map_err(text)?;
    let current = g.host.reader_receipt()?;
    if g.document.receipt.document_id != e.document
        || current.generation != e.generation
        || generation != e.generation
        || current.installation_id != e.origin.installation_id
        || source.document_id() != e.document
        || source.provenance() != &e.origin
        || source.vault() != g.document.vault
        || source.original_sha256() != g.document.snapshot.original_sha256
        || source.extraction_revision()
            != g.document.snapshot.extraction_revision().map_err(text)?
    {
        return Err("original capture identity or source changed".into());
    }
    policy(
        g,
        Capability::CapturePropose,
        Resource::Vault {
            vault_id: source.vault().into(),
        },
        "native-editor-capture",
    )
}
fn begin_value(e: &Editor) -> Result<Value> {
    let state = if e.receipt.is_some() {
        "saved"
    } else if e.frozen.is_some() || e.last.as_ref().is_some_and(|a| a.completed.is_none()) {
        "uncertain"
    } else {
        "editing"
    };
    let mut value = cache(e, state)?;
    let body = e.drafts.read_chunk(&e.draft, None, 0, 512).map_err(text)?;
    value["source"] = e.source_metadata.clone();
    value["total_bytes"] = json!(body.total_bytes);
    value["body_loaded"] = json!(body.total_bytes == body.text.len());
    if body.total_bytes == body.text.len() {
        value["body"] = json!(body.text);
    }
    Ok(value)
}
fn begin(candidate_id: String, provider: String) -> Result<Value> {
    if provider != "a" {
        return Err("only the installed full-text provider a is available".into());
    }
    let mut slot = mutex(&EDITOR)?;
    if let Some(existing) = slot.as_ref() {
        if existing.candidate == candidate_id {
            return begin_value(existing);
        }
        return Err("resolve or discard the current editor first".into());
    }
    let resources = mutex(&RESOURCES)?
        .clone()
        .ok_or("editor resources unavailable")?;
    let bytes = read_bounded(
        &resources.join("editor-a.package"),
        extension_package::package::MAX_PACKAGE_BYTES,
    )?;
    let key = std::fs::read_to_string(resources.join("editor-a-publickey.txt")).map_err(text)?;
    let package = extension_package::package::verify_for_host(
        &bytes,
        key.trim(),
        &extension_api::SUPPORTED_HOST_APIS,
    )
    .map_err(text)?;
    let (_, asset) = package
        .file("assets/ui-composition.json")
        .ok_or("editor descriptor missing")?;
    let descriptor = CompositionDescriptor::from_slice(asset).map_err(text)?;
    if !matches!(
        descriptor.view,
        extension_api::composition::NativeEditorView::TextEditor { .. }
    ) {
        return Err("provider a requires a full-text descriptor".into());
    }
    let s = service()?;
    let mut g = mutex(&s.gate)?;
    let candidate = g.candidate.clone().ok_or("no actual selected proposal")?;
    if candidate.id != candidate_id || candidate.receipt.is_some() {
        return Err("stale or already saved proposal".into());
    }
    g.host
        .read_surface_selection(&g.session, &candidate.selection_request)
        .map_err(text)?;
    let vault = g.document.vault.clone();
    policy(
        &mut g,
        Capability::CapturePropose,
        Resource::Vault {
            vault_id: vault.clone(),
        },
        "native-editor-begin",
    )?;
    let generation = g.host.reader_receipt()?.generation;
    let mut drafts = AnnotationDrafts::new();
    let draft = match &candidate.draft {
        Draft::Single(selection) => drafts.create_pdf(
            &g.core,
            g.document.receipt.document_id,
            &g.document.vault,
            selection.clone(),
            g.origin.clone(),
            generation,
        ),
        Draft::Compound(selection) => drafts.create_pdf_compound(
            &g.core,
            g.document.receipt.document_id,
            &g.document.vault,
            selection.clone(),
            g.origin.clone(),
            generation,
        ),
    }
    .map_err(text)?;
    let binding = next(&mut g, "editor")?;
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(text)?;
    let boot = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut events = ProviderEvents::new();
    let controller = Controller::new().map_err(text)?;
    let (mount, service, installation_id) = g.host.with_component_authority(|store, host| {
        let package_id = &package.manifest().package_id;
        let receipt = match store.current(package_id) {
            Ok(r) => r,
            Err(_) => store
                .install(&format!("install-{boot}"), 0, &package)
                .map_err(text)?,
        };
        let frozen = store.current_package(&receipt).map_err(text)?;
        if frozen.digest() != package.digest() {
            return Err("installed editor package differs".into());
        }
        store
            .bind_component_vault(
                &format!("bind-{boot}"),
                receipt.revision,
                package_id,
                &vault,
            )
            .map_err(text)?;
        let facts = store
            .component_installation(package_id, AuthorityRead::Activation)
            .map_err(text)?;
        let mounted = host
            .mount_component(
                store,
                UserMountIntent::new(
                    facts,
                    Resource::Draft {
                        vault_id: vault.clone(),
                        draft_id: boot.clone(),
                    },
                )
                .map_err(text)?,
                now(),
            )
            .map_err(text)?;
        let mount = mounted.handle().clone();
        let service_result = EditServices::new(
            &mut drafts,
            &mut events,
            host,
            store,
            &mount,
            draft.clone(),
            now(),
        );
        let service = match service_result {
            Ok(service) => service,
            Err(error) => {
                // No binding has reached JS and no VM was launched; preserve
                // Closing instead of inventing a native resource release ACK.
                let _ = host.close_component(&mount);
                return Err(text(error));
            }
        };
        Ok((mount, service, receipt.installation_id))
    })?;
    let source_metadata = json!({"candidate_id":candidate.id,"title":g.document.title,"quote":candidate.draft.exact(),"document_id":g.document.receipt.document_id,"vault_id":vault,"original_sha256":g.document.snapshot.original_sha256,"extraction_revision":g.document.snapshot.extraction_revision().map_err(text)?,"capture_generation":generation,"provenance":g.origin});
    let e = Editor {
        binding,
        candidate: candidate.id,
        operation: candidate.operation,
        document: g.document.receipt.document_id,
        generation,
        origin: g.origin.clone(),
        source_metadata,
        drafts,
        draft,
        events,
        service,
        mount,
        package,
        controller,
        boot,
        next: 0,
        pending_run: None,
        installation_id,
        last: None,
        frozen: None,
        receipt: None,
    };
    let value = begin_value(&e)?;
    eprintln!(
        "NATIVE_EDITOR_BEGIN {}",
        json!({"binding":e.binding,"package_sha256":e.package.digest(),"document_id":e.document,"capture_generation":e.generation})
    );
    *slot = Some(e);
    Ok(value)
}
fn run_vm(e: &mut Editor) -> Result<Option<String>> {
    if e.pending_run.is_some() {
        return resume_run(e);
    }
    e.next = e
        .next
        .checked_add(1)
        .ok_or("editor run sequence exhausted")?;
    let run_id = format!("{}-{}", e.boot, e.next);
    let s = service()?;
    // No worker lease or spawn is attempted until the ManagedRun is retained.
    let managed = {
        let mut g = mutex(&s.gate)?;
        g.host.with_component_authority(|store, host| {
            ManagedRun::prepare_component(host, store, &e.mount, now(), &run_id).map_err(text)
        })?
    };
    e.pending_run = Some(Arc::new(Mutex::new(PendingRun {
        run_id,
        handle: None,
        start_fact: StartFact::NotCalled,
        lease_observation: LeaseObservation::Unknown,
        poisoned: false,
        events: VecDeque::new(),
        managed,
        invocation: None,
        last: Vec::new(),
        calls: 0,
        accepted: false,
        retry: None,
        fatal: None,
        reaped: false,
        not_spawned: false,
        retirement_recorded: false,
        lease_finished: false,
        finished: false,
    })));
    resume_run(e)
}
fn prepare_and_start(e: &mut Editor, p: &mut PendingRun) -> Result<()> {
    let s = service()?;
    {
        let mut g = mutex(&s.gate)?;
        g.host.with_component_authority(|store, _| {
            let receipt = store
                .current(&e.package.manifest().package_id)
                .map_err(text)?;
            if receipt.installation_id != e.installation_id {
                return Err("editor installation replaced".into());
            }
            store
                .prepare_worker_lease(
                    &receipt,
                    &e.boot,
                    &p.run_id,
                    &runtime_process::current().map_err(text)?,
                )
                .map_err(text)?;
            store.mark_worker_spawn_attempted(&p.run_id).map_err(text)
        })?;
    }
    // An unwind across start has an UNKNOWN send outcome, never NotSpawned.
    p.start_fact = StartFact::InProgress;
    match e.controller.start(WorkerLaunch {
        run_id: p.run_id.clone(),
        startup_timeout: Duration::from_secs(2),
        execution_timeout: ExecutionLimits::default().deadline,
    }) {
        Ok(handle) => {
            p.handle = Some(handle);
            p.start_fact = StartFact::Started;
            Ok(())
        }
        // Controller.start Err only occurs before Start enqueue succeeds (pinned
        // controller.rs:242-307). Keep this fact distinct from unknown panic.
        Err(error) => {
            p.start_fact = StartFact::Rejected;
            p.not_spawned = true;
            Err(text(error))
        }
    }
}
fn resume_run(e: &mut Editor) -> Result<Option<String>> {
    resume_run_until(e, None)
}
fn resume_run_until(e: &mut Editor, deadline: Option<Instant>) -> Result<Option<String>> {
    // Editor retains an Arc throughout drive, including unwind. Poison recovery
    // permits retirement only; it must never resume publication or Boot.
    let owner = e
        .pending_run
        .as_ref()
        .ok_or("editor pending run missing")?
        .clone();
    let mut p = match owner.lock() {
        Ok(p) => p,
        Err(poison) => {
            let mut p = poison.into_inner();
            p.poisoned = true;
            p.fatal = Some("editor run panicked; retirement only".into());
            p
        }
    };
    if p.finished {
        drop(p);
        e.pending_run = None;
        return Err(
            "previous run already retired; business remains unavailable after panic".into(),
        );
    }
    if p.start_fact == StartFact::NotCalled
        && p.fatal.is_none()
        && !p.poisoned
        && let Err(error) = prepare_and_start(e, &mut p)
    {
        p.fatal = Some(error);
    }
    if matches!(p.start_fact, StartFact::NotCalled | StartFact::Rejected)
        && (p.fatal.is_some() || p.poisoned)
    {
        p.not_spawned = true; // Proven no Controller Start enqueue occurred.
    }
    let result = drive_run(e, &mut p, deadline);
    let finished = p.finished;
    drop(p);
    if finished {
        e.pending_run = None;
    }
    result
}
// Ordinary APIs still use mutex(&EDITOR) and reject a poisoned Editor. Explicit
// discard may recover only to terminate/reap/finish the retained PendingRun.
fn editor_for_retirement() -> Result<MutexGuard<'static, Option<Editor>>> {
    match EDITOR.lock() {
        Ok(slot) => Ok(slot),
        Err(poison) => {
            let slot = poison.into_inner();
            if let Some(e) = slot.as_ref()
                && let Some(owner) = &e.pending_run
            {
                let mut p = owner.lock().unwrap_or_else(|poison| poison.into_inner());
                p.poisoned = true;
                p.fatal = Some("editor panicked; retirement only".into());
            }
            Ok(slot)
        }
    }
}
fn retire_run(e: &mut Editor, p: &mut PendingRun) -> Result<()> {
    if !p.reaped && !p.not_spawned {
        return Err("worker exit remains unconfirmed".into());
    }
    if !p.retirement_recorded {
        if p.reaped {
            p.managed.record_reaped().map_err(text)?;
        } else {
            p.managed.record_not_spawned().map_err(text)?;
        }
        p.retirement_recorded = true;
    }
    if !p.lease_finished {
        let s = service()?;
        let mut g = mutex(&s.gate)?;
        g.host.with_component_authority(|store, _| {
            let leases = store.worker_leases().map_err(text)?;
            let matching = leases.iter().find(|lease| lease.run_id == p.run_id);
            if let Some(lease) = matching {
                if lease.installation.installation_id != e.installation_id
                    || lease.supervisor_boot != e.boot
                {
                    p.lease_observation = LeaseObservation::Unknown;
                    return Err("editor lease identity mismatch; retained".into());
                }
                p.lease_observation = LeaseObservation::Present;
            } else {
                p.lease_observation = LeaseObservation::Absent;
            }
            // Even prepare/mark Err is reconciled by an actual query. Never infer
            // absent row from Err; delete only this run after actual retirement.
            #[cfg(feature = "native-qualification")]
            qualification::fault("finish")?;
            p.managed
                .finish(|| match p.lease_observation {
                    LeaseObservation::Present => store.finish_worker_lease(&p.run_id).map_err(api),
                    LeaseObservation::Absent => Ok(()),
                    LeaseObservation::Unknown => Err(api("editor lease remains unknown")),
                })
                .map_err(text)
        })?;
        p.lease_finished = true;
    }
    if let Some(handle) = &p.handle {
        e.controller.forget(handle).map_err(text)?;
    }
    p.finished = true;
    if p.reaped {
        eprintln!(
            "NATIVE_EDITOR_REAP {}",
            json!({"run_id":p.run_id,"calls":p.calls})
        );
    } else {
        eprintln!("NATIVE_EDITOR_NOT_SPAWNED {}", json!({"run_id":p.run_id}));
    }
    Ok(())
}
fn drive_run(
    e: &mut Editor,
    p: &mut PendingRun,
    deadline: Option<Instant>,
) -> Result<Option<String>> {
    // Service/authority errors are not permission to lose the running Child owner.
    // On a later call even a previously failed lease finish is retried from observed state.
    if p.reaped || p.not_spawned {
        retire_run(e, p)?;
        return run_result(p);
    }
    if p.handle.is_none() {
        p.events.extend(e.controller.poll());
        // A panic inside start may have happened after enqueue. Recover only a
        // real event's owned handle, never infer a pid or invent NotSpawned.
        p.handle = p.events.iter().find_map(|event| {
            let handle = event_handle(event);
            (handle.run_id() == p.run_id).then(|| handle.clone())
        });
        if p.handle.is_some() {
            p.start_fact = StartFact::Started;
        }
    }
    let handle = p
        .handle
        .clone()
        .ok_or("Controller start outcome unknown; owner and lease retained")?;
    let limits = ExecutionLimits::default();
    let end = deadline.unwrap_or_else(|| {
        Instant::now() + {
            #[cfg(feature = "native-qualification")]
            {
                qualification::wait_limit()
            }
            #[cfg(not(feature = "native-qualification"))]
            {
                Duration::from_secs(8)
            }
        }
    });
    if p.invocation.is_none() && p.fatal.is_none() {
        #[cfg(feature = "native-qualification")]
        let invocation = qualification::fault("invocation")
            .and_then(|()| e.service.invocation(Some(&p.run_id)).map_err(text));
        #[cfg(not(feature = "native-qualification"))]
        let invocation = e.service.invocation(Some(&p.run_id)).map_err(text);
        match invocation {
            Ok(i) => p.invocation = Some(i),
            Err(error) => p.fatal = Some(text(error)),
        }
    }
    loop {
        if CANCEL.load(Ordering::SeqCst) {
            p.fatal
                .get_or_insert_with(|| "editor operation cancelled; input retained".to_owned());
        }
        if p.fatal.is_some() || p.retry.is_some() {
            let _ = p.managed.quarantine();
            // Backpressure/unknown terminate is retried on subsequent iterations.
            let _ = e.controller.terminate(&handle);
        }
        // Never return from inside this batch: poll has transferred all events to us.
        p.events.extend(e.controller.poll());
        while let Some(event) = p.events.pop_front() {
            let result: Result<()> = (|| {
                match event {
                    ProcessEvent::Guarded { handle, identity } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        if p.fatal.is_some() || p.retry.is_some() {
                            return Ok(());
                        }
                        #[cfg(feature = "native-qualification")]
                        qualification::fault("gate")?;
                        let s = service()?;
                        let mut g = mutex(&s.gate)?;
                        g.host.with_component_authority(|store, host| {
                            store
                                .bind_worker_lease(handle.run_id(), identity)
                                .map_err(text)?;
                            #[cfg(feature = "native-qualification")]
                            if qualification::hold_real_guard() {
                                return Ok(());
                            }
                            p.managed
                                .release_component_boot(host, store, now(), || {
                                    e.controller
                                        .release_boot(
                                            &handle,
                                            FrozenExecution {
                                                module: e.package.module_bytes().to_vec(),
                                                limits: limits.clone(),
                                                request_limit: 65536,
                                            },
                                        )
                                        .map_err(api)
                                })
                                .map_err(text)
                        })?;
                    }
                    ProcessEvent::Call {
                        handle,
                        call_id,
                        bytes,
                    } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        p.calls += 1;
                        if p.fatal.is_some() || p.retry.is_some() {
                            return Ok(());
                        }
                        let admit = || {
                            if e.controller.status(&handle).is_ok_and(|s| {
                                s.state == WorkerState::Running && s.pending_call == Some(call_id)
                            }) {
                                Ok(())
                            } else {
                                Err(api("editor call no longer running"))
                            }
                        };
                        let s = service()?;
                        let mut g = mutex(&s.gate)?;
                        let result = g.host.with_component_authority(|store, host| {
                            e.service
                                .call(
                                    &mut e.drafts,
                                    &mut e.events,
                                    host,
                                    store,
                                    p.invocation.as_ref().ok_or("editor invocation missing")?,
                                    &bytes,
                                    now(),
                                    &admit,
                                )
                                .map_err(text)
                        });
                        match result {
                            Ok(reply) => {
                                match e.controller.reply(&handle, call_id, reply.clone()) {
                                    Ok(()) => p.last = reply,
                                    Err(error) => p.retry = Some(text(error)),
                                }
                            }
                            Err(error) => {
                                if !e.controller.status(&handle).is_ok_and(|s| {
                                    s.state == WorkerState::Running
                                        && s.pending_call == Some(call_id)
                                }) {
                                    p.retry = Some(error);
                                } else {
                                    p.fatal = Some(error);
                                }
                            }
                        }
                    }
                    ProcessEvent::ResultReceived { handle, outcome } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        if p.fatal.is_some() || p.retry.is_some() {
                            return Ok(());
                        }
                        let s = service()?;
                        let mut g = mutex(&s.gate)?;
                        let result = g.host.with_component_authority(|store, host| {
                            e.service
                                .validate_result(
                                    &mut e.drafts,
                                    &mut e.events,
                                    host,
                                    store,
                                    p.invocation.as_ref().ok_or("editor invocation missing")?,
                                    now(),
                                )
                                .map_err(text)?;
                            p.managed
                                .accept_component_result(host, store, now(), || match &outcome {
                                    WorkerOutcome::Completed(r)
                                        if r.calls == p.calls && r.output.as_bytes() == p.last =>
                                    {
                                        Ok(outcome)
                                    }
                                    WorkerOutcome::Failed(_) => Ok(outcome),
                                    _ => Err(api("editor result differs from authenticated reply")),
                                })
                                .map_err(text)
                        });
                        match result {
                            Ok(WorkerOutcome::Completed(result)) => {
                                eprintln!(
                                    "NATIVE_EDITOR_VM {}",
                                    json!({"run_id":p.run_id,"calls":result.calls,"fuel":result.fuel_consumed})
                                );
                                p.accepted = true;
                            }
                            Ok(WorkerOutcome::Failed(error))
                                if error.kind == RuntimeErrorKind::DeadlineExceeded =>
                            {
                                p.retry = Some(error.code)
                            }
                            Ok(WorkerOutcome::Failed(error)) => {
                                p.fatal =
                                    Some(format!("editor runtime {:?}:{}", error.kind, error.code))
                            }
                            Err(error) => p.fatal = Some(error),
                        }
                    }
                    ProcessEvent::Reaped { handle, .. } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        p.reaped = true;
                        #[cfg(feature = "native-qualification")]
                        qualification::observed_reap(&p.run_id);
                    }
                    ProcessEvent::Failed { handle, error } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        if error.kind == runtime_process::ProcessErrorKind::Timeout {
                            p.retry = Some(text(error));
                        } else {
                            p.fatal = Some(text(error));
                        }
                        if e.controller.status(&handle).map_err(text)?.state
                            == WorkerState::NotSpawned
                        {
                            p.not_spawned = true;
                        }
                    }
                    ProcessEvent::Quarantined { handle, error } => {
                        if handle.run_id() != p.run_id {
                            return Err("foreign editor run event".into());
                        }
                        p.fatal =
                            Some(format!("editor worker unconfirmed; lease retained:{error}"));
                    }
                    ProcessEvent::Spawned { .. } | ProcessEvent::Ready { .. } => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                p.fatal = Some(error);
            }
        }
        #[cfg(feature = "native-qualification")]
        if qualification::yield_real_guard() {
            return Err("qualification yielded at actual GuardReady; owner retained".into());
        }
        if p.reaped || p.not_spawned {
            retire_run(e, p)?;
            return run_result(p);
        }
        if Instant::now() >= end {
            p.fatal.get_or_insert_with(|| {
                "editor worker cleanup unknown; persistent lease retained".into()
            });
            let _ = p.managed.quarantine();
            let _ = e.controller.terminate(&handle);
            return Err(
                "editor worker cleanup unknown; pending owner and persistent lease retained".into(),
            );
        }
        thread::sleep(Duration::from_millis(1));
    }
}
fn event_handle(event: &ProcessEvent) -> &WorkerHandle {
    match event {
        ProcessEvent::Guarded { handle, .. }
        | ProcessEvent::Call { handle, .. }
        | ProcessEvent::ResultReceived { handle, .. }
        | ProcessEvent::Reaped { handle, .. }
        | ProcessEvent::Failed { handle, .. }
        | ProcessEvent::Quarantined { handle, .. }
        | ProcessEvent::Spawned { handle, .. }
        | ProcessEvent::Ready { handle, .. } => handle,
    }
}
fn run_result(p: &mut PendingRun) -> Result<Option<String>> {
    if let Some(error) = p.fatal.take() {
        return Err(error);
    }
    if !p.accepted && p.retry.is_none() {
        return Err("editor worker reaped without authenticated result".into());
    }
    Ok(p.retry.take())
}

fn apply(binding: String, base: u64, input: String) -> Result<Value> {
    let mut slot = mutex(&EDITOR)?;
    let e = slot.as_mut().ok_or("editor not open")?;
    validate_binding(e, &binding)?;
    if e.frozen.is_some() {
        return Err("saving editor is immutable".into());
    }
    if let Some(last) = &e.last {
        if last.base == base && last.text == input {
            if last.completed.is_some() {
                let s = service()?;
                let mut g = mutex(&s.gate)?;
                g.host.with_component_authority(|store, host| {
                    let i = e.service.invocation(None).map_err(text)?;
                    e.service
                        .validate_result(&mut e.drafts, &mut e.events, host, store, &i, now())
                        .map_err(text)?;
                    Ok(())
                })?;
                let mut v = cache(e, "editing")?;
                v["applied"] = json!(true);
                return Ok(v);
            }
        } else if last.completed.is_none() {
            return Err("different input cannot replace an unresolved operation".into());
        }
    }
    let same = e
        .last
        .as_ref()
        .is_some_and(|a| a.base == base && a.text == input);
    let s = service()?;
    if !same {
        let current = e
            .drafts
            .read_chunk(&e.draft, None, 0, 512)
            .map_err(text)?
            .revision;
        if current != base {
            return Err("stale editor text revision".into());
        }
        let snapshot = {
            let mut g = mutex(&s.gate)?;
            g.host.with_component_authority(|store, host| {
                e.service
                    .publish(
                        &mut e.drafts,
                        &mut e.events,
                        host,
                        store,
                        base,
                        EventAction::InputChanged,
                        input.clone(),
                        now(),
                    )
                    .map_err(text)
            })?
        };
        e.last = Some(Apply {
            base,
            text: input,
            cursor: snapshot.cursor,
            completed: None,
        });
    }
    cache(e, "applying")?;
    let cursor = e.last.as_ref().ok_or("pending input missing")?.cursor;
    let mut retries = 0;
    for _ in 0..8192 {
        if run_vm(e)?.is_some() {
            retries += 1;
            if retries > 16 {
                return Err("editor bounded recovery exhausted; input retained".into());
            }
        }
        let snapshot = {
            let mut g = mutex(&s.gate)?;
            g.host.with_component_authority(|store, host| {
                let i = e.service.invocation(None).map_err(text)?;
                e.service
                    .validate_result(&mut e.drafts, &mut e.events, host, store, &i, now())
                    .map_err(text)
            })?
        };
        if snapshot.cursor != cursor {
            return Err("editor event replaced unexpectedly".into());
        }
        if snapshot.state == EventState::Completed {
            let revision = e
                .drafts
                .read_chunk(&e.draft, None, 0, 512)
                .map_err(text)?
                .revision;
            e.last.as_mut().ok_or("pending input missing")?.completed = Some(revision);
            let mut v = cache(e, "editing")?;
            v["applied"] = json!(true);
            return Ok(v);
        }
    }
    Err("editor VM budget exhausted; input retained".into())
}
fn save(binding: String, expected: u64) -> Result<Value> {
    let mut slot = mutex(&EDITOR)?;
    let e = slot.as_mut().ok_or("editor not open")?;
    validate_binding(e, &binding)?;
    if e.last.as_ref().is_some_and(|a| a.completed.is_none()) {
        return Err("input has not been applied".into());
    }
    let revision = e
        .drafts
        .read_chunk(&e.draft, None, 0, 512)
        .map_err(text)?
        .revision;
    if revision != expected {
        return Err("stale save revision".into());
    }
    if let Some(receipt) = e.receipt.as_ref() {
        let mut value = cache(e, "saved")?;
        value["receipt"] = json!(receipt);
        return Ok(value);
    }
    let s = service()?;
    let mut g = mutex(&s.gate)?;
    // Receipt discovery is read-only and remains possible after capture revoke.
    let recovered = if let Some(frozen) = e.frozen.as_ref() {
        e.drafts.reconcile(&g.core, frozen).map_err(text)?
    } else {
        None
    };
    let receipt = if let Some(receipt) = recovered {
        receipt
    } else {
        validate_source(e, &mut g)?;
        if e.frozen.is_none() {
            e.frozen = Some(
                e.drafts
                    .freeze_save(&e.draft, expected, &e.operation)
                    .map_err(text)?,
            );
        }
        cache(e, "saving")?;
        e.drafts
            .commit(
                &mut g.core,
                e.frozen.as_ref().ok_or("save identity missing")?,
            )
            .map_err(text)?
    };
    e.receipt = Some(receipt.clone());
    if let Some(candidate) = g.candidate.as_mut().filter(|c| c.id == e.candidate) {
        candidate.receipt = Some(receipt.clone());
    }
    let mut value = cache(e, "saved")?;
    value["receipt"] = json!(receipt);
    Ok(value)
}
fn discard(binding: String) -> Result<Value> {
    let mut slot = editor_for_retirement()?;
    let e = slot.as_mut().ok_or("editor not open")?;
    validate_binding(e, &binding)?;
    if e.pending_run.is_some() {
        if let Some(owner) = e.pending_run.as_ref() {
            let mut p = owner.lock().unwrap_or_else(|poison| poison.into_inner());
            p.fatal.get_or_insert_with(|| {
                "editor discarded; pending input retained until cleanup".into()
            });
        }
        // A fatal business outcome after complete retirement does not prevent discard.
        let _ = resume_run(e);
        if e.pending_run.is_some() {
            return Err("editor worker cleanup remains unresolved".into());
        }
    }
    let s = service()?;
    let mut g = mutex(&s.gate)?;
    g.host.with_component_authority(|store, host| {
        if store
            .worker_leases()
            .map_err(text)?
            .iter()
            .any(|lease| lease.installation.installation_id == e.installation_id)
        {
            return Err("worker cleanup unresolved".into());
        }
        e.service
            .close(&mut e.drafts, &mut e.events)
            .map_err(text)?;
        host.close_component(&e.mount).map_err(text)?;
        // No native Input release ACK is available. Closing quota remains held.
        Ok(())
    })?;
    e.drafts.remove(&e.draft).map_err(text)?;
    *slot = None;
    *mutex(&STATUS)? = Value::Null;
    Ok(json!({"binding":binding,"state":"closing","pending_native_release_ack":true}))
}
fn encode(result: Result<Value>) -> String {
    match result {
        Ok(v) => json!({"ok":true,"value":v}).to_string(),
        Err(e) => json!({"ok":false,"error":e,"code":"editor_state_unknown","effect_unknown":true})
            .to_string(),
    }
}
fn reject(message: &str) -> String {
    json!({"ok":false,"error":message,"code":"input_rejected","effect_unknown":false}).to_string()
}
fn busy_error() -> String {
    json!({"ok":false,"error":"the original editor operation may still be active","code":"editor_busy","effect_unknown":true}).to_string()
}
struct Busy;
impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}
fn shutdown_error() -> String {
    json!({"ok":false,"code":"editor_shutting_down","effect_unknown":true,"error":"editor is closing; only explicit retirement is available"}).to_string()
}
async fn background(work: impl FnOnce() -> Result<Value> + Send + 'static) -> Result<String> {
    background_with_admission(false, work).await
}
async fn background_with_admission(
    retirement_only: bool,
    work: impl FnOnce() -> Result<Value> + Send + 'static,
) -> Result<String> {
    if !retirement_only && SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Ok(shutdown_error());
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok(busy_error());
    }
    // Recheck after acquiring BUSY: shutdown may latch while this caller was
    // racing admission. Existing jobs retire; new publication is rejected.
    if !retirement_only {
        CANCEL.store(false, Ordering::SeqCst);
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            CANCEL.store(true, Ordering::SeqCst);
            BUSY.store(false, Ordering::SeqCst);
            return Ok(shutdown_error());
        }
    }
    // The background job owns admission even when JS drops its Later future.
    let guard = Busy;
    let result = crate::dynamic_module::Job::start(move || {
        let _guard = guard;
        let result = work();
        if result.is_err()
            && let Ok(slot) = EDITOR.lock()
            && let Some(e) = slot.as_ref()
        {
            let state =
                if e.frozen.is_some() || e.last.as_ref().is_some_and(|a| a.completed.is_none()) {
                    "uncertain"
                } else {
                    "editing"
                };
            let _ = cache(e, state);
        }
        result
    })
    .await;
    Ok(encode(result))
}
/// Attempt bounded owned retirement while the reader Service is still present.
/// false retains Service/PendingRun/SQL lease; it is never an OS reap assertion.
pub(crate) fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    CANCEL.store(true, Ordering::SeqCst);
}
pub fn shutdown() -> bool {
    begin_shutdown();
    let deadline = Instant::now() + {
        #[cfg(feature = "native-qualification")]
        {
            qualification::shutdown_limit()
        }
        #[cfg(not(feature = "native-qualification"))]
        {
            Duration::from_secs(10)
        }
    };
    while BUSY.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    // Exclude new jobs between observing !BUSY and inspecting the retained owner.
    if BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        eprintln!("NATIVE_EDITOR_SHUTDOWN busy=true retirement=unconfirmed service_retained=true");
        return false;
    }
    let _admission = Busy;
    let mut slot = match EDITOR.try_lock() {
        Ok(slot) => slot,
        Err(std::sync::TryLockError::Poisoned(poison)) => {
            let slot = poison.into_inner();
            if let Some(e) = slot.as_ref()
                && let Some(owner) = &e.pending_run
            {
                let mut p = owner.lock().unwrap_or_else(|poison| poison.into_inner());
                p.poisoned = true;
                p.fatal = Some("editor panicked; shutdown retirement only".into());
            }
            slot
        }
        Err(std::sync::TryLockError::WouldBlock) => {
            eprintln!(
                "NATIVE_EDITOR_SHUTDOWN editor_locked=true retirement=unconfirmed service_retained=true"
            );
            return false;
        }
    };
    let Some(e) = slot.as_mut() else {
        eprintln!("NATIVE_EDITOR_SHUTDOWN pending_owner=false retirement=not_needed");
        return true;
    };
    if let Some(owner) = e.pending_run.as_ref() {
        let mut p = owner.lock().unwrap_or_else(|poison| poison.into_inner());
        p.fatal
            .get_or_insert_with(|| "editor shutdown; retirement only".into());
    }
    // No Gate held over waiting. The same shutdown deadline bounds both the
    // BUSY wait and drain; do not silently append the ordinary 8s drive budget.
    let result = if e.pending_run.is_some() {
        if Instant::now() < deadline {
            resume_run_until(e, Some(deadline)).map(|_| ())
        } else {
            Err("shutdown retirement budget exhausted; owner retained".into())
        }
    } else {
        Ok(())
    };
    let retained = e.pending_run.is_some();
    eprintln!(
        "NATIVE_EDITOR_SHUTDOWN {}",
        json!({
            "pending_owner":retained,"retirement_confirmed":!retained,
            "service_retained":retained,"retirement_error":result.err(),
            "native_release_ack":false
        })
    );
    !retained
}

#[module]
pub struct NativeEditor;
#[module]
impl NativeEditor {
    pub fn begin(candidate_id: String, provider: String) -> Later<String> {
        Later::new(background(move || begin(candidate_id, provider)))
    }
    pub fn apply(binding: String, base_revision: u64, text: String) -> Later<String> {
        Later::new(async move {
            if BUSY.load(Ordering::SeqCst) {
                return Ok(busy_error());
            }
            if text.len() > 1048576 || base_revision == 0 {
                return Ok(reject(
                    "input exceeds the supported budget or revision is invalid",
                ));
            }
            background(move || apply(binding, base_revision, text)).await
        })
    }
    pub fn save(binding: String, expected_revision: u64) -> Later<String> {
        Later::new(background(move || save(binding, expected_revision)))
    }
    /// Requests termination only; status becomes uncertain after actual owned reap.
    /// Pending input/event remain available for the identical apply retry.
    pub fn pause(binding: String) -> String {
        encode((|| {
            let mut value = mutex(&STATUS)?;
            if value["binding"] != binding {
                return Err("stale editor binding".into());
            }
            if value["state"] != "applying" || !BUSY.load(Ordering::SeqCst) {
                return Err("no active apply can be paused".into());
            }
            CANCEL.store(true, Ordering::SeqCst);
            value["state"] = json!("cancelling");
            Ok(value.clone())
        })())
    }
    pub fn discard(binding: String) -> Later<String> {
        Later::new(background_with_admission(true, move || discard(binding)))
    }
    pub fn status(binding: String) -> String {
        encode((|| {
            let value = mutex(&STATUS)?.clone();
            if value["binding"] != binding {
                return Err("stale editor binding".into());
            }
            Ok(value)
        })())
    }
    pub fn read(binding: String, revision: u64, offset: u32, max_bytes: u16) -> Later<String> {
        Later::new(background(move || {
            let slot = mutex(&EDITOR)?;
            let e = slot.as_ref().ok_or("editor not open")?;
            validate_binding(e, &binding)?;
            let c = e
                .drafts
                .read_chunk(
                    &e.draft,
                    Some(revision),
                    offset as usize,
                    max_bytes as usize,
                )
                .map_err(text)?;
            Ok(
                json!({"binding":binding,"revision":c.revision,"offset":c.offset,"total_bytes":c.total_bytes,"text":c.text}),
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    //! Real native reader authority/Store/Core, with fixture-owned draft text.
    //! No editor guest, native widget or release-ACK qualification is implied.
    use super::fixture::Fixture;
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll},
    };
    fn fixture_text(body: &str) -> u64 {
        let mut slot = EDITOR.lock().unwrap();
        let e = slot.as_mut().unwrap();
        let revision = e
            .drafts
            .read_chunk(&e.draft, None, 0, 512)
            .unwrap()
            .revision;
        let owner = e.drafts.create_owner().unwrap();
        let upload = e
            .drafts
            .begin_replace(&owner, &e.draft, revision, body.len(), now())
            .unwrap();
        // These deliberately small valid-UTF8 fixture strings fit one app chunk.
        e.drafts
            .append_chunk(&owner, &upload, 0, body, now())
            .unwrap();
        let revision = e.drafts.commit_replace(&owner, &upload, now()).unwrap();
        e.drafts.revoke_owner(&owner).unwrap();
        revision
    }
    #[test]
    fn committed_frozen_save_is_reconciled_after_original_capture_revocation() {
        let (_fixture, _candidate, binding) = Fixture::new("receipt");
        let revision = fixture_text("fixture thought 🙂");
        let expected = {
            let mut slot = EDITOR.lock().unwrap();
            let e = slot.as_mut().unwrap();
            let svc = service().unwrap();
            let mut g = svc.gate.lock().unwrap();
            validate_source(e, &mut g).unwrap();
            let frozen = e
                .drafts
                .freeze_save(&e.draft, revision, &e.operation)
                .unwrap();
            let receipt = e.drafts.commit(&mut g.core, &frozen).unwrap();
            e.frozen = Some(frozen);
            assert!(e.receipt.is_none()); // The native response/receipt was lost.
            receipt
        };
        let disabled: Value = serde_json::from_str(&ReaderSurface::disable()).unwrap();
        assert_eq!(disabled["ok"], true);
        {
            let slot = EDITOR.lock().unwrap();
            let svc = service().unwrap();
            let mut g = svc.gate.lock().unwrap();
            assert!(validate_source(slot.as_ref().unwrap(), &mut g).is_err());
        }
        let recovered = save(binding.clone(), revision).unwrap();
        assert_eq!(
            recovered["receipt"],
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(
            save(binding, revision).unwrap()["receipt"],
            recovered["receipt"]
        );
        let svc = service().unwrap();
        let g = svc.gate.lock().unwrap();
        let record = g.core.query_pdf_complete(expected.capture_id).unwrap();
        assert_eq!(record.user_note, "fixture thought 🙂");
        assert_eq!(record.receipt, expected);
    }
    #[test]
    fn uncommitted_frozen_save_cannot_write_after_capture_revocation() {
        let (_fixture, _candidate, binding) = Fixture::new("denied");
        let revision = fixture_text("not yet saved");
        let operation = {
            let mut slot = EDITOR.lock().unwrap();
            let e = slot.as_mut().unwrap();
            e.frozen = Some(
                e.drafts
                    .freeze_save(&e.draft, revision, &e.operation)
                    .unwrap(),
            );
            e.operation.clone()
        };
        let disabled: Value = serde_json::from_str(&ReaderSurface::disable()).unwrap();
        assert_eq!(disabled["ok"], true);
        assert!(save(binding, revision).is_err());
        let svc = service().unwrap();
        let g = svc.gate.lock().unwrap();
        assert!(g.core.query_operation(&operation).unwrap().is_none());
        let mut slot = EDITOR.lock().unwrap();
        let e = slot.as_mut().unwrap();
        assert!(
            e.drafts
                .reconcile(&g.core, e.frozen.as_ref().unwrap())
                .unwrap()
                .is_none()
        );
        assert!(e.receipt.is_none());
    }
    #[test]
    fn repeated_begin_preserves_body_binding_and_original_source_metadata() {
        let (_fixture, candidate, binding) = Fixture::new("begin");
        let body = "literal 🙂 body";
        let revision = fixture_text(body);
        let first = begin(candidate.clone(), "a".into()).unwrap();
        assert_eq!(first["binding"], binding);
        assert_eq!(first["revision"], revision);
        assert_eq!(first["body"], body);
        assert_eq!(first["body_loaded"], true);
        {
            let svc = service().unwrap();
            svc.gate.lock().unwrap().document.title = "changed current title".into();
        }
        let second = begin(candidate, "a".into()).unwrap();
        assert_eq!(second, first);
        assert!(begin("another-candidate".into(), "a".into()).is_err());
    }
    #[test]
    fn busy_duplicate_retains_unknown_effect_and_does_not_enter_job() {
        let (_fixture, _candidate, binding) = Fixture::new("busy");
        let revision = fixture_text("retained");
        BUSY.store(true, Ordering::SeqCst);
        let mut future = Box::pin(background(|| panic!("busy request must not execute")));
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let Poll::Ready(Ok(reply)) = future.as_mut().poll(&mut cx) else {
            panic!("busy response not ready")
        };
        let value: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(value["code"], "editor_busy");
        assert_eq!(value["effect_unknown"], true);
        assert!(BUSY.load(Ordering::SeqCst));
        // Simulate the original job's guard completing, never a second job.
        drop(Busy);
        assert!(!BUSY.load(Ordering::SeqCst));
        let slot = EDITOR.lock().unwrap();
        let e = slot.as_ref().unwrap();
        assert_eq!(e.binding, binding);
        let actual = e.drafts.read_chunk(&e.draft, None, 0, 512).unwrap();
        assert_eq!(actual.revision, revision);
        assert_eq!(actual.text, "retained");
    }
    #[test]
    fn latched_shutdown_rejects_business_but_allows_explicit_discard() {
        let (_fixture, _, binding) = Fixture::new("shutdown-admission");
        SHUTTING_DOWN.store(true, Ordering::SeqCst);
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let mut blocked = std::pin::pin!(background(|| panic!("latched business must not run")));
        let Poll::Ready(Ok(reply)) = blocked.as_mut().poll(&mut cx) else {
            panic!("latched call must reject immediately")
        };
        let reply: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["code"], "editor_shutting_down");
        assert!(!BUSY.load(Ordering::SeqCst));
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/dynamic-pdf-resources");
        assert!(
            initialize(&resources).is_err(),
            "live editor must prevent lifecycle reset"
        );
        let mut retired = std::pin::pin!(background_with_admission(true, move || discard(binding)));
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if let Poll::Ready(result) = retired.as_mut().poll(&mut cx) {
                let reply: Value = serde_json::from_str(&result.unwrap()).unwrap();
                assert_eq!(reply["ok"], true);
                break;
            }
            assert!(Instant::now() < until, "explicit retirement did not return");
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            SHUTTING_DOWN.load(Ordering::SeqCst),
            "discard must not reopen admission"
        );
        assert!(EDITOR.lock().unwrap().is_none());
        initialize(&resources).unwrap();
        assert!(!SHUTTING_DOWN.load(Ordering::SeqCst));
    }
}

#[cfg(any(test, feature = "native-qualification"))]
#[path = "native_editor_fixture.rs"]
mod fixture;
#[cfg(feature = "native-qualification")]
#[path = "native_editor_qualification.rs"]
pub mod qualification;
