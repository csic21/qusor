use extension_api::*;
use extension_host::{Grant, durable::*};
use std::cell::RefCell;
use std::collections::BTreeMap;

struct Journal(RefCell<BTreeMap<String, AuthoritySnapshot>>);
impl AuthoritySource for Journal {
    fn authority_snapshot(
        &self,
        package: &str,
        _: AuthorityRead<'_>,
    ) -> Result<AuthoritySnapshot, ApiError> {
        self.0
            .borrow()
            .get(package)
            .cloned()
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "record unavailable"))
    }
}
fn snapshot(id: i64, package: &str) -> AuthoritySnapshot {
    AuthoritySnapshot {
        installation_id: id,
        revision: 2,
        generation: 2,
        digest: "a".repeat(64),
        vault_id: "vault".into(),
        enable_intent: true,
        required_api: None,
        manifest: Manifest {
            manifest_version: 1,
            package_id: package.into(),
            publisher_id: "org.example".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 1,
                max_minor: 2,
            },
            required_capabilities: vec![Capability::DocumentMetadataRead],
            optional_capabilities: vec![],
            contributions: vec![],
            orthogonal_capabilities: vec![],
            provides: vec![],
            requires: vec![],
        },
        grants: vec![DurableGrant {
            grant: Grant {
                capability: Capability::DocumentMetadataRead,
                resource: Resource::Document {
                    vault_id: "vault".into(),
                    document_id: "doc".into(),
                },
            },
            expires_at: 100,
            resource_version: Some("v1".into()),
        }],
    }
}
fn journal() -> Journal {
    Journal(RefCell::new(
        [
            ("org.example.a".into(), snapshot(1, "org.example.a")),
            ("org.example.b".into(), snapshot(2, "org.example.b")),
        ]
        .into(),
    ))
}
fn host() -> DurableHost {
    DurableHost::new("vault", [Capability::DocumentMetadataRead]).unwrap()
}
fn request(api: ApiVersion) -> VersionedRequest {
    VersionedRequest {
        resource_version: Some("v1".into()),
        request: CapabilityRequest {
            api_version: api,
            request_id: "read".into(),
            capability: Capability::DocumentMetadataRead,
            resource: Resource::Document {
                vault_id: "vault".into(),
                document_id: "doc".into(),
            },
        },
    }
}

#[test]
fn one_host_restores_two_packages_with_explicit_versions_and_fresh_sessions() {
    let source = journal();
    let mut h = host();
    let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    let b = h
        .activate(&source, "org.example.b", HOST_API_V0_2, 1)
        .unwrap();
    assert_eq!(
        h.dispatch(&source, &a, 2, None, request(HOST_API), |c| Ok(c
            .package_id
            .clone()))
            .unwrap(),
        "org.example.a"
    );
    assert_eq!(
        h.dispatch(&source, &b, 2, None, request(HOST_API_V0_2), |c| Ok(c
            .package_id
            .clone()))
            .unwrap(),
        "org.example.b"
    );
    assert!(
        h.dispatch(&source, &a, 2, None, request(HOST_API_V0_2), must_not_run)
            .is_err()
    );
    let mut restarted = host();
    assert!(
        restarted
            .dispatch(&source, &a, 2, None, request(HOST_API), must_not_run)
            .is_err()
    );
    let fresh = restarted
        .activate(&source, "org.example.a", HOST_API, 3)
        .unwrap();
    assert!(
        restarted
            .dispatch(&source, &fresh, 3, None, request(HOST_API), |_| Ok(()))
            .is_ok()
    );
}

#[test]
fn changed_or_unreadable_durable_state_fences_only_that_package() {
    for change in [
        "generation",
        "digest",
        "manifest",
        "scope",
        "missing",
        "intent",
    ] {
        let source = journal();
        let mut h = host();
        let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
        let b = h.activate(&source, "org.example.b", HOST_API, 1).unwrap();
        let old = source.0.borrow()["org.example.a"].clone();
        {
            let mut records = source.0.borrow_mut();
            let item = records.get_mut("org.example.a").unwrap();
            match change {
                "generation" => item.generation += 1,
                "digest" => item.digest = "b".repeat(64),
                "manifest" => item.manifest.package_version.patch += 1,
                "scope" => {
                    item.grants[0].grant.resource = Resource::Vault {
                        vault_id: "vault".into(),
                    }
                }
                "intent" => item.enable_intent = false,
                "missing" => {
                    records.remove("org.example.a");
                }
                _ => unreachable!(),
            }
        }
        assert!(
            h.dispatch(&source, &a, 2, None, request(HOST_API), must_not_run)
                .is_err(),
            "{change}"
        );
        source.0.borrow_mut().insert("org.example.a".into(), old);
        assert!(
            h.dispatch(&source, &a, 2, None, request(HOST_API), must_not_run)
                .is_err()
        );
        assert!(
            h.dispatch(&source, &b, 2, None, request(HOST_API), |_| Ok(()))
                .is_ok()
        );
    }
}

#[test]
fn failed_reactivation_expiry_and_identity_alias_do_not_resurrect_authority() {
    let source = journal();
    let mut h = host();
    let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    assert!(
        h.activate(
            &source,
            "org.example.a",
            ApiVersion {
                major: 0,
                minor: 99
            },
            2
        )
        .is_err()
    );
    assert!(
        h.dispatch(&source, &a, 2, None, request(HOST_API), must_not_run)
            .is_err()
    );
    let a = h.activate(&source, "org.example.a", HOST_API, 3).unwrap();
    assert!(
        h.dispatch(&source, &a, 100, None, request(HOST_API), must_not_run)
            .is_err()
    );
    assert!(
        h.activate(&source, "org.example.a", HOST_API, 1).is_err(),
        "clock rollback must not revive intent"
    );
    let mut h = host();
    let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    source
        .0
        .borrow_mut()
        .get_mut("org.example.b")
        .unwrap()
        .installation_id = 1;
    assert!(h.activate(&source, "org.example.b", HOST_API, 1).is_err());
    assert!(
        h.dispatch(&source, &a, 2, None, request(HOST_API), |_| Ok(()))
            .is_ok()
    );
}

#[test]
fn precommit_validation_uses_same_policy_and_does_not_publish_a_session() {
    let source = journal();
    let mut h = host();
    let record = source.0.borrow()["org.example.a"].clone();
    let grants: Vec<_> = record.grants.iter().map(|g| g.grant.clone()).collect();
    h.validate_activation_intent(&record.manifest, &record.digest, &grants, HOST_API)
        .unwrap();
    assert!(
        h.validate_activation_intent(&record.manifest, &record.digest, &[], HOST_API)
            .is_err()
    );
    let mut bad = grants;
    bad[0].resource = Resource::Document {
        vault_id: "other".into(),
        document_id: "doc".into(),
    };
    assert!(
        h.validate_activation_intent(&record.manifest, &record.digest, &bad, HOST_API)
            .is_err()
    );
    let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    h.close(&a).unwrap();
    assert!(
        h.dispatch(&source, &a, 2, None, request(HOST_API), must_not_run)
            .is_err()
    );
}

#[test]
fn durable_exact_version_needs_the_current_host_resolved_version() {
    let source = journal();
    let mut h = host();
    let a = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    for version in [None, Some("v2".into())] {
        let mut call = request(HOST_API);
        call.resource_version = version;
        assert!(
            h.dispatch(&source, &a, 2, None, call, must_not_run)
                .is_err()
        );
    }
    assert!(
        h.dispatch(&source, &a, 2, None, request(HOST_API), |_| Ok(()))
            .is_ok()
    );
}

#[test]
fn negotiated_api_rejects_grant_semantics_from_a_newer_version() {
    let source = journal();
    {
        let mut records = source.0.borrow_mut();
        let record = records.get_mut("org.example.a").unwrap();
        record.manifest.required_capabilities = vec![Capability::SelectionRead];
        record.grants[0].grant.capability = Capability::SelectionRead;
    }
    let mut h = DurableHost::new("vault", [Capability::SelectionRead]).unwrap();
    let record = source.0.borrow()["org.example.a"].clone();
    let grants = vec![record.grants[0].grant.clone()];
    assert!(
        h.validate_activation_intent(&record.manifest, &record.digest, &grants, HOST_API)
            .is_err()
    );
    assert!(h.activate(&source, "org.example.a", HOST_API, 1).is_err());
    assert!(
        h.activate(&source, "org.example.a", HOST_API_V0_2, 1)
            .is_ok()
    );
}

#[test]
fn reactivation_rejects_counter_rollback_or_same_generation_permission_changes() {
    let source = journal();
    let mut h = host();
    let old = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    source
        .0
        .borrow_mut()
        .get_mut("org.example.a")
        .unwrap()
        .grants[0]
        .grant
        .resource = Resource::Vault {
        vault_id: "vault".into(),
    };
    assert!(h.activate(&source, "org.example.a", HOST_API, 2).is_err());
    assert!(
        h.dispatch(&source, &old, 2, None, request(HOST_API), must_not_run)
            .is_err()
    );
    source
        .0
        .borrow_mut()
        .insert("org.example.a".into(), snapshot(1, "org.example.a"));
    let current = h.activate(&source, "org.example.a", HOST_API, 2).unwrap();
    source
        .0
        .borrow_mut()
        .get_mut("org.example.a")
        .unwrap()
        .generation = 1;
    assert!(h.activate(&source, "org.example.a", HOST_API, 3).is_err());
    assert!(
        h.dispatch(&source, &current, 3, None, request(HOST_API), must_not_run)
            .is_err()
    );
}

#[test]
fn retired_uninstalled_metadata_packages_do_not_exhaust_live_registration_capacity() {
    struct Removed<'a>(&'a Journal, i64, String);
    impl AuthoritySource for Removed<'_> {
        fn authority_snapshot(
            &self,
            p: &str,
            r: AuthorityRead<'_>,
        ) -> Result<AuthoritySnapshot, ApiError> {
            self.0.authority_snapshot(p, r)
        }
        fn removed_installation(&self, p: &str) -> Result<RemovedInstallation, ApiError> {
            assert_eq!(p, self.2);
            Ok(RemovedInstallation {
                installation_id: self.1,
                package_id: p.into(),
                vault_id: "vault".into(),
                revision: 3,
                generation: 3,
            })
        }
    }
    let mut h = host();
    for id in 1..=300 {
        let package = format!("org.example.p{id}");
        let source = Journal(RefCell::new(
            [(package.clone(), snapshot(id, &package))].into(),
        ));
        let session = h.activate(&source, &package, HOST_API, 1).unwrap();
        source.0.borrow_mut().clear();
        let removed = Removed(&source, id, package.clone());
        h.retire_removed(&removed, id, &package).unwrap();
        assert!(
            h.dispatch(&removed, &session, 1, None, request(HOST_API), must_not_run)
                .is_err()
        );
    }
}

fn must_not_run(_: &extension_host::AuthorizedContext) -> Result<(), ApiError> {
    panic!("rejected durable authority reached service")
}

#[test]
fn boot_readiness_is_fresh_and_does_not_authorize_a_later_call() {
    let source = journal();
    let mut h = host();
    let first = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    h.validate_current(&source, &first, 2, Some("bound-worker"))
        .unwrap();
    let original = source.0.borrow_mut().remove("org.example.a").unwrap();
    assert!(h.validate_current(&source, &first, 3, None).is_err());
    source
        .0
        .borrow_mut()
        .insert("org.example.a".into(), original);
    assert!(h.validate_current(&source, &first, 4, None).is_err());
    let second = h.activate(&source, "org.example.a", HOST_API, 4).unwrap();
    h.validate_current(&source, &second, 5, None).unwrap();
    source.0.borrow_mut().remove("org.example.a");
    assert!(
        h.dispatch(&source, &second, 6, None, request(HOST_API), must_not_run)
            .is_err()
    );
    let third = h
        .activate(&journal(), "org.example.a", HOST_API, 7)
        .unwrap();
    assert!(h.validate_current(&journal(), &third, 100, None).is_err());
}

#[test]
fn managed_run_rejects_boot_and_late_result_after_authority_changes() {
    let source = journal();
    let mut h = host();
    let session = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    let mut run = ManagedRun::prepare(&mut h, &source, &session, 2, "boot-1").unwrap();
    assert_eq!(run.phase(), RunPhase::Prepared);
    source.0.borrow_mut().remove("org.example.a");
    let mut boot_sent = false;
    assert!(
        run.release_boot(&mut h, &source, 3, || {
            boot_sent = true;
            Ok(())
        })
        .is_err()
    );
    assert!(!boot_sent);
    assert_eq!(run.phase(), RunPhase::Quarantined);
    assert!(
        run.finish(|| panic!("unconfirmed exit cleared lease"))
            .is_err()
    );
    assert!(
        run.accept_result::<()>(&mut h, &journal(), 4, || panic!("late result accepted"))
            .is_err()
    );
    // Pure policy test: the native controller's real reap is verified separately.
    run.record_reaped().unwrap();
    run.finish(|| Ok(())).unwrap();
    assert!(run.finish(|| panic!("duplicate cleanup invoked")).is_err());

    let source = journal();
    let session = h.activate(&source, "org.example.a", HOST_API, 5).unwrap();
    let mut run = ManagedRun::prepare(&mut h, &source, &session, 5, "boot-2").unwrap();
    run.release_boot(&mut h, &source, 6, || Ok(())).unwrap();
    source.0.borrow_mut().remove("org.example.a");
    assert!(
        run.accept_result::<()>(&mut h, &source, 7, || panic!("revoked result accepted"))
            .is_err()
    );
    assert_eq!(run.phase(), RunPhase::Quarantined);
}

#[test]
fn accepted_result_is_not_completion_and_failed_cleanup_stays_quarantined() {
    let source = journal();
    let mut h = host();
    let session = h.activate(&source, "org.example.a", HOST_API, 1).unwrap();
    let mut run = ManagedRun::prepare(&mut h, &source, &session, 1, "boot-1").unwrap();
    run.release_boot(&mut h, &source, 2, || Ok(())).unwrap();
    assert!(
        run.release_boot(&mut h, &source, 3, || panic!("duplicate Boot"))
            .is_err()
    );
    assert_eq!(
        run.accept_result(&mut h, &source, 3, || Ok(42)).unwrap(),
        42
    );
    assert_eq!(run.phase(), RunPhase::ResultAccepted);
    assert!(run.finish(|| panic!("result does not prove reap")).is_err());
    run.record_reaped().unwrap();
    assert!(run.record_reaped().is_err());
    assert!(
        run.finish(|| Err(ApiError::new(
            ErrorCode::InvalidState,
            "storage unavailable"
        )))
        .is_err()
    );
    assert_eq!(run.phase(), RunPhase::Quarantined);
    assert!(
        run.accept_result::<()>(&mut h, &source, 4, || panic!("duplicate result"))
            .is_err()
    );
    run.finish(|| Ok(())).unwrap();
    assert_eq!(run.phase(), RunPhase::Finished);

    let mut unstarted = ManagedRun::prepare(&mut h, &source, &session, 5, "boot-2").unwrap();
    unstarted.record_not_spawned().unwrap();
    assert!(
        unstarted
            .release_boot(&mut h, &source, 6, || panic!("failed spawn booted"))
            .is_err()
    );
    unstarted.finish(|| Ok(())).unwrap();
}
