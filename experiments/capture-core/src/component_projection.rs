//! Shared read-only projection broker for headless and native qualification.
//!
//! This is coordination, not a paragraph parser or a runtime. The caller owns
//! invocation admission, serialization, scheduling and actual worker reap. The
//! explicit trusted constructor binds one Host mount to one application Draft.
//! No editor-event owner or upload can be reached through this service.
use core_api::{
    contract::sha256,
    knowledge::{KnowledgeError, KnowledgeErrorCode},
};
use extension_api::{
    ApiError, ErrorCode, HOST_API_V0_3,
    component_invocation::*,
    continuation::{MAX_CHECKPOINT_BYTES, decode_checkpoint, encode_checkpoint},
    draft::*,
    projection as wire,
};
use extension_host::{
    component::{AuthorizedComponentContext, MountHandle},
    durable::{ComponentAuthoritySource, DurableHost},
};
use learning_application::{
    annotation_draft::AnnotationDrafts, draft_text::DraftId, provider_projections as app,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_QUERY: AtomicU64 = AtomicU64::new(0);
fn error(code: ErrorCode, text: &'static str) -> ApiError {
    ApiError::new(code, text)
}
fn stale() -> ApiError {
    error(
        ErrorCode::StaleContext,
        "projection invocation changed or closed",
    )
}
fn application(e: KnowledgeError) -> ApiError {
    let code = match e.code {
        KnowledgeErrorCode::InvalidInput => ErrorCode::InvalidRequest,
        KnowledgeErrorCode::Conflict | KnowledgeErrorCode::NotFound => ErrorCode::StaleContext,
        KnowledgeErrorCode::BudgetExceeded => ErrorCode::BudgetExceeded,
        KnowledgeErrorCode::UnsupportedVersion => ErrorCode::UnsupportedVersion,
        _ => ErrorCode::InvalidState,
    };
    error(code, "projection application rejected")
}
fn bounded_u32(n: usize) -> Result<u32, ApiError> {
    n.try_into().map_err(|_| {
        error(
            ErrorCode::BudgetExceeded,
            "projection integer exceeds wire budget",
        )
    })
}
fn bounded_u16(n: usize) -> Result<u16, ApiError> {
    n.try_into().map_err(|_| {
        error(
            ErrorCode::BudgetExceeded,
            "projection integer exceeds wire budget",
        )
    })
}

/// The trusted coordinator captures this once for an actual invocation. It is
/// deliberately not deserializable, and mode/run/query cannot be filled by guest.
#[derive(Clone, Debug)]
pub struct ProjectionInvocation {
    scope: Arc<()>,
    query_id: String,
    generation: u64,
    base_revision: u64,
    run_id: Option<String>,
}
struct Query {
    token: app::QueryToken,
    context: wire::ProjectionContext,
}
pub struct ProjectionServices {
    scope: Arc<()>,
    mount: MountHandle,
    draft: DraftId,
    projections: app::ProviderProjections,
    app_mount: app::ProjectionMount,
    query: Option<Query>,
    closed: bool,
}
impl ProjectionServices {
    /// Only a trusted composition root may associate the opaque application
    /// Draft with this explicitly approved Host resource. This is not a grant.
    pub fn new(
        drafts: &AnnotationDrafts,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        mount: &MountHandle,
        draft: DraftId,
        now: u64,
    ) -> Result<Self, ApiError> {
        if !mount.descriptor().supports_paragraph_projection() {
            return Err(error(
                ErrorCode::UnsupportedVersion,
                "signed provider lacks paragraph projection support",
            ));
        }
        if drafts.source(&draft).map_err(application)?.0.vault() != mount.resource().vault_id() {
            return Err(error(
                ErrorCode::PermissionDenied,
                "application draft differs from mounted vault",
            ));
        }
        host.validate_component_current(source, mount, now, None)?;
        let mut projections = app::ProviderProjections::new();
        let app_mount = projections
            .register(drafts, draft.clone())
            .map_err(application)?;
        Ok(Self {
            scope: Arc::new(()),
            mount: mount.clone(),
            draft,
            projections,
            app_mount,
            query: None,
            closed: false,
        })
    }
    /// Starts/replaces only the read-only query. The outer scheduler must retire
    /// the previous worker before starting its successor; no reap is inferred.
    #[allow(clippy::too_many_arguments)] // Exact trusted viewport and authority are supplied together.
    pub fn start(
        &mut self,
        drafts: &AnnotationDrafts,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        base_revision: u64,
        first_paragraph: u32,
        max_rows: u8,
        now: u64,
    ) -> Result<wire::ProjectionContext, ApiError> {
        if self.closed {
            return Err(stale());
        }
        host.validate_component_current(source, &self.mount, now, None)?;
        let serial = NEXT_QUERY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| {
                error(
                    ErrorCode::BudgetExceeded,
                    "projection query identity exhausted",
                )
            })?;
        // Host mount identity is itself non-reused. The checked process sequence
        // additionally separates multiple services constructed for the same mount.
        let id = sha256(
            format!(
                "qusor-projection-query/v1:{}:{serial}",
                self.mount.mount_id()
            )
            .as_bytes(),
        )[..32]
            .to_owned();
        let token = self
            .projections
            .start(
                drafts,
                &self.app_mount,
                base_revision,
                first_paragraph,
                max_rows,
                now,
            )
            .map_err(application)?;
        let metadata = self
            .projections
            .metadata(drafts, &token, now)
            .map_err(application)?;
        let context = wire::ProjectionContext {
            resource: self.mount.resource().clone(),
            mount_id: self.mount.mount_id().into(),
            query_id: id,
            base_revision,
            query_generation: metadata.context.generation,
            first_paragraph,
            max_rows,
            total_bytes: bounded_u32(metadata.context.total_bytes)?,
        };
        context.validate()?;
        self.query = Some(Query {
            token,
            context: context.clone(),
        });
        Ok(context)
    }
    pub fn invocation(&self, run_id: Option<&str>) -> Result<ProjectionInvocation, ApiError> {
        if self.closed {
            return Err(stale());
        }
        let q = self.query.as_ref().ok_or_else(stale)?;
        Ok(ProjectionInvocation {
            scope: self.scope.clone(),
            query_id: q.context.query_id.clone(),
            generation: q.context.query_generation,
            base_revision: q.context.base_revision,
            run_id: run_id.map(str::to_owned),
        })
    }
    fn validate_invocation(&self, invocation: &ProjectionInvocation) -> Result<(), ApiError> {
        let q = self.query.as_ref().ok_or_else(stale)?;
        if self.closed
            || !Arc::ptr_eq(&self.scope, &invocation.scope)
            || invocation.query_id != q.context.query_id
            || invocation.generation != q.context.query_generation
            || invocation.base_revision != q.context.base_revision
        {
            return Err(stale());
        }
        Ok(())
    }
    fn validate_authorized(&self, authorized: &AuthorizedComponentContext) -> Result<(), ApiError> {
        if authorized.mount_id != self.mount.mount_id()
            || authorized.resource != *self.mount.resource()
        {
            return Err(stale());
        }
        Ok(())
    }
    fn current(
        &mut self,
        drafts: &AnnotationDrafts,
        now: u64,
    ) -> Result<app::ProjectionSnapshot, ApiError> {
        let q = self.query.as_ref().ok_or_else(stale)?;
        let snapshot = self
            .projections
            .metadata(drafts, &q.token, now)
            .map_err(application)?;
        if !matches!(
            snapshot.state,
            app::ProjectionState::Active | app::ProjectionState::Completed
        ) {
            return Err(stale());
        }
        Ok(snapshot)
    }
    /// Trusted query cancellation only. Pending editing input/uploads are not
    /// available here. Native cancellation remains possible after grant revocation.
    pub fn cancel(&mut self, drafts: &AnnotationDrafts, now: u64) -> Result<(), ApiError> {
        if self.closed {
            return Err(stale());
        }
        if let Some(q) = self.query.take() {
            let metadata = self
                .projections
                .metadata(drafts, &q.token, now)
                .map_err(application)?;
            if metadata.state == app::ProjectionState::Active {
                self.projections
                    .cancel(drafts, &q.token, metadata.checkpoint_revision, now)
                    .map_err(application)?;
            }
        }
        self.query = None;
        Ok(())
    }
    /// Close this projection service, not the shared editing Host mount. Actual
    /// mount revocation and native/worker release are the caller's separate work.
    pub fn close(&mut self) -> Result<(), ApiError> {
        if !self.closed {
            self.projections
                .close(&self.app_mount)
                .map_err(application)?;
        }
        self.closed = true;
        self.query = None;
        Ok(())
    }
    /// Each call uses its immutable trusted invocation binding and a fresh Host
    /// callback. `admit` must represent this exact run/call; never an old permit.
    #[allow(clippy::too_many_arguments)] // Explicit application, fresh authority and live-run gate.
    pub fn call(
        &mut self,
        drafts: &AnnotationDrafts,
        host: &mut DurableHost,
        source: &dyn ComponentAuthoritySource,
        invocation: &ProjectionInvocation,
        raw: &[u8],
        now: u64,
        admit: &impl Fn() -> Result<(), ApiError>,
    ) -> Result<Vec<u8>, ApiError> {
        if raw.len() > wire::MAX_PROJECTION_REQUEST_BYTES {
            return Err(error(
                ErrorCode::BudgetExceeded,
                "projection request exceeds budget",
            ));
        }
        self.validate_invocation(invocation)?;
        let mount = self.mount.clone();
        if let Ok(request) = ComponentInvocationRequest::from_slice(raw) {
            host.dispatch_component_invocation(
                source,
                &mount,
                now,
                invocation.run_id.as_deref(),
                request.clone(),
                |authorized| {
                    admit()?;
                    self.validate_invocation(invocation)?;
                    self.validate_authorized(authorized)?;
                    self.current(drafts, now)?;
                    Ok(ComponentInvocationResponse {
                        api_version: HOST_API_V0_3,
                        component_invocation_protocol: COMPONENT_INVOCATION_PROTOCOL,
                        request_id: request.request_id,
                        context: ComponentInvocationContext::ParagraphProjection {},
                    }
                    .to_json()?
                    .into_bytes())
                },
            )
        } else if let Ok(request) = wire::ProjectionRequest::from_slice(raw) {
            host.dispatch_projection(
                source,
                &mount,
                now,
                invocation.run_id.as_deref(),
                request.clone(),
                |authorized| {
                    admit()?;
                    self.validate_invocation(invocation)?;
                    self.validate_authorized(authorized)?;
                    self.projection(drafts, request, now)
                },
            )
        } else if let Ok(request) = DraftRequest::from_slice(raw) {
            let copy = request.clone();
            host.dispatch_draft(
                source,
                &mount,
                now,
                invocation.run_id.as_deref(),
                request,
                |_authorized| {
                    admit()?;
                    self.validate_invocation(invocation)?;
                    let DraftOperation::ReadChunk {
                        revision: Some(revision),
                        offset,
                        max_bytes,
                    } = copy.operation
                    else {
                        return Err(error(
                            ErrorCode::PermissionDenied,
                            "projection forbids draft writes and unpinned reads",
                        ));
                    };
                    let q = self.query.as_ref().ok_or_else(stale)?;
                    if revision != q.context.base_revision {
                        return Err(stale());
                    }
                    self.current(drafts, now)?;
                    let chunk = drafts
                        .read_chunk(
                            &self.draft,
                            Some(revision),
                            offset as usize,
                            usize::from(max_bytes),
                        )
                        .map_err(application)?;
                    let extension_api::Resource::Draft { draft_id, .. } = self.mount.resource()
                    else {
                        unreachable!()
                    };
                    Ok(DraftResponse {
                        api_version: HOST_API_V0_3,
                        draft_protocol: DRAFT_PROTOCOL,
                        request_id: copy.request_id,
                        draft_id: draft_id.clone(),
                        mount_id: self.mount.mount_id().into(),
                        outcome: DraftOutcome::Snapshot {
                            revision: chunk.revision,
                            offset: bounded_u32(chunk.offset)?,
                            total_bytes: bounded_u32(chunk.total_bytes)?,
                            text: chunk.text,
                        },
                    }
                    .to_json()?
                    .into_bytes())
                },
            )
        } else {
            // Legacy edit Context/Continuation cannot probe a different purpose.
            Err(error(
                ErrorCode::PermissionDenied,
                "request is not part of read-only projection",
            ))
        }
    }
    fn projection(
        &mut self,
        drafts: &AnnotationDrafts,
        request: wire::ProjectionRequest,
        now: u64,
    ) -> Result<Vec<u8>, ApiError> {
        match request {
            wire::ProjectionRequest::ReadContext { request_id, .. } => {
                self.current(drafts, now)?;
                let context = self.query.as_ref().ok_or_else(stale)?.context.clone();
                Ok(wire::ProjectionResponse::Context {
                    api_version: HOST_API_V0_3,
                    projection_protocol: wire::PROJECTION_PROTOCOL,
                    request_id,
                    context,
                }
                .to_json()?
                .into_bytes())
            }
            wire::ProjectionRequest::Query {
                request_id,
                resource,
                mount_id,
                query_id,
                base_revision,
                operation,
                ..
            } => {
                let q = self.query.as_ref().ok_or_else(stale)?;
                if query_id != q.context.query_id
                    || base_revision != q.context.base_revision
                    || mount_id != q.context.mount_id
                    || resource != q.context.resource
                {
                    return Err(stale());
                }
                let context = q.context.clone();
                let outcome = match operation {
                    wire::ProjectionOperation::ReadMetadata {} => {
                        let snapshot = self.current(drafts, now)?;
                        wire::ProjectionOutcome::Metadata {
                            metadata: metadata(snapshot, &context)?,
                        }
                    }
                    wire::ProjectionOperation::ReadCheckpointChunk {
                        checkpoint_revision,
                        offset,
                        max_bytes,
                    } => {
                        let chunk = self
                            .projections
                            .read_checkpoint(
                                drafts,
                                &q.token,
                                checkpoint_revision,
                                usize::from(offset),
                                usize::from(max_bytes),
                                now,
                            )
                            .map_err(application)?;
                        wire::ProjectionOutcome::CheckpointChunk {
                            checkpoint_revision: chunk.checkpoint_revision,
                            offset: bounded_u16(chunk.offset)?,
                            total_bytes: bounded_u16(chunk.total_bytes)?,
                            checkpoint_base64: encode_checkpoint(&chunk.bytes)?,
                        }
                    }
                    wire::ProjectionOperation::AdvanceProjection {
                        expected_checkpoint_revision,
                        checkpoint_base64,
                        window,
                        complete,
                    } => {
                        if let Some(w) = &window {
                            w.validate_for_context(&context)?;
                        }
                        let snapshot = self
                            .projections
                            .advance(
                                drafts,
                                &q.token,
                                app::AdvanceProjection {
                                    expected_checkpoint_revision,
                                    checkpoint: decode_checkpoint(
                                        &checkpoint_base64,
                                        MAX_CHECKPOINT_BYTES,
                                    )?,
                                    window: window.map(|w| app::ProjectionWindow {
                                        reached_eof: w.reached_eof,
                                        rows: w
                                            .rows
                                            .into_iter()
                                            .map(|r| app::RowSpan {
                                                ordinal: r.ordinal,
                                                start: r.start as usize,
                                                end: r.end as usize,
                                                complete: r.complete,
                                            })
                                            .collect(),
                                    }),
                                    complete,
                                },
                                now,
                            )
                            .map_err(application)?;
                        wire::ProjectionOutcome::Metadata {
                            metadata: metadata(snapshot, &context)?,
                        }
                    }
                    wire::ProjectionOperation::Cancel {
                        expected_checkpoint_revision,
                    } => {
                        let snapshot = self
                            .projections
                            .cancel(drafts, &q.token, expected_checkpoint_revision, now)
                            .map_err(application)?;
                        wire::ProjectionOutcome::Metadata {
                            metadata: metadata(snapshot, &context)?,
                        }
                    }
                };
                if let wire::ProjectionOutcome::Metadata { metadata } = &outcome {
                    metadata.validate_for_context(&context)?;
                }
                Ok(wire::ProjectionResponse::Query {
                    api_version: HOST_API_V0_3,
                    projection_protocol: wire::PROJECTION_PROTOCOL,
                    request_id,
                    resource: context.resource,
                    mount_id: context.mount_id,
                    query_id: context.query_id,
                    base_revision: context.base_revision,
                    outcome,
                }
                .to_json()?
                .into_bytes())
            }
        }
    }
}
fn metadata(
    snapshot: app::ProjectionSnapshot,
    context: &wire::ProjectionContext,
) -> Result<wire::ProjectionMetadata, ApiError> {
    if snapshot.context.base_revision != context.base_revision
        || snapshot.context.generation != context.query_generation
        || snapshot.context.first_paragraph != context.first_paragraph
        || snapshot.context.max_rows != context.max_rows
        || bounded_u32(snapshot.context.total_bytes)? != context.total_bytes
    {
        return Err(stale());
    }
    let metadata = wire::ProjectionMetadata {
        checkpoint_revision: snapshot.checkpoint_revision,
        checkpoint_bytes: bounded_u16(snapshot.checkpoint_bytes)?,
        state: match snapshot.state {
            app::ProjectionState::Active => wire::ProjectionState::Active,
            app::ProjectionState::Completed => wire::ProjectionState::Completed,
            app::ProjectionState::Cancelled => wire::ProjectionState::Cancelled,
            app::ProjectionState::Invalidated => wire::ProjectionState::Invalidated,
            app::ProjectionState::Expired => wire::ProjectionState::Expired,
        },
        window: snapshot
            .window
            .map(|w| {
                Ok::<_, ApiError>(wire::ProjectionWindow {
                    reached_eof: w.reached_eof,
                    rows: w
                        .rows
                        .into_iter()
                        .map(|r| {
                            Ok(wire::RowSpan {
                                ordinal: r.ordinal,
                                start: bounded_u32(r.start)?,
                                end: bounded_u32(r.end)?,
                                complete: r.complete,
                            })
                        })
                        .collect::<Result<_, ApiError>>()?,
                })
            })
            .transpose()?,
        window_ready: snapshot.window_ready,
    };
    metadata.validate_for_context(context)?;
    Ok(metadata)
}
