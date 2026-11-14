#![cfg(target_os = "linux")]
use extension_api::*;
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_ports::{AuthorityRead, AuthoritySource, Grant};
use extension_store::*;
use rusqlite::Connection;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-grant-set-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> PathBuf {
        self.0.join("store")
    }
    fn db(&self) -> Connection {
        Connection::open(self.store().join("state.sqlite3")).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Policy {
    vault_id: String,
    supported: Vec<Capability>,
}
impl Policy {
    fn new() -> Self {
        Self {
            vault_id: "vault".into(),
            supported: vec![
                Capability::DocumentMetadataRead,
                Capability::ReaderSurface,
                Capability::SelectionRead,
                Capability::CapturePropose,
            ],
        }
    }
    fn validate_intent(
        &self,
        manifest: &Manifest,
        grants: &[Grant],
        api: ApiVersion,
    ) -> Result<()> {
        manifest.validate()?;
        if !is_supported_api(api) || !manifest.supports_api(api) {
            return Err(StoreError::unsupported("activation API is unsupported"));
        }
        if manifest
            .required_capabilities
            .iter()
            .any(|c| !self.supported.contains(c))
        {
            return Err(StoreError::invalid("required capability unavailable"));
        }
        for (idx, grant) in grants.iter().enumerate() {
            grant.resource.validate()?;
            if !self.supported.contains(&grant.capability)
                || (!manifest.required_capabilities.contains(&grant.capability)
                    && !manifest.optional_capabilities.contains(&grant.capability))
                || grant.resource.vault_id() != self.vault_id
                || !capability_resource_valid(grant.capability, &grant.resource, api)
            {
                return Err(StoreError::invalid(
                    "grant exceeds declared capability or vault scope",
                ));
            }
            if grants[..idx].contains(grant) {
                return Err(StoreError::invalid("duplicate grant"));
            }
        }
        if !manifest
            .required_capabilities
            .iter()
            .all(|req| grants.iter().any(|g| g.capability == *req))
        {
            return Err(StoreError::invalid(
                "required capabilities need explicit grants",
            ));
        }
        Ok(())
    }
}
impl GrantValidator for Policy {
    fn validate(&self, package: &VerifiedPackage, scope: &GrantScope) -> Result<()> {
        self.validate_intent(
            package.manifest(),
            &[Grant {
                capability: scope.capability,
                resource: scope.resource(),
            }],
            HOST_API,
        )
    }
    fn validate_set(&self, package: &VerifiedPackage, scope: &GrantSet) -> Result<()> {
        let grants = scope
            .grants
            .iter()
            .map(|g| Grant {
                capability: g.capability,
                resource: g.resource.clone(),
            })
            .collect::<Vec<_>>();
        self.validate_intent(package.manifest(), &grants, scope.api_version)
    }
}
fn services() -> StoreServices {
    StoreServices {
        grant_validator: Arc::new(Policy::new()),
        ..StoreServices::default()
    }
}
fn package(reader: bool, version: u32) -> VerifiedPackage {
    let caps = if reader {
        vec![Capability::ReaderSurface]
    } else {
        vec![Capability::DocumentMetadataRead]
    };
    let optional = if reader {
        vec![
            Capability::SelectionRead,
            Capability::CapturePropose,
            Capability::DocumentMetadataRead,
        ]
    } else {
        vec![]
    };
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.reader".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: version,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: if reader { 2 } else { 1 },
            max_minor: 2,
        },
        required_capabilities: caps,
        optional_capabilities: optional,
        contributions: vec![],
    };
    let seed = [71; 32];
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: b"\0asm\x01\0\0\0".to_vec(),
        }],
        &seed,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    verify_for_host(&bytes, &publisher_key(&seed), &SUPPORTED_HOST_APIS).unwrap()
}
fn doc() -> Resource {
    Resource::Document {
        vault_id: "vault".into(),
        document_id: "doc".into(),
    }
}
fn set() -> GrantSet {
    GrantSet {
        api_version: HOST_API_V0_2,
        vault_id: "vault".into(),
        expires_at: u64::MAX,
        grants: vec![
            PersistentGrant {
                capability: Capability::ReaderSurface,
                resource: doc(),
                resource_version: Some("version".into()),
            },
            PersistentGrant {
                capability: Capability::SelectionRead,
                resource: doc(),
                resource_version: Some("version".into()),
            },
            PersistentGrant {
                capability: Capability::CapturePropose,
                resource: Resource::Vault {
                    vault_id: "vault".into(),
                },
                resource_version: None,
            },
        ],
    }
}
fn open(s: &Scratch) -> Store {
    Store::open_with_services(&s.store(), &SUPPORTED_HOST_APIS, services()).unwrap()
}

#[test]
fn real_three_capability_set_survives_reopen_without_legacy_projection() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let p = package(true, 1);
    let installed = store.install("install", 0, &p).unwrap();
    assert!(
        store
            .authority_snapshot("org.test.reader", AuthorityRead::Activation)
            .is_err()
    );
    let receipt = store
        .grant_set("grant-set", 1, "org.test.reader", set())
        .unwrap();
    assert_eq!((receipt.revision, receipt.generation), (2, 2));
    assert!(store.saved_grant(&receipt).is_err());
    assert!(store.execution_snapshot("org.test.reader", None).is_err());
    let snapshot = store
        .authority_snapshot("org.test.reader", AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(snapshot.grants.len(), 3);
    assert_eq!(snapshot.required_api, Some(HOST_API_V0_2));
    assert!(snapshot.enable_intent);
    assert!(matches!(
        store.saved_authorization(&receipt).unwrap(),
        SavedAuthorization::V2(_)
    ));
    drop(store);
    assert!(Store::open(&scratch.store()).is_err());
    let mut store = open(&scratch);
    assert_eq!(
        store
            .saved_authorization(&store.current("org.test.reader").unwrap())
            .unwrap(),
        SavedAuthorization::V2(SavedGrantSet {
            installation_id: installed.installation_id,
            digest: p.digest().into(),
            generation: 2,
            policy_version: 2,
            scope: set()
        })
    );
    assert_eq!(
        store
            .grant_set("grant-set", 1, "org.test.reader", set())
            .unwrap(),
        receipt
    );
    let mut changed = set();
    changed.expires_at -= 1;
    assert_eq!(
        store
            .grant_set("grant-set", 1, "org.test.reader", changed)
            .unwrap_err()
            .code(),
        StoreErrorCode::Conflict
    );
    assert_eq!(store.current("org.test.reader").unwrap().revision, 2);
    store
        .stop("revoke", 2, "org.test.reader", "revoke")
        .unwrap();
    assert!(
        store
            .authority_snapshot("org.test.reader", AuthorityRead::Current { run_id: None })
            .is_err()
    );
}

#[test]
fn defaults_unknown_versions_and_oversized_rows_fail_closed() {
    let scratch = Scratch::new();
    let mut store = Store::open_for_host(&scratch.store(), &SUPPORTED_HOST_APIS).unwrap();
    let p = package(true, 1);
    store.install("install", 0, &p).unwrap();
    assert_eq!(
        store
            .grant_set("set", 1, "org.test.reader", set())
            .unwrap_err()
            .code(),
        StoreErrorCode::External
    );
    assert_eq!(store.current("org.test.reader").unwrap().revision, 1);
    drop(store);
    let mut store = open(&scratch);
    let receipt = store.grant_set("set", 1, "org.test.reader", set()).unwrap();
    let good: String = scratch
        .db()
        .query_row("SELECT record FROM grants", [], |r| r.get(0))
        .unwrap();
    for bad in [
        good.replace("\"policy_version\":2", "\"policy_version\":99"),
        "x".repeat(MAX_AUTHORIZATION_BYTES + 1),
        good.replace("\"generation\":2", "\"generation\":3"),
        good.replace(
            "\"api_version\":{\"major\":0,\"minor\":2}",
            "\"api_version\":{\"major\":0,\"minor\":1}",
        ),
    ] {
        scratch
            .db()
            .execute("UPDATE grants SET record=?1", [&bad])
            .unwrap();
        assert!(store.saved_authorization(&receipt).is_err());
        assert!(
            store
                .authority_snapshot("org.test.reader", AuthorityRead::Current { run_id: None })
                .is_err()
        );
    }
    scratch
        .db()
        .execute("UPDATE grants SET record=?1", [&good])
        .unwrap();
    drop(store);
    scratch
        .db()
        .execute(
            "UPDATE grants SET record=?1",
            [" ".repeat(MAX_AUTHORIZATION_BYTES + 1)],
        )
        .unwrap();
    assert!(Store::open_for_host(&scratch.store(), &SUPPORTED_HOST_APIS).is_err());
}

#[test]
fn grant_set_rejects_transient_ambiguous_and_cross_vault_intent_before_commit() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    store.install("install", 0, &package(true, 1)).unwrap();
    let mut invalid = Vec::new();
    let mut scope = set();
    scope.grants.clear();
    invalid.push(scope);
    let mut scope = set();
    scope.api_version = HOST_API;
    invalid.push(scope);
    let mut scope = set();
    scope.grants.push(scope.grants[0].clone());
    invalid.push(scope);
    let mut scope = set();
    scope.grants[1].resource_version = Some("different".into());
    invalid.push(scope);
    let mut scope = set();
    scope.grants[1].resource = Resource::Selection {
        vault_id: "vault".into(),
        selection_id: "lease".into(),
    };
    invalid.push(scope);
    let mut scope = set();
    scope.grants[0].resource_version = None;
    invalid.push(scope);
    let mut scope = set();
    scope.grants[2].resource_version = Some("version".into());
    invalid.push(scope);
    let mut scope = set();
    scope.grants[0].resource = Resource::Document {
        vault_id: "other".into(),
        document_id: "doc".into(),
    };
    invalid.push(scope);
    let mut scope = set();
    scope.expires_at = 1;
    invalid.push(scope);
    let mut scope = set();
    scope.grants = (0..33)
        .map(|i| PersistentGrant {
            capability: Capability::ReaderSurface,
            resource: Resource::Document {
                vault_id: "vault".into(),
                document_id: format!("d{i}"),
            },
            resource_version: Some("v".into()),
        })
        .collect();
    invalid.push(scope);
    for (index, scope) in invalid.into_iter().enumerate() {
        assert!(
            store
                .grant_set(&format!("bad-{index}"), 1, "org.test.reader", scope)
                .is_err()
        );
        assert_eq!(store.current("org.test.reader").unwrap().revision, 1);
    }
    assert_eq!(
        scratch
            .db()
            .query_row("SELECT count(*) FROM grants", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn profile2_metadata_cannot_downgrade_even_when_manifest_supports_legacy() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let p = package(false, 1);
    store.install("install", 0, &p).unwrap();
    let mut scope = set();
    scope.grants = vec![PersistentGrant {
        capability: Capability::DocumentMetadataRead,
        resource: doc(),
        resource_version: Some("version".into()),
    }];
    store.grant_set("set", 1, "org.test.reader", scope).unwrap();
    let snapshot = store
        .authority_snapshot("org.test.reader", AuthorityRead::Activation)
        .unwrap();
    assert_eq!(snapshot.required_api, Some(HOST_API_V0_2));
    drop(store);
    assert!(Store::open(&scratch.store()).is_err());
}

#[test]
fn legacy_bytes_commands_and_cross_profile_update_semantics_are_preserved() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let p = package(false, 1);
    store.install("install", 0, &p).unwrap();
    let scope = GrantScope {
        capability: Capability::DocumentMetadataRead,
        library: "vault".into(),
        document: "doc".into(),
        version: "version".into(),
        expires_at: u64::MAX,
    };
    let receipt = store
        .grant("grant", 1, "org.test.reader", scope.clone())
        .unwrap();
    let json: String = scratch
        .db()
        .query_row("SELECT record FROM grants", [], |r| r.get(0))
        .unwrap();
    let expected = SavedGrant {
        installation_id: receipt.installation_id,
        digest: p.digest().into(),
        generation: 2,
        policy_version: 1,
        scope,
    };
    assert_eq!(json, serde_json::to_string(&expected).unwrap());
    assert!(matches!(
        store.saved_authorization(&receipt).unwrap(),
        SavedAuthorization::V1(_)
    ));
    let mut v2 = set();
    v2.grants = vec![PersistentGrant {
        capability: Capability::DocumentMetadataRead,
        resource: doc(),
        resource_version: Some("version".into()),
    }];
    assert!(
        store
            .grant_set("grant", 2, "org.test.reader", v2.clone())
            .is_err()
    );
    let set_receipt = store.grant_set("set", 2, "org.test.reader", v2).unwrap();
    assert_eq!(set_receipt.generation, 3);
    assert!(store.saved_grant(&set_receipt).is_err());
    let update = package(false, 2);
    let updated = store.install("update", 3, &update).unwrap();
    assert!(store.saved_authorization(&updated).is_err());
    assert!(
        store
            .authority_snapshot("org.test.reader", AuthorityRead::Activation)
            .is_err()
    );
    store.uninstall("remove", 4, "org.test.reader").unwrap();
    let installed = store.install("again", 0, &update).unwrap();
    assert_ne!(installed.installation_id, receipt.installation_id);
    assert!(store.saved_authorization(&installed).is_err());
}

#[test]
fn maximum_set_is_whole_and_policy_precheck_is_outside_the_write_transaction() {
    struct SqlPolicy {
        policy: Policy,
        database: PathBuf,
    }
    impl GrantValidator for SqlPolicy {
        fn validate(&self, package: &VerifiedPackage, scope: &GrantScope) -> Result<()> {
            self.policy.validate(package, scope)
        }
        fn validate_set(&self, package: &VerifiedPackage, scope: &GrantSet) -> Result<()> {
            let db = Connection::open(&self.database).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            db.execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
                .expect("policy preflight must be outside a write transaction");
            self.policy.validate_set(package, scope)
        }
    }
    let scratch = Scratch::new();
    let mut store = Store::open_with_services(
        &scratch.store(),
        &SUPPORTED_HOST_APIS,
        StoreServices {
            grant_validator: Arc::new(SqlPolicy {
                policy: Policy::new(),
                database: scratch.store().join("state.sqlite3"),
            }),
            ..StoreServices::default()
        },
    )
    .unwrap();
    store.install("install", 0, &package(true, 1)).unwrap();
    let mut scope = set();
    scope.grants = (0..MAX_PERSISTENT_GRANTS)
        .map(|i| PersistentGrant {
            capability: Capability::ReaderSurface,
            resource: Resource::Document {
                vault_id: "vault".into(),
                document_id: format!("d{i}"),
            },
            resource_version: Some("v".into()),
        })
        .collect();
    let receipt = store
        .grant_set("maximum", 1, "org.test.reader", scope.clone())
        .unwrap();
    let snapshot = store
        .authority_snapshot("org.test.reader", AuthorityRead::Current { run_id: None })
        .unwrap();
    assert_eq!(snapshot.grants.len(), MAX_PERSISTENT_GRANTS);
    assert_eq!(
        store.saved_authorization(&receipt).unwrap(),
        SavedAuthorization::V2(SavedGrantSet {
            installation_id: receipt.installation_id,
            digest: receipt.digest.clone(),
            generation: receipt.generation,
            policy_version: 2,
            scope
        })
    );
}

#[test]
fn pending_private_change_and_unbound_spawn_prevent_new_persistent_intent() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    store.install("install", 0, &package(true, 1)).unwrap();
    // A corrupt pending record must still block mutation before any parser/executor.
    scratch
        .db()
        .execute("INSERT INTO private_pending VALUES (1,'pending','{}')", [])
        .unwrap();
    assert!(
        store
            .grant_set("blocked-pending", 1, "org.test.reader", set())
            .is_err()
    );
    scratch
        .db()
        .execute("DELETE FROM private_pending", [])
        .unwrap();
    let receipt = store
        .grant_set("first", 1, "org.test.reader", set())
        .unwrap();
    // This fixture exercises durable exclusion only, not OS identity or cleanup.
    let owner = extension_runtime_api::ProcessIdentity {
        pid: 1,
        boot_id: "00000000-0000-0000-0000-000000000001".into(),
        start_time_ticks: 1,
        uid: 1,
        exe_device: 1,
        exe_inode: 1,
        run_id: String::new(),
    };
    let boot = "a".repeat(32);
    let run = format!("{boot}-1");
    store
        .prepare_worker_lease(&receipt, &boot, &run, &owner)
        .unwrap();
    store.mark_worker_spawn_attempted(&run).unwrap();
    assert!(
        store
            .grant_set("blocked-worker", 2, "org.test.reader", set())
            .is_err()
    );
    assert!(
        store
            .authority_snapshot("org.test.reader", AuthorityRead::Activation)
            .is_err()
    );
    assert!(
        store
            .authority_snapshot(
                "org.test.reader",
                AuthorityRead::Current { run_id: Some(&run) }
            )
            .is_err()
    );
    assert_eq!(store.current("org.test.reader").unwrap().revision, 2);
    assert_eq!(store.worker_leases().unwrap().len(), 1);
}

#[test]
fn worker_run_sequences_survive_restart_beyond_the_probe_lifetime_budget() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    store.install("install", 0, &package(true, 1)).unwrap();
    let receipt = store
        .grant_set("first", 1, "org.test.reader", set())
        .unwrap();
    // Persistence/exclusion fixture only. The process adapter and real consumer
    // separately verify these identities against OS processes before binding.
    let owner = extension_runtime_api::ProcessIdentity {
        pid: 1,
        boot_id: "00000000-0000-0000-0000-000000000001".into(),
        start_time_ticks: 1,
        uid: 1,
        exe_device: 1,
        exe_inode: 1,
        run_id: String::new(),
    };
    let boot = "a".repeat(32);
    for sequence in [1, 256, 257, 65_536, u64::MAX] {
        let run = format!("{boot}-{sequence}");
        store
            .prepare_worker_lease(&receipt, &boot, &run, &owner)
            .unwrap();
        store.mark_worker_spawn_attempted(&run).unwrap();
        let mut worker = owner.clone();
        worker.pid = 2;
        worker.start_time_ticks = 2;
        worker.run_id = run.clone();
        store.bind_worker_lease(&run, worker).unwrap();
        drop(store);
        store = open(&scratch);
        assert_eq!(store.worker_leases().unwrap()[0].run_id, run);
        assert!(
            store
                .authority_snapshot("org.test.reader", AuthorityRead::Activation)
                .is_err()
        );
        store
            .authority_snapshot(
                "org.test.reader",
                AuthorityRead::Current { run_id: Some(&run) },
            )
            .unwrap();
        store.finish_worker_lease(&run).unwrap();
    }
    for suffix in ["0", "01", "+1", "1.0", "18446744073709551616"] {
        assert!(
            store
                .prepare_worker_lease(&receipt, &boot, &format!("{boot}-{suffix}"), &owner)
                .is_err()
        );
        assert!(store.worker_leases().unwrap().is_empty());
    }
}
