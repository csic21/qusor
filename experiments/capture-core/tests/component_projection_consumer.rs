//! Actual ordinary signed projection guest, Store and supervised OS workers.
//! Golden rows are compared, never calculated here. Source is a synthetic Core
//! snapshot, not parser/gesture/Surface evidence. No native widget is mounted.
#[path = "component_support/fixture.rs"]
mod fixture;
use extension_api::continuation::*;
use extension_api::{draft::*, projection::*, *};
use extension_host::{component::MountHandle, durable::*};
use extension_package::package::{VerifiedPackage, verify_for_host};
use extension_runtime_api::{ExecutionLimits, RuntimeErrorKind};
use extension_store::Store;
use learning_application::{
    KnowledgeApplication, annotation_draft::AnnotationDrafts, draft_text::DraftId,
    provider_events as edit,
};
use qusor_capture_probe::{
    component_edit::{EditInvocation, EditServices},
    component_projection::{ProjectionInvocation, ProjectionServices},
};
use runtime_process::{
    Controller, FrozenExecution, ProcessErrorKind, ProcessEvent, WorkerLaunch, WorkerOutcome,
    WorkerState,
};
use runtime_wasm::WasmiRuntime;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::ExitCode,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;
const BOOT: &str = "42c9807ad2c044b6b839cfa763c0124a";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn api(message: impl std::fmt::Display) -> ApiError {
    eprintln!("PROJECTION_COORDINATOR {message}");
    ApiError::new(ErrorCode::InvalidState, "projection coordinator rejected")
}
fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}
struct Scratch {
    path: PathBuf,
    preserve: bool,
}
impl Scratch {
    fn new() -> Result<Self, String> {
        let path =
            std::env::temp_dir().join(format!("qusor-projection-process-{}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(fail)?;
        Ok(Self {
            path,
            preserve: true,
        })
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.path);
        } else {
            eprintln!("PROJECTION_FIXTURE_RETAINED {}", self.path.display());
        }
    }
}
#[derive(Default)]
struct Stats {
    vms: usize,
    calls: usize,
    reaped: usize,
    recoveries: usize,
    max_fuel: u64,
    max_calls: usize,
    max_broker_us: u128,
    partial_seen: bool,
    old_invocations_rejected: usize,
    old_query_requests_rejected: usize,
    late_results_rejected: usize,
    direct_writes_rejected: usize,
    recovery_by_checkpoint: BTreeMap<(String, u64), usize>,
    records: Vec<serde_json::Value>,
}
fn live(controller: &Controller, handle: &runtime_process::WorkerHandle, call: u64) -> bool {
    controller
        .status(handle)
        .is_ok_and(|s| s.state == WorkerState::Running && s.pending_call == Some(call))
}
fn metadata_request(context: &ProjectionContext) -> ProjectionRequest {
    ProjectionRequest::Query {
        api_version: HOST_API_V0_3,
        projection_protocol: 1,
        request_id: "trusted-status".into(),
        resource: context.resource.clone(),
        mount_id: context.mount_id.clone(),
        query_id: context.query_id.clone(),
        base_revision: context.base_revision,
        operation: ProjectionOperation::ReadMetadata {},
    }
}
fn native_metadata(
    svc: &mut ProjectionServices,
    drafts: &AnnotationDrafts,
    host: &mut DurableHost,
    store: &Store,
    context: &ProjectionContext,
) -> Result<ProjectionMetadata, String> {
    if !store.worker_leases().map_err(fail)?.is_empty() {
        return Err("native metadata attempted while lease remains".into());
    }
    let token = svc.invocation(None).map_err(fail)?;
    let bytes = svc
        .call(
            drafts,
            host,
            store,
            &token,
            metadata_request(context)
                .to_json()
                .map_err(fail)?
                .as_bytes(),
            now(),
            &|| Ok(()),
        )
        .map_err(fail)?;
    let ProjectionResponse::Query {
        outcome: ProjectionOutcome::Metadata { metadata },
        ..
    } = ProjectionResponse::from_slice(&bytes).map_err(fail)?
    else {
        return Err("expected native projection metadata".into());
    };
    metadata.validate_for_context(context).map_err(fail)?;
    Ok(metadata)
}
fn inspect(reply: &[u8], stats: &mut Stats) {
    if let Ok(ProjectionResponse::Query {
        outcome: ProjectionOutcome::Metadata { metadata },
        ..
    }) = ProjectionResponse::from_slice(reply)
    {
        stats.partial_seen |= metadata
            .window
            .is_some_and(|w| w.rows.iter().any(|r| !r.complete));
    }
}
// Test-only purpose routing; the OS lifecycle below remains one implementation.
enum Task<'a> {
    Projection(&'a mut ProjectionServices),
    Edit {
        service: &'a mut EditServices,
        events: &'a mut edit::ProviderEvents,
    },
}
enum Invocation {
    Projection(ProjectionInvocation),
    Edit(EditInvocation),
}
impl Task<'_> {
    fn invocation(&self, run: Option<&str>) -> Result<Invocation, ApiError> {
        match self {
            Self::Projection(s) => s.invocation(run).map(Invocation::Projection),
            Self::Edit { service, .. } => service.invocation(run).map(Invocation::Edit),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        drafts: &mut AnnotationDrafts,
        host: &mut DurableHost,
        store: &Store,
        i: &Invocation,
        raw: &[u8],
        now: u64,
        admit: &dyn Fn() -> Result<(), ApiError>,
    ) -> Result<Vec<u8>, ApiError> {
        match (self, i) {
            (Self::Projection(s), Invocation::Projection(i)) => {
                s.call(drafts, host, store, i, raw, now, &|| admit())
            }
            (Self::Edit { service, events }, Invocation::Edit(i)) => {
                service.call(drafts, events, host, store, i, raw, now, admit)
            }
            _ => Err(api("task purpose mismatch")),
        }
    }
    fn cancel(&mut self, drafts: &AnnotationDrafts, now: u64) -> Result<(), ApiError> {
        match self {
            Self::Projection(s) => s.cancel(drafts, now),
            Self::Edit { .. } => Err(api("edit cannot cancel projection")),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        drafts: &AnnotationDrafts,
        host: &mut DurableHost,
        store: &Store,
        revision: u64,
        first: u32,
        max: u8,
        now: u64,
    ) -> Result<ProjectionContext, ApiError> {
        match self {
            Self::Projection(s) => s.start(drafts, host, store, revision, first, max, now),
            Self::Edit { .. } => Err(api("edit cannot start projection")),
        }
    }
    fn validate_result(
        &mut self,
        drafts: &mut AnnotationDrafts,
        host: &mut DurableHost,
        store: &Store,
        i: &Invocation,
        now: u64,
    ) -> Result<(), ApiError> {
        match (self, i) {
            (Self::Edit { service, events }, Invocation::Edit(i)) => service
                .validate_result(drafts, events, host, store, i, now)
                .map(|_| ()),
            (Self::Projection(_), Invocation::Projection(_)) => Ok(()),
            _ => Err(api("result purpose mismatch")),
        }
    }
}
#[allow(clippy::too_many_arguments)] // Concrete authority/run/query values stay explicit at the integration boundary.
fn run_vm(
    controller: &Controller,
    store: &mut Store,
    host: &mut DurableHost,
    mount: &MountHandle,
    svc: &mut Task<'_>,
    drafts: &mut AnnotationDrafts,
    package: &VerifiedPackage,
    context: &mut ProjectionContext,
    stats: &mut Stats,
    replace_before_result: bool,
) -> Result<Option<String>, String> {
    let run = format!("{BOOT}-{}", stats.vms + 1);
    let mut managed =
        ManagedRun::prepare_component(host, store, mount, now(), &run).map_err(fail)?;
    let receipt = store
        .current(&package.manifest().package_id)
        .map_err(fail)?;
    store
        .prepare_worker_lease(
            &receipt,
            BOOT,
            &run,
            &runtime_process::current().map_err(fail)?,
        )
        .map_err(fail)?;
    store.mark_worker_spawn_attempted(&run).map_err(fail)?;
    let limits = ExecutionLimits::default();
    assert_eq!(limits.deadline, Duration::from_millis(250));
    assert_eq!(limits.total_fuel, 1_000_000);
    assert_eq!(limits.host_calls, 8);
    let worker = controller
        .start(WorkerLaunch {
            run_id: run.clone(),
            startup_timeout: Duration::from_secs(2),
            execution_timeout: limits.deadline,
        })
        .map_err(fail)?;
    let invocation = svc.invocation(Some(&run)).map_err(fail)?;
    let frozen_context = context.clone();
    let frozen_query = context.query_id.clone();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_reply = Vec::new();
    let mut calls = 0;
    let mut accepted = false;
    let mut retry = None;
    let mut fatal = None;
    let mut replaced = false;
    loop {
        if Instant::now() >= deadline {
            let _ = controller.terminate(&worker);
            return Err(format!(
                "supervisor wait exhausted; lease remains unresolved: {:?}",
                controller.status(&worker)
            ));
        }
        for event in controller.poll() {
            match event {
                ProcessEvent::Spawned { handle, .. } | ProcessEvent::Ready { handle } => {
                    if handle.run_id() != run {
                        return Err("foreign worker event".into());
                    }
                }
                ProcessEvent::Guarded { handle, identity } => {
                    if let Err(e) = store.bind_worker_lease(handle.run_id(), identity) {
                        fatal = Some(format!("lease bind:{e}"));
                        let _ = managed.quarantine();
                        let _ = controller.terminate(&handle);
                        continue;
                    }
                    if let Err(e) = managed.release_component_boot(host, store, now(), || {
                        controller
                            .release_boot(
                                &handle,
                                FrozenExecution {
                                    module: package.module_bytes().to_vec(),
                                    limits: limits.clone(),
                                    request_limit: 65536,
                                },
                            )
                            .map_err(api)
                    }) {
                        fatal = Some(format!("boot:{e}"));
                        let _ = managed.quarantine();
                        let _ = controller.terminate(&handle);
                    }
                }
                ProcessEvent::Call {
                    handle,
                    call_id,
                    bytes,
                } => {
                    calls += 1;
                    if retry.is_some() || fatal.is_some() || !live(controller, &handle, call_id) {
                        if retry.is_none() && fatal.is_none() {
                            retry = Some(format!(
                                "queued_call_not_live:{:?}",
                                controller.status(&handle)
                            ));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                        continue;
                    }
                    let admission = || {
                        if live(controller, &handle, call_id) {
                            Ok(())
                        } else {
                            Err(ApiError::new(
                                ErrorCode::StaleContext,
                                "projection invocation not live",
                            ))
                        }
                    };
                    if stats.direct_writes_rejected == 0 {
                        let request = DraftRequest {
                            api_version: HOST_API_V0_3,
                            draft_protocol: 1,
                            request_id: "forbidden-write".into(),
                            resource: context.resource.clone(),
                            mount_id: context.mount_id.clone(),
                            operation: DraftOperation::BeginReplace {
                                expected_revision: context.base_revision,
                                total_bytes: 0,
                            },
                        };
                        match svc.call(
                            drafts,
                            host,
                            store,
                            &invocation,
                            request.to_json().map_err(fail)?.as_bytes(),
                            now(),
                            &admission,
                        ) {
                            Err(e) if e.code == ErrorCode::PermissionDenied => {
                                stats.direct_writes_rejected += 1
                            }
                            Err(e)
                                if e.code == ErrorCode::StaleContext
                                    && !live(controller, &handle, call_id) =>
                            {
                                retry = Some(format!("negative_probe_admission_expired:{e:?}"));
                                let _ = managed.quarantine();
                                let _ = controller.terminate(&handle);
                                continue;
                            }
                            other => {
                                fatal = Some(format!(
                                    "readonly write probe did not yield PermissionDenied:{other:?}"
                                ));
                                let _ = managed.quarantine();
                                let _ = controller.terminate(&handle);
                                continue;
                            }
                        }
                    }
                    let started = Instant::now();
                    match svc.call(drafts, host, store, &invocation, &bytes, now(), &admission) {
                        Ok(reply) => {
                            stats.max_broker_us =
                                stats.max_broker_us.max(started.elapsed().as_micros());
                            inspect(&reply, stats);
                            if reply.len() > 4096 {
                                fatal = Some("oversized reply".into());
                                let _ = managed.quarantine();
                                let _ = controller.terminate(&handle);
                                continue;
                            }
                            match controller.reply(&handle, call_id, reply.clone()) {
                                Ok(()) => last_reply = reply,
                                Err(e) if e.code == "stale_process_reply" => {
                                    retry = Some(format!(
                                        "reply_unknown:{e:?}:{:?}",
                                        controller.status(&handle)
                                    ));
                                    let _ = managed.quarantine();
                                    let _ = controller.terminate(&handle);
                                }
                                Err(e) => {
                                    fatal = Some(format!("reply transport:{e:?}"));
                                    let _ = managed.quarantine();
                                    let _ = controller.terminate(&handle);
                                }
                            }
                        }
                        Err(e)
                            if e.code == ErrorCode::StaleContext
                                && !live(controller, &handle, call_id) =>
                        {
                            retry = Some(format!(
                                "admission_not_live:{e:?}:{:?}",
                                controller.status(&handle)
                            ));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                        Err(e) => {
                            fatal = Some(format!("broker authority/protocol:{e:?}"));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::ResultReceived { handle, outcome } => {
                    if retry.is_some() || fatal.is_some() {
                        continue;
                    }
                    if replace_before_result
                        && !replaced
                        && matches!(&outcome, WorkerOutcome::Completed(_))
                    {
                        let old_request = metadata_request(context).to_json().map_err(fail)?;
                        // Cancel only application projection identity while this old
                        // lease remains. Native start(None) is forbidden until reap.
                        svc.cancel(drafts, now()).map_err(fail)?;
                        if svc
                            .call(
                                drafts,
                                host,
                                store,
                                &invocation,
                                old_request.as_bytes(),
                                now(),
                                &|| panic!("old invocation reached admission"),
                            )
                            .is_ok()
                        {
                            fatal = Some("old invocation admitted".into());
                        }
                        stats.old_invocations_rejected += 1;
                        replaced = true;
                    }
                    // Both gates run consecutively under this single coordinator;
                    // no callback or application mutation occurs between them.
                    let task_valid = svc.validate_result(drafts, host, store, &invocation, now());
                    let result = managed.accept_component_result(host, store, now(), || {
                        if context.query_id != frozen_query || svc.invocation(Some(&run)).is_err() {
                            return Err(ApiError::new(
                                ErrorCode::StaleContext,
                                "query replaced before result",
                            ));
                        }
                        task_valid?;
                        match &outcome {
                            WorkerOutcome::Completed(r)
                                if r.calls == calls && r.output.as_bytes() == last_reply =>
                            {
                                Ok(outcome)
                            }
                            WorkerOutcome::Failed(_) => Ok(outcome),
                            _ => Err(ApiError::new(
                                ErrorCode::InvalidState,
                                "result differs from authenticated reply",
                            )),
                        }
                    });
                    match result {
                        Ok(WorkerOutcome::Completed(r)) => {
                            accepted = true;
                            stats.max_fuel = stats.max_fuel.max(r.fuel_consumed);
                            stats.max_calls = stats.max_calls.max(r.calls);
                        }
                        Ok(WorkerOutcome::Failed(e))
                            if e.kind == RuntimeErrorKind::DeadlineExceeded =>
                        {
                            retry = Some(format!("runtime_deadline:{}", e.code));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                        Ok(WorkerOutcome::Failed(e)) => {
                            fatal = Some(format!("nonretryable_runtime:{:?}:{}", e.kind, e.code));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                        Err(e) if replaced && e.code == ErrorCode::StaleContext => {
                            stats.late_results_rejected += 1;
                            accepted = true;
                        }
                        Err(e) => {
                            fatal = Some(format!("result authority/payload:{e:?}"));
                            let _ = managed.quarantine();
                            let _ = controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::Reaped { handle, .. } => {
                    managed.record_reaped().map_err(fail)?;
                    managed
                        .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                        .map_err(fail)?;
                    controller.forget(&handle).map_err(fail)?;
                    stats.vms += 1;
                    stats.calls += calls;
                    stats.reaped += 1;
                    if calls > 8 {
                        return Err("guest call budget exceeded".into());
                    }
                    if let Some(reason) = fatal {
                        return Err(format!("fatal after owned reap:{reason}"));
                    }
                    if !accepted && retry.is_none() {
                        return Err(
                            "reaped without authenticated result or classified interruption".into(),
                        );
                    }
                    if replaced {
                        // Only after actual reap, finish lease and forget may a
                        // native query replacement use Current(None).
                        *context = svc
                            .start(
                                drafts,
                                host,
                                store,
                                frozen_context.base_revision,
                                frozen_context.first_paragraph,
                                frozen_context.max_rows,
                                now(),
                            )
                            .map_err(fail)?;
                        let old_request =
                            metadata_request(&frozen_context).to_json().map_err(fail)?;
                        if svc
                            .call(
                                drafts,
                                host,
                                store,
                                &invocation,
                                old_request.as_bytes(),
                                now(),
                                &|| panic!("retired invocation reached admission"),
                            )
                            .is_ok()
                        {
                            return Err("retired invocation admitted after replacement".into());
                        }
                        stats.old_invocations_rejected += 1;
                        let new_invocation = svc.invocation(None).map_err(fail)?;
                        if svc
                            .call(
                                drafts,
                                host,
                                store,
                                &new_invocation,
                                old_request.as_bytes(),
                                now(),
                                &|| Ok(()),
                            )
                            .is_ok()
                        {
                            return Err("old query id admitted after replacement".into());
                        }
                        stats.old_query_requests_rejected += 1;
                    }
                    return Ok(retry);
                }
                ProcessEvent::Failed { handle, error } => {
                    if error.kind == ProcessErrorKind::Timeout {
                        if retry.is_none() {
                            retry = Some(format!(
                                "controller_timeout:{error:?}:{:?}",
                                controller.status(&handle)
                            ));
                        }
                    } else {
                        fatal = Some(format!("nonretryable_process:{error:?}"));
                    }
                    if controller.status(&handle).map_err(fail)?.state == WorkerState::NotSpawned {
                        managed.record_not_spawned().map_err(fail)?;
                        managed
                            .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                            .map_err(fail)?;
                        controller.forget(&handle).map_err(fail)?;
                        return Err("worker did not spawn".into());
                    }
                    let _ = managed.quarantine();
                    let _ = controller.terminate(&handle);
                }
                ProcessEvent::Quarantined { handle, error } => {
                    return Err(format!(
                        "unconfirmed worker; lease retained:{:?}:{error:?}",
                        controller.status(&handle)
                    ));
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}
#[allow(clippy::too_many_arguments)] // Preserve actual failure observations while application values still exist.
fn preserve_failure(
    scratch: &std::path::Path,
    reason: &str,
    case: &serde_json::Value,
    svc: &mut ProjectionServices,
    drafts: &mut AnnotationDrafts,
    host: &mut DurableHost,
    store: &Store,
    context: &ProjectionContext,
    edits: &mut edit::ProviderEvents,
    edit_mount: &edit::EventMount,
    cursor: edit::EventCursor,
    edit_now: u64,
    draft: &DraftId,
    before: &ProjectionMetadata,
    stats: &Stats,
) -> Result<PathBuf, String> {
    let leases = store.worker_leases().map_err(fail)?;
    let mut observations = serde_json::json!({"fixture_only":true,"reason":reason,"case_id":case["id"],
        "query_context":context,"checkpoint_before":before,"lease_count":leases.len(),
        "owned_workers_reaped":stats.reaped,"vms":stats.vms,"calls":stats.calls,
        "max_completed_vm_fuel":stats.max_fuel,"max_vm_calls":stats.max_calls,
        "max_broker_us":stats.max_broker_us,"recovery_records":stats.records,
        "native_input_copy":null,"current_draft_copy":null,"editing_checkpoint_copy":null,
        "projection_metadata_after":null,"projection_checkpoint_base64":null,"unavailable":[]});
    let mut unavailable = Vec::<String>::new();
    if leases.is_empty() {
        // A copied literal is not an observation. Read the actual application
        // input and draft only after every owned invocation has been reaped.
        let mut input = String::new();
        loop {
            match edits.read_input_chunk(drafts, edit_mount, cursor, input.len(), 512, edit_now) {
                Ok(c) => {
                    input.push_str(&c.text);
                    if input.len() == c.total_bytes {
                        observations["native_input_copy"] =
                            serde_json::json!({"text":input,"total_bytes":c.total_bytes});
                        break;
                    }
                    if c.text.is_empty() {
                        unavailable.push("input read made no progress".into());
                        break;
                    }
                }
                Err(e) => {
                    unavailable.push(format!("native input unavailable:{e}"));
                    break;
                }
            }
        }
        match drafts.read_chunk(draft, None, 0, 512) {
            Ok(first) => {
                let revision = first.revision;
                let total = first.total_bytes;
                let mut body = first.text;
                while body.len() < total {
                    match drafts.read_chunk(draft, Some(revision), body.len(), 512) {
                        Ok(c) if !c.text.is_empty() => body.push_str(&c.text),
                        other => {
                            unavailable
                                .push(format!("current draft remainder unavailable:{other:?}"));
                            break;
                        }
                    }
                }
                if body.len() == total {
                    observations["current_draft_copy"] =
                        serde_json::json!({"revision":revision,"total_bytes":total,"text":body});
                }
            }
            Err(e) => unavailable.push(format!("current draft unavailable:{e}")),
        }
        match edits.snapshot(drafts, edit_mount, cursor, edit_now) {
            Ok(m) => {
                let mut bytes = Vec::new();
                while bytes.len() < m.checkpoint_bytes {
                    match edits.read_checkpoint_chunk(
                        drafts,
                        edit_mount,
                        cursor,
                        m.checkpoint_revision,
                        bytes.len(),
                        2048,
                        edit_now,
                    ) {
                        Ok(c) if !c.bytes.is_empty() => bytes.extend(c.bytes),
                        other => {
                            unavailable.push(format!("editing checkpoint unavailable:{other:?}"));
                            break;
                        }
                    }
                }
                if bytes.len() == m.checkpoint_bytes {
                    observations["editing_checkpoint_copy"] =
                        serde_json::json!({"revision":m.checkpoint_revision,"bytes":bytes});
                }
            }
            Err(e) => unavailable.push(format!("editing metadata unavailable:{e}")),
        }
        match native_metadata(svc, drafts, host, store, context) {
            Ok(m) => {
                observations["projection_metadata_after"] =
                    serde_json::to_value(&m).map_err(fail)?;
                let token = svc.invocation(None).map_err(fail)?;
                let mut offset = 0usize;
                let mut bytes = Vec::new();
                while offset < usize::from(m.checkpoint_bytes) {
                    let request = ProjectionRequest::Query {
                        api_version: HOST_API_V0_3,
                        projection_protocol: 1,
                        request_id: "native-failure-checkpoint".into(),
                        resource: context.resource.clone(),
                        mount_id: context.mount_id.clone(),
                        query_id: context.query_id.clone(),
                        base_revision: context.base_revision,
                        operation: ProjectionOperation::ReadCheckpointChunk {
                            checkpoint_revision: m.checkpoint_revision,
                            offset: offset.try_into().map_err(fail)?,
                            max_bytes: 2048,
                        },
                    };
                    let read = request.to_json().and_then(|r| {
                        svc.call(drafts, host, store, &token, r.as_bytes(), now(), &|| Ok(()))
                    });
                    match read.and_then(|b| ProjectionResponse::from_slice(&b)) {
                        Ok(ProjectionResponse::Query {
                            outcome:
                                ProjectionOutcome::CheckpointChunk {
                                    checkpoint_base64, ..
                                },
                            ..
                        }) => {
                            let part = extension_api::continuation::decode_checkpoint(
                                &checkpoint_base64,
                                2048,
                            )
                            .map_err(fail)?;
                            if part.is_empty() {
                                unavailable.push("projection checkpoint made no progress".into());
                                break;
                            }
                            offset += part.len();
                            bytes.extend(part);
                        }
                        other => {
                            unavailable
                                .push(format!("projection checkpoint unavailable:{other:?}"));
                            break;
                        }
                    }
                }
                if bytes.len() == usize::from(m.checkpoint_bytes) {
                    observations["projection_checkpoint_base64"] =
                        extension_api::continuation::encode_checkpoint(&bytes)
                            .map_err(fail)?
                            .into();
                }
            }
            Err(e) => unavailable.push(format!("fresh projection metadata unavailable:{e}")),
        }
    } else {
        unavailable.push("Owned reap unconfirmed: lease retained; no false finish or live-lease native bypass for diagnostic copying".into());
    }
    observations["unavailable"] = serde_json::to_value(unavailable).map_err(fail)?;
    let path = scratch.join("failure.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&observations).map_err(fail)?,
    )
    .map_err(fail)?;
    Ok(path)
}
fn seed(drafts: &mut AnnotationDrafts, id: &DraftId, text: &str) -> Result<u64, String> {
    let owner = drafts.create_owner().map_err(fail)?;
    let upload = drafts
        .begin_replace(&owner, id, 1, text.len(), now())
        .map_err(fail)?;
    let mut offset = 0;
    while offset < text.len() {
        let mut end = (offset + 512).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        drafts
            .append_chunk(&owner, &upload, offset, &text[offset..end], now())
            .map_err(fail)?;
        offset = end;
    }
    let revision = drafts
        .commit_replace(&owner, &upload, now())
        .map_err(fail)?;
    drafts.revoke_owner(&owner).map_err(fail)?;
    Ok(revision)
}
fn edit_snapshot(
    service: &EditServices,
    drafts: &mut AnnotationDrafts,
    events: &mut edit::ProviderEvents,
    host: &mut DurableHost,
    store: &Store,
) -> Result<edit::EventSnapshot, String> {
    let i = service.invocation(None).map_err(fail)?;
    service
        .validate_result(drafts, events, host, store, &i, now())
        .map_err(fail)
}
fn pending_fingerprint(
    service: &mut EditServices,
    drafts: &mut AnnotationDrafts,
    events: &mut edit::ProviderEvents,
    host: &mut DurableHost,
    store: &Store,
    mount: &MountHandle,
) -> Result<serde_json::Value, String> {
    let s = edit_snapshot(service, drafts, events, host, store)?;
    let i = service.invocation(None).map_err(fail)?;
    let r = ContinuationRequest {
        api_version: HOST_API_V0_3,
        continuation_protocol: 1,
        request_id: "checkpoint".into(),
        resource: mount.resource().clone(),
        mount_id: mount.mount_id().into(),
        sequence: s.cursor.sequence,
        base_revision: s.cursor.base_revision,
        operation: ContinuationOperation::ReadCheckpointChunk {
            checkpoint_revision: s.checkpoint_revision,
            offset: 0,
            max_bytes: 2048,
        },
    };
    let bytes = service
        .call(
            drafts,
            events,
            host,
            store,
            &i,
            r.to_json().map_err(fail)?.as_bytes(),
            now(),
            &|| Ok(()),
        )
        .map_err(fail)?;
    let r = ContinuationResponse::from_slice(&bytes).map_err(fail)?;
    let ContinuationOutcome::CheckpointChunk {
        checkpoint_base64, ..
    } = r.outcome
    else {
        return Err("checkpoint response".into());
    };
    let r = extension_api::composition::ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "input".into(),
        operation: extension_api::composition::ContextOperation::ReadInputChunk {
            sequence: s.cursor.sequence,
            expected_revision: s.cursor.base_revision,
            offset: 0,
            max_bytes: 512,
        },
    };
    let bytes = service
        .call(
            drafts,
            events,
            host,
            store,
            &i,
            r.to_json().map_err(fail)?.as_bytes(),
            now(),
            &|| Ok(()),
        )
        .map_err(fail)?;
    let input =
        extension_api::composition::ComponentContextResponse::from_slice(&bytes).map_err(fail)?;
    Ok(
        serde_json::json!({"sequence":s.cursor.sequence,"base_revision":s.cursor.base_revision,"checkpoint_revision":s.checkpoint_revision,"checkpoint":checkpoint_base64,"state":format!("{:?}",s.state),"last_effect":format!("{:?}",s.last_effect),"input":input}),
    )
}
#[allow(clippy::too_many_arguments)]
fn project_complete(
    controller: &Controller,
    store: &mut Store,
    host: &mut DurableHost,
    mount: &MountHandle,
    svc: &mut ProjectionServices,
    drafts: &mut AnnotationDrafts,
    package: &VerifiedPackage,
    context: &mut ProjectionContext,
    stats: &mut Stats,
    golden: &[(u32, u32)],
) -> Result<(), String> {
    let mut retries = 0;
    for _ in 0..256 {
        if let Some(cause) = run_vm(
            controller,
            store,
            host,
            mount,
            &mut Task::Projection(svc),
            drafts,
            package,
            context,
            stats,
            false,
        )? {
            retries += 1;
            if retries > 3 {
                return Err(format!("coexist projection recovery exhausted:{cause}"));
            }
        }
        let m = native_metadata(svc, drafts, host, store, context)?;
        if m.state == ProjectionState::Completed {
            let expected = ProjectionWindow {
                rows: golden
                    .iter()
                    .enumerate()
                    .map(|(i, (start, end))| RowSpan {
                        ordinal: i as u32,
                        start: *start,
                        end: *end,
                        complete: true,
                    })
                    .collect(),
                reached_eof: true,
            };
            if m.window != Some(expected) {
                return Err("coexist projection golden mismatch".into());
            }
            return Ok(());
        }
    }
    Err("coexist projection VM bound exhausted".into())
}
#[allow(clippy::too_many_arguments)]
fn coexistence(
    controller: &Controller,
    store: &mut Store,
    host: &mut DurableHost,
    core: &KnowledgeApplication<SqliteStore>,
    drafts: &mut AnnotationDrafts,
    events: &mut edit::ProviderEvents,
    package: &VerifiedPackage,
    selection: core_api::pdf_contract::PdfDraft,
    stats: &mut Stats,
) -> Result<serde_json::Value, String> {
    let draft = drafts
        .create_pdf(
            core,
            1,
            "vault",
            selection,
            core_api::knowledge::Provenance {
                package_id: "ordinary.synthetic-source".into(),
                publisher_id: "ordinary.author".into(),
                package_digest: "a".repeat(64),
                installation_id: 1,
            },
            1,
        )
        .map_err(fail)?;
    let initial = "a\r\nb\nc\r\n";
    let mut revision = seed(drafts, &draft, initial)?;
    let mounted = host
        .mount_component(
            store,
            UserMountIntent::new(
                store
                    .component_installation(
                        &package.manifest().package_id,
                        AuthorityRead::Activation,
                    )
                    .map_err(fail)?,
                Resource::Draft {
                    vault_id: "vault".into(),
                    draft_id: "e".repeat(32),
                },
            )
            .map_err(fail)?,
            now(),
        )
        .map_err(fail)?;
    let mount = mounted.handle();
    let mut projections =
        ProjectionServices::new(drafts, host, store, mount, draft.clone(), now()).map_err(fail)?;
    let mut context = projections
        .start(drafts, host, store, revision, 0, 8, now())
        .map_err(fail)?;
    let mut service = EditServices::new(drafts, events, host, store, mount, draft.clone(), now())
        .map_err(fail)?;
    let phases = [
        (
            edit::EventAction::InputChanged,
            initial,
            initial,
            vec![(0, 1), (3, 4), (5, 6)],
        ),
        (
            edit::EventAction::ReplaceParagraph { index: 1 },
            "中\n🙂",
            "a\r\n中\n🙂\nc\r\n",
            vec![(0, 1), (3, 6), (7, 11), (12, 13)],
        ),
        (
            edit::EventAction::MoveParagraph { from: 0, to: 3 },
            "",
            "中\r\n🙂\nc\na\r\n",
            vec![(0, 3), (5, 9), (10, 11), (12, 13)],
        ),
        (
            edit::EventAction::MoveParagraph { from: 3, to: 0 },
            "",
            "a\r\n中\n🙂\nc\r\n",
            vec![(0, 1), (3, 6), (7, 11), (12, 13)],
        ),
    ];
    let mut pending_checks = 0;
    let mut stale_revision_checks = 0;
    let mut records = Vec::new();
    for (phase, (action, input, expected, golden)) in phases.into_iter().enumerate() {
        let old_invocation = projections.invocation(None).map_err(fail)?;
        let old_context = context.clone();
        let old_revision = revision;
        service
            .publish(
                drafts,
                events,
                host,
                store,
                revision,
                action,
                input.into(),
                now(),
            )
            .map_err(fail)?;
        let mut began = false;
        let mut appended = false;
        let mut retries = 0;
        let mut completed = false;
        for _ in 0..256 {
            if let Some(cause) = run_vm(
                controller,
                store,
                host,
                mount,
                &mut Task::Edit {
                    service: &mut service,
                    events,
                },
                drafts,
                package,
                &mut context,
                stats,
                false,
            )? {
                retries += 1;
                if retries > 3 {
                    return Err(format!("coexist edit recovery exhausted:{cause}"));
                }
            }
            let s = edit_snapshot(&service, drafts, events, host, store)?;
            let pause = phase == 0
                && ((!began && matches!(s.last_effect, Some(edit::EffectOutcome::Started)))
                    || (!appended
                        && matches!(
                            s.last_effect,
                            Some(edit::EffectOutcome::ChunkAccepted { .. })
                        )));
            if pause {
                began |= matches!(s.last_effect, Some(edit::EffectOutcome::Started));
                appended |= matches!(
                    s.last_effect,
                    Some(edit::EffectOutcome::ChunkAccepted { .. })
                );
                let before = pending_fingerprint(&mut service, drafts, events, host, store, mount)?;
                let source_before = drafts.read_chunk(&draft, None, 0, 512).map_err(fail)?;
                context = projections
                    .start(drafts, host, store, revision, 0, 8, now())
                    .map_err(fail)?;
                project_complete(
                    controller,
                    store,
                    host,
                    mount,
                    &mut projections,
                    drafts,
                    package,
                    &mut context,
                    stats,
                    &[(0, 1), (3, 4), (5, 6)],
                )?;
                let after = pending_fingerprint(&mut service, drafts, events, host, store, mount)?;
                let source_after = drafts.read_chunk(&draft, None, 0, 512).map_err(fail)?;
                if before != after
                    || source_before.text != source_after.text
                    || source_before.revision != source_after.revision
                {
                    return Err("projection changed live pending edit or source".into());
                }
                if !store.worker_leases().map_err(fail)?.is_empty() {
                    return Err("pending pause retained worker lease".into());
                }
                pending_checks += 1;
            }
            if s.state == edit::EventState::Completed {
                completed = true;
                break;
            }
        }
        if !completed {
            return Err("coexist edit VM bound exhausted".into());
        }
        if phase == 0 && (!began || !appended) {
            return Err("real Begin/Append pause not observed".into());
        }
        let actual = drafts.read_chunk(&draft, None, 0, 512).map_err(fail)?;
        if actual.text != expected || actual.revision != old_revision + 1 {
            return Err(format!(
                "coexist phase{phase} text/revision mismatch:{:?}",
                actual.text
            ));
        }
        revision = actual.revision;
        // Query identity is deliberately not replaced before this check: only the
        // actual application's changed Draft revision can make the old read stale.
        let request = metadata_request(&context).to_json().map_err(fail)?;
        let current_query_token = projections.invocation(None).map_err(fail)?;
        if !matches!(projections.call(drafts,host,store,&current_query_token,request.as_bytes(),now(),&||Ok(())),Err(e) if e.code==ErrorCode::StaleContext)
        {
            return Err("old projection accepted changed Draft revision".into());
        }
        // The earlier frozen token is also rejected, but not used as the sole proof.
        if projections
            .call(
                drafts,
                host,
                store,
                &old_invocation,
                metadata_request(&old_context)
                    .to_json()
                    .map_err(fail)?
                    .as_bytes(),
                now(),
                &|| Ok(()),
            )
            .is_ok()
        {
            return Err("old projection invocation accepted".into());
        }
        stale_revision_checks += 1;
        context = projections
            .start(drafts, host, store, revision, 0, 8, now())
            .map_err(fail)?;
        project_complete(
            controller,
            store,
            host,
            mount,
            &mut projections,
            drafts,
            package,
            &mut context,
            stats,
            &golden,
        )?;
        records.push(serde_json::json!({"phase":phase,"revision":revision,"text":actual.text,"pending_checks":pending_checks}));
    }
    service.close(drafts, events).map_err(fail)?;
    projections.close().map_err(fail)?;
    host.close_component(mount).map_err(fail)?;
    for ticket in host.pending_component_cleanup() {
        host.confirm_component_released(&ticket).map_err(fail)?;
    }
    drafts.remove(&draft).map_err(fail)?;
    Ok(
        serde_json::json!({"phases":records,"real_begin_append_projection_pauses":pending_checks,"actual_revision_stale_queries":stale_revision_checks,"same_signed_module_mount_draft":true,"real_clock":true}),
    )
}
fn execute() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let evidence = root.join("evidence/component-author-b-projection");
    let path = std::env::var_os("PROJECTION_PACKAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.join("provider-b.qpk"));
    let key = match std::env::var("PROJECTION_PUBLIC_KEY") {
        Ok(key) => key,
        Err(_) => fs::read_to_string(evidence.join("publisher-public-key.txt")).map_err(fail)?,
    };
    let package = verify_for_host(
        &fs::read(path).map_err(fail)?,
        key.trim(),
        &SUPPORTED_HOST_APIS,
    )
    .map_err(fail)?;
    let (_, asset) = package
        .file("assets/ui-composition.json")
        .ok_or("signed composition asset missing")?;
    let descriptor =
        extension_api::composition::CompositionDescriptor::from_slice(asset).map_err(fail)?;
    if !descriptor.supports_paragraph_projection() {
        return Err("signed projection support missing".into());
    }
    let case_path = std::env::var_os("PROJECTION_CASES")
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.join("projection-cases.json"));
    let cases: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(case_path).map_err(fail)?).map_err(fail)?;
    let coexist_only = std::env::args().any(|v| v == "--coexistence-only");
    let cases: Vec<_> = if coexist_only {
        Vec::new()
    } else if std::env::args().any(|v| v == "--small") {
        cases
            .into_iter()
            .filter(|c| [0, 2, 3].contains(&c["id"].as_u64().unwrap()))
            .collect()
    } else {
        cases
    };
    let mut scratch = Scratch::new()?;
    let mut store =
        Store::open_for_host(&scratch.path.join("packages"), &SUPPORTED_HOST_APIS).map_err(fail)?;
    let receipt = store.install("install", 0, &package).map_err(fail)?;
    store
        .bind_component_vault(
            "bind-vault",
            receipt.revision,
            &package.manifest().package_id,
            "vault",
        )
        .map_err(fail)?;
    let mut host = DurableHost::new("vault", [Capability::NoteDraftEdit]).map_err(fail)?;
    host.configure_component_identity([27; 16]).map_err(fail)?;
    let mut core =
        KnowledgeApplication::new(SqliteStore::open(scratch.path.join("knowledge")).map_err(fail)?);
    let (original, snapshot, selection) = fixture::pdf_fixture();
    core.import_document(
        "source",
        "vault",
        "Synthetic Core snapshot",
        &original,
        &snapshot,
    )
    .map_err(fail)?;
    let mut drafts = AnnotationDrafts::new();
    let mut edits = edit::ProviderEvents::new();
    let controller = Controller::new().map_err(fail)?;
    let mut stats = Stats::default();
    let mut survived = 0;
    for (index, case) in cases.iter().enumerate() {
        let draft = drafts
            .create_pdf(
                &core,
                1,
                "vault",
                selection.clone(),
                core_api::knowledge::Provenance {
                    package_id: "ordinary.synthetic-source".into(),
                    publisher_id: "ordinary.author".into(),
                    package_digest: "a".repeat(64),
                    installation_id: 1,
                },
                1,
            )
            .map_err(fail)?;
        let text = case["text"].as_str().ok_or("fixture text missing")?;
        let revision = seed(&mut drafts, &draft, text)?;
        let edit_mount = edits.register(&mut drafts, draft.clone()).map_err(fail)?;
        let pending = "pending🙂\0";
        // Hold only the editing fixture clock fixed: this tests timer independence,
        // not a promise that an actual native timer can never expire an upload.
        let edit_now = now();
        let event = edits
            .publish(
                &mut drafts,
                &edit_mount,
                revision,
                edit::EventAction::ReplaceParagraph { index: 0 },
                pending.into(),
                edit_now,
            )
            .map_err(fail)?;
        edits
            .advance(
                &mut drafts,
                &edit_mount,
                edit::AdvanceEvent {
                    cursor: event.cursor,
                    expected_checkpoint_revision: 1,
                    effect: Some(edit::DraftEffect::BeginReplace {
                        expected_revision: revision,
                        total_bytes: pending.len(),
                    }),
                    checkpoint: vec![7, 0, 255],
                    complete: false,
                },
                edit_now,
            )
            .map_err(fail)?;
        let reserve = drafts.reserved_text_bytes();
        let mounted = host
            .mount_component(
                &store,
                UserMountIntent::new(
                    store
                        .component_installation(
                            &package.manifest().package_id,
                            AuthorityRead::Activation,
                        )
                        .map_err(fail)?,
                    Resource::Draft {
                        vault_id: "vault".into(),
                        draft_id: format!("{:032x}", index + 1000),
                    },
                )
                .map_err(fail)?,
                now(),
            )
            .map_err(fail)?;
        let mut svc = ProjectionServices::new(
            &drafts,
            &mut host,
            &store,
            mounted.handle(),
            draft.clone(),
            now(),
        )
        .map_err(fail)?;
        let first = case["first"].as_u64().ok_or("fixture first missing")? as u32;
        let max_rows = case["max_rows"].as_u64().ok_or("fixture max missing")? as u8;
        let old_context = svc
            .start(&drafts, &mut host, &store, revision, first, max_rows, now())
            .map_err(fail)?;
        let old_invocation = svc.invocation(None).map_err(fail)?;
        svc.cancel(&drafts, now()).map_err(fail)?;
        let mut context = svc
            .start(&drafts, &mut host, &store, revision, first, max_rows, now())
            .map_err(fail)?;
        if svc
            .call(
                &drafts,
                &mut host,
                &store,
                &old_invocation,
                metadata_request(&old_context)
                    .to_json()
                    .map_err(fail)?
                    .as_bytes(),
                now(),
                &|| panic!("stale invocation reached admission"),
            )
            .is_ok()
        {
            return Err("stale invocation admitted".into());
        }
        stats.old_invocations_rejected += 1;
        let new_token = svc.invocation(None).map_err(fail)?;
        if svc
            .call(
                &drafts,
                &mut host,
                &store,
                &new_token,
                metadata_request(&old_context)
                    .to_json()
                    .map_err(fail)?
                    .as_bytes(),
                now(),
                &|| Ok(()),
            )
            .is_ok()
        {
            return Err("old query request admitted".into());
        }
        stats.old_query_requests_rejected += 1;
        let mut done = None;
        let mut inject_late = index == 0;
        for _ in 0..2064 {
            let before = native_metadata(&mut svc, &drafts, &mut host, &store, &context)?;
            if before.state == ProjectionState::Completed && !inject_late {
                done = Some(before);
                break;
            }
            let vm_outcome = run_vm(
                &controller,
                &mut store,
                &mut host,
                mounted.handle(),
                &mut Task::Projection(&mut svc),
                &mut drafts,
                &package,
                &mut context,
                &mut stats,
                inject_late,
            );
            let cause = match vm_outcome {
                Ok(cause) => cause,
                Err(reason) => {
                    let diagnostic = preserve_failure(
                        &scratch.path,
                        &reason,
                        case,
                        &mut svc,
                        &mut drafts,
                        &mut host,
                        &store,
                        &context,
                        &mut edits,
                        &edit_mount,
                        event.cursor,
                        edit_now,
                        &draft,
                        &before,
                        &stats,
                    );
                    return Err(format!("{reason}; diagnostic={diagnostic:?}"));
                }
            };
            if stats.late_results_rejected > 0 {
                inject_late = false;
            }
            let after = native_metadata(&mut svc, &drafts, &mut host, &store, &context)?;
            if let Some(cause) = cause {
                stats.recoveries += 1;
                let count = stats
                    .recovery_by_checkpoint
                    .entry((context.query_id.clone(), after.checkpoint_revision))
                    .or_default();
                *count += 1;
                let record = serde_json::json!({"case":case["id"],"cause":cause,"checkpoint_before":before.checkpoint_revision,"checkpoint_after":after.checkpoint_revision,"advance_applied":before.checkpoint_revision!=after.checkpoint_revision,"state_after":format!("{:?}",after.state),"same_checkpoint_retries":count});
                eprintln!("PROJECTION_RECOVERY {record}");
                stats.records.push(record);
                if *count > 3 || stats.recoveries > 64 {
                    let reason = "bounded projection recovery exhausted";
                    let diagnostic = preserve_failure(
                        &scratch.path,
                        reason,
                        case,
                        &mut svc,
                        &mut drafts,
                        &mut host,
                        &store,
                        &context,
                        &mut edits,
                        &edit_mount,
                        event.cursor,
                        edit_now,
                        &draft,
                        &before,
                        &stats,
                    );
                    return Err(format!("{reason}; diagnostic={diagnostic:?}"));
                }
            }
            if after.state == ProjectionState::Completed && !inject_late {
                done = Some(after);
                break;
            }
        }
        let complete = done.ok_or("projection VM budget exhausted")?;
        let golden = ProjectionWindow {
            rows: serde_json::from_value(case["expected"].clone()).map_err(fail)?,
            reached_eof: case["eof"].as_bool().ok_or("fixture EOF missing")?,
        };
        if complete.window.as_ref() != Some(&golden) || !complete.window_ready {
            return Err(format!("golden projection mismatch case={}", case["id"]));
        }
        if drafts.reserved_text_bytes() != reserve {
            return Err("projection changed edit reservation".into());
        }
        if edits
            .read_input_chunk(&mut drafts, &edit_mount, event.cursor, 0, 512, edit_now)
            .map_err(fail)?
            .text
            != pending
        {
            return Err("projection changed pending input".into());
        }
        if edits
            .read_checkpoint_chunk(&mut drafts, &edit_mount, event.cursor, 2, 0, 2048, edit_now)
            .map_err(fail)?
            .bytes
            != [7, 0, 255]
        {
            return Err("projection changed editing checkpoint".into());
        }
        if edits
            .snapshot(&mut drafts, &edit_mount, event.cursor, edit_now)
            .map_err(fail)?
            .state
            != edit::EventState::Active
        {
            return Err("projection retired edit upload".into());
        }
        survived += 1;
        svc.close().map_err(fail)?;
        host.close_component(mounted.handle()).map_err(fail)?;
        for ticket in host.pending_component_cleanup() {
            host.confirm_component_released(&ticket).map_err(fail)?;
        }
        // The synthetic native input has just been copied/compared. Its explicit
        // discard closes only this editing fixture after projection is finished.
        edits.close(&mut drafts, &edit_mount).map_err(fail)?;
        drafts.remove(&draft).map_err(fail)?;
        if !store.worker_leases().map_err(fail)?.is_empty() {
            return Err("worker lease remained".into());
        }
        eprintln!("PROJECTION_OS_CASE_PASSED {} vms={}", case["id"], stats.vms);
    }
    if !coexist_only && (stats.late_results_rejected != 1 || stats.direct_writes_rejected != 1) {
        return Err("planned late-result or live readonly-write fixture was not observed".into());
    }
    let coexistence = coexistence(
        &controller,
        &mut store,
        &mut host,
        &core,
        &mut drafts,
        &mut edits,
        &package,
        selection.clone(),
        &mut stats,
    )?;
    println!(
        "{}",
        serde_json::json!({"coexistence":coexistence,"ok":true,"cases":cases.len(),"vms":stats.vms,"calls":stats.calls,"owned_workers_reaped":stats.reaped,"max_completed_vm_fuel":stats.max_fuel,"max_vm_calls":stats.max_calls,"max_broker_us":stats.max_broker_us,"partial_seen":stats.partial_seen,"runtime_interruptions":stats.recoveries,"recovery_records":stats.records,"old_invocations_rejected":stats.old_invocations_rejected,"old_query_requests_rejected":stats.old_query_requests_rejected,"late_results_rejected":stats.late_results_rejected,"direct_writes_rejected":stats.direct_writes_rejected,"edit_upload_input_checkpoint_preserved":survived,"edit_fixture_clock":"fixed_per_case_timer_independence_only","worker_leases_empty":true,"default_deadline_ms":250,"native_widgets_mounted":false,"source":"synthetic_Core_snapshot_not_parser_or_Surface_lease_evidence"})
    );
    scratch.preserve = false;
    Ok(())
}
fn main() -> ExitCode {
    match runtime_process::worker_main(&WasmiRuntime) {
        Ok(true) => return ExitCode::SUCCESS,
        Ok(false) => {}
        Err(e) => {
            eprintln!("projection worker:{e:?}");
            return ExitCode::FAILURE;
        }
    }
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("PROJECTION_OS_FAILED {e}");
            println!(
                "{}",
                serde_json::json!({"ok":false,"error":e,"fixture_path":format!("{}/qusor-projection-process-{}",std::env::temp_dir().display(),std::process::id())})
            );
            ExitCode::FAILURE
        }
    }
}
