//! Shared experimental edit broker. No runtime, native UI, fault injection or
//! Core write lives here. The trusted coordinator serializes application access,
//! admits each actual Controller call, and owns lease/reap/result lifecycle.
use extension_api::{
    ApiError, ErrorCode, HOST_API_V0_3, Resource, component_invocation::*, composition::*,
    continuation::*, draft::*,
};
use extension_host::{
    component::{AuthorizedComponentContext, MountHandle},
    durable::{ComponentAuthoritySource, DurableHost},
};
use learning_application::{
    annotation_draft::AnnotationDrafts, draft_text::DraftId, provider_events::*,
};
use std::sync::Arc;
fn stale() -> ApiError {
    ApiError::new(ErrorCode::StaleContext, "edit invocation changed or closed")
}
fn application(e: core_api::knowledge::KnowledgeError) -> ApiError {
    use core_api::knowledge::KnowledgeErrorCode as C;
    ApiError::new(
        match e.code {
            C::InvalidInput => ErrorCode::InvalidRequest,
            C::Conflict | C::NotFound => ErrorCode::StaleContext,
            C::BudgetExceeded => ErrorCode::BudgetExceeded,
            C::UnsupportedVersion => ErrorCode::UnsupportedVersion,
            _ => ErrorCode::InvalidState,
        },
        "edit application rejected",
    )
}
fn metadata(s: EventSnapshot) -> ContinuationMetadata {
    ContinuationMetadata {
        checkpoint_revision: s.checkpoint_revision,
        checkpoint_bytes: s.checkpoint_bytes as u16,
        state: match s.state {
            EventState::Active => ContinuationState::Active,
            EventState::Completed => ContinuationState::Completed,
            EventState::Cancelled => ContinuationState::Cancelled,
            EventState::Expired => ContinuationState::Expired,
        },
        last_effect: s.last_effect.map(|e| match e {
            EffectOutcome::Started => EventEffectOutcome::Started {},
            EffectOutcome::ChunkAccepted { next_offset } => EventEffectOutcome::ChunkAccepted {
                next_offset: next_offset as u32,
            },
            EffectOutcome::Replaced { revision } => EventEffectOutcome::Replaced { revision },
            EffectOutcome::Cancelled => EventEffectOutcome::Cancelled {},
        }),
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
/// Captured by trusted scheduling, never deserialized or guest-selected.
#[derive(Clone, Debug)]
pub struct EditInvocation {
    scope: Arc<()>,
    generation: u64,
    cursor: EventCursor,
    run_id: Option<String>,
}
pub struct EditServices {
    scope: Arc<()>,
    mount: MountHandle,
    draft: DraftId,
    event_mount: EventMount,
    cursor: Option<EventCursor>,
    generation: u64,
    closed: bool,
}
impl EditServices {
    /// Trusted association of one opaque application Draft with one approved Host mount.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        draft: DraftId,
        now: u64,
    ) -> Result<Self, ApiError> {
        if drafts.source(&draft).map_err(application)?.0.vault() != mount.resource().vault_id() {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "application draft differs from mounted vault",
            ));
        }
        host.validate_component_current(source, mount, now, None)?;
        let event_mount = events
            .register(drafts, draft.clone())
            .map_err(application)?;
        Ok(Self {
            scope: Arc::new(()),
            mount: mount.clone(),
            draft,
            event_mount,
            cursor: None,
            generation: 0,
            closed: false,
        })
    }
    /// Input is a complete native snapshot of this action, never host-rebuilt text.
    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &mut self,
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        base_revision: u64,
        action: EventAction,
        input: String,
        now: u64,
    ) -> Result<EventSnapshot, ApiError> {
        if self.closed {
            return Err(stale());
        }
        let next = self.generation.checked_add(1).ok_or_else(stale)?;
        host.validate_component_current(source, &self.mount, now, None)?;
        let snapshot = events
            .publish(drafts, &self.event_mount, base_revision, action, input, now)
            .map_err(application)?;
        self.cursor = Some(snapshot.cursor);
        self.generation = next;
        Ok(snapshot)
    }
    pub fn invocation(&self, run_id: Option<&str>) -> Result<EditInvocation, ApiError> {
        if self.closed {
            return Err(stale());
        }
        Ok(EditInvocation {
            scope: self.scope.clone(),
            generation: self.generation,
            cursor: self.cursor.ok_or_else(stale)?,
            run_id: run_id.map(str::to_owned),
        })
    }
    fn validate_invocation(&self, i: &EditInvocation) -> Result<(), ApiError> {
        if self.closed
            || !Arc::ptr_eq(&self.scope, &i.scope)
            || self.generation != i.generation
            || self.cursor != Some(i.cursor)
        {
            return Err(stale());
        }
        Ok(())
    }
    fn authorized(&self, c: &AuthorizedComponentContext) -> Result<(), ApiError> {
        if c.mount_id != self.mount.mount_id() || c.resource != *self.mount.resource() {
            return Err(stale());
        }
        Ok(())
    }
    fn current(
        &self,
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
        i: &EditInvocation,
        now: u64,
    ) -> Result<EventSnapshot, ApiError> {
        self.validate_invocation(i)?;
        let s = events
            .snapshot(drafts, &self.event_mount, i.cursor, now)
            .map_err(application)?;
        if !matches!(s.state, EventState::Active | EventState::Completed) {
            return Err(stale());
        }
        Ok(s)
    }
    /// Fresh authority gate for a real result; this does not infer runtime success.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_result(
        &self,
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        i: &EditInvocation,
        now: u64,
    ) -> Result<EventSnapshot, ApiError> {
        self.validate_invocation(i)?;
        host.validate_component_current(source, &self.mount, now, i.run_id.as_deref())?;
        self.current(drafts, events, i, now)
    }
    /// Application cleanup only. Host mount closure and real release ACK are external.
    pub fn close(
        &mut self,
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
    ) -> Result<(), ApiError> {
        if self.closed {
            return Err(stale());
        }
        events
            .close(drafts, &self.event_mount)
            .map_err(application)?;
        self.closed = true;
        self.cursor = None;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        drafts: &mut AnnotationDrafts,
        events: &mut ProviderEvents,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        i: &EditInvocation,
        raw: &[u8],
        now: u64,
        admit: &dyn Fn() -> Result<(), ApiError>,
    ) -> Result<Vec<u8>, ApiError> {
        if raw.len() > 65536 {
            return Err(ApiError::new(
                ErrorCode::BudgetExceeded,
                "edit request exceeds budget",
            ));
        }
        self.validate_invocation(i)?;
        let mount = self.mount.clone();
        if let Ok(r) = ComponentInvocationRequest::from_slice(raw) {
            return host.dispatch_component_invocation(
                source,
                &mount,
                now,
                i.run_id.as_deref(),
                r.clone(),
                |c| {
                    admit()?;
                    self.authorized(c)?;
                    self.current(drafts, events, i, now)?;
                    Ok(ComponentInvocationResponse {
                        api_version: HOST_API_V0_3,
                        component_invocation_protocol: COMPONENT_INVOCATION_PROTOCOL,
                        request_id: r.request_id,
                        context: ComponentInvocationContext::EditEvent {},
                    }
                    .to_json()?
                    .into_bytes())
                },
            );
        }
        if let Ok(r) = ComponentContextRequest::from_slice(raw) {
            return host.dispatch_component_context(
                source,
                &mount,
                now,
                i.run_id.as_deref(),
                r.clone(),
                |c| {
                    admit()?;
                    self.authorized(c)?;
                    self.validate_invocation(i)?;
                    if let ContextOperation::ReadInputChunk {
                        sequence,
                        expected_revision,
                        ..
                    } = &r.operation
                        && (*sequence != i.cursor.sequence
                            || *expected_revision != i.cursor.base_revision)
                    {
                        return Err(stale());
                    }
                    let outcome = match r.operation {
                        ContextOperation::ReadContext {} => {
                            let s = self.current(drafts, events, i, now)?;
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
                            }
                        }
                        ContextOperation::ReadInputChunk {
                            sequence,
                            expected_revision,
                            offset,
                            max_bytes,
                        } => {
                            if sequence != i.cursor.sequence
                                || expected_revision != i.cursor.base_revision
                            {
                                return Err(stale());
                            }
                            let chunk = events
                                .read_input_chunk(
                                    drafts,
                                    &self.event_mount,
                                    i.cursor,
                                    offset as usize,
                                    max_bytes as usize,
                                    now,
                                )
                                .map_err(application)?;
                            ContextOutcome::InputChunk {
                                offset: chunk.offset as u32,
                                total_bytes: chunk.total_bytes as u32,
                                text: chunk.text,
                            }
                        }
                    };
                    Ok(ComponentContextResponse {
                        api_version: HOST_API_V0_3,
                        context_protocol: COMPONENT_CONTEXT_PROTOCOL,
                        request_id: r.request_id,
                        resource: c.resource.clone(),
                        mount_id: c.mount_id.clone(),
                        sequence: i.cursor.sequence,
                        expected_revision: i.cursor.base_revision,
                        outcome,
                    }
                    .to_json()?
                    .into_bytes())
                },
            );
        }
        if let Ok(r) = DraftRequest::from_slice(raw) {
            return host.dispatch_draft(source, &mount, now, i.run_id.as_deref(), r.clone(), |c| {
                admit()?;
                if c.mount_id != self.mount.mount_id() || c.resource != *self.mount.resource() {
                    return Err(stale());
                }
                let DraftOperation::ReadChunk {
                    revision: Some(revision),
                    offset,
                    max_bytes,
                } = r.operation
                else {
                    return Err(ApiError::new(
                        ErrorCode::PermissionDenied,
                        "event tasks allow only pinned draft reads",
                    ));
                };
                self.validate_invocation(i)?;
                if revision != i.cursor.base_revision {
                    return Err(stale());
                }
                let chunk = drafts
                    .read_chunk(
                        &self.draft,
                        Some(revision),
                        offset as usize,
                        max_bytes as usize,
                    )
                    .map_err(application)?;
                if self.current(drafts, events, i, now)?.state != EventState::Active {
                    return Err(stale());
                }
                let Resource::Draft { draft_id, .. } = &c.resource else {
                    return Err(stale());
                };
                Ok(DraftResponse {
                    api_version: HOST_API_V0_3,
                    draft_protocol: DRAFT_PROTOCOL,
                    request_id: r.request_id,
                    draft_id: draft_id.clone(),
                    mount_id: c.mount_id.clone(),
                    outcome: DraftOutcome::Snapshot {
                        revision: chunk.revision,
                        offset: chunk.offset as u32,
                        total_bytes: chunk.total_bytes as u32,
                        text: chunk.text,
                    },
                }
                .to_json()?
                .into_bytes())
            });
        }
        let r = ContinuationRequest::from_slice(raw)?;
        host.dispatch_continuation(source, &mount, now, i.run_id.as_deref(), r.clone(), |c| {
            admit()?;
            self.authorized(c)?;
            self.validate_invocation(i)?;
            if r.sequence != i.cursor.sequence || r.base_revision != i.cursor.base_revision {
                return Err(stale());
            }
            // Each application operation validates before touching its clock.
            // Do not pre-read metadata: a bad CAS must not renew the event.
            // Completed replay is retained even though its base is now old.
            let outcome = match r.operation {
                ContinuationOperation::ReadMetadata {} => ContinuationOutcome::Metadata {
                    metadata: metadata(
                        events
                            .snapshot(drafts, &self.event_mount, i.cursor, now)
                            .map_err(application)?,
                    ),
                },
                ContinuationOperation::ReadCheckpointChunk {
                    checkpoint_revision,
                    offset,
                    max_bytes,
                } => {
                    let c = events
                        .read_checkpoint_chunk(
                            drafts,
                            &self.event_mount,
                            i.cursor,
                            checkpoint_revision,
                            offset as usize,
                            max_bytes as usize,
                            now,
                        )
                        .map_err(application)?;
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
                    let checkpoint = decode_checkpoint(
                        &checkpoint_base64,
                        extension_api::continuation::MAX_CHECKPOINT_BYTES,
                    )?;
                    admit()?;
                    self.validate_invocation(i)?;
                    events
                        .advance(
                            drafts,
                            &self.event_mount,
                            AdvanceEvent {
                                cursor: i.cursor,
                                expected_checkpoint_revision,
                                checkpoint,
                                effect: wire_effect.map(effect),
                                complete,
                            },
                            now,
                        )
                        .map_err(application)?;
                    ContinuationOutcome::Metadata {
                        metadata: metadata(
                            events
                                .snapshot(drafts, &self.event_mount, i.cursor, now)
                                .map_err(application)?,
                        ),
                    }
                }
                ContinuationOperation::CancelEvent {
                    expected_checkpoint_revision,
                } => {
                    admit()?;
                    self.validate_invocation(i)?;
                    events
                        .cancel_event(
                            drafts,
                            &self.event_mount,
                            i.cursor,
                            expected_checkpoint_revision,
                            now,
                        )
                        .map_err(application)?;
                    ContinuationOutcome::Metadata {
                        metadata: metadata(
                            events
                                .snapshot(drafts, &self.event_mount, i.cursor, now)
                                .map_err(application)?,
                        ),
                    }
                }
            };
            Ok(ContinuationResponse {
                api_version: HOST_API_V0_3,
                continuation_protocol: CONTINUATION_PROTOCOL,
                request_id: r.request_id,
                resource: c.resource.clone(),
                mount_id: c.mount_id.clone(),
                sequence: i.cursor.sequence,
                base_revision: i.cursor.base_revision,
                outcome,
            }
            .to_json()?
            .into_bytes())
        })
    }
}
