//! Real same-executable workers consume an ordinary external signed component.
//! Synthetic source facts are explicit; native save uses a separate ordinary capture identity.
//!
//! Default fixtures are the stable repository evidence packages. Optional overrides:
//! set COMPONENT_PACKAGE and CAPTURE_PACKAGE to filesystem paths for the
//! ordinary signed fixtures (absolute paths recommended: Cargo runs from this package).
//! COMPONENT_PUBLIC_KEY and CAPTURE_PUBLIC_KEY contain raw 64hex public keys, not paths.
//! Stable fixtures: evidence/component-author-a/{provider-a,capture-identity}.qpk;
//! publisher-public-key.txt in that directory records the public key for both.
//! Run `cargo test --test component_text_consumer`; append `-- --large` for a
//! complete 1MiB NUL/worst-JSON stream. Small mode includes Unicode/CRLF and empty text.
//! Source facts remain synthetic; no parser, reader gesture, live selection lease
//! or native widget release is claimed by this headless consumer.
#[path = "component_support/fixture.rs"]
mod fixture;
#[path = "component_support/save_gate.rs"]
mod save_gate;
use extension_api::{composition::*, continuation::*, *};
use extension_host::{component::MountHandle, durable::*};
use extension_package::package::verify_for_host;
use extension_runtime_api::ExecutionLimits;
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
    collections::BTreeSet,
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
}
impl Broker {
    fn wire(
        &mut self,
        raw: &[u8],
        ctx: &extension_host::component::AuthorizedComponentContext,
    ) -> Result<(Vec<u8>, Option<&'static str>), ApiError> {
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
                let label = match &wire_effect {
                    Some(EventEffect::BeginReplace { .. }) => Some("begin_reply_lost"),
                    Some(EventEffect::AppendChunk { .. }) => Some("append_reply_lost"),
                    Some(EventEffect::CommitReplace {}) => Some("commit_reply_lost"),
                    _ => None,
                };
                self.events
                    .advance(
                        &mut self.drafts,
                        &self.event_mount,
                        AdvanceEvent {
                            cursor,
                            expected_checkpoint_revision,
                            checkpoint: decode_checkpoint(
                                &checkpoint_base64,
                                extension_api::continuation::MAX_CHECKPOINT_BYTES,
                            )?,
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
                    fault = Some(label);
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
    ) -> Result<(Vec<u8>, Option<&'static str>), ApiError> {
        if let Ok(r) = ComponentContextRequest::from_slice(raw) {
            h.dispatch_component_context(s, m, now(), Some(run), r, |ctx| self.wire(raw, ctx))
        } else {
            let r = ContinuationRequest::from_slice(raw)?;
            h.dispatch_continuation(s, m, now(), Some(run), r, |ctx| self.wire(raw, ctx))
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
) {
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
    let mut event_rejected = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "worker did not reap: {:?}",
            controller.status(&worker)
        );
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
                    let began = Instant::now();
                    let (reply, fault) = broker
                        .call(host, store, mount, handle.run_id(), &bytes)
                        .unwrap();
                    stats.max_call_us = stats.max_call_us.max(began.elapsed().as_micros());
                    assert!(reply.len() <= 4096);
                    if let Some(label) = fault {
                        eprintln!(
                            "FIXTURE_REPLY_LOST {label} after applied effect; terminate owned worker"
                        );
                        injected = true;
                        stats.lost += 1;
                        managed.quarantine().unwrap();
                        controller.terminate(&handle).unwrap();
                    } else {
                        last_reply = reply.clone();
                        controller.reply(&handle, call_id, reply).unwrap();
                    }
                }
                ProcessEvent::ResultReceived { handle, outcome } => {
                    assert!(!injected);
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
                                if r.calls == count && r.output.as_bytes() == last_reply => {}
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
                        Ok(WorkerOutcome::Completed(_)) => accepted = true,
                        Ok(WorkerOutcome::Failed(f)) => {
                            panic!("worker VM failed {}: {:?}", f.code, f.kind)
                        }
                        Err(error) if broker.cursor != run_cursor => {
                            assert_eq!(error.code, ErrorCode::StaleContext);
                            event_rejected = true;
                            stats.late_results_rejected += 1;
                        }
                        Err(error) => panic!("unexpected result rejection {error:?}"),
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
                        injected || accepted || event_rejected,
                        "no authenticated completed VM result"
                    );
                    assert!(count <= 8);
                    stats.calls += count;
                    stats.vms += 1;
                    return;
                }
                ProcessEvent::Failed { handle, error } => {
                    assert!(
                        injected,
                        "worker failed {error:?} state {:?}",
                        controller.status(&handle)
                    );
                    assert_ne!(
                        controller.status(&handle).unwrap().state,
                        WorkerState::NotSpawned
                    );
                }
                ProcessEvent::Quarantined { error, .. } => {
                    panic!("unexpected OS quarantine {error:?}")
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
fn main() {
    match runtime_process::worker_main(&WasmiRuntime) {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            eprintln!("managed worker {e}");
            std::process::exit(1);
        }
    }
    let evidence = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evidence/component-author-a");
    let path = std::env::var_os("COMPONENT_PACKAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.join("provider-a.qpk"));
    let default_key = fs::read_to_string(evidence.join("publisher-public-key.txt"))
        .expect("stable external fixture public key missing");
    let key = std::env::var("COMPONENT_PUBLIC_KEY").unwrap_or_else(|_| default_key.trim().into());
    let large = std::env::args().any(|s| s == "--large");
    let package = verify_for_host(&fs::read(path).unwrap(), &key, &SUPPORTED_HOST_APIS).unwrap();
    let package_id = package.manifest().package_id.clone();
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
    store.install("install-editor", 0, &package).unwrap();
    store
        .bind_component_vault("bind-editor", 1, &package_id, "vault")
        .unwrap();
    let capture_path = std::env::var_os("CAPTURE_PACKAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.join("capture-identity.qpk"));
    let capture_key =
        std::env::var("CAPTURE_PUBLIC_KEY").unwrap_or_else(|_| default_key.trim().into());
    let capture = verify_for_host(
        &fs::read(capture_path).unwrap(),
        &capture_key,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    let capture_id = &capture.manifest().package_id;
    assert_ne!(capture_id, &package_id);
    assert_eq!(
        capture.manifest().required_capabilities,
        vec![Capability::CapturePropose]
    );
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
    host.configure_component_identity([41; 16]).unwrap();
    let capture_session = host
        .activate(&store, capture_id, HOST_API_V0_2, now())
        .unwrap();
    let mut core =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("knowledge")).unwrap());
    let (original, snapshot, selection) = fixture::pdf_fixture();
    core.import_document(
        "synthetic-map-fixture",
        "vault",
        "Explicit synthetic source",
        &original,
        &snapshot,
    )
    .unwrap();
    let controller = Controller::new().unwrap();
    let mut stats = Stats::default();
    let mut injected = BTreeSet::new();
    let mut drafts = AnnotationDrafts::new();
    let mut events = ProviderEvents::new();
    let mut receipts = Vec::new();
    let samples = if large {
        vec!["\0".repeat(1024 * 1024)]
    } else {
        vec!["中文😀\r\n\0\n".repeat(80), String::new()]
    };
    for (index, input) in samples.into_iter().enumerate() {
        // Source/selection is a trusted synthetic Core fixture, not a guest proposal or gesture.
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
        let facts = store
            .component_installation(&package_id, AuthorityRead::Activation)
            .unwrap();
        let mounted = host
            .mount_component(
                &store,
                UserMountIntent::new(
                    facts,
                    Resource::Draft {
                        vault_id: "vault".into(),
                        draft_id: format!("{:032x}", index + 1),
                    },
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        let event_mount = events.register(&mut drafts, draft.clone()).unwrap();
        let mut broker = Broker {
            drafts,
            event_mount_draft: draft.clone(),
            events,
            event_mount,
            cursor: EventCursor {
                sequence: 1,
                base_revision: 1,
            },
            injected,
            inject: !large,
            late_input: None,
        };
        let event = broker
            .events
            .publish(
                &mut broker.drafts,
                &broker.event_mount,
                1,
                EventAction::InputChanged,
                input.clone(),
                now(),
            )
            .unwrap();
        broker.cursor = event.cursor;
        if !large && stats.late_results_rejected == 0 {
            broker.late_input = Some(input.clone());
        }
        for _ in 0..2200 {
            run_vm(
                &controller,
                &mut store,
                &mut host,
                mounted.handle(),
                &mut broker,
                package.module_bytes(),
                &package_id,
                stats.vms + 1,
                &mut stats,
            );
            let state = broker
                .events
                .snapshot(
                    &mut broker.drafts,
                    &broker.event_mount,
                    broker.cursor,
                    now(),
                )
                .unwrap()
                .state;
            if state == EventState::Completed {
                break;
            }
            if stats.vms % 128 == 0 {
                eprintln!("COMPONENT_PROGRESS vms={} calls={}", stats.vms, stats.calls);
            }
        }
        assert_eq!(
            broker
                .events
                .snapshot(
                    &mut broker.drafts,
                    &broker.event_mount,
                    broker.cursor,
                    now()
                )
                .unwrap()
                .state,
            EventState::Completed
        );
        assert_eq!(
            read_text(&broker.drafts, &draft).as_bytes(),
            input.as_bytes()
        );
        eprintln!(
            "REAL_COMPONENT_INPUT_COMPLETED bytes={} vms={}",
            input.len(),
            stats.vms
        );
        // Publication/editor access alone never authorizes a knowledge commit.
        let receipt = save_gate::save(
            &mut host,
            &store,
            &capture_session,
            &mut broker.drafts,
            &draft,
            &mut core,
            &origin,
            capture_receipt.generation,
            &format!("explicit-save-{}", index + 1),
            &input,
        );
        receipts.push(receipt);
        broker
            .events
            .close(&mut broker.drafts, &broker.event_mount)
            .unwrap();
        host.close_component(mounted.handle()).unwrap();
        // No native widget exists in this headless consumer; owned workers reaped and app jobs closed.
        for ticket in host.pending_component_cleanup() {
            host.confirm_component_released(&ticket).unwrap();
        }
        broker.drafts.remove(&draft).unwrap();
        drafts = broker.drafts;
        events = broker.events;
        injected = broker.injected;
    }
    if !large {
        assert_eq!(injected.len(), 3);
        assert_eq!(stats.late_results_rejected, 1);
    }
    assert!(store.worker_leases().unwrap().is_empty());
    // A real durable revocation must reject before the callback can freeze text.
    // This is native-fixture intent, not a fabricated gesture/selection approval.
    let unsaved = drafts
        .create_pdf(
            &core,
            1,
            "vault",
            selection,
            origin.clone(),
            capture_receipt.generation,
        )
        .unwrap();
    let current_capture = store.current(capture_id).unwrap();
    store
        .stop(
            "revoke-capture-after-saves",
            current_capture.revision,
            capture_id,
            "revoke",
        )
        .unwrap();
    let denied = host.dispatch(
        &store,
        &capture_session,
        now(),
        None,
        VersionedRequest {
            resource_version: None,
            request: CapabilityRequest {
                api_version: HOST_API_V0_2,
                request_id: "revoked-native-save".into(),
                capability: Capability::CapturePropose,
                resource: Resource::Vault {
                    vault_id: "vault".into(),
                },
            },
        },
        |_| -> Result<(), ApiError> { panic!("revoked capture reached freeze/commit callback") },
    );
    assert!(denied.is_err());
    assert_eq!(
        drafts.state(&unsaved).unwrap(),
        learning_application::draft_text::DraftState::Editing
    );
    drafts.remove(&unsaved).unwrap();
    let archive = core.export_all().unwrap();
    assert_eq!(archive.pdf_captures.len(), receipts.len());
    assert_eq!(core.original_pdf(1).unwrap(), original);
    let bytes = serde_json::to_vec(&archive).unwrap();
    drop(core);
    let reopened =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("knowledge")).unwrap());
    assert_eq!(
        serde_json::to_vec(&reopened.export_all().unwrap()).unwrap(),
        bytes
    );
    let mut restored =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("restored")).unwrap());
    assert!(restored.export_all().unwrap().commits.is_empty());
    restored.restore_all(&archive).unwrap();
    assert_eq!(
        serde_json::to_vec(&restored.export_all().unwrap()).unwrap(),
        bytes
    );
    println!(
        "{}",
        serde_json::json!({"ok":true,"external_signed_component":package_id,"capture_identity":capture_id,"capture_fixture_module_executed":false,"large":large,"vms":stats.vms,"calls":stats.calls,"default_deadline_ms":250,"fixture_lost_replies":stats.lost,"late_event_results_rejected":stats.late_results_rejected,"owned_workers_reaped":stats.vms,"max_call_us":stats.max_call_us,"knowledge_saves":receipts.len(),"sqlite_reopen_and_empty_restore":true,"revoked_save_rejected_before_freeze":true,"source":"synthetic_Core_snapshot_not_parser_or_Surface_lease_evidence"})
    );
}
