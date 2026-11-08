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
use qusor_capture_probe::component_edit::EditServices;
use runtime_process::{
    Controller, FrozenExecution, ProcessEvent, WorkerLaunch, WorkerOutcome, WorkerState,
};
use runtime_wasm::{ExecutionLimits, RuntimeErrorKind};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static RESOURCES: Mutex<Option<PathBuf>> = Mutex::new(None);
static EDITOR: Mutex<Option<Editor>> = Mutex::new(None);
static STATUS: Mutex<Value> = Mutex::new(Value::Null);
static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
struct Apply {
    base: u64,
    text: String,
    cursor: EventCursor,
    completed: Option<u64>,
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
    *mutex(&RESOURCES)? = Some(resources.into());
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
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(text)?;
    let boot = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut events = ProviderEvents::new();
    let controller = Controller::new().map_err(text)?;
    let (mount, service) = g.host.with_component_authority(|store, host| {
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
        Ok((mount, service))
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
    e.next = e
        .next
        .checked_add(1)
        .ok_or("editor run sequence exhausted")?;
    let run = format!("{}-{}", e.boot, e.next);
    let s = service()?;
    let mut managed = {
        let mut g = mutex(&s.gate)?;
        g.host.with_component_authority(|store, host| {
            let m =
                ManagedRun::prepare_component(host, store, &e.mount, now(), &run).map_err(text)?;
            let receipt = store
                .current(&e.package.manifest().package_id)
                .map_err(text)?;
            store
                .prepare_worker_lease(
                    &receipt,
                    &e.boot,
                    &run,
                    &runtime_process::current().map_err(text)?,
                )
                .map_err(text)?;
            store.mark_worker_spawn_attempted(&run).map_err(text)?;
            Ok(m)
        })?
    };
    let limits = ExecutionLimits::default();
    let handle = match e.controller.start(WorkerLaunch {
        run_id: run.clone(),
        startup_timeout: Duration::from_secs(2),
        execution_timeout: limits.deadline,
    }) {
        Ok(h) => h,
        Err(error) => {
            managed.record_not_spawned().map_err(text)?;
            let mut g = mutex(&s.gate)?;
            g.host.with_component_authority(|store, _| {
                managed
                    .finish(|| store.finish_worker_lease(&run).map_err(api))
                    .map_err(text)
            })?;
            return Err(text(error));
        }
    };
    let invocation = e.service.invocation(Some(&run)).map_err(text)?;
    let end = Instant::now() + Duration::from_secs(8);
    let mut last = Vec::new();
    let mut calls = 0;
    let mut accepted = false;
    let mut retry = None;
    let mut fatal = None;
    loop {
        if Instant::now() >= end {
            let _ = e.controller.terminate(&handle);
            return Err("editor worker cleanup unknown; persistent lease retained".into());
        }
        if CANCEL.load(Ordering::SeqCst) {
            fatal = Some("editor operation cancelled; input retained".to_owned());
            let _ = e.controller.terminate(&handle);
        }
        for event in e.controller.poll() {
            match event {
                ProcessEvent::Guarded { handle, identity } => {
                    let mut g = mutex(&s.gate)?;
                    let r = g.host.with_component_authority(|store, host| {
                        store
                            .bind_worker_lease(handle.run_id(), identity)
                            .map_err(text)?;
                        managed
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
                    });
                    if let Err(error) = r {
                        fatal = Some(error);
                        let _ = managed.quarantine();
                        let _ = e.controller.terminate(&handle);
                    }
                }
                ProcessEvent::Call {
                    handle,
                    call_id,
                    bytes,
                } => {
                    calls += 1;
                    if retry.is_some() || fatal.is_some() {
                        continue;
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
                    let result = {
                        let mut g = mutex(&s.gate)?;
                        g.host.with_component_authority(|store, host| {
                            e.service
                                .call(
                                    &mut e.drafts,
                                    &mut e.events,
                                    host,
                                    store,
                                    &invocation,
                                    &bytes,
                                    now(),
                                    &admit,
                                )
                                .map_err(text)
                        })
                    };
                    match result {
                        Ok(reply) => match e.controller.reply(&handle, call_id, reply.clone()) {
                            Ok(()) => last = reply,
                            Err(error) => {
                                retry = Some(text(error));
                                let _ = managed.quarantine();
                                let _ = e.controller.terminate(&handle);
                            }
                        },
                        Err(error) => {
                            if !e.controller.status(&handle).is_ok_and(|s| {
                                s.state == WorkerState::Running && s.pending_call == Some(call_id)
                            }) {
                                retry = Some(error);
                            } else {
                                fatal = Some(error);
                            }
                            let _ = managed.quarantine();
                            let _ = e.controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::ResultReceived { handle, outcome } => {
                    if retry.is_some() || fatal.is_some() {
                        continue;
                    }
                    let result = {
                        let mut g = mutex(&s.gate)?;
                        g.host.with_component_authority(|store, host| {
                            e.service
                                .validate_result(
                                    &mut e.drafts,
                                    &mut e.events,
                                    host,
                                    store,
                                    &invocation,
                                    now(),
                                )
                                .map_err(text)?;
                            managed
                                .accept_component_result(host, store, now(), || match &outcome {
                                    WorkerOutcome::Completed(r)
                                        if r.calls == calls && r.output.as_bytes() == last =>
                                    {
                                        Ok(outcome)
                                    }
                                    WorkerOutcome::Failed(_) => Ok(outcome),
                                    _ => Err(api("editor result differs from authenticated reply")),
                                })
                                .map_err(text)
                        })
                    };
                    match result {
                        Ok(WorkerOutcome::Completed(result)) => {
                            eprintln!(
                                "NATIVE_EDITOR_VM {}",
                                json!({"run_id":run,"calls":result.calls,"fuel":result.fuel_consumed})
                            );
                            accepted = true;
                        }
                        Ok(WorkerOutcome::Failed(error))
                            if error.kind == RuntimeErrorKind::DeadlineExceeded =>
                        {
                            retry = Some(error.code);
                            let _ = managed.quarantine();
                            let _ = e.controller.terminate(&handle);
                        }
                        Ok(WorkerOutcome::Failed(error)) => {
                            fatal = Some(format!("editor runtime {:?}:{}", error.kind, error.code));
                        }
                        Err(error) => {
                            fatal = Some(error);
                            let _ = managed.quarantine();
                            let _ = e.controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::Reaped { handle, .. } => {
                    managed.record_reaped().map_err(text)?;
                    {
                        let mut g = mutex(&s.gate)?;
                        g.host.with_component_authority(|store, _| {
                            managed
                                .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                                .map_err(text)
                        })?;
                    }
                    e.controller.forget(&handle).map_err(text)?;
                    eprintln!("NATIVE_EDITOR_REAP {}", json!({"run_id":run,"calls":calls}));
                    if let Some(error) = fatal {
                        return Err(error);
                    }
                    if !accepted && retry.is_none() {
                        return Err("editor worker reaped without authenticated result".into());
                    }
                    return Ok(retry);
                }
                ProcessEvent::Failed { handle, error } => {
                    if error.kind == runtime_process::ProcessErrorKind::Timeout {
                        retry = Some(text(error));
                    } else {
                        fatal = Some(text(error));
                    }
                    if e.controller.status(&handle).map_err(text)?.state == WorkerState::NotSpawned
                    {
                        managed.record_not_spawned().map_err(text)?;
                        let mut g = mutex(&s.gate)?;
                        g.host.with_component_authority(|store, _| {
                            managed
                                .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                                .map_err(text)
                        })?;
                        e.controller.forget(&handle).map_err(text)?;
                        return Err("editor worker definitely not spawned".into());
                    }
                    let _ = managed.quarantine();
                    let _ = e.controller.terminate(&handle);
                }
                ProcessEvent::Quarantined { error, .. } => {
                    return Err(format!("editor worker unconfirmed; lease retained:{error}"));
                }
                ProcessEvent::Spawned { .. } | ProcessEvent::Ready { .. } => {}
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
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
    let mut slot = mutex(&EDITOR)?;
    let e = slot.as_mut().ok_or("editor not open")?;
    validate_binding(e, &binding)?;
    let s = service()?;
    let mut g = mutex(&s.gate)?;
    g.host.with_component_authority(|store, host| {
        if !store.worker_leases().map_err(text)?.is_empty() {
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
async fn background(work: impl FnOnce() -> Result<Value> + Send + 'static) -> Result<String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok(busy_error());
    }
    CANCEL.store(false, Ordering::SeqCst);
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
pub fn shutdown() {
    CANCEL.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    while BUSY.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    eprintln!(
        "NATIVE_EDITOR_SHUTDOWN pending_operation={}",
        BUSY.load(Ordering::SeqCst)
    );
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
        Later::new(background(move || discard(binding)))
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
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll},
    };
    struct Fixture {
        data: PathBuf,
        _serial: std::sync::MutexGuard<'static, ()>,
    }
    impl Fixture {
        fn new(label: &str) -> (Self, String, String) {
            let serial = super::super::reader_guest_tests::SERVICE_TEST
                .lock()
                .unwrap();
            assert!(!BUSY.load(Ordering::SeqCst));
            assert!(EDITOR.lock().unwrap().is_none());
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let reader_resources = root.join("../target/plugin-surface-resources");
            initialize(&root.join("../target/dynamic-pdf-resources")).unwrap();
            let data = std::env::temp_dir().join(format!(
                "native-editor-{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&data).unwrap();
            super::super::initialize(&reader_resources, &data).unwrap();
            let opened: Value =
                serde_json::from_str(&ReaderSurface::open(current_report_digest().unwrap()))
                    .unwrap();
            assert_eq!(opened["ok"], true);
            let cfg = &opened["value"];
            let handle = cfg["handle"].as_str().unwrap().to_owned();
            let generation = cfg["generation"].as_u64().unwrap();
            let ready: Value = serde_json::from_str(&ReaderSurface::ready(
                handle.clone(),
                generation,
                cfg["frame_id"].as_str().unwrap().into(),
                current_report_digest().unwrap(),
            ))
            .unwrap();
            assert_eq!(ready["ok"], true);
            let point = |index: usize| {
                let svc = service().unwrap();
                let g = svc.gate.lock().unwrap();
                let q = g.report.pages[0].characters[index].pixel_quad.unwrap();
                Point {
                    x: q.iter().map(|p| p[0] as f64).sum::<f64>() / 4.,
                    y: q.iter().map(|p| p[1] as f64).sum::<f64>() / 4.,
                }
            };
            let a = point(0);
            let b = point(4);
            let down: Value = serde_json::from_str(&ReaderSurface::down(
                handle.clone(),
                generation,
                71,
                a.x,
                a.y,
            ))
            .unwrap();
            assert_eq!(down["ok"], true);
            let up: Value = serde_json::from_str(&ReaderSurface::up(
                handle,
                generation,
                71,
                down["value"]["gesture"].as_u64().unwrap(),
                b.x,
                b.y,
            ))
            .unwrap();
            assert_eq!(up["ok"], true);
            let candidate = up["value"]["candidate"]["candidate_id"]
                .as_str()
                .unwrap()
                .to_owned();
            let value = begin(candidate.clone(), "a".into()).unwrap();
            let binding = value["binding"].as_str().unwrap().into();
            (
                Self {
                    data,
                    _serial: serial,
                },
                candidate,
                binding,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            BUSY.store(false, Ordering::SeqCst);
            CANCEL.store(false, Ordering::SeqCst);
            EDITOR.lock().unwrap().take();
            *STATUS.lock().unwrap() = Value::Null;
            super::super::shutdown().unwrap();
            std::fs::remove_dir_all(&self.data).unwrap();
        }
    }
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
}
