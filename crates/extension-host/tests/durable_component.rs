use extension_api::{composition::*, draft::*, *};
use extension_host::{component::MountHandle, durable::*};
use std::cell::{Cell, RefCell};

struct Source {
    snapshot: RefCell<ComponentInstallation>,
    reads: Cell<usize>,
    unavailable: Cell<bool>,
    expected_run: RefCell<Option<String>>,
}
impl ComponentAuthoritySource for Source {
    fn component_installation(
        &self,
        package: &str,
        read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError> {
        self.reads.set(self.reads.get() + 1);
        if self.unavailable.get() || package != self.snapshot.borrow().manifest.package_id {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "source unavailable",
            ));
        }
        if let AuthorityRead::Current { run_id } = read
            && run_id != self.expected_run.borrow().as_deref()
        {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "worker mismatch",
            ));
        }
        Ok(self.snapshot.borrow().clone())
    }
}
impl AuthoritySource for Source {
    fn authority_snapshot(
        &self,
        _: &str,
        _: AuthorityRead<'_>,
    ) -> Result<AuthoritySnapshot, ApiError> {
        Err(ApiError::new(
            ErrorCode::PermissionDenied,
            "no durable grants",
        ))
    }
    fn removed_installation(&self, _: &str) -> Result<RemovedInstallation, ApiError> {
        let s = self.snapshot.borrow();
        Ok(RemovedInstallation {
            installation_id: s.installation_id,
            package_id: s.manifest.package_id.clone(),
            vault_id: s.vault_id.clone(),
            revision: s.revision + 1,
            generation: s.generation + 1,
        })
    }
}
fn snapshot() -> ComponentInstallation {
    ComponentInstallation {
        installation_id: 1,
        revision: 1,
        generation: 1,
        digest: "a".repeat(64),
        vault_id: "vault".into(),
        manifest: Manifest {
            manifest_version: 1,
            package_id: "org.test.editor".into(),
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
                id: "org.test.editor.input".into(),
                title: "Input".into(),
                kind: ContributionKind::ComponentProvider,
                required_capabilities: vec![Capability::NoteDraftEdit],
            }],
            orthogonal_capabilities: vec![],
            provides: vec![],
            requires: vec![],
        },
        composition: CompositionDescriptor {
            composition_protocol: COMPOSITION_PROTOCOL,
            api_version: HOST_API_V0_3,
            contribution_id: "org.test.editor.input".into(),
            role: ComponentRole::AnnotationPlainTextEditor,
            contract: ANNOTATION_EDITOR_CONTRACT.into(),
            view: NativeEditorView::TextEditor {
                label: "Thought".into(),
                placeholder: "Write".into(),
            },
        },
        composition_sha256: "b".repeat(64),
    }
}
fn source() -> Source {
    Source {
        snapshot: RefCell::new(snapshot()),
        reads: Cell::new(0),
        unavailable: Cell::new(false),
        expected_run: RefCell::new(None),
    }
}
fn host() -> DurableHost {
    let mut h = DurableHost::new("vault", [Capability::NoteDraftEdit]).unwrap();
    h.configure_component_identity([7; 16]).unwrap();
    h
}
fn intent(s: &Source, n: u8) -> UserMountIntent {
    UserMountIntent::new(
        s.snapshot.borrow().clone(),
        Resource::Draft {
            vault_id: "vault".into(),
            draft_id: format!("{n:032x}"),
        },
    )
    .unwrap()
}
fn request(m: &MountHandle) -> DraftRequest {
    DraftRequest {
        api_version: HOST_API_V0_3,
        draft_protocol: DRAFT_PROTOCOL,
        request_id: "read".into(),
        resource: m.resource().clone(),
        mount_id: m.mount_id().into(),
        operation: DraftOperation::ReadChunk {
            revision: None,
            offset: 0,
            max_bytes: 512,
        },
    }
}
#[test]
fn explicit_mount_is_not_a_reader_grant_and_every_callback_revalidates() {
    let s = source();
    let mut h = host();
    let m = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    assert!(h.activate(&s, "org.test.editor", HOST_API_V0_3, 1).is_err());
    // Failed ordinary activation fences this same registration, not a parallel Host.
    assert!(
        h.validate_component_current(&s, m.handle(), 1, None)
            .is_err()
    );
    let n = h.mount_component(&s, intent(&s, 2), 2).unwrap();
    assert_eq!(m.handle().installation_id(), n.handle().installation_id());
    *s.expected_run.borrow_mut() = Some("bound-run".into());
    h.validate_component_current(&s, n.handle(), 2, Some("bound-run"))
        .unwrap();
    assert_eq!(
        h.dispatch_draft(
            &s,
            n.handle(),
            2,
            Some("bound-run"),
            request(n.handle()),
            |c| Ok(c.package_digest.clone())
        )
        .unwrap(),
        "a".repeat(64)
    );
    s.snapshot.borrow_mut().generation += 1;
    assert!(
        h.dispatch_draft(
            &s,
            n.handle(),
            3,
            Some("bound-run"),
            request(n.handle()),
            |_| -> Result<(), ApiError> { panic!("revoked handler") }
        )
        .is_err()
    );
    let reads = s.reads.get();
    assert!(
        h.validate_component_current(&s, n.handle(), 3, None)
            .is_err()
    );
    assert_eq!(reads, s.reads.get());
}
#[test]
fn closing_quota_survives_mount_churn_and_current_counter_changes() {
    let s = source();
    let mut h = host();
    let mut native = None;
    for n in 0..16 {
        let m = h.mount_component(&s, intent(&s, n), 1).unwrap();
        if let Some(id) = native {
            assert_eq!(m.handle().installation_id(), id);
        }
        native = Some(m.handle().installation_id());
        h.close_component(m.handle()).unwrap();
    }
    assert!(h.mount_component(&s, intent(&s, 17), 2).is_err());
    {
        let mut c = s.snapshot.borrow_mut();
        c.revision += 1;
        c.generation += 1;
    }
    assert!(h.mount_component(&s, intent(&s, 17), 2).is_err());
    assert_eq!(h.pending_component_cleanup().len(), 16);
    assert!(h.retire_removed(&s, 1, "org.test.editor").is_err());
    let ticket = h.pending_component_cleanup().remove(0);
    h.confirm_component_released(&ticket).unwrap();
    let m = h.mount_component(&s, intent(&s, 17), 3).unwrap();
    assert_eq!(Some(m.handle().installation_id()), native);
    h.close_component(m.handle()).unwrap();
    for ticket in h.pending_component_cleanup() {
        h.confirm_component_released(&ticket).unwrap();
    }
    h.retire_removed(&s, 1, "org.test.editor").unwrap();
}
#[test]
fn intent_binds_full_facts_and_foreign_or_old_mount_cannot_fence_successor() {
    let s = source();
    let mut h = host();
    let old = intent(&s, 1);
    s.snapshot.borrow_mut().composition_sha256 = "c".repeat(64);
    assert!(h.mount_component(&s, old, 1).is_err());
    let a = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    let b = h.mount_component(&s, intent(&s, 2), 1).unwrap();
    h.close_component(a.handle()).unwrap();
    let reads = s.reads.get();
    assert!(
        h.validate_component_current(&s, a.handle(), 2, None)
            .is_err()
    );
    assert_eq!(s.reads.get(), reads);
    let mut other = host();
    assert!(
        other
            .validate_component_current(&s, b.handle(), 2, None)
            .is_err()
    );
    assert_eq!(s.reads.get(), reads);
    h.validate_component_current(&s, b.handle(), 2, None)
        .unwrap();
    s.snapshot.borrow_mut().vault_id = "other".into();
    assert!(
        h.validate_component_current(&s, b.handle(), 2, None)
            .is_err()
    );
}
#[test]
fn no_same_counter_asset_change_alias_or_rollback_is_accepted() {
    let s = source();
    let mut h = host();
    let a = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    h.close_component(a.handle()).unwrap();
    s.snapshot.borrow_mut().composition_sha256 = "c".repeat(64);
    assert!(h.mount_component(&s, intent(&s, 2), 1).is_err());
    {
        let mut c = s.snapshot.borrow_mut();
        c.revision = 2;
        c.generation = 2;
    }
    let b = h.mount_component(&s, intent(&s, 2), 2).unwrap();
    assert_eq!(a.handle().installation_id(), b.handle().installation_id());
    {
        let mut c = s.snapshot.borrow_mut();
        c.revision = 1;
        c.generation = 1;
    }
    assert!(h.mount_component(&s, intent(&s, 3), 2).is_err());
    {
        let mut c = s.snapshot.borrow_mut();
        c.revision = 3;
        c.generation = 3;
        c.manifest.package_id = "org.test.alias".into();
        c.manifest.contributions[0].id = "org.test.alias.input".into();
        c.composition.contribution_id = "org.test.alias.input".into();
    }
    assert!(h.mount_component(&s, intent(&s, 3), 3).is_err());
}

#[test]
fn new_persistent_id_cannot_bypass_predecessor_closing_quota() {
    let s = source();
    let mut h = host();
    let first = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    let native_id = first.handle().installation_id();
    h.close_component(first.handle()).unwrap();
    for n in 2..=16 {
        let m = h.mount_component(&s, intent(&s, n), 1).unwrap();
        h.close_component(m.handle()).unwrap();
    }
    s.snapshot.borrow_mut().installation_id = 2;
    assert!(h.mount_component(&s, intent(&s, 17), 2).is_err());
    assert_eq!(h.pending_component_cleanup().len(), 16);
    // Returning to the old identity must still encounter its original quota.
    s.snapshot.borrow_mut().installation_id = 1;
    assert!(h.mount_component(&s, intent(&s, 17), 2).is_err());
    assert!(h.retire_removed(&s, 1, "org.test.editor").is_err());
    for ticket in h.pending_component_cleanup() {
        assert_eq!(ticket.installation_id(), native_id);
        h.confirm_component_released(&ticket).unwrap();
    }
    h.retire_removed(&s, 1, "org.test.editor").unwrap();
    s.snapshot.borrow_mut().installation_id = 2;
    let successor = h.mount_component(&s, intent(&s, 17), 3).unwrap();
    assert_ne!(successor.handle().installation_id(), native_id);
    // A predecessor callback cannot fence the newly registered successor.
    assert!(
        h.validate_component_current(&s, first.handle(), 3, None)
            .is_err()
    );
    h.validate_component_current(&s, successor.handle(), 3, None)
        .unwrap();
}

#[test]
fn component_execution_uses_shared_lifecycle_and_fresh_mount_authority() {
    let s = source();
    let mut h = host();
    let mounted = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    let mut run =
        ManagedRun::prepare_component(&mut h, &s, mounted.handle(), 1, "component-run-1").unwrap();
    assert_eq!(run.mount().mount_id(), mounted.handle().mount_id());
    assert!(run.finish(|| panic!("finish before reap")).is_err());
    *s.expected_run.borrow_mut() = Some("component-run-1".into());
    run.release_component_boot(&mut h, &s, 2, || Ok(()))
        .unwrap();
    assert!(
        run.release_component_boot(&mut h, &s, 2, || panic!("duplicate Boot"))
            .is_err()
    );
    assert_eq!(
        run.accept_component_result(&mut h, &s, 3, || Ok(42))
            .unwrap(),
        42
    );
    run.record_reaped().unwrap();
    assert!(
        run.finish(|| Err(ApiError::new(
            ErrorCode::InvalidState,
            "cleanup unavailable"
        )))
        .is_err()
    );
    assert_eq!(run.phase(), RunPhase::Quarantined);
    run.finish(|| Ok(())).unwrap();
    assert_eq!(run.phase(), RunPhase::Finished);
}

#[test]
fn component_revocation_before_boot_or_late_result_never_runs_callback() {
    let s = source();
    let mut h = host();
    let mounted = h.mount_component(&s, intent(&s, 1), 1).unwrap();
    let mut run =
        ManagedRun::prepare_component(&mut h, &s, mounted.handle(), 1, "component-run-1").unwrap();
    s.unavailable.set(true);
    assert!(
        run.release_component_boot(&mut h, &s, 2, || panic!("revoked Boot"))
            .is_err()
    );
    assert_eq!(run.phase(), RunPhase::Quarantined);
    // No observed child means neither no-spawn nor reap can be inferred here.
    assert!(run.finish(|| panic!("unknown spawn cleared")).is_err());
    s.unavailable.set(false);
    *s.expected_run.borrow_mut() = None;
    let next = h.mount_component(&s, intent(&s, 2), 3).unwrap();
    let mut run =
        ManagedRun::prepare_component(&mut h, &s, next.handle(), 3, "component-run-2").unwrap();
    *s.expected_run.borrow_mut() = Some("component-run-2".into());
    run.release_component_boot(&mut h, &s, 4, || Ok(()))
        .unwrap();
    s.snapshot.borrow_mut().generation += 1;
    s.snapshot.borrow_mut().revision += 1;
    assert!(
        run.accept_component_result::<()>(&mut h, &s, 5, || panic!("revoked result"))
            .is_err()
    );
    assert_eq!(run.phase(), RunPhase::Quarantined);
    run.record_reaped().unwrap();
    run.finish(|| Ok(())).unwrap();
}
