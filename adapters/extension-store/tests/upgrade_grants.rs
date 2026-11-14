#![cfg(unix)]
use extension_api::*;
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_ports::{AuthorityRead, AuthoritySource, Grant};
use extension_store::{
    GrantScope, GrantSet, GrantValidator, PersistentGrant, Result, SavedAuthorization, Store,
    StoreError, StoreErrorCode, StoreServices, UpgradeReceipt,
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-upgrade-grants-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn store(&self) -> PathBuf {
        self.0.join("store")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

fn open(scratch: &Scratch) -> Store {
    let services = StoreServices {
        grant_validator: Arc::new(Policy::new()),
        ..StoreServices::default()
    };
    Store::open_with_services(&scratch.store(), &SUPPORTED_HOST_APIS, services).unwrap()
}

fn build_pkg(
    seed: [u8; 32],
    publisher_id: &str,
    major: u32,
    host_api: ApiRange,
    required: Vec<Capability>,
    optional: Vec<Capability>,
    wasm_tag: &[u8],
) -> VerifiedPackage {
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.upgrade".into(),
        publisher_id: publisher_id.into(),
        package_version: PackageVersion {
            major,
            minor: 0,
            patch: 0,
        },
        host_api,
        required_capabilities: required,
        optional_capabilities: optional,
        contributions: vec![],
    };
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    wasm.extend_from_slice(wasm_tag);
    let bytes = build_signed_for_host(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
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

fn v1_scope() -> GrantScope {
    GrantScope {
        capability: Capability::DocumentMetadataRead,
        library: "vault".into(),
        document: "doc".into(),
        version: "version".into(),
        expires_at: u64::MAX,
    }
}

fn v2_set(with_capture: bool) -> GrantSet {
    let mut grants = vec![
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
    ];
    if with_capture {
        grants.push(PersistentGrant {
            capability: Capability::CapturePropose,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
            resource_version: None,
        });
    }
    GrantSet {
        api_version: HOST_API_V0_2,
        vault_id: "vault".into(),
        expires_at: u64::MAX,
        grants,
    }
}

#[test]
fn same_privilege_v1_upgrade_preserves_grants_and_enable_intent() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [71; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    let granted = store
        .grant("grant-1", 1, "org.test.upgrade", v1_scope())
        .unwrap();
    assert_eq!((granted.revision, granted.generation), (2, 2));

    let v2 = build_pkg(
        [71; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v2",
    );
    let UpgradeReceipt {
        installation,
        grants_preserved,
    } = store.upgrade_package("upgrade-2", 2, &v2).unwrap();
    assert!(grants_preserved);
    assert_eq!(installation.revision, 3);
    assert_eq!(installation.generation, 3);
    assert_eq!(installation.digest, v2.digest());
    assert_eq!(installation.status, "disabled");

    let saved = store.saved_grant(&installation).unwrap();
    assert_eq!(saved.digest, v2.digest());
    assert_eq!(saved.generation, 3);
    assert_eq!(saved.scope, v1_scope());

    let snapshot = store
        .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
        .unwrap();
    assert!(snapshot.enable_intent);
    assert_eq!(snapshot.digest, v2.digest());
    assert_eq!(snapshot.grants.len(), 1);
    assert_eq!(
        snapshot.grants[0].grant.capability,
        Capability::DocumentMetadataRead
    );
}

#[test]
fn same_privilege_v2_upgrade_preserves_grants_and_prunes_dropped_optional_capability() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [72; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        vec![Capability::ReaderSurface],
        vec![Capability::SelectionRead, Capability::CapturePropose],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    store
        .grant_set("grant-1", 1, "org.test.upgrade", v2_set(true))
        .unwrap();

    // v2 keeps required ReaderSurface + optional SelectionRead, drops optional CapturePropose.
    let v2 = build_pkg(
        [72; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        vec![Capability::ReaderSurface],
        vec![Capability::SelectionRead],
        b"v2",
    );
    let receipt = store.upgrade_package("upgrade-2", 2, &v2).unwrap();
    assert!(receipt.grants_preserved);
    assert_eq!(receipt.installation.status, "disabled");

    let SavedAuthorization::V2(saved) = store.saved_authorization(&receipt.installation).unwrap()
    else {
        panic!("expected V2 saved authorization");
    };
    assert_eq!(saved.digest, v2.digest());
    assert_eq!(saved.scope.grants.len(), 2);
    assert!(
        saved
            .scope
            .grants
            .iter()
            .all(|g| g.capability != Capability::CapturePropose)
    );

    let snapshot = store
        .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
        .unwrap();
    assert!(snapshot.enable_intent);
    assert_eq!(snapshot.grants.len(), 2);
}

#[test]
fn privilege_escalating_upgrade_clears_grants_and_requires_reauthorization() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [73; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        vec![Capability::ReaderSurface],
        vec![Capability::SelectionRead],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    store
        .grant_set("grant-1", 1, "org.test.upgrade", v2_set(false))
        .unwrap();

    // v2 escalates required_capabilities with CapturePropose (not previously granted).
    let v2 = build_pkg(
        [73; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        vec![Capability::ReaderSurface, Capability::CapturePropose],
        vec![Capability::SelectionRead],
        b"v2",
    );
    let receipt = store.upgrade_package("upgrade-2", 2, &v2).unwrap();
    assert!(!receipt.grants_preserved);
    assert_eq!(receipt.installation.status, "disabled");
    assert_eq!(receipt.installation.digest, v2.digest());

    let err = store
        .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);

    // User explicitly re-authorizes the expanded capability set.
    let regranted = store
        .grant_set("regrant-3", 3, "org.test.upgrade", v2_set(true))
        .unwrap();
    assert_eq!(regranted.status, "disabled");
    let snapshot = store
        .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
        .unwrap();
    assert!(snapshot.enable_intent);
    assert_eq!(snapshot.grants.len(), 3);
}

#[test]
fn api_incompatible_upgrade_clears_grants_until_reauthorized() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [74; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 2,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    store
        .grant("grant-1", 1, "org.test.upgrade", v1_scope())
        .unwrap();

    // v2 drops HOST_API (0.1) support required by the V1 grant.
    let v2 = build_pkg(
        [74; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 2,
            max_minor: 2,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v2",
    );
    let receipt = store.upgrade_package("upgrade-2", 2, &v2).unwrap();
    assert!(!receipt.grants_preserved);
    assert_eq!(receipt.installation.status, "disabled");
    assert_eq!(
        store
            .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );
}

#[test]
fn revoked_or_disabled_package_stays_unauthorized_after_upgrade() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [75; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    store
        .grant("grant-1", 1, "org.test.upgrade", v1_scope())
        .unwrap();
    let stopped = store
        .stop("revoke-2", 2, "org.test.upgrade", "revoke")
        .unwrap();
    assert_eq!(stopped.status, "disabled");

    let v2 = build_pkg(
        [75; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v2",
    );
    let receipt = store.upgrade_package("upgrade-3", 3, &v2).unwrap();
    assert!(!receipt.grants_preserved);
    assert_eq!(receipt.installation.status, "disabled");
    assert_eq!(
        store
            .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );
}

#[test]
fn identity_change_downgrade_or_revision_conflict_rejects_without_mutating_grants() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v2 = build_pkg(
        [76; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v2",
    );
    store.install("install-1", 0, &v2).unwrap();
    store
        .grant("grant-1", 1, "org.test.upgrade", v1_scope())
        .unwrap();

    // Publisher key mismatch.
    let wrong_key = build_pkg(
        [77; 32],
        "org.test",
        3,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v3-wrong-key",
    );
    assert_eq!(
        store
            .upgrade_package("upgrade-key", 2, &wrong_key)
            .unwrap_err()
            .code(),
        StoreErrorCode::Conflict
    );

    // Non-advancing version (downgrade to v1).
    let downgrade = build_pkg(
        [76; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v1-downgrade",
    );
    assert_eq!(
        store
            .upgrade_package("upgrade-down", 2, &downgrade)
            .unwrap_err()
            .code(),
        StoreErrorCode::Conflict
    );

    // Stale expected revision.
    let v3 = build_pkg(
        [76; 32],
        "org.test",
        3,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v3",
    );
    assert_eq!(
        store
            .upgrade_package("upgrade-stale", 1, &v3)
            .unwrap_err()
            .code(),
        StoreErrorCode::Conflict
    );

    // Original v2 authorization and revision remain intact.
    let snapshot = store
        .authority_snapshot("org.test.upgrade", AuthorityRead::Activation)
        .unwrap();
    assert_eq!(snapshot.digest, v2.digest());
    assert!(snapshot.enable_intent);
    assert_eq!(store.current("org.test.upgrade").unwrap().revision, 2);
}

#[test]
fn upgrade_package_replays_idempotently_and_rejects_operation_reuse() {
    let scratch = Scratch::new();
    let mut store = open(&scratch);
    let v1 = build_pkg(
        [78; 32],
        "org.test",
        1,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v1",
    );
    store.install("install-1", 0, &v1).unwrap();
    store
        .grant("grant-1", 1, "org.test.upgrade", v1_scope())
        .unwrap();

    let v2 = build_pkg(
        [78; 32],
        "org.test",
        2,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v2",
    );
    let first = store.upgrade_package("upgrade-op", 2, &v2).unwrap();
    let replay = store.upgrade_package("upgrade-op", 2, &v2).unwrap();
    assert_eq!(first, replay);

    let v3 = build_pkg(
        [78; 32],
        "org.test",
        3,
        ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        vec![Capability::DocumentMetadataRead],
        vec![],
        b"v3",
    );
    assert_eq!(
        store
            .upgrade_package("upgrade-op", 2, &v3)
            .unwrap_err()
            .code(),
        StoreErrorCode::Conflict
    );
}
