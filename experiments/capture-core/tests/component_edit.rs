//! Memory installation authority and real application state. No guest/process/native qualification.
#[path = "component_support/fixture.rs"]
mod fixture;
use extension_api::{component_invocation::*, composition::*, continuation::*, draft::*, *};
use extension_host::{component::MountHandle, durable::*};
use learning_application::{annotation_draft::AnnotationDrafts, provider_events::*};
use qusor_capture_probe::component_edit::{EditInvocation, EditServices};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;
static NEXT: AtomicU64 = AtomicU64::new(0);
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
    events: ProviderEvents,
    service: EditServices,
    cursor: EventCursor,
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
            .begin_replace(&owner, &draft, 1, "🙂a".len(), 0)
            .unwrap();
        drafts.append_chunk(&owner, &upload, 0, "🙂a", 0).unwrap();
        drafts.commit_replace(&owner, &upload, 0).unwrap();
        drafts.revoke_owner(&owner).unwrap();
        let source = Source {
            snapshot: RefCell::new(facts(true)),
            unavailable: Cell::new(false),
            reads: Cell::new(0),
        };
        let mut host = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
        host.configure_component_identity([92; 16]).unwrap();
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
        let mut events = ProviderEvents::new();
        let mut service = EditServices::new(
            &mut drafts,
            &mut events,
            &mut host,
            &source,
            &mount,
            draft,
            0,
        )
        .unwrap();
        let cursor = service
            .publish(
                &mut drafts,
                &mut events,
                &mut host,
                &source,
                2,
                EventAction::InputChanged,
                "🙂x".into(),
                0,
            )
            .unwrap()
            .cursor;
        Self {
            host,
            source,
            mount,
            drafts,
            events,
            service,
            cursor,
            _scratch: scratch,
        }
    }
    fn token(&self) -> EditInvocation {
        self.service.invocation(Some("memory-run-1")).unwrap()
    }
    fn call(&mut self, i: &EditInvocation, raw: &str, now: u64) -> Result<Vec<u8>, ApiError> {
        self.service.call(
            &mut self.drafts,
            &mut self.events,
            &mut self.host,
            &self.source,
            i,
            raw.as_bytes(),
            now,
            &|| Ok(()),
        )
    }
    fn continuation(&self, operation: ContinuationOperation) -> String {
        ContinuationRequest {
            api_version: HOST_API_V0_3,
            continuation_protocol: 1,
            request_id: "edit".into(),
            resource: self.mount.resource().clone(),
            mount_id: self.mount.mount_id().into(),
            sequence: self.cursor.sequence,
            base_revision: self.cursor.base_revision,
            operation,
        }
        .to_json()
        .unwrap()
    }
    fn state(&mut self, i: &EditInvocation, now: u64) -> ContinuationState {
        let raw = self.continuation(ContinuationOperation::ReadMetadata {});
        let b = self.call(i, &raw, now).unwrap();
        let r = ContinuationResponse::from_slice(&b).unwrap();
        let ContinuationOutcome::Metadata { metadata } = r.outcome else {
            panic!("metadata")
        };
        metadata.state
    }
}
#[test]
fn invalid_input_and_source_offsets_do_not_renew_activity() {
    for source in [false, true] {
        for offset in [1, 99] {
            let mut w = World::new();
            let i = w.token();
            let raw = if source {
                DraftRequest {
                    api_version: HOST_API_V0_3,
                    draft_protocol: 1,
                    request_id: "read".into(),
                    resource: w.mount.resource().clone(),
                    mount_id: w.mount.mount_id().into(),
                    operation: DraftOperation::ReadChunk {
                        revision: Some(2),
                        offset,
                        max_bytes: 128,
                    },
                }
                .to_json()
                .unwrap()
            } else {
                ComponentContextRequest {
                    api_version: HOST_API_V0_3,
                    context_protocol: 1,
                    request_id: "read".into(),
                    operation: ContextOperation::ReadInputChunk {
                        sequence: w.cursor.sequence,
                        expected_revision: 2,
                        offset,
                        max_bytes: 128,
                    },
                }
                .to_json()
                .unwrap()
            };
            assert!(w.call(&i, &raw, 29).is_err());
            assert_eq!(w.state(&i, 30), ContinuationState::Expired);
        }
    }
}
#[test]
fn denied_admission_has_no_effect_or_clock_renewal() {
    let mut w = World::new();
    let i = w.token();
    let raw = w.continuation(ContinuationOperation::AdvanceEvent {
        expected_checkpoint_revision: 1,
        checkpoint_base64: String::new(),
        effect: Some(EventEffect::BeginReplace {
            expected_revision: 2,
            total_bytes: 3,
        }),
        complete: false,
    });
    let denied = || {
        Err(ApiError::new(
            ErrorCode::StaleContext,
            "controller not running",
        ))
    };
    assert!(
        w.service
            .call(
                &mut w.drafts,
                &mut w.events,
                &mut w.host,
                &w.source,
                &i,
                raw.as_bytes(),
                29,
                &denied
            )
            .is_err()
    );
    let raw = w.continuation(ContinuationOperation::ReadMetadata {});
    let b = w.call(&i, &raw, 0).unwrap();
    let ContinuationOutcome::Metadata { metadata } =
        ContinuationResponse::from_slice(&b).unwrap().outcome
    else {
        panic!("metadata")
    };
    assert_eq!(metadata.checkpoint_revision, 1);
    assert_eq!(metadata.checkpoint_bytes, 0);
    assert!(metadata.last_effect.is_none());
    assert_eq!(w.state(&i, 30), ContinuationState::Expired);
}
#[test]
fn replacement_epoch_and_other_service_reject_old_invocations() {
    let mut w = World::new();
    let old = w.token();
    w.cursor = w
        .service
        .publish(
            &mut w.drafts,
            &mut w.events,
            &mut w.host,
            &w.source,
            2,
            EventAction::InputChanged,
            "new".into(),
            1,
        )
        .unwrap()
        .cursor;
    let new = w.token();
    let raw = w.continuation(ContinuationOperation::ReadMetadata {});
    assert!(w.call(&old, &raw, 29).is_err());
    let mut other = World::new();
    let raw_other = other.continuation(ContinuationOperation::ReadMetadata {});
    assert!(other.call(&new, &raw_other, 29).is_err());
    assert_eq!(w.state(&new, 31), ContinuationState::Expired);
}
#[test]
fn bootstrap_and_read_context_use_explicit_edit_purpose() {
    let mut w = World::new();
    let i = w.token();
    let r = ComponentInvocationRequest {
        api_version: HOST_API_V0_3,
        component_invocation_protocol: 1,
        request_id: "bootstrap".into(),
        operation: ComponentInvocationOperation::ReadContext {},
    };
    let b = w.call(&i, &r.to_json().unwrap(), 0).unwrap();
    assert_eq!(
        ComponentInvocationResponse::from_slice(&b).unwrap().context,
        ComponentInvocationContext::EditEvent {}
    );
    let r = ComponentContextRequest {
        api_version: HOST_API_V0_3,
        context_protocol: 1,
        request_id: "ctx".into(),
        operation: ContextOperation::ReadContext {},
    };
    let b = w.call(&i, &r.to_json().unwrap(), 0).unwrap();
    assert!(matches!(
        ComponentContextResponse::from_slice(&b).unwrap().outcome,
        ContextOutcome::Context {
            action: ComponentAction::InputChanged { total_bytes: 5 }
        }
    ));
}
