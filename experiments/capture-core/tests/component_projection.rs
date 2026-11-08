//! Synthetic Core and memory installation authority only; not a signed-package,
//! real Store, process, native gesture or resource-release qualification.
#[path = "component_support/fixture.rs"]
mod fixture;
use extension_api::{
    component_invocation::*, composition::*, continuation::encode_checkpoint, draft::*,
    projection::*, *,
};
use extension_host::{component::MountHandle, durable::*};
use learning_application::{
    annotation_draft::AnnotationDrafts, draft_text::DraftId, provider_events::*,
};
use qusor_capture_probe::component_projection::{ProjectionInvocation, ProjectionServices};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn native_binding_rejects_a_draft_from_another_vault_without_fencing_current_mount() {
    let mut w = World::new();
    let scratch = Scratch::new();
    let mut app =
        learning_application::KnowledgeApplication::new(SqliteStore::open(&scratch.0).unwrap());
    let (original, snapshot, selection) = fixture::pdf_fixture();
    app.import_document(
        "other-source",
        "other-vault",
        "Other source",
        &original,
        &snapshot,
    )
    .unwrap();
    let mut drafts = AnnotationDrafts::new();
    let other = drafts
        .create_pdf(
            &app,
            1,
            "other-vault",
            selection,
            core_api::knowledge::Provenance {
                package_id: "ordinary.source".into(),
                publisher_id: "ordinary.author".into(),
                package_digest: "c".repeat(64),
                installation_id: 3,
            },
            1,
        )
        .unwrap();
    assert!(
        matches!(ProjectionServices::new(&drafts, &mut w.host, &w.source, &w.mount, other, 0), Err(e) if e.code == ErrorCode::PermissionDenied)
    );
    let token = w.token();
    let request = w
        .query(ProjectionOperation::ReadMetadata {})
        .to_json()
        .unwrap();
    w.raw(&token, request.as_bytes()).unwrap();
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "qusor-projection-broker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Source {
    snapshot: RefCell<ComponentInstallation>,
    unavailable: Cell<bool>,
    reads: Cell<usize>,
}
impl ComponentAuthoritySource for Source {
    fn component_installation(
        &self,
        package: &str,
        read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError> {
        self.reads.set(self.reads.get() + 1);
        if let AuthorityRead::Current { run_id: Some(run) } = read
            && run != "memory-run-1"
        {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "memory own-run mismatch",
            ));
        }
        if self.unavailable.get() || package != self.snapshot.borrow().manifest.package_id {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "memory authority unavailable",
            ));
        }
        Ok(self.snapshot.borrow().clone())
    }
}
fn facts(protocols: bool) -> ComponentInstallation {
    ComponentInstallation {
        installation_id: 1,
        revision: 1,
        generation: 1,
        digest: "a".repeat(64),
        vault_id: "vault".into(),
        manifest: Manifest {
            manifest_version: 1,
            package_id: "org.test.projection".into(),
            publisher_id: "org.test".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 3,
                max_minor: 3,
            },
            required_capabilities: vec![Capability::NoteDraftEdit],
            optional_capabilities: vec![],
            contributions: vec![Contribution {
                id: "org.test.projection.editor".into(),
                title: "Editor".into(),
                kind: ContributionKind::ComponentProvider,
                required_capabilities: vec![Capability::NoteDraftEdit],
            }],
        },
        composition: CompositionDescriptor {
            composition_protocol: 1,
            api_version: HOST_API_V0_3,
            contribution_id: "org.test.projection.editor".into(),
            role: ComponentRole::AnnotationPlainTextEditor,
            contract: ANNOTATION_EDITOR_CONTRACT.into(),
            view: NativeEditorView::ParagraphEditor {
                label: "Thought".into(),
                placeholder: "Write".into(),
                protocols: protocols.then_some(ParagraphProtocols {
                    component_invocation: 1,
                    paragraph_projection: 1,
                }),
            },
        },
        composition_sha256: "b".repeat(64),
    }
}
struct World {
    host: DurableHost,
    source: Source,
    mount: MountHandle,
    drafts: AnnotationDrafts,
    draft: DraftId,
    services: ProjectionServices,
    context: ProjectionContext,
    _scratch: Scratch,
}
impl World {
    fn new() -> Self {
        let scratch = Scratch::new();
        let mut app =
            learning_application::KnowledgeApplication::new(SqliteStore::open(&scratch.0).unwrap());
        let (original, snapshot, selection) = fixture::pdf_fixture();
        app.import_document("source", "vault", "Synthetic source", &original, &snapshot)
            .unwrap();
        let mut drafts = AnnotationDrafts::new();
        let draft = drafts
            .create_pdf(
                &app,
                1,
                "vault",
                selection,
                core_api::knowledge::Provenance {
                    package_id: "ordinary.capture".into(),
                    publisher_id: "ordinary.author".into(),
                    package_digest: "c".repeat(64),
                    installation_id: 3,
                },
                1,
            )
            .unwrap();
        let owner = drafts.create_owner().unwrap();
        let upload = drafts
            .begin_replace(&owner, &draft, 1, "a\r\n中🙂\0\n".len(), 0)
            .unwrap();
        drafts
            .append_chunk(&owner, &upload, 0, "a\r\n中🙂\0\n", 0)
            .unwrap();
        drafts.commit_replace(&owner, &upload, 0).unwrap();
        drafts.revoke_owner(&owner).unwrap();
        let source = Source {
            snapshot: RefCell::new(facts(true)),
            unavailable: Cell::new(false),
            reads: Cell::new(0),
        };
        let mut host = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
        host.configure_component_identity([93; 16]).unwrap();
        let mounted = host
            .mount_component(
                &source,
                UserMountIntent::new(
                    source.snapshot.borrow().clone(),
                    Resource::Draft {
                        vault_id: "vault".into(),
                        draft_id: "d".repeat(32),
                    },
                )
                .unwrap(),
                0,
            )
            .unwrap();
        let mount = mounted.handle().clone();
        let mut services =
            ProjectionServices::new(&drafts, &mut host, &source, &mount, draft.clone(), 0).unwrap();
        let context = services
            .start(&drafts, &mut host, &source, 2, 0, 2, 0)
            .unwrap();
        Self {
            host,
            source,
            mount,
            drafts,
            draft,
            services,
            context,
            _scratch: scratch,
        }
    }
    fn token(&self) -> ProjectionInvocation {
        self.services.invocation(Some("memory-run-1")).unwrap()
    }
    fn raw(&mut self, token: &ProjectionInvocation, raw: &[u8]) -> Result<Vec<u8>, ApiError> {
        self.services.call(
            &self.drafts,
            &mut self.host,
            &self.source,
            token,
            raw,
            0,
            &|| Ok(()),
        )
    }
    fn query(&self, operation: ProjectionOperation) -> ProjectionRequest {
        ProjectionRequest::Query {
            api_version: HOST_API_V0_3,
            projection_protocol: 1,
            request_id: "query".into(),
            resource: self.context.resource.clone(),
            mount_id: self.context.mount_id.clone(),
            query_id: self.context.query_id.clone(),
            base_revision: 2,
            operation,
        }
    }
    fn metadata(&mut self, token: &ProjectionInvocation) -> ProjectionMetadata {
        let bytes = self
            .raw(
                token,
                self.query(ProjectionOperation::ReadMetadata {})
                    .to_json()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        let ProjectionResponse::Query {
            outcome: ProjectionOutcome::Metadata { metadata },
            ..
        } = ProjectionResponse::from_slice(&bytes).unwrap()
        else {
            panic!("metadata expected")
        };
        metadata
    }
}
fn advance(cas: u64, window: Option<ProjectionWindow>, complete: bool) -> ProjectionOperation {
    ProjectionOperation::AdvanceProjection {
        expected_checkpoint_revision: cas,
        checkpoint_base64: encode_checkpoint(&[0, 9, 255]).unwrap(),
        window,
        complete,
    }
}

#[test]
fn own_bootstrap_context_partial_window_cas_and_source_chunks() {
    let mut w = World::new();
    let token = w.token();
    let bootstrap = ComponentInvocationRequest {
        api_version: HOST_API_V0_3,
        component_invocation_protocol: 1,
        request_id: "purpose".into(),
        operation: ComponentInvocationOperation::ReadContext {},
    };
    let bytes = w
        .raw(&token, bootstrap.to_json().unwrap().as_bytes())
        .unwrap();
    assert_eq!(
        ComponentInvocationResponse::from_slice(&bytes)
            .unwrap()
            .context,
        ComponentInvocationContext::ParagraphProjection {}
    );
    let read = ProjectionRequest::ReadContext {
        api_version: HOST_API_V0_3,
        projection_protocol: 1,
        request_id: "own".into(),
    };
    let bytes = w.raw(&token, read.to_json().unwrap().as_bytes()).unwrap();
    assert!(
        matches!(ProjectionResponse::from_slice(&bytes).unwrap(), ProjectionResponse::Context { context, .. } if context==w.context)
    );
    let partial = ProjectionWindow {
        rows: vec![
            RowSpan {
                ordinal: 0,
                start: 0,
                end: 1,
                complete: true,
            },
            RowSpan {
                ordinal: 1,
                start: 3,
                end: 6,
                complete: false,
            },
        ],
        reached_eof: false,
    };
    let request = w.query(advance(1, Some(partial), false)).to_json().unwrap();
    let first = w.raw(&token, request.as_bytes()).unwrap();
    assert_eq!(w.raw(&token, request.as_bytes()).unwrap(), first);
    let metadata = w.metadata(&token);
    assert_eq!(metadata.checkpoint_revision, 2);
    assert!(!metadata.window_ready);
    let chunk = w
        .query(ProjectionOperation::ReadCheckpointChunk {
            checkpoint_revision: 2,
            offset: 0,
            max_bytes: 2048,
        })
        .to_json()
        .unwrap();
    let response = w.raw(&token, chunk.as_bytes()).unwrap();
    assert!(
        matches!(ProjectionResponse::from_slice(&response).unwrap(), ProjectionResponse::Query { outcome: ProjectionOutcome::CheckpointChunk { checkpoint_base64, .. }, .. } if checkpoint_base64==encode_checkpoint(&[0,9,255]).unwrap())
    );
    let source = DraftRequest {
        api_version: HOST_API_V0_3,
        draft_protocol: 1,
        request_id: "source".into(),
        resource: w.context.resource.clone(),
        mount_id: w.context.mount_id.clone(),
        operation: DraftOperation::ReadChunk {
            revision: Some(2),
            offset: 3,
            max_bytes: 512,
        },
    };
    let bytes = w.raw(&token, source.to_json().unwrap().as_bytes()).unwrap();
    assert!(
        matches!(DraftResponse::from_slice(&bytes).unwrap().outcome, DraftOutcome::Snapshot { text, revision:2, .. } if text=="中🙂\0\n")
    );
}

#[test]
fn readonly_dispatch_refuses_writes_legacy_context_and_dead_invocation() {
    let mut w = World::new();
    let token = w.token();
    let before = w.metadata(&token);
    let request = w.query(advance(1, None, false)).to_json().unwrap();
    let reads = w.source.reads.get();
    assert!(
        w.services
            .call(
                &w.drafts,
                &mut w.host,
                &w.source,
                &token,
                request.as_bytes(),
                100,
                &|| Err(ApiError::new(ErrorCode::StaleContext, "not live"))
            )
            .is_err()
    );
    assert!(w.source.reads.get() > reads); // admission occurs inside a fresh authorized callback
    assert_eq!(w.metadata(&token), before);
    for operation in [
        DraftOperation::BeginReplace {
            expected_revision: 2,
            total_bytes: 0,
        },
        DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 512,
        },
    ] {
        let request = DraftRequest {
            api_version: HOST_API_V0_3,
            draft_protocol: 1,
            request_id: "bad".into(),
            resource: w.context.resource.clone(),
            mount_id: w.context.mount_id.clone(),
            operation,
        };
        assert!(
            w.raw(&token, request.to_json().unwrap().as_bytes())
                .is_err()
        );
    }
    let continuation = extension_api::continuation::ContinuationRequest {
        api_version: HOST_API_V0_3,
        continuation_protocol: 1,
        request_id: "edit-cp".into(),
        resource: w.context.resource.clone(),
        mount_id: w.context.mount_id.clone(),
        sequence: 1,
        base_revision: 2,
        operation: extension_api::continuation::ContinuationOperation::ReadMetadata {},
    };
    assert!(
        w.raw(&token, continuation.to_json().unwrap().as_bytes())
            .is_err()
    );
    let context = ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "edit".into(),
        operation: ContextOperation::ReadContext {},
    };
    assert!(
        w.raw(&token, context.to_json().unwrap().as_bytes())
            .is_err()
    );
    let injected = serde_json::json!({"api_version":{"major":0,"minor":3},"component_invocation_protocol":1,"request_id":"purpose","operation":{"kind":"read_context"},"mode":"edit_event"});
    assert!(
        w.raw(&token, &serde_json::to_vec(&injected).unwrap())
            .is_err()
    );
    assert_eq!(w.metadata(&token), before);
    assert_eq!(
        w.drafts.read_chunk(&w.draft, Some(2), 0, 512).unwrap().text,
        "a\r\n中🙂\0\n"
    );
}

#[test]
fn query_replacement_foreign_tokens_current_revision_and_closed_mount_reject() {
    let mut w = World::new();
    let old = w.token();
    let old_request = w
        .query(ProjectionOperation::ReadMetadata {})
        .to_json()
        .unwrap();
    let context = w
        .services
        .start(&w.drafts, &mut w.host, &w.source, 2, 1, 2, 0)
        .unwrap();
    assert_ne!(context.query_id, w.context.query_id);
    w.context = context;
    assert!(w.raw(&old, old_request.as_bytes()).is_err());
    let token = w.token();
    assert!(w.raw(&token, old_request.as_bytes()).is_err());
    let other = World::new();
    assert!(
        w.raw(
            &other.token(),
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
    let owner = w.drafts.create_owner().unwrap();
    let upload = w.drafts.begin_replace(&owner, &w.draft, 2, 3, 0).unwrap();
    w.drafts.append_chunk(&owner, &upload, 0, "new", 0).unwrap();
    w.drafts.commit_replace(&owner, &upload, 0).unwrap();
    assert!(
        w.raw(
            &token,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
    w.services.close().unwrap();
    assert!(w.raw(&token, old_request.as_bytes()).is_err());

    let mut w = World::new();
    let token = w.token();
    w.host.close_component(&w.mount).unwrap();
    assert!(
        w.raw(
            &token,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
    let mut w = World::new();
    let token = w.token();
    w.source.unavailable.set(true);
    assert!(
        w.raw(
            &token,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn cancel_and_restart_projection_leave_active_edit_input_checkpoint_and_upload() {
    let mut w = World::new();
    let mut edits = ProviderEvents::new();
    let mount = edits.register(&mut w.drafts, w.draft.clone()).unwrap();
    let event = edits
        .publish(
            &mut w.drafts,
            &mount,
            2,
            EventAction::ReplaceParagraph { index: 0 },
            "pending🙂\0".into(),
            0,
        )
        .unwrap();
    edits
        .advance(
            &mut w.drafts,
            &mount,
            AdvanceEvent {
                cursor: event.cursor,
                expected_checkpoint_revision: 1,
                effect: Some(DraftEffect::BeginReplace {
                    expected_revision: 2,
                    total_bytes: 12,
                }),
                checkpoint: vec![7, 0, 255],
                complete: false,
            },
            0,
        )
        .unwrap();
    let reserved = w.drafts.reserved_text_bytes();
    let old = w.token();
    w.services.cancel(&w.drafts, 0).unwrap();
    assert!(
        w.raw(
            &old,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
    w.context = w
        .services
        .start(&w.drafts, &mut w.host, &w.source, 2, 0, 2, 0)
        .unwrap();
    let token = w.token();
    w.metadata(&token);
    assert_eq!(w.drafts.reserved_text_bytes(), reserved);
    assert_eq!(
        edits
            .read_input_chunk(&mut w.drafts, &mount, event.cursor, 0, 512, 0)
            .unwrap()
            .text,
        "pending🙂\0"
    );
    assert_eq!(
        edits
            .read_checkpoint_chunk(&mut w.drafts, &mount, event.cursor, 2, 0, 2048, 0)
            .unwrap()
            .bytes,
        vec![7, 0, 255]
    );
    assert_eq!(
        edits
            .snapshot(&mut w.drafts, &mount, event.cursor, 0)
            .unwrap()
            .state,
        EventState::Active
    );
    w.services.close().unwrap();
    assert_eq!(
        edits
            .snapshot(&mut w.drafts, &mount, event.cursor, 0)
            .unwrap()
            .state,
        EventState::Active
    );
}

#[test]
fn absent_signed_support_and_deleted_application_draft_never_start_or_read() {
    let w = World::new();
    let source = Source {
        snapshot: RefCell::new(facts(false)),
        unavailable: Cell::new(false),
        reads: Cell::new(0),
    };
    let mut host = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
    host.configure_component_identity([94; 16]).unwrap();
    let mounted = host
        .mount_component(
            &source,
            UserMountIntent::new(
                source.snapshot.borrow().clone(),
                Resource::Draft {
                    vault_id: "vault".into(),
                    draft_id: "e".repeat(32),
                },
            )
            .unwrap(),
            0,
        )
        .unwrap();
    assert!(
        matches!(ProjectionServices::new(&w.drafts, &mut host, &source, mounted.handle(), w.draft.clone(), 0), Err(e) if e.code==ErrorCode::UnsupportedVersion)
    );
    let mut w = World::new();
    let token = w.token();
    w.drafts.remove(&w.draft).unwrap();
    assert!(
        w.raw(
            &token,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
    w.services.close().unwrap();
    let mut w = World::new();
    let wrong_run = w.services.invocation(Some("foreign-run")).unwrap();
    assert!(
        w.raw(
            &wrong_run,
            w.query(ProjectionOperation::ReadMetadata {})
                .to_json()
                .unwrap()
                .as_bytes()
        )
        .is_err()
    );
}
