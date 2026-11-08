//! Compatibility coordinator for qualification. SQL ownership lives in extension-store.
use crate::package::VerifiedPackage;
use extension_api::{ApiVersion, Capability, CapabilityRequest, HOST_API};
use extension_host::{Grant, Host, Session};
use extension_store::{
    Checkpoint, CheckpointObserver, GrantValidator, PrivateStateExecutor, StoreError, StoreServices,
};
pub use extension_store::{
    GrantScope, PrivateReceipt, PrivateSnapshot, PrivateStatus, Receipt, RetainedPair, SavedGrant,
    WorkerLease,
};
use qusor_wasm_probe::{ExistingAuthority, Options, Package, Probe, RunResult};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};
mod managed;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn legacy(error: StoreError) -> Box<dyn std::error::Error> {
    match error {
        StoreError::Package(value) => Box::new(value),
        StoreError::Protocol(value) => Box::new(value),
        value => Box::new(value),
    }
}
#[derive(Default)]
struct ProbeServices {
    requested: Mutex<Option<String>>,
}
impl ProbeServices {
    fn set(&self, point: Option<&str>) {
        *self.requested.lock().expect("qualification observer lock") = point.map(str::to_owned);
    }
}
impl GrantValidator for ProbeServices {
    fn validate(
        &self,
        package: &VerifiedPackage,
        scope: &GrantScope,
    ) -> extension_store::Result<()> {
        let mut host = Host::new(&scope.library, [Capability::DocumentMetadataRead])?;
        let id = host.register(package.manifest().clone(), package.digest())?;
        host.replace_grants(
            id,
            vec![Grant {
                capability: scope.capability,
                resource: scope.resource(),
            }],
        )?;
        host.enable(id)?;
        Ok(())
    }
}
impl PrivateStateExecutor for ProbeServices {
    fn preflight(&self, module: &[u8], schema: u32, state: &[u8]) -> extension_store::Result<()> {
        qusor_wasm_probe::private_runtime::preflight(module, schema, state)
            .map_err(|e| StoreError::external(e.to_string()))
    }
    fn migrate(
        &self,
        module: &[u8],
        from: u32,
        to: u32,
        state: &[u8],
    ) -> extension_store::Result<Vec<u8>> {
        qusor_wasm_probe::private_runtime::run_migration(module, from, to, state)
            .map_err(|e| StoreError::external(e.to_string()))
    }
}
impl CheckpointObserver for ProbeServices {
    fn checkpoint(&self, point: Checkpoint) {
        let name = match point {
            Checkpoint::BeforeTransaction => "before_transaction",
            Checkpoint::AfterContent => "after_content",
            Checkpoint::BeforeCommit => "before_commit",
            Checkpoint::AfterCommit => "after_commit",
            Checkpoint::BeforePrepare => "before_prepare",
            Checkpoint::AfterPrepare => "after_prepare",
            Checkpoint::AfterMigration => "after_migration",
            Checkpoint::AfterSnapshot => "after_snapshot",
        };
        if self
            .requested
            .lock()
            .expect("qualification observer lock")
            .as_deref()
            == Some(name)
        {
            let _ =
                std::io::stderr().write_all(format!("injected process exit: {name}\n").as_bytes());
            std::process::exit(86);
        }
    }
}
pub struct Store {
    durable: extension_store::Store,
    sessions: BTreeMap<String, LiveSession>,
    services: Arc<ProbeServices>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_for_host(path, &[HOST_API])
    }
    pub fn open_for_host(path: &Path, supported: &[ApiVersion]) -> Result<Self> {
        let services = Arc::new(ProbeServices::default());
        let durable = extension_store::Store::open_with_services(
            path,
            supported,
            StoreServices {
                grant_validator: services.clone(),
                private_executor: services.clone(),
                observer: services.clone(),
            },
        )
        .map_err(legacy)?;
        Ok(Self {
            durable,
            sessions: BTreeMap::new(),
            services,
        })
    }
    fn prune_sessions(&mut self) {
        let Ok(items) = self.durable.status() else {
            self.sessions.clear();
            return;
        };
        let durable = &self.durable;
        self.sessions.retain(|_, live| {
            items
                .iter()
                .find(|item| item.installation_id == live.installation_id)
                .is_some_and(|item| {
                    durable
                        .saved_grant(item)
                        .is_ok_and(|grant| grant == live.grant)
                })
        });
    }
    pub fn status(&self) -> Result<Vec<Receipt>> {
        self.durable.status().map_err(legacy)
    }
    pub fn frozen_package_for_action(&self, id: &str) -> Result<(Receipt, VerifiedPackage)> {
        self.durable.frozen_package_for_action(id).map_err(legacy)
    }
    fn current_package(&self, item: &Receipt) -> Result<VerifiedPackage> {
        self.durable.current_package(item).map_err(legacy)
    }
    pub fn private_status(&self, id: &str) -> Result<PrivateStatus> {
        self.durable.private_status(id).map_err(legacy)
    }
    pub fn worker_leases(&self) -> Result<Vec<WorkerLease>> {
        self.durable.worker_leases().map_err(legacy)
    }
    pub(crate) fn prepare_worker_lease(
        &mut self,
        installation: &Receipt,
        boot: &str,
        run: &str,
        owner: &crate::process_identity::ProcessIdentity,
    ) -> Result<()> {
        self.durable
            .prepare_worker_lease(installation, boot, run, owner)
            .map_err(legacy)
    }
    pub(crate) fn mark_worker_spawn_attempted(&mut self, run: &str) -> Result<()> {
        self.durable
            .mark_worker_spawn_attempted(run)
            .map_err(legacy)
    }
    pub(crate) fn bind_worker_lease(
        &mut self,
        run: &str,
        worker: crate::process_identity::ProcessIdentity,
    ) -> Result<()> {
        self.durable.bind_worker_lease(run, worker).map_err(legacy)
    }
    pub(crate) fn finish_worker_lease(&mut self, run: &str) -> Result<()> {
        self.durable.finish_worker_lease(run).map_err(legacy)
    }
    pub fn install(
        &mut self,
        operation: &str,
        expected: i64,
        package: &VerifiedPackage,
        crash: Option<&str>,
    ) -> Result<Receipt> {
        validate_command(operation, expected, crash)?;
        self.services.set(crash);
        let result = self.durable.install(operation, expected, package);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn uninstall(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        crash: Option<&str>,
    ) -> Result<Receipt> {
        validate_command(operation, expected, crash)?;
        self.services.set(crash);
        let result = self.durable.uninstall(operation, expected, package_id);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn grant(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        scope: GrantScope,
        crash: Option<&str>,
    ) -> Result<Receipt> {
        validate_command(operation, expected, crash)?;
        self.services.set(crash);
        let result = self.durable.grant(operation, expected, package_id, scope);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn initialize_private(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        input: &[u8],
        crash: Option<&str>,
    ) -> Result<PrivateReceipt> {
        validate_private_command(operation, expected, crash, true)?;
        self.services.set(crash);
        let result = self
            .durable
            .initialize_private(operation, expected, package_id, input);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn upgrade_private(
        &mut self,
        operation: &str,
        expected: i64,
        package: &VerifiedPackage,
        crash: Option<&str>,
    ) -> Result<PrivateReceipt> {
        validate_private_command(operation, expected, crash, false)?;
        self.services.set(crash);
        let result = self.durable.upgrade_private(operation, expected, package);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn rollback_private(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        pair_id: i64,
        crash: Option<&str>,
    ) -> Result<PrivateReceipt> {
        validate_private_command(operation, expected, crash, false)?;
        self.services.set(crash);
        let result = self
            .durable
            .rollback_private(operation, expected, package_id, pair_id);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    pub fn stop(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        action: &str,
        crash: Option<&str>,
    ) -> Result<Receipt> {
        self.stop_for_worker(operation, expected, package_id, action, crash, None)
    }
    pub(crate) fn stop_for_worker(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        action: &str,
        crash: Option<&str>,
        run: Option<&str>,
    ) -> Result<Receipt> {
        if !["disable", "revoke"].contains(&action) {
            return Err("invalid stop action".into());
        }
        validate_command(operation, expected, crash)?;
        self.services.set(crash);
        let result = self
            .durable
            .stop_for_worker(operation, expected, package_id, action, run);
        self.services.set(None);
        let receipt = result.map_err(legacy)?;
        self.prune_sessions();
        Ok(receipt)
    }
    /// Rebuild the authoritative policy host and a fresh opaque native session.
    /// This is policy recovery only; no Wasm execution or Core read is claimed.
    pub fn resume(
        &mut self,
        package_id: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<String> {
        if self.sessions.len() >= 256 {
            return Err("session budget exhausted".into());
        }
        let (item, grant) = self
            .durable
            .execution_snapshot(package_id, None)
            .map_err(legacy)?;
        self.durable.check_private_ready(&item).map_err(legacy)?;
        grant.matches(library, document, version)?;
        let package = self.current_package(&item)?;
        let mut host = Host::new(library, [Capability::DocumentMetadataRead])?;
        let id = host.register(package.manifest().clone(), package.digest())?;
        host.replace_grants(
            id,
            vec![Grant {
                capability: grant.scope.capability,
                resource: grant.scope.resource(),
            }],
        )?;
        host.enable(id)?;
        let session = host.open_session(id)?;
        let mut entropy = [0u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut entropy)?;
        let token = format!("{:x}", Sha256::digest(entropy));
        self.sessions
            .retain(|_, live| live.installation_id != item.installation_id);
        self.sessions.insert(
            token.clone(),
            LiveSession {
                host,
                session,
                native_installation: id,
                installation_id: item.installation_id,
                grant_package: item.package_id.clone(),
                grant,
            },
        );
        Ok(token)
    }

    fn checked_session_grant(
        &self,
        token: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<(Receipt, SavedGrant)> {
        self.checked_session_grant_for_worker(token, library, document, version, None)
    }
    fn checked_session_grant_for_worker(
        &self,
        token: &str,
        library: &str,
        document: &str,
        version: &str,
        run_id: Option<&str>,
    ) -> Result<(Receipt, SavedGrant)> {
        let live = self.sessions.get(token).ok_or("unknown or stale session")?;
        let (item, current) = self
            .durable
            .execution_snapshot(&live.grant_package, run_id)
            .map_err(legacy)?;
        if item.installation_id != live.installation_id {
            return Err("installation removed".into());
        }
        if current != live.grant {
            return Err("stale grant".into());
        }
        current.matches(library, document, version)?;
        Ok((item, current))
    }

    fn dispatch_session_check(&mut self, token: &str, current: &SavedGrant) -> Result<()> {
        let live = self.sessions.get_mut(token).ok_or("unknown session")?;
        let permit = live.host.authorize(
            &live.session,
            CapabilityRequest {
                api_version: extension_api::HOST_API,
                request_id: "recovery-check".into(),
                capability: Capability::DocumentMetadataRead,
                resource: current.scope.resource(),
            },
        )?;
        live.host.dispatch(permit, |_| Ok(()))?;
        Ok(())
    }

    pub fn check_session(
        &mut self,
        token: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<()> {
        let (item, current) = self.checked_session_grant(token, library, document, version)?;
        self.durable.check_private_ready(&item).map_err(legacy)?;
        self.dispatch_session_check(token, &current)
    }

    /// Consume one recovered session to run the stored immutable package.
    /// This synchronous synthetic service has no Core access or persistent worker.
    pub fn execute(
        &mut self,
        token: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<RunResult> {
        self.check_session(token, library, document, version)?;
        let live = self
            .sessions
            .remove(token)
            .ok_or("unknown or stale session")?;
        let item = self
            .status()?
            .into_iter()
            .find(|item| item.installation_id == live.installation_id)
            .ok_or("installation removed")?;
        let package = self.current_package(&item)?;
        let runtime = Package::from_host_bytes(
            package.manifest().clone(),
            package.digest().into(),
            package.module_bytes().to_vec(),
        )?;
        let response = serde_json::to_vec(&serde_json::json!({
            "synthetic": true, "library": library, "document": document,
            "version": version, "title": "Synthetic metadata fixture",
        }))?;
        let request = CapabilityRequest {
            api_version: extension_api::HOST_API,
            request_id: "execution-check".into(),
            capability: Capability::DocumentMetadataRead,
            resource: live.grant.scope.resource(),
        };
        let probe = Probe::from_authority(
            runtime,
            Options::default(),
            ExistingAuthority {
                host: live.host,
                installation: live.native_installation,
                session: live.session,
                request,
                response,
                expires_at: Some(live.grant.scope.expires_at),
            },
        )?;
        Ok(probe.run()?)
    }
}
struct LiveSession {
    host: Host,
    session: Session,
    native_installation: extension_host::InstallationId,
    installation_id: i64,
    grant_package: String,
    grant: SavedGrant,
}
pub fn read_package(path: &Path) -> Result<Vec<u8>> {
    extension_store::read_package(path).map_err(legacy)
}
fn validate_command(operation: &str, expected: i64, crash: Option<&str>) -> Result<()> {
    if operation.is_empty()
        || operation.len() > 128
        || !operation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        || expected < 0
    {
        return Err("invalid operation ID or expected revision".into());
    }
    if crash.is_some_and(|point| {
        ![
            "before_transaction",
            "after_content",
            "before_commit",
            "after_commit",
        ]
        .contains(&point)
    }) {
        return Err("unknown crash injection point".into());
    }
    Ok(())
}

fn validate_private_command(
    operation: &str,
    expected: i64,
    crash: Option<&str>,
    initialize: bool,
) -> Result<()> {
    validate_command(operation, expected, None)?;
    if crash.is_some_and(|point| {
        !if initialize {
            [
                "before_prepare",
                "after_snapshot",
                "before_commit",
                "after_commit",
            ]
            .as_slice()
        } else {
            [
                "before_prepare",
                "after_prepare",
                "after_migration",
                "after_snapshot",
                "before_commit",
                "after_commit",
            ]
            .as_slice()
        }
        .contains(&point)
    }) {
        return Err("unknown private crash injection point".into());
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod compatibility_tests {
    use super::*;
    use crate::package::{PackageFile, build_signed, publisher_key, verify};

    #[test]
    fn receipt_replays_do_not_revoke_unchanged_live_sessions() {
        let root =
            std::env::temp_dir().join(format!("qusor-management-replay-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let manifest = extension_api::Manifest::from_json(r#"{"manifest_version":1,"package_id":"org.test.replay","publisher_id":"org.test","package_version":{"major":1,"minor":0,"patch":0},"host_api":{"major":0,"min_minor":1,"max_minor":1},"required_capabilities":["document.metadata.read"],"optional_capabilities":[],"contributions":[{"id":"org.test.replay.command","title":"Metadata","kind":"command","required_capabilities":["document.metadata.read"]}]}"#).unwrap();
        let seed = [13; 32];
        let bytes = build_signed(
            manifest,
            "module.wasm",
            vec![PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: b"\0asm\x01\0\0\0".to_vec(),
            }],
            &seed,
        )
        .unwrap();
        let package = verify(&bytes, &publisher_key(&seed)).unwrap();
        let mut store = Store::open(&root.join("store")).unwrap();
        let installed = store.install("install", 0, &package, None).unwrap();
        let scope = GrantScope {
            capability: Capability::DocumentMetadataRead,
            library: "vault".into(),
            document: "doc".into(),
            version: "version".into(),
            expires_at: u64::MAX,
        };
        let granted = store
            .grant("grant", 1, &installed.package_id, scope.clone(), None)
            .unwrap();
        let token = store
            .resume(&installed.package_id, "vault", "doc", "version")
            .unwrap();
        assert_eq!(
            store.install("install", 0, &package, None).unwrap(),
            installed
        );
        store
            .check_session(&token, "vault", "doc", "version")
            .unwrap();
        assert_eq!(
            store
                .grant("grant", 1, &installed.package_id, scope, None)
                .unwrap(),
            granted
        );
        store
            .check_session(&token, "vault", "doc", "version")
            .unwrap();
        store
            .stop(
                "revoke",
                granted.revision,
                &installed.package_id,
                "revoke",
                None,
            )
            .unwrap();
        assert!(
            store
                .check_session(&token, "vault", "doc", "version")
                .is_err()
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
