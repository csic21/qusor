//! Real signed A → B → A on the same application draft, with fresh mounts and workers.
//! Default packages: evidence/component-author-a and evidence/component-author-b.
//! Optional COMPONENT_PACKAGE/PUBLIC_KEY, PARAGRAPH_PACKAGE/PUBLIC_KEY and
//! CAPTURE_PACKAGE/PUBLIC_KEY override paths and raw public hex keys independently.
//! Synthetic Core source facts do not claim parser, gesture or Surface-lease coverage.
//! Parent code only maps wire/app DTOs and verifies fixed expected bytes; paragraph
//! scanning and transformed output are computed exclusively by B's signed guest.
//! `--faults` runs an independent eight-case interruption/revocation report.
#[path = "component_support/fixture.rs"]
mod fixture;
#[path = "component_support/save_gate.rs"]
mod save_gate;
use extension_api::{composition::*, continuation::*, draft::*, *};
use extension_host::{component::MountHandle, durable::*};
use extension_package::package::{VerifiedPackage, verify_for_host};
use extension_runtime_api::{ExecutionLimits, RuntimeErrorKind};
use extension_store::{GrantSet, PersistentGrant, Store, StoreServices};
use learning_application::{
    KnowledgeApplication, annotation_draft::AnnotationDrafts, draft_text::DraftId,
    provider_events::*,
};
use runtime_process::{
    Controller, FrozenExecution, ProcessEvent, WorkerLaunch, WorkerOutcome, WorkerState,
};
use runtime_wasm::WasmiRuntime;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;
const BOOT: &str = "3a7f027ad2c044b6b839cfa763c0124a";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn api(error: impl std::fmt::Display) -> ApiError {
    eprintln!("COMPONENT_BROKER_ERROR {error}");
    ApiError::new(ErrorCode::InvalidState, "component application rejected")
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "qusor-component-consumer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn state(s: EventState) -> ContinuationState {
    match s {
        EventState::Active => ContinuationState::Active,
        EventState::Completed => ContinuationState::Completed,
        EventState::Cancelled => ContinuationState::Cancelled,
        EventState::Expired => ContinuationState::Expired,
    }
}
fn effect_outcome(e: EffectOutcome) -> EventEffectOutcome {
    match e {
        EffectOutcome::Started => EventEffectOutcome::Started {},
        EffectOutcome::ChunkAccepted { next_offset } => EventEffectOutcome::ChunkAccepted {
            next_offset: next_offset as u32,
        },
        EffectOutcome::Replaced { revision } => EventEffectOutcome::Replaced { revision },
        EffectOutcome::Cancelled => EventEffectOutcome::Cancelled {},
    }
}
fn metadata(s: EventSnapshot) -> ContinuationMetadata {
    ContinuationMetadata {
        checkpoint_revision: s.checkpoint_revision,
        checkpoint_bytes: s.checkpoint_bytes as u16,
        state: state(s.state),
        last_effect: s.last_effect.map(effect_outcome),
    }
}
fn effect(e: EventEffect) -> DraftEffect {
    match e {
        EventEffect::BeginReplace {
            expected_revision,
            total_bytes,
        } => DraftEffect::BeginReplace {
            expected_revision,
            total_bytes: total_bytes as usize,
        },
        EventEffect::AppendChunk { offset, text } => DraftEffect::AppendChunk {
            offset: offset as usize,
            text,
        },
        EventEffect::CommitReplace {} => DraftEffect::CommitReplace,
        EventEffect::CancelReplace {} => DraftEffect::CancelReplace,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPhase {
    Scan,
    Begin,
    Append,
}
#[derive(Clone, Copy, Debug)]
enum FaultAction {
    DropReply,
    LateReply,
    CloseMount,
    RevokeInstallation,
}
#[derive(Clone, Copy, Debug)]
struct TriggeredFault {
    label: &'static str,
    action: FaultAction,
}
#[derive(Clone, Copy, Debug)]
struct PlannedFault {
    phase: FaultPhase,
    trigger: TriggeredFault,
}
struct Broker {
    drafts: AnnotationDrafts,
    event_mount_draft: DraftId,
    events: ProviderEvents,
    event_mount: EventMount,
    cursor: EventCursor,
    // Fixture fault injection labels only; not an OS failure simulation result.
    injected: BTreeSet<&'static str>,
    inject: bool,
    late_input: Option<String>,
    invocation_cursor: EventCursor,
    planned_fault: Option<PlannedFault>,
}
impl Broker {
    fn wire(
        &mut self,
        raw: &[u8],
        ctx: &extension_host::component::AuthorizedComponentContext,
        admit: &dyn Fn() -> Result<(), ApiError>,
    ) -> Result<(Vec<u8>, Option<TriggeredFault>), ApiError> {
        admit()?;
        if let Ok(r) = ComponentContextRequest::from_slice(raw) {
            let (sequence, expected_revision, outcome) = match r.operation {
                ContextOperation::ReadContext {} => {
                    let s = self
                        .events
                        .snapshot(&mut self.drafts, &self.event_mount, self.cursor, now())
                        .map_err(api)?;
                    (
                        s.cursor.sequence,
                        s.cursor.base_revision,
                        ContextOutcome::Context {
                            action: match s.action {
                                EventAction::InputChanged => ComponentAction::InputChanged {
                                    total_bytes: s.input_bytes as u32,
                                },
                                EventAction::MoveParagraph { from, to } => {
                                    ComponentAction::MoveParagraph { from, to }
                                }
                                EventAction::ReplaceParagraph { index } => {
                                    ComponentAction::ReplaceParagraph {
                                        index,
                                        input_bytes: s.input_bytes as u32,
                                    }
                                }
                            },
                        },
                    )
                }
                ContextOperation::ReadInputChunk {
                    sequence,
                    expected_revision,
                    offset,
                    max_bytes,
                } => {
                    let cursor = EventCursor {
                        sequence,
                        base_revision: expected_revision,
                    };
                    let c = self
                        .events
                        .read_input_chunk(
                            &mut self.drafts,
                            &self.event_mount,
                            cursor,
                            offset as usize,
                            max_bytes as usize,
                            now(),
                        )
                        .map_err(api)?;
                    (
                        sequence,
                        expected_revision,
                        ContextOutcome::InputChunk {
                            offset: c.offset as u32,
                            total_bytes: c.total_bytes as u32,
                            text: c.text,
                        },
                    )
                }
            };
            return Ok((
                ComponentContextResponse {
                    api_version: HOST_API_V0_3,
                    context_protocol: COMPONENT_CONTEXT_PROTOCOL,
                    request_id: r.request_id,
                    resource: ctx.resource.clone(),
                    mount_id: ctx.mount_id.clone(),
                    sequence,
                    expected_revision,
                    outcome,
                }
                .to_json()?
                .into_bytes(),
                None,
            ));
        }
        let r = ContinuationRequest::from_slice(raw)?;
        let cursor = EventCursor {
            sequence: r.sequence,
            base_revision: r.base_revision,
        };
        let mut fault = None;
        let outcome = match r.operation {
            ContinuationOperation::ReadMetadata {} => ContinuationOutcome::Metadata {
                metadata: metadata(
                    self.events
                        .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                        .map_err(api)?,
                ),
            },
            ContinuationOperation::ReadCheckpointChunk {
                checkpoint_revision,
                offset,
                max_bytes,
            } => {
                let c = self
                    .events
                    .read_checkpoint_chunk(
                        &mut self.drafts,
                        &self.event_mount,
                        cursor,
                        checkpoint_revision,
                        offset as usize,
                        max_bytes as usize,
                        now(),
                    )
                    .map_err(api)?;
                ContinuationOutcome::CheckpointChunk {
                    checkpoint_revision: c.checkpoint_revision,
                    offset: c.offset as u16,
                    total_bytes: c.total_bytes as u16,
                    checkpoint_base64: encode_checkpoint(&c.bytes)?,
                }
            }
            ContinuationOperation::AdvanceEvent {
                expected_checkpoint_revision,
                checkpoint_base64,
                effect: wire_effect,
                complete,
            } => {
                let phase = match &wire_effect {
                    Some(EventEffect::BeginReplace { .. }) => Some(FaultPhase::Begin),
                    Some(EventEffect::AppendChunk { .. }) => Some(FaultPhase::Append),
                    None if !complete => Some(FaultPhase::Scan),
                    _ => None,
                };
                let label = match &wire_effect {
                    Some(EventEffect::BeginReplace { .. }) => Some("begin_reply_lost"),
                    Some(EventEffect::AppendChunk { .. }) => Some("append_reply_lost"),
                    Some(EventEffect::CommitReplace {}) => Some("commit_reply_lost"),
                    _ => None,
                };
                let checkpoint = decode_checkpoint(
                    &checkpoint_base64,
                    extension_api::continuation::MAX_CHECKPOINT_BYTES,
                )?;
                admit()?;
                self.events
                    .advance(
                        &mut self.drafts,
                        &self.event_mount,
                        AdvanceEvent {
                            cursor,
                            expected_checkpoint_revision,
                            checkpoint,
                            effect: wire_effect.map(effect),
                            complete,
                        },
                        now(),
                    )
                    .map_err(api)?;
                if self.inject
                    && let Some(label) = label
                    && self.injected.insert(label)
                {
                    fault = Some(TriggeredFault {
                        label,
                        action: FaultAction::DropReply,
                    });
                }
                if let Some(plan) = self.planned_fault
                    && phase == Some(plan.phase)
                {
                    self.planned_fault = None;
                    let snapshot = self
                        .events
                        .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                        .map_err(api)?;
                    assert!(snapshot.checkpoint_revision > 1 && snapshot.checkpoint_bytes > 0);
                    if plan.phase == FaultPhase::Scan {
                        // This is a pre-Begin scanning checkpoint, not an unrelated
                        // no-output step after an upload has already started.
                        assert!(snapshot.last_effect.is_none());
                        assert_eq!(
                            self.drafts
                                .read_chunk(&self.event_mount_draft, None, 0, 512)
                                .map_err(api)?
                                .revision,
                            cursor.base_revision
                        );
                    }
                    fault = Some(plan.trigger);
                }
                ContinuationOutcome::Metadata {
                    metadata: metadata(
                        self.events
                            .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                            .map_err(api)?,
                    ),
                }
            }
            ContinuationOperation::CancelEvent {
                expected_checkpoint_revision,
            } => {
                self.events
                    .cancel_event(
                        &mut self.drafts,
                        &self.event_mount,
                        cursor,
                        expected_checkpoint_revision,
                        now(),
                    )
                    .map_err(api)?;
                ContinuationOutcome::Metadata {
                    metadata: metadata(
                        self.events
                            .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                            .map_err(api)?,
                    ),
                }
            }
        };
        Ok((
            ContinuationResponse {
                api_version: HOST_API_V0_3,
                continuation_protocol: CONTINUATION_PROTOCOL,
                request_id: r.request_id,
                resource: ctx.resource.clone(),
                mount_id: ctx.mount_id.clone(),
                sequence: r.sequence,
                base_revision: r.base_revision,
                outcome,
            }
            .to_json()?
            .into_bytes(),
            fault,
        ))
    }
    fn call(
        &mut self,
        h: &mut DurableHost,
        s: &Store,
        m: &MountHandle,
        run: &str,
        raw: &[u8],
        admit: &dyn Fn() -> Result<(), ApiError>,
    ) -> Result<(Vec<u8>, Option<TriggeredFault>), ApiError> {
        if let Ok(r) = ComponentContextRequest::from_slice(raw) {
            h.dispatch_component_context(s, m, now(), Some(run), r, |ctx| {
                self.wire(raw, ctx, admit)
            })
        } else if let Ok(r) = DraftRequest::from_slice(raw) {
            let copy = r.clone();
            h.dispatch_draft(s, m, now(), Some(run), r, |ctx| {
                admit()?;
                let DraftOperation::ReadChunk {
                    revision: Some(revision),
                    offset,
                    max_bytes,
                } = copy.operation
                else {
                    return Err(ApiError::new(
                        ErrorCode::PermissionDenied,
                        "direct draft writes or unpinned reads are forbidden in provider events",
                    ));
                };
                if self.cursor != self.invocation_cursor
                    || revision != self.invocation_cursor.base_revision
                {
                    return Err(ApiError::new(
                        ErrorCode::StaleContext,
                        "source read differs from immutable invocation",
                    ));
                }
                let event = self
                    .events
                    .snapshot(
                        &mut self.drafts,
                        &self.event_mount,
                        self.invocation_cursor,
                        now(),
                    )
                    .map_err(api)?;
                if event.state != EventState::Active {
                    return Err(ApiError::new(
                        ErrorCode::StaleContext,
                        "source event no longer active",
                    ));
                }
                let c = self
                    .drafts
                    .read_chunk(
                        &self.event_mount_draft,
                        Some(revision),
                        offset as usize,
                        max_bytes as usize,
                    )
                    .map_err(api)?;
                let Resource::Draft { draft_id, .. } = &ctx.resource else {
                    unreachable!()
                };
                Ok((
                    DraftResponse {
                        api_version: HOST_API_V0_3,
                        draft_protocol: DRAFT_PROTOCOL,
                        request_id: copy.request_id,
                        draft_id: draft_id.clone(),
                        mount_id: ctx.mount_id.clone(),
                        outcome: DraftOutcome::Snapshot {
                            revision: c.revision,
                            offset: c.offset as u32,
                            total_bytes: c.total_bytes as u32,
                            text: c.text,
                        },
                    }
                    .to_json()?
                    .into_bytes(),
                    None,
                ))
            })
        } else {
            let r = ContinuationRequest::from_slice(raw)?;
            h.dispatch_continuation(s, m, now(), Some(run), r, |ctx| self.wire(raw, ctx, admit))
        }
    }
}
#[derive(Default)]
struct Stats {
    vms: u64,
    calls: usize,
    lost: usize,
    max_call_us: u128,
    late_results_rejected: usize,
    previous_mount: Option<MountHandle>,
    direct_draft_writes_rejected: usize,
    max_completed_vm_fuel: u64,
    max_vm_calls: usize,
    mount_close_callback_rejections: usize,
    store_revoke_callback_rejections: usize,
    queued_calls_refused: usize,
    automatic_recoveries: usize,
    injected_late_replies: usize,
    unplanned_watchdog_timeouts: usize,
    recovery_by_checkpoint: BTreeMap<(String, u64, u64, u64), usize>,
    recovery_records: Vec<serde_json::Value>,
    retry_exhausted: bool,
    nonretryable_failures: usize,
    runtime_deadline_failures: usize,
    definite_not_spawned: u64,
}
fn call_live(
    controller: &Controller,
    handle: &runtime_process::WorkerHandle,
    call_id: u64,
) -> bool {
    controller
        .status(handle)
        .is_ok_and(|s| s.state == WorkerState::Running && s.pending_call == Some(call_id))
}
fn interrupted_reply_retryable(
    controller: &Controller,
    handle: &runtime_process::WorkerHandle,
) -> bool {
    controller.status(handle).is_ok_and(|s| {
        s.state != WorkerState::Running
            && s.error
                .is_none_or(|e| e.kind == runtime_process::ProcessErrorKind::Timeout)
    })
}
fn checkpoint_record(broker: &mut Broker) -> Option<EventSnapshot> {
    broker
        .events
        .snapshot(
            &mut broker.drafts,
            &broker.event_mount,
            broker.cursor,
            now(),
        )
        .ok()
}
#[allow(clippy::too_many_arguments)] // Record one exact run failure and its atomic checkpoint evidence.
fn recover_run(
    stats: &mut Stats,
    broker: &mut Broker,
    mount: &MountHandle,
    handle: &runtime_process::WorkerHandle,
    before_status: String,
    controller: &Controller,
    cause: &str,
    before: Option<EventSnapshot>,
    planned: bool,
) {
    let after = checkpoint_record(broker);
    let cp = after.or(before).map_or(0, |s| s.checkpoint_revision);
    let key = (
        mount.mount_id().to_owned(),
        broker.cursor.sequence,
        broker.cursor.base_revision,
        cp,
    );
    let count = stats.recovery_by_checkpoint.entry(key).or_default();
    *count += 1;
    stats.automatic_recoveries += 1;
    if planned {
        stats.injected_late_replies += 1;
    }
    if !planned
        && controller.status(handle).is_ok_and(|s| {
            s.error
                .is_some_and(|e| e.kind == runtime_process::ProcessErrorKind::Timeout)
        })
    {
        stats.unplanned_watchdog_timeouts += 1;
    }
    let summary = |snapshot: Option<EventSnapshot>| {
        snapshot.map(|s|serde_json::json!({"checkpoint_revision":s.checkpoint_revision,"checkpoint_bytes":s.checkpoint_bytes,"state":format!("{:?}",s.state),"last_effect":format!("{:?}",s.last_effect)}))
    };
    let record = serde_json::json!({"run":handle.run_id(),"cause":cause,"planned_fixture":planned,"status_before":before_status,"status_after":format!("{:?}",controller.status(handle)),"checkpoint_before":summary(before),"checkpoint_after":summary(after),"checkpoint_retries":*count});
    eprintln!("RECOVERABLE_RUN_INTERRUPTION {record}");
    stats.recovery_records.push(record);
    stats.retry_exhausted = *count > 3 || stats.automatic_recoveries > 64;
}
fn timeout_status(controller: &Controller, handle: &runtime_process::WorkerHandle) -> bool {
    controller.status(handle).is_ok_and(|s| {
        s.error
            .is_some_and(|e| e.kind == runtime_process::ProcessErrorKind::Timeout)
    })
}
fn fatal_run(
    stats: &mut Stats,
    broker: &mut Broker,
    controller: &Controller,
    handle: &runtime_process::WorkerHandle,
    cause: &str,
) {
    stats.nonretryable_failures += 1;
    let cp = checkpoint_record(broker);
    let record = serde_json::json!({"run":handle.run_id(),"cause":cause,"recoverable":false,"status":format!("{:?}",controller.status(handle)),"checkpoint":format!("{cp:?}")});
    eprintln!("NONRETRYABLE_RUN_FAILURE {record}");
    stats.recovery_records.push(record);
}
fn fail_preserving_pending(
    broker: &mut Broker,
    stats: &Stats,
    reason: &str,
    termination: &str,
) -> ! {
    // This explicit qualification-failure copy is not production draft recovery.
    // Unconfirmed children retain their durable lease; no termination is fabricated.
    let mut input = String::new();
    loop {
        let c = broker.events.read_input_chunk(
            &mut broker.drafts,
            &broker.event_mount,
            broker.cursor,
            input.len(),
            512,
            now(),
        );
        let Ok(c) = c else {
            break;
        };
        input.push_str(&c.text);
        if input.len() == c.total_bytes {
            break;
        }
    }
    let path = std::env::temp_dir().join(format!(
        "qusor-paragraph-pending-failure-{}.json",
        std::process::id()
    ));
    let copy = serde_json::json!({"fixture_only":true,"reason":reason,"termination":termination,"input":input,"current_text":read_text(&broker.drafts,&broker.event_mount_draft),"recoveries":stats.recovery_records});
    fs::write(&path, serde_json::to_vec(&copy).unwrap())
        .expect("write explicit native pending failure copy");
    println!(
        "{}",
        serde_json::json!({"ok":false,"reason":reason,"termination":termination,"pending_fixture_copy":path,"automatic_recoveries":stats.automatic_recoveries})
    );
    std::process::exit(1)
}
#[allow(clippy::too_many_arguments)] // Keep actual authority, OS controller and application objects explicit.
fn run_vm(
    controller: &Controller,
    store: &mut Store,
    host: &mut DurableHost,
    mount: &MountHandle,
    broker: &mut Broker,
    module: &[u8],
    package: &str,
    sequence: u64,
    stats: &mut Stats,
    expected_rejection: Option<&str>,
) -> bool {
    let run = format!("{BOOT}-{sequence}");
    let mut managed = ManagedRun::prepare_component(host, store, mount, now(), &run).unwrap();
    let receipt = store.current(package).unwrap();
    store
        .prepare_worker_lease(&receipt, BOOT, &run, &runtime_process::current().unwrap())
        .unwrap();
    store.mark_worker_spawn_attempted(&run).unwrap();
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
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut injected = false;
    let mut accepted = false;
    let mut last_reply = Vec::new();
    let mut count = 0;
    let run_cursor = broker.cursor;
    broker.invocation_cursor = run_cursor;
    let mut guest_rejected = false;
    let mut event_rejected = false;
    let mut recoverable = false;
    let mut fatal_reason: Option<String> = None;
    loop {
        if Instant::now() >= deadline {
            let _ = controller.terminate(&worker);
            fail_preserving_pending(
                broker,
                stats,
                "supervisor_wait_exhausted",
                "unconfirmed_lease_retained",
            );
        }
        for event in controller.poll() {
            match event {
                ProcessEvent::Spawned { handle, .. } | ProcessEvent::Ready { handle } => {
                    assert_eq!(handle.run_id(), run)
                }
                ProcessEvent::Guarded { handle, identity } => {
                    store.bind_worker_lease(handle.run_id(), identity).unwrap();
                    managed
                        .release_component_boot(host, store, now(), || {
                            controller
                                .release_boot(
                                    &handle,
                                    FrozenExecution {
                                        module: module.to_vec(),
                                        limits: limits.clone(),
                                        request_limit: 65536,
                                    },
                                )
                                .map_err(api)
                        })
                        .unwrap();
                }
                ProcessEvent::Call {
                    handle,
                    call_id,
                    bytes,
                } => {
                    count += 1;
                    if recoverable
                        || injected
                        || fatal_reason.is_some()
                        || !call_live(controller, &handle, call_id)
                    {
                        stats.queued_calls_refused += 1;
                        if !recoverable && !injected && fatal_reason.is_none() {
                            if timeout_status(controller, &handle)
                                || interrupted_reply_retryable(controller, &handle)
                            {
                                let cp = checkpoint_record(broker);
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    format!("{:?}", controller.status(&handle)),
                                    controller,
                                    "queued_call_no_longer_live",
                                    cp,
                                    false,
                                );
                                recoverable = true;
                            } else {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    "queued_call_not_live_without_timeout_proof",
                                );
                                fatal_reason =
                                    Some("queued_call_not_live_without_timeout_proof".into());
                            }
                            managed.quarantine().unwrap();
                            let _ = controller.terminate(&handle);
                        }
                        continue;
                    }
                    if stats.direct_draft_writes_rejected == 0 {
                        let forbidden = DraftRequest {
                            api_version: HOST_API_V0_3,
                            draft_protocol: DRAFT_PROTOCOL,
                            request_id: "forbidden-direct-write".into(),
                            resource: mount.resource().clone(),
                            mount_id: mount.mount_id().into(),
                            operation: DraftOperation::BeginReplace {
                                expected_revision: run_cursor.base_revision,
                                total_bytes: 0,
                            },
                        };
                        assert!(
                            broker
                                .call(
                                    host,
                                    store,
                                    mount,
                                    handle.run_id(),
                                    forbidden.to_json().unwrap().as_bytes(),
                                    &|| if call_live(controller, &handle, call_id) {
                                        Ok(())
                                    } else {
                                        Err(ApiError::new(
                                            ErrorCode::StaleContext,
                                            "invocation no longer live",
                                        ))
                                    }
                                )
                                .is_err()
                        );
                        assert_eq!(
                            broker
                                .drafts
                                .read_chunk(&broker.event_mount_draft, None, 0, 512)
                                .unwrap()
                                .revision,
                            run_cursor.base_revision
                        );
                        stats.direct_draft_writes_rejected += 1;
                    }
                    let began = Instant::now();
                    let before_status = format!("{:?}", controller.status(&handle));
                    let cp_before = checkpoint_record(broker);
                    let admission = || {
                        if call_live(controller, &handle, call_id) {
                            Ok(())
                        } else {
                            Err(ApiError::new(
                                ErrorCode::StaleContext,
                                "invocation no longer live",
                            ))
                        }
                    };
                    let response =
                        broker.call(host, store, mount, handle.run_id(), &bytes, &admission);
                    let (reply, fault) = match response {
                        Ok(value) => value,
                        Err(error) => {
                            if error.code == ErrorCode::StaleContext
                                && error.message == "invocation no longer live"
                                && interrupted_reply_retryable(controller, &handle)
                            {
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    before_status,
                                    controller,
                                    &format!("admission_not_live:{error:?}"),
                                    cp_before,
                                    false,
                                );
                                recoverable = true;
                            } else {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    &format!("broker_policy_or_protocol:{error:?}"),
                                );
                                fatal_reason = Some(format!("broker_policy_or_protocol:{error:?}"));
                            }
                            managed.quarantine().unwrap();
                            let _ = controller.terminate(&handle);
                            continue;
                        }
                    };
                    stats.max_call_us = stats.max_call_us.max(began.elapsed().as_micros());
                    assert!(reply.len() <= 4096);
                    if let Some(fault) = fault {
                        eprintln!(
                            "TRUSTED_FIXTURE_FAULT {} after applied checkpoint/effect; action={:?}",
                            fault.label, fault.action
                        );
                        injected = true;
                        stats.lost += 1;
                        managed.quarantine().unwrap();
                        match fault.action {
                            FaultAction::DropReply => {}
                            FaultAction::LateReply => {
                                // Deterministically exercise a real stale Controller reply
                                // after the app checkpoint was already atomically admitted.
                                controller.terminate(&handle).unwrap();
                                let end = Instant::now() + Duration::from_secs(2);
                                while call_live(controller, &handle, call_id)
                                    && Instant::now() < end
                                {
                                    thread::sleep(Duration::from_millis(1));
                                }
                                let error = controller
                                    .reply(&handle, call_id, reply.clone())
                                    .expect_err("forced late reply must be rejected");
                                assert_eq!(error.code, "stale_process_reply");
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    before_status,
                                    controller,
                                    &format!("late_reply_fixture:{error:?}"),
                                    cp_before,
                                    true,
                                );
                                recoverable = true;
                            }
                            FaultAction::CloseMount | FaultAction::RevokeInstallation => {
                                if matches!(fault.action, FaultAction::CloseMount) {
                                    host.close_component(mount).unwrap();
                                    stats.mount_close_callback_rejections += 1;
                                } else {
                                    let current = store.current(package).unwrap();
                                    store
                                        .stop_for_worker(
                                            &format!("fixture-revoke-{sequence}"),
                                            current.revision,
                                            package,
                                            "revoke",
                                            Some(handle.run_id()),
                                        )
                                        .unwrap();
                                    stats.store_revoke_callback_rejections += 1;
                                }
                                let request = ComponentContextRequest {
                                    api_version: HOST_API_V0_3,
                                    context_protocol: COMPONENT_CONTEXT_PROTOCOL,
                                    request_id: "late-own-run-after-revoke".into(),
                                    operation: ContextOperation::ReadContext {},
                                };
                                assert!(
                                    host.dispatch_component_context(
                                        store,
                                        mount,
                                        now(),
                                        Some(handle.run_id()),
                                        request,
                                        |_| -> Result<(), ApiError> {
                                            panic!(
                                                "revoked own-run callback reached application input"
                                            )
                                        }
                                    )
                                    .is_err()
                                );
                            }
                        }
                        let _ = controller.terminate(&handle);
                    } else {
                        last_reply = reply.clone();
                        if let Err(error) = controller.reply(&handle, call_id, reply) {
                            if error.code == "stale_process_reply"
                                && interrupted_reply_retryable(controller, &handle)
                            {
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    before_status,
                                    controller,
                                    &format!("reply_unknown:{error:?}"),
                                    cp_before,
                                    false,
                                );
                                recoverable = true;
                            } else {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    &format!("nonretryable_reply:{error:?}"),
                                );
                                fatal_reason = Some(format!("nonretryable_reply:{error:?}"));
                            }
                            managed.quarantine().unwrap();
                            let _ = controller.terminate(&handle);
                        }
                    }
                }
                ProcessEvent::ResultReceived { handle, outcome } => {
                    if injected || fatal_reason.is_some() {
                        continue;
                    }
                    if recoverable {
                        // This run is already quarantined: do not accept business output.
                        // A terminal failure still needs fresh authority and strict kind
                        // classification before we may resume another invocation.
                        if let WorkerOutcome::Failed(f) = outcome {
                            if let Err(error) = host.validate_component_current(
                                store,
                                mount,
                                now(),
                                Some(handle.run_id()),
                            ) {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    &format!("interrupted_terminal_authority:{error:?}"),
                                );
                                fatal_reason =
                                    Some(format!("interrupted_terminal_authority:{error:?}"));
                            } else if f.kind == RuntimeErrorKind::DeadlineExceeded {
                                stats.runtime_deadline_failures += 1;
                                eprintln!(
                                    "AUTHENTICATED_INTERRUPTED_DEADLINE run={} code={}",
                                    handle.run_id(),
                                    f.code
                                );
                            } else {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    &format!(
                                        "interrupted_nonretryable_runtime:{:?}:{}",
                                        f.kind, f.code
                                    ),
                                );
                                fatal_reason = Some(format!(
                                    "interrupted_nonretryable_runtime:{:?}:{}",
                                    f.kind, f.code
                                ));
                            }
                        }
                        continue;
                    }
                    if let Some(input) = broker.late_input.take() {
                        let revision = broker
                            .drafts
                            .read_chunk(&broker.event_mount_draft, None, 0, 512)
                            .unwrap()
                            .revision;
                        broker.cursor = broker
                            .events
                            .publish(
                                &mut broker.drafts,
                                &broker.event_mount,
                                revision,
                                EventAction::InputChanged,
                                input,
                                now(),
                            )
                            .unwrap()
                            .cursor;
                    }
                    let result = managed.accept_component_result(host, store, now(), || {
                        if broker.cursor != run_cursor {
                            return Err(ApiError::new(
                                ErrorCode::StaleContext,
                                "event changed before result acceptance",
                            ));
                        }
                        let snapshot = broker
                            .events
                            .snapshot(&mut broker.drafts, &broker.event_mount, run_cursor, now())
                            .map_err(api)?;
                        if !matches!(snapshot.state, EventState::Active | EventState::Completed) {
                            return Err(ApiError::new(
                                ErrorCode::StaleContext,
                                "event no longer live",
                            ));
                        }
                        match &outcome {
                            WorkerOutcome::Completed(r)
                                if r.calls == count
                                    && (r.output.as_bytes() == last_reply
                                        || rejection_matches(&r.output, expected_rejection)) => {}
                            WorkerOutcome::Failed(_) => {}
                            _ => {
                                return Err(ApiError::new(
                                    ErrorCode::InvalidState,
                                    "result differs from accepted event reply",
                                ));
                            }
                        }
                        Ok(outcome)
                    });
                    match result {
                        Ok(WorkerOutcome::Completed(r)) => {
                            stats.max_completed_vm_fuel =
                                stats.max_completed_vm_fuel.max(r.fuel_consumed);
                            stats.max_vm_calls = stats.max_vm_calls.max(r.calls);
                            guest_rejected = rejection_matches(&r.output, expected_rejection);
                            accepted = true;
                        }
                        Ok(WorkerOutcome::Failed(f)) => {
                            if f.kind == RuntimeErrorKind::DeadlineExceeded {
                                let cp = checkpoint_record(broker);
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    format!("{:?}", controller.status(&handle)),
                                    controller,
                                    &format!("authenticated_runtime_deadline:{}", f.code),
                                    cp,
                                    false,
                                );
                                stats.runtime_deadline_failures += 1;
                                recoverable = true;
                            } else {
                                fatal_run(
                                    stats,
                                    broker,
                                    controller,
                                    &handle,
                                    &format!(
                                        "authenticated_runtime_failure:{:?}:{}",
                                        f.kind, f.code
                                    ),
                                );
                                fatal_reason = Some(format!(
                                    "authenticated_runtime_failure:{:?}:{}",
                                    f.kind, f.code
                                ));
                            }
                            managed.quarantine().unwrap();
                            let _ = controller.terminate(&handle);
                        }
                        Err(error) if broker.cursor != run_cursor => {
                            assert_eq!(error.code, ErrorCode::StaleContext);
                            event_rejected = true;
                            stats.late_results_rejected += 1;
                        }
                        Err(error) => {
                            fatal_run(
                                stats,
                                broker,
                                controller,
                                &handle,
                                &format!("result_authority_or_payload_rejected:{error:?}"),
                            );
                            fatal_reason =
                                Some(format!("result_authority_or_payload_rejected:{error:?}"));
                            let _ = controller.terminate(&handle);
                        }
                    }
                    assert_eq!(handle.run_id(), run);
                }
                ProcessEvent::Reaped { handle, .. } => {
                    managed.record_reaped().unwrap();
                    managed
                        .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                        .unwrap();
                    controller.forget(&handle).unwrap();
                    assert!(
                        injected
                            || accepted
                            || event_rejected
                            || recoverable
                            || fatal_reason.is_some(),
                        "no authenticated completed VM result"
                    );
                    assert!(count <= 8);
                    stats.calls += count;
                    stats.vms += 1;
                    if let Some(reason) = fatal_reason {
                        fail_preserving_pending(broker, stats, &reason, "owned_reaped");
                    }
                    if stats.retry_exhausted {
                        fail_preserving_pending(
                            broker,
                            stats,
                            "recovery_budget_exhausted",
                            "owned_reaped",
                        );
                    }
                    return guest_rejected;
                }
                ProcessEvent::Failed { handle, error } => {
                    eprintln!(
                        "PROCESS_FAILURE run={} error={error:?} status={:?}",
                        handle.run_id(),
                        controller.status(&handle)
                    );
                    if !injected && fatal_reason.is_none() {
                        if error.kind == runtime_process::ProcessErrorKind::Timeout {
                            if !recoverable {
                                let cp = checkpoint_record(broker);
                                recover_run(
                                    stats,
                                    broker,
                                    mount,
                                    &handle,
                                    format!("{:?}", controller.status(&handle)),
                                    controller,
                                    &format!("process_timeout:{error:?}"),
                                    cp,
                                    false,
                                );
                                recoverable = true;
                            }
                        } else {
                            // A prior reply-unknown must never hide a later protocol,
                            // identity, configuration or other non-timeout failure.
                            fatal_run(
                                stats,
                                broker,
                                controller,
                                &handle,
                                &format!("nonretryable_process:{error:?}"),
                            );
                            fatal_reason = Some(format!("nonretryable_process:{error:?}"));
                        }
                        if controller.status(&handle).unwrap().state == WorkerState::NotSpawned {
                            managed.record_not_spawned().unwrap();
                            managed
                                .finish(|| store.finish_worker_lease(handle.run_id()).map_err(api))
                                .unwrap();
                            controller.forget(&handle).unwrap();
                            stats.vms += 1;
                            stats.definite_not_spawned += 1;
                            fail_preserving_pending(
                                broker,
                                stats,
                                "nonretryable_definite_spawn_failure",
                                "definite_not_spawned",
                            );
                        }
                        managed.quarantine().unwrap();
                        let _ = controller.terminate(&handle);
                    }
                }
                ProcessEvent::Quarantined { error, .. } => {
                    eprintln!("UNCONFIRMED_OS_QUARANTINE {error:?}; retaining durable lease");
                    fail_preserving_pending(
                        broker,
                        stats,
                        "owned_reap_unconfirmed",
                        "unconfirmed_lease_retained",
                    );
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}
fn read_text(d: &AnnotationDrafts, id: &DraftId) -> String {
    let mut text = String::new();
    let first = d.read_chunk(id, None, 0, 512).unwrap();
    let rev = first.revision;
    let total = first.total_bytes;
    text.push_str(&first.text);
    while text.len() < total {
        let c = d.read_chunk(id, Some(rev), text.len(), 512).unwrap();
        assert!(!c.text.is_empty());
        text.push_str(&c.text);
    }
    text
}

fn rejection_matches(output: &str, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return false;
    };
    value.as_object().is_some_and(|o| {
        o.len() == 2
            && o.get("status").and_then(|v| v.as_str()) == Some("rejected")
            && o.get("reason").and_then(|v| v.as_str()) == Some(expected)
    })
}

#[allow(clippy::too_many_arguments)] // Explicit real policy, registry, worker and fixture action.
fn stage(
    host: &mut DurableHost,
    store: &mut Store,
    drafts: &mut AnnotationDrafts,
    events: &mut ProviderEvents,
    draft: &DraftId,
    resource: &Resource,
    package: &VerifiedPackage,
    controller: &Controller,
    stats: &mut Stats,
    action: EventAction,
    input: String,
    expected_rejection: Option<&str>,
    injected: &mut BTreeSet<&'static str>,
    inject: bool,
) -> String {
    let package_id = &package.manifest().package_id;
    let facts = store
        .component_installation(package_id, AuthorityRead::Activation)
        .unwrap();
    let mounted = host
        .mount_component(
            store,
            UserMountIntent::new(facts, resource.clone()).unwrap(),
            now(),
        )
        .unwrap();
    if let Some(old) = stats.previous_mount.take() {
        let request = ComponentContextRequest {
            api_version: HOST_API_V0_3,
            context_protocol: COMPONENT_CONTEXT_PROTOCOL,
            request_id: "late-previous-provider".into(),
            operation: ContextOperation::ReadContext {},
        };
        assert!(
            host.dispatch_component_context(
                store,
                &old,
                now(),
                None,
                request,
                |_| -> Result<(), ApiError> { panic!("predecessor callback reached new provider") }
            )
            .is_err()
        );
        host.validate_component_current(store, mounted.handle(), now(), None)
            .unwrap();
    }
    let event_mount = events.register(drafts, draft.clone()).unwrap();
    let revision = drafts.read_chunk(draft, None, 0, 512).unwrap().revision;
    let event = events
        .publish(drafts, &event_mount, revision, action, input.clone(), now())
        .unwrap();
    let mut broker = Broker {
        drafts: std::mem::take(drafts),
        events: std::mem::take(events),
        event_mount_draft: draft.clone(),
        event_mount,
        cursor: event.cursor,
        invocation_cursor: event.cursor,
        planned_fault: None,
        injected: std::mem::take(injected),
        inject,
        late_input: None,
    };
    let mut rejected = false;
    for _ in 0..2200 {
        rejected = run_vm(
            controller,
            store,
            host,
            mounted.handle(),
            &mut broker,
            package.module_bytes(),
            package_id,
            stats.vms + 1,
            stats,
            expected_rejection,
        );
        let snapshot = broker
            .events
            .snapshot(&mut broker.drafts, &broker.event_mount, event.cursor, now())
            .unwrap();
        if rejected || snapshot.state == EventState::Completed {
            break;
        }
    }
    let snapshot = broker
        .events
        .snapshot(&mut broker.drafts, &broker.event_mount, event.cursor, now())
        .unwrap();
    assert_eq!(rejected, expected_rejection.is_some());
    if rejected {
        assert_eq!(snapshot.state, EventState::Active);
        assert_eq!(
            broker
                .drafts
                .read_chunk(draft, None, 0, 512)
                .unwrap()
                .revision,
            revision
        );
        // A rejected transform never cancels or rewrites pending native input.
        let mut preserved = String::new();
        while preserved.len() < input.len() {
            let chunk = broker
                .events
                .read_input_chunk(
                    &mut broker.drafts,
                    &broker.event_mount,
                    event.cursor,
                    preserved.len(),
                    512,
                    now(),
                )
                .unwrap();
            preserved.push_str(&chunk.text);
        }
        assert_eq!(preserved.as_bytes(), input.as_bytes());
    } else {
        assert_eq!(snapshot.state, EventState::Completed);
    }
    if let EventAction::MoveParagraph { from, to } = action
        && from == to
    {
        assert_eq!(
            broker
                .drafts
                .read_chunk(draft, None, 0, 512)
                .unwrap()
                .revision,
            revision,
            "same-index move must not create replacement"
        );
    }
    let actual = read_text(&broker.drafts, draft);
    broker
        .events
        .close(&mut broker.drafts, &broker.event_mount)
        .unwrap();
    host.close_component(mounted.handle()).unwrap();
    let old_request = ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: COMPONENT_CONTEXT_PROTOCOL,
        request_id: "old-callback".into(),
        operation: ContextOperation::ReadContext {},
    };
    assert!(
        host.dispatch_component_context(
            store,
            mounted.handle(),
            now(),
            None,
            old_request,
            |_| -> Result<(), ApiError> {
                panic!("closed provider callback reached successor data")
            }
        )
        .is_err()
    );
    // This headless path has no native widget; all workers really reaped and app jobs closed.
    for ticket in host.pending_component_cleanup() {
        host.confirm_component_released(&ticket).unwrap();
    }
    stats.previous_mount = Some(mounted.handle().clone());
    *drafts = broker.drafts;
    *events = broker.events;
    *injected = broker.injected;
    actual
}
#[allow(clippy::too_many_arguments)] // The fault fixture keeps actual authority, worker and app ownership explicit.
fn run_faults(
    host: &mut DurableHost,
    store: &mut Store,
    core: &mut KnowledgeApplication<SqliteStore>,
    a: &VerifiedPackage,
    b: &VerifiedPackage,
    selection: &core_api::pdf_contract::PdfDraft,
    origin: &core_api::knowledge::Provenance,
    generation: i64,
    capture_session: &DurableSession,
) {
    let initial = format!("{}\r\nsecond\nthird\n", "a".repeat(384));
    let row = "用户输入🙂\r\n\0".to_owned();
    let expected = format!("{}\r\n{}\nthird\n", "a".repeat(384), row);
    let plans = [
        PlannedFault {
            phase: FaultPhase::Scan,
            trigger: TriggeredFault {
                label: "scan_checkpoint_late_reply",
                action: FaultAction::LateReply,
            },
        },
        PlannedFault {
            phase: FaultPhase::Scan,
            trigger: TriggeredFault {
                label: "scan_checkpoint_reply_lost",
                action: FaultAction::DropReply,
            },
        },
        PlannedFault {
            phase: FaultPhase::Scan,
            trigger: TriggeredFault {
                label: "scan_mount_closed",
                action: FaultAction::CloseMount,
            },
        },
        PlannedFault {
            phase: FaultPhase::Begin,
            trigger: TriggeredFault {
                label: "begin_mount_closed",
                action: FaultAction::CloseMount,
            },
        },
        PlannedFault {
            phase: FaultPhase::Append,
            trigger: TriggeredFault {
                label: "append_mount_closed",
                action: FaultAction::CloseMount,
            },
        },
        PlannedFault {
            phase: FaultPhase::Scan,
            trigger: TriggeredFault {
                label: "scan_store_revoked",
                action: FaultAction::RevokeInstallation,
            },
        },
        PlannedFault {
            phase: FaultPhase::Begin,
            trigger: TriggeredFault {
                label: "begin_store_revoked",
                action: FaultAction::RevokeInstallation,
            },
        },
        PlannedFault {
            phase: FaultPhase::Append,
            trigger: TriggeredFault {
                label: "append_store_revoked",
                action: FaultAction::RevokeInstallation,
            },
        },
    ];
    let controller = Controller::new().unwrap();
    let mut stats = Stats::default();
    let mut drafts = AnnotationDrafts::new();
    let mut events = ProviderEvents::new();
    let mut injected = BTreeSet::new();
    let mut pending_copied = 0;
    let mut resumed = 0;
    let mut late_resumed = 0;
    for (index, plan) in plans.into_iter().enumerate() {
        let draft = drafts
            .create_pdf(
                core,
                1,
                "vault",
                selection.clone(),
                origin.clone(),
                generation,
            )
            .unwrap();
        let resource = Resource::Draft {
            vault_id: "vault".into(),
            draft_id: format!("{:032x}", index + 100),
        };
        let actual = stage(
            host,
            store,
            &mut drafts,
            &mut events,
            &draft,
            &resource,
            a,
            &controller,
            &mut stats,
            EventAction::InputChanged,
            initial.clone(),
            None,
            &mut injected,
            false,
        );
        assert_eq!(actual.as_bytes(), initial.as_bytes());
        let revision = drafts.read_chunk(&draft, None, 0, 512).unwrap().revision;
        let facts = store
            .component_installation(&b.manifest().package_id, AuthorityRead::Activation)
            .unwrap();
        let mounted = host
            .mount_component(store, UserMountIntent::new(facts, resource).unwrap(), now())
            .unwrap();
        let event_mount = events.register(&mut drafts, draft.clone()).unwrap();
        let event = events
            .publish(
                &mut drafts,
                &event_mount,
                revision,
                EventAction::ReplaceParagraph { index: 1 },
                row.clone(),
                now(),
            )
            .unwrap();
        let mut broker = Broker {
            drafts,
            event_mount_draft: draft.clone(),
            events,
            event_mount,
            cursor: event.cursor,
            injected,
            inject: false,
            late_input: None,
            invocation_cursor: event.cursor,
            planned_fault: Some(plan),
        };
        let before_rejections =
            stats.mount_close_callback_rejections + stats.store_revoke_callback_rejections;
        for _ in 0..100 {
            assert!(!run_vm(
                &controller,
                store,
                host,
                mounted.handle(),
                &mut broker,
                b.module_bytes(),
                &b.manifest().package_id,
                stats.vms + 1,
                &mut stats,
                None
            ));
            let current = broker
                .events
                .snapshot(&mut broker.drafts, &broker.event_mount, event.cursor, now())
                .unwrap();
            if current.state == EventState::Completed
                || stats.mount_close_callback_rejections + stats.store_revoke_callback_rejections
                    > before_rejections
            {
                break;
            }
        }
        assert!(
            broker.planned_fault.is_none(),
            "requested real guest phase was not observed"
        );
        let current = broker
            .events
            .snapshot(&mut broker.drafts, &broker.event_mount, event.cursor, now())
            .unwrap();
        match plan.trigger.action {
            FaultAction::DropReply | FaultAction::LateReply => {
                assert_eq!(current.state, EventState::Completed);
                assert_eq!(
                    read_text(&broker.drafts, &draft).as_bytes(),
                    expected.as_bytes()
                );
                if matches!(plan.trigger.action, FaultAction::LateReply) {
                    late_resumed += 1;
                } else {
                    resumed += 1;
                }
                save_gate::save(
                    host,
                    store,
                    capture_session,
                    &mut broker.drafts,
                    &draft,
                    core,
                    origin,
                    generation,
                    &format!("save-resumed-scan-checkpoint-{index}"),
                    &expected,
                );
            }
            FaultAction::CloseMount | FaultAction::RevokeInstallation => {
                assert_eq!(current.state, EventState::Active);
                assert_eq!(
                    broker
                        .drafts
                        .read_chunk(&draft, None, 0, 512)
                        .unwrap()
                        .revision,
                    revision
                );
                assert_eq!(
                    read_text(&broker.drafts, &draft).as_bytes(),
                    initial.as_bytes()
                );
                // This is a trusted native copy action, not a revoked provider call.
                // Keep the original native input reference; do not close/discard first.
                let mut copied = String::new();
                while copied.len() < row.len() {
                    let c = broker
                        .events
                        .read_input_chunk(
                            &mut broker.drafts,
                            &broker.event_mount,
                            event.cursor,
                            copied.len(),
                            512,
                            now(),
                        )
                        .unwrap();
                    assert!(!c.text.is_empty());
                    copied.push_str(&c.text);
                }
                assert_eq!(copied.as_bytes(), row.as_bytes());
                pending_copied += 1;
                eprintln!(
                    "TRUSTED_NATIVE_COPY_CONFIRMED {} bytes={}; explicit discard now closes event",
                    plan.trigger.label,
                    copied.len()
                );
            }
        }
        broker
            .events
            .close(&mut broker.drafts, &broker.event_mount)
            .unwrap();
        host.close_component(mounted.handle()).unwrap();
        for ticket in host.pending_component_cleanup() {
            host.confirm_component_released(&ticket).unwrap();
        }
        stats.previous_mount = Some(mounted.handle().clone());
        broker.drafts.remove(&draft).unwrap();
        drafts = broker.drafts;
        events = broker.events;
        injected = broker.injected;
        assert!(store.worker_leases().unwrap().is_empty());
        eprintln!(
            "PARAGRAPH_FAULT_CASE_PASSED {} vms={}",
            plan.trigger.label, stats.vms
        );
    }
    assert_eq!(resumed, 1);
    assert_eq!(late_resumed, 1);
    assert_eq!(pending_copied, 6);
    assert_eq!(stats.mount_close_callback_rejections, 3);
    assert_eq!(stats.store_revoke_callback_rejections, 3);
    assert_eq!(core.export_all().unwrap().pdf_captures.len(), 2);
    println!(
        "{}",
        serde_json::json!({"ok":true,"mode":"faults","cases":8,"scan_checkpoint_lost_reply_resumed":resumed,"late_reply_fixture_recovered":late_resumed,"mount_close_callbacks_rejected":stats.mount_close_callback_rejections,"store_generation_callbacks_rejected":stats.store_revoke_callback_rejections,"pending_native_copies_before_confirmed_discard":pending_copied,"current_text_unchanged_on_six_cancellations":true,"vms":stats.vms,"owned_workers_reaped":stats.vms-stats.definite_not_spawned,"definite_not_spawned_runs":stats.definite_not_spawned,"calls":stats.calls,"fixture_interruptions":stats.lost,"automatic_recoveries":stats.automatic_recoveries,"unplanned_watchdog_timeouts":stats.unplanned_watchdog_timeouts,"runtime_deadline_failures":stats.runtime_deadline_failures,"nonretryable_failures":stats.nonretryable_failures,"queued_calls_refused":stats.queued_calls_refused,"recovery_records":stats.recovery_records,"default_deadline_ms":250,"max_completed_vm_fuel":stats.max_completed_vm_fuel,"max_vm_calls":stats.max_vm_calls,"max_call_us":stats.max_call_us,"knowledge_saves":2,"native_widgets_mounted":false,"source":"synthetic_Core_snapshot_not_parser_or_Surface_lease_evidence"})
    );
}
fn main() {
    match runtime_process::worker_main(&WasmiRuntime) {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            eprintln!("managed worker {e}");
            std::process::exit(1);
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let a_dir = root.join("evidence/component-author-a");
    let b_dir = root.join("evidence/component-author-b");
    let key =
        fs::read_to_string(a_dir.join("publisher-public-key.txt")).expect("stable A key required");
    let load = |path: PathBuf, key: &str| {
        verify_for_host(
            &fs::read(path).expect("signed ordinary fixture missing"),
            key.trim(),
            &SUPPORTED_HOST_APIS,
        )
        .unwrap()
    };
    let a = load(
        std::env::var_os("COMPONENT_PACKAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|| a_dir.join("provider-a.qpk")),
        &std::env::var("COMPONENT_PUBLIC_KEY").unwrap_or_else(|_| key.clone()),
    );
    let b_key = std::env::var("PARAGRAPH_PUBLIC_KEY").unwrap_or_else(|_| {
        fs::read_to_string(b_dir.join("publisher-public-key.txt")).expect("stable B key required")
    });
    let b = load(
        std::env::var_os("PARAGRAPH_PACKAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|| b_dir.join("provider-b.qpk")),
        &b_key,
    );
    let capture = load(
        std::env::var_os("CAPTURE_PACKAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|| a_dir.join("capture-identity.qpk")),
        &std::env::var("CAPTURE_PUBLIC_KEY").unwrap_or_else(|_| key.clone()),
    );
    let scratch = Scratch::new();
    let shared = Arc::new(Mutex::new(
        DurableHost::new(
            "vault",
            [Capability::NoteDraftEdit, Capability::CapturePropose],
        )
        .unwrap(),
    ));
    let mut store = Store::open_with_services(
        &scratch.0.join("packages"),
        &SUPPORTED_HOST_APIS,
        StoreServices {
            grant_validator: Arc::new(save_gate::Policy(shared.clone())),
            ..StoreServices::default()
        },
    )
    .unwrap();
    assert_ne!(a.manifest().package_id, b.manifest().package_id);
    for (index, package) in [&a, &b].into_iter().enumerate() {
        store
            .install(&format!("install-provider-{index}"), 0, package)
            .unwrap();
        store
            .bind_component_vault(
                &format!("bind-provider-{index}"),
                1,
                &package.manifest().package_id,
                "vault",
            )
            .unwrap();
    }
    let capture_id = &capture.manifest().package_id;
    store.install("install-capture", 0, &capture).unwrap();
    let capture_receipt = store
        .grant_set(
            "grant-capture",
            1,
            capture_id,
            GrantSet {
                api_version: HOST_API_V0_2,
                vault_id: "vault".into(),
                expires_at: now() + 3600,
                grants: vec![PersistentGrant {
                    capability: Capability::CapturePropose,
                    resource: Resource::Vault {
                        vault_id: "vault".into(),
                    },
                    resource_version: None,
                }],
            },
        )
        .unwrap();
    let origin = core_api::knowledge::Provenance {
        package_id: capture_id.clone(),
        publisher_id: capture.manifest().publisher_id.clone(),
        package_digest: capture.digest().into(),
        installation_id: capture_receipt.installation_id,
    };
    let mut host = shared.lock().unwrap();
    host.configure_component_identity([53; 16]).unwrap();
    let capture_session = host
        .activate(&store, capture_id, HOST_API_V0_2, now())
        .unwrap();
    let mut core =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("knowledge")).unwrap());
    let (original, snapshot, selection) = fixture::pdf_fixture();
    core.import_document(
        "synthetic-source",
        "vault",
        "Explicit synthetic source",
        &original,
        &snapshot,
    )
    .unwrap();
    if std::env::args().any(|arg| arg == "--faults") {
        run_faults(
            &mut host,
            &mut store,
            &mut core,
            &a,
            &b,
            &selection,
            &origin,
            capture_receipt.generation,
            &capture_session,
        );
        return;
    }
    let cases: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(root.join("fixtures/paragraph-cases.json")).unwrap())
            .unwrap();
    assert_eq!(cases.len(), 20);
    let mut drafts = AnnotationDrafts::new();
    let mut events = ProviderEvents::new();
    let controller = Controller::new().unwrap();
    let mut stats = Stats::default();
    let mut injected = BTreeSet::new();
    let mut rejected = 0;
    for (index, case) in cases.iter().enumerate() {
        let label = case["id"].as_str().unwrap();
        let initial = case["text"].as_str().unwrap();
        let draft = drafts
            .create_pdf(
                &core,
                1,
                "vault",
                selection.clone(),
                origin.clone(),
                capture_receipt.generation,
            )
            .unwrap();
        let resource = Resource::Draft {
            vault_id: "vault".into(),
            draft_id: format!("{:032x}", index + 1),
        };
        let after_a = stage(
            &mut host,
            &mut store,
            &mut drafts,
            &mut events,
            &draft,
            &resource,
            &a,
            &controller,
            &mut stats,
            EventAction::InputChanged,
            initial.into(),
            None,
            &mut injected,
            false,
        );
        assert_eq!(after_a.as_bytes(), initial.as_bytes());
        let op = &case["operation"];
        let (action, input) = match op["kind"].as_str().unwrap() {
            "move" => (
                EventAction::MoveParagraph {
                    from: op["from"].as_u64().unwrap() as u32,
                    to: op["to"].as_u64().unwrap() as u32,
                },
                String::new(),
            ),
            "replace" => (
                EventAction::ReplaceParagraph {
                    index: op["index"].as_u64().unwrap() as u32,
                },
                op["input"].as_str().unwrap().into(),
            ),
            _ => panic!("unknown fixed fixture operation"),
        };
        let reject = case
            .get("expected_error")
            .map(|_| "paragraph_boundary_conflict");
        let after_b = stage(
            &mut host,
            &mut store,
            &mut drafts,
            &mut events,
            &draft,
            &resource,
            &b,
            &controller,
            &mut stats,
            action,
            input,
            reject,
            &mut injected,
            true,
        );
        let expected = if reject.is_some() {
            rejected += 1;
            initial
        } else {
            case["expected"].as_str().unwrap()
        };
        assert_eq!(after_b.as_bytes(), expected.as_bytes(), "B fixture {label}");
        // Display/readback only: no parent paragraph parsing or candidate construction.
        // The same native whole-text value is explicitly re-emitted into A's fallback input.
        let after_return = stage(
            &mut host,
            &mut store,
            &mut drafts,
            &mut events,
            &draft,
            &resource,
            &a,
            &controller,
            &mut stats,
            EventAction::InputChanged,
            after_b,
            None,
            &mut injected,
            false,
        );
        assert_eq!(after_return.as_bytes(), expected.as_bytes());
        save_gate::save(
            &mut host,
            &store,
            &capture_session,
            &mut drafts,
            &draft,
            &mut core,
            &origin,
            capture_receipt.generation,
            &format!("save-case-{}", index + 1),
            expected,
        );
        drafts.remove(&draft).unwrap();
        eprintln!("PARAGRAPH_CASE_PASSED {label} vms={}", stats.vms);
    }
    assert_eq!(rejected, 4);
    assert!(store.worker_leases().unwrap().is_empty());
    let archive = core.export_all().unwrap();
    assert_eq!(archive.pdf_captures.len(), 20);
    assert_eq!(core.original_pdf(1).unwrap(), original);
    let bytes = serde_json::to_vec(&archive).unwrap();
    let installed_b = store.current(&b.manifest().package_id).unwrap();
    store
        .uninstall(
            "uninstall-paragraph-after-saves",
            installed_b.revision,
            &b.manifest().package_id,
        )
        .unwrap();
    host.retire_removed(
        &store,
        installed_b.installation_id,
        &b.manifest().package_id,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&core.export_all().unwrap()).unwrap(),
        bytes
    );
    drop(core);
    let reopened =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("knowledge")).unwrap());
    assert_eq!(
        serde_json::to_vec(&reopened.export_all().unwrap()).unwrap(),
        bytes
    );
    let mut restored =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("restore")).unwrap());
    restored.restore_all(&archive).unwrap();
    assert_eq!(
        serde_json::to_vec(&restored.export_all().unwrap()).unwrap(),
        bytes
    );
    println!(
        "{}",
        serde_json::json!({"ok":true,"cases":20,"boundary_rejections":rejected,"a_b_a_same_draft":20,"vms":stats.vms,"calls":stats.calls,"owned_workers_reaped":stats.vms-stats.definite_not_spawned,"definite_not_spawned_runs":stats.definite_not_spawned,"default_deadline_ms":250,"max_completed_vm_fuel":stats.max_completed_vm_fuel,"max_vm_calls":stats.max_vm_calls,"max_call_us":stats.max_call_us,"fixture_lost_replies":stats.lost,"automatic_recoveries":stats.automatic_recoveries,"unplanned_watchdog_timeouts":stats.unplanned_watchdog_timeouts,"runtime_deadline_failures":stats.runtime_deadline_failures,"nonretryable_failures":stats.nonretryable_failures,"queued_calls_refused":stats.queued_calls_refused,"recovery_records":stats.recovery_records,"knowledge_saves":20,"direct_draft_writes_rejected":stats.direct_draft_writes_rejected,"sqlite_reopen_and_empty_restore":true,"uninstall_preserves_knowledge":true,"source":"synthetic_Core_snapshot_not_parser_or_Surface_lease_evidence","capture_fixture_module_executed":false,"native_widgets_mounted":false})
    );
}
