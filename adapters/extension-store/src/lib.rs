//! Durable package/install state. This store provides no runtime authorization or sessions.
use extension_api::{ApiVersion, Capability, HOST_API, Resource};
use extension_package::package::{MAX_PACKAGE_BYTES, VerifiedPackage, verify_for_host};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::Path,
};

mod verified_cache;
mod worker_leases;
pub use worker_leases::WorkerLease;
mod private_lifecycle;
pub use private_lifecycle::{PrivateReceipt, PrivateSnapshot, PrivateStatus, RetainedPair};

mod error;
pub use error::{StoreError, StoreErrorCode};
pub type Result<T> = std::result::Result<T, StoreError>;
mod component;
mod grant_set;
pub use grant_set::*;
mod services;
pub use services::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub operation_id: String,
    pub installation_id: i64,
    pub package_id: String,
    pub digest: String,
    pub revision: i64,
    pub generation: i64,
    pub status: String,
}

/// Dedicated local 0700 directory, single writer. Not a defense against the account owner.
/// Grants persist as enable intent; runtime sessions remain process-local.
/// Installation status does not claim that a runtime has started.
// A borrowed pre-exec descriptor must not prolong this instance's writer lease.
// The Connection field is declared first and closes before this guard is dropped.
struct WriterLock(File);
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub struct Store {
    db: Connection,
    _lock: WriterLock,
    services: StoreServices,
    supported_apis: Vec<ApiVersion>,
    verified_cache: std::cell::RefCell<verified_cache::VerifiedCache>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_for_host(path, &[HOST_API])
    }

    /// Trusted host compatibility, not a package-supplied permission or saved grant.
    /// Legacy callers continue to support only 0.1. Every stored byte is reverified
    /// against this running host's explicit version set on open and before use.
    pub fn open_for_host(path: &Path, supported_apis: &[ApiVersion]) -> Result<Self> {
        Self::open_with_services(path, supported_apis, StoreServices::default())
    }

    pub fn open_with_services(
        path: &Path,
        supported_apis: &[ApiVersion],
        services: StoreServices,
    ) -> Result<Self> {
        extension_api::validate_supported_apis(supported_apis)?;
        if !cfg!(unix) {
            return Err(StoreError::unsupported(
                "store requires a qualified native filesystem profile",
            ));
        }
        private_directory(path)?;
        for name in ["writer.lock", "state.sqlite3", "state.sqlite3-journal"] {
            reject_nonfile(&path.join(name))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("writer.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| StoreError::busy("store has another writer"))?;
        let lock = WriterLock(lock);
        let mut db = Connection::open(path.join("state.sqlite3"))?;
        db.execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        let schema: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        match schema {
            0 => {
                let existing: i64 = db.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |r| r.get(0),
                )?;
                if existing != 0 {
                    return Err(StoreError::corrupt(
                        "unversioned nonempty store needs repair",
                    ));
                }
                let tx = db.transaction()?;
                tx.execute_batch("CREATE TABLE content (digest TEXT PRIMARY KEY, bytes BLOB NOT NULL);
                    CREATE TABLE trust (package_id TEXT PRIMARY KEY, publisher TEXT NOT NULL, key TEXT NOT NULL);
                    CREATE TABLE installations (
                      id INTEGER PRIMARY KEY AUTOINCREMENT, package_id TEXT NOT NULL UNIQUE,
                      digest TEXT NOT NULL REFERENCES content(digest), revision INTEGER NOT NULL,
                      generation INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('disabled','uninstalled')));
                    CREATE TABLE operations (operation_id TEXT PRIMARY KEY, input_hash TEXT NOT NULL, receipt TEXT NOT NULL);
                    PRAGMA user_version=1;")?;
                tx.commit()?;
            }
            1 => {}
            2 => {}
            3 => {}
            4 => {}
            _ => return Err(StoreError::corrupt("unknown store schema needs repair")),
        }
        if schema < 2 {
            let tx = db.transaction()?;
            tx.execute_batch("CREATE TABLE grants (installation_id INTEGER PRIMARY KEY REFERENCES installations(id) ON DELETE CASCADE, record TEXT NOT NULL); CREATE TABLE installation_library (installation_id INTEGER PRIMARY KEY REFERENCES installations(id) ON DELETE CASCADE, library TEXT NOT NULL); PRAGMA user_version=2;")?;
            tx.commit()?;
        }
        if schema < 3 {
            private_lifecycle::create_schema(&mut db)?;
        }
        if schema < 4 {
            worker_leases::create_schema(&mut db)?;
        }
        let integrity: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::corrupt("store integrity check failed"));
        }
        let mut store = Self {
            db,
            _lock: lock,
            services,
            supported_apis: supported_apis.to_vec(),
            verified_cache: std::cell::RefCell::new(verified_cache::VerifiedCache::default()),
        };
        store.check_current()?;
        store.check_grants()?;
        store.check_private()?;
        store.worker_leases()?;
        store.recover_private()?;
        Ok(store)
    }

    /// Trusted adapter access to frozen, verified installed bytes. This grants no authority.
    pub fn frozen_package_for_action(
        &self,
        package_id: &str,
    ) -> Result<(Receipt, VerifiedPackage)> {
        let receipt = self.current(package_id)?;
        if receipt.status == "uninstalled" {
            return Err(StoreError::conflict("package uninstalled"));
        }
        self.ensure_worker_access(receipt.installation_id, None)?;
        self.check_private_ready(&receipt)?;
        let package = self.current_package(&receipt)?;
        Ok((receipt, package))
    }

    pub fn status(&self) -> Result<Vec<Receipt>> {
        let mut query = self.db.prepare(
            "SELECT id,package_id,digest,revision,generation,status FROM installations ORDER BY id",
        )?;
        let rows = query.query_map([], |r| {
            Ok(Receipt {
                operation_id: String::new(),
                installation_id: r.get(0)?,
                package_id: r.get(1)?,
                digest: r.get(2)?,
                revision: r.get(3)?,
                generation: r.get(4)?,
                status: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The trusted caller explicitly pins a key on first install. Existing bindings
    /// cannot be overridden by passing another key on an update or reinstall.
    pub fn install(
        &mut self,
        operation: &str,
        expected: i64,
        package: &VerifiedPackage,
    ) -> Result<Receipt> {
        validate_command(operation, expected)?;
        let input = input_hash(&(
            "install",
            expected,
            package.digest(),
            package.publisher_key(),
        ))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        self.check_budget(Some(package))?;
        let manifest = package.manifest();
        let binding: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT publisher,key FROM trust WHERE package_id=?1",
                [&manifest.package_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if binding.as_ref().is_some_and(|(publisher, key)| {
            publisher != &manifest.publisher_id || key != package.publisher_key()
        }) {
            return Err(StoreError::conflict("publisher key binding conflict"));
        }
        let current = self
            .status()?
            .into_iter()
            .find(|r| r.package_id == manifest.package_id);
        let (revision, generation) = if let Some(old) = &current {
            if old.status == "uninstalled" {
                if expected != 0 {
                    return Err(StoreError::conflict("reinstall requires revision zero"));
                }
                (checked_next(old.revision)?, checked_next(old.generation)?)
            } else {
                self.ensure_no_pending(old.installation_id)?;
                self.ensure_worker_access(old.installation_id, None)?;
                if self.has_private_head(old.installation_id)? {
                    return Err(StoreError::conflict(
                        "private state requires paired upgrade",
                    ));
                }
                if old.revision != expected {
                    return Err(StoreError::conflict("installation revision conflict"));
                }
                let bytes: Vec<u8> = self.db.query_row(
                    "SELECT bytes FROM content WHERE digest=?1 AND length(bytes)<=?2",
                    params![old.digest, MAX_PACKAGE_BYTES as i64],
                    |r| r.get(0),
                )?;
                let previous =
                    verify_for_host(&bytes, package.publisher_key(), &self.supported_apis)?;
                if manifest.package_version <= previous.manifest().package_version
                    || old.digest == package.digest()
                {
                    return Err(StoreError::conflict(
                        "update requires newer version and different digest",
                    ));
                }
                (checked_next(old.revision)?, checked_next(old.generation)?)
            }
        } else {
            if expected != 0 {
                return Err(StoreError::conflict(
                    "new installation requires revision zero",
                ));
            }
            (1, 1)
        };
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO content VALUES (?1,?2)",
            params![package.digest(), package.raw_bytes()],
        )?;
        // INSERT OR IGNORE must not bless corrupt or mismatched pre-existing bytes.
        let stored: Vec<u8> = tx.query_row(
            "SELECT bytes FROM content WHERE digest=?1 AND length(bytes)<=?2",
            params![package.digest(), MAX_PACKAGE_BYTES as i64],
            |r| r.get(0),
        )?;
        let frozen = verify_for_host(&stored, package.publisher_key(), &self.supported_apis)?;
        if frozen.digest() != package.digest() {
            return Err(StoreError::corrupt("stored content digest mismatch"));
        }
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        tx.execute(
            "INSERT OR IGNORE INTO trust VALUES (?1,?2,?3)",
            params![
                manifest.package_id,
                manifest.publisher_id,
                package.publisher_key()
            ],
        )?;
        tx.execute("DELETE FROM grants WHERE installation_id IN (SELECT id FROM installations WHERE package_id=?1)", [&manifest.package_id])?;
        if current.as_ref().is_some_and(|r| r.status == "uninstalled") {
            tx.execute(
                "DELETE FROM installations WHERE package_id=?1",
                [&manifest.package_id],
            )?;
        }
        tx.execute("INSERT INTO installations(package_id,digest,revision,generation,status) VALUES (?1,?2,?3,?4,'disabled')
                    ON CONFLICT(package_id) DO UPDATE SET digest=excluded.digest,revision=excluded.revision,generation=excluded.generation,status='disabled'",
                   params![manifest.package_id, package.digest(), revision, generation])?;
        let id = tx.query_row(
            "SELECT id FROM installations WHERE package_id=?1",
            [&manifest.package_id],
            |r| r.get(0),
        )?;
        let receipt = Receipt {
            operation_id: operation.into(),
            installation_id: id,
            package_id: manifest.package_id.clone(),
            digest: package.digest().into(),
            revision,
            generation,
            status: "disabled".into(),
        };
        tx.execute(
            "INSERT INTO operations VALUES (?1,?2,?3)",
            params![operation, input, serde_json::to_string(&receipt)?],
        )?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }

    /// Tombstone only: retains content and history, has no access to Core knowledge.
    pub fn uninstall(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
    ) -> Result<Receipt> {
        validate_command(operation, expected)?;
        let input = input_hash(&("uninstall", expected, package_id))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        self.check_budget(None)?;
        let old = self
            .status()?
            .into_iter()
            .find(|r| r.package_id == package_id)
            .ok_or_else(|| StoreError::conflict("installation not found"))?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            status: "uninstalled".into(),
            ..old
        };
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        tx.execute(
            "DELETE FROM grants WHERE installation_id=?1",
            [receipt.installation_id],
        )?;
        tx.execute(
            "UPDATE installations SET revision=?1,generation=?2,status='uninstalled' WHERE id=?3",
            params![
                receipt.revision,
                receipt.generation,
                receipt.installation_id
            ],
        )?;
        tx.execute(
            "INSERT INTO operations VALUES (?1,?2,?3)",
            params![operation, input, serde_json::to_string(&receipt)?],
        )?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }

    /// Only exact document metadata scopes can persist. No selection or write approval.
    pub fn grant(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        scope: GrantScope,
    ) -> Result<Receipt> {
        validate_command(operation, expected)?;
        scope.validate()?;
        let input = input_hash(&("grant", expected, package_id, &scope))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        if scope.expires_at <= now()? {
            return Err(StoreError::conflict("grant already expired"));
        }
        self.check_budget(None)?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let bound: Option<String> = self
            .db
            .query_row(
                "SELECT library FROM installation_library WHERE installation_id=?1",
                [old.installation_id],
                |r| r.get(0),
            )
            .optional()?;
        if bound
            .as_ref()
            .is_some_and(|library| library != &scope.library)
        {
            return Err(StoreError::conflict(
                "installation belongs to another library",
            ));
        }
        self.check_private_ready(&old)?;
        let package = self.current_package(&old)?;
        self.services.grant_validator.validate(&package, &scope)?;
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        let record = SavedGrant {
            installation_id: receipt.installation_id,
            digest: receipt.digest.clone(),
            generation: receipt.generation,
            policy_version: 1,
            scope,
        };
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO installation_library VALUES (?1,?2)",
            params![receipt.installation_id, record.scope.library],
        )?;
        tx.execute("INSERT INTO grants VALUES (?1,?2) ON CONFLICT(installation_id) DO UPDATE SET record=excluded.record", params![receipt.installation_id, serde_json::to_string(&record)?])?;
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        save_management(&tx, operation, &input, &receipt)?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }

    /// Experimental disable and revoke both clear auto-enable intent and its grant.
    pub fn stop(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        action: &str,
    ) -> Result<Receipt> {
        self.stop_for_worker(operation, expected, package_id, action, None)
    }
    pub fn stop_for_worker(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        action: &str,
        run_id: Option<&str>,
    ) -> Result<Receipt> {
        if !["disable", "revoke"].contains(&action) {
            return Err(StoreError::invalid("invalid stop action"));
        }
        validate_command(operation, expected)?;
        let input = input_hash(&(action, expected, package_id))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        self.check_budget(None)?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_control(old.installation_id, run_id)?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        tx.execute(
            "DELETE FROM grants WHERE installation_id=?1",
            [receipt.installation_id],
        )?;
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        save_management(&tx, operation, &input, &receipt)?;
        if let Some(run_id) = run_id {
            worker_leases::fence(&tx, run_id, &receipt)?;
        }
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }

    /// Read durable facts only. A Host must independently authorize each operation.
    pub fn execution_snapshot(
        &self,
        package_id: &str,
        run_id: Option<&str>,
    ) -> Result<(Receipt, SavedGrant)> {
        let item = self.current(package_id)?;
        self.ensure_no_pending(item.installation_id)?;
        self.ensure_worker_access(item.installation_id, run_id)?;
        let grant = self.saved_grant(&item)?;
        Ok((item, grant))
    }

    pub fn current(&self, package_id: &str) -> Result<Receipt> {
        self.status()?
            .into_iter()
            .find(|r| r.package_id == package_id)
            .ok_or_else(|| StoreError::conflict("installation not found"))
    }
    pub fn current_package(&self, item: &Receipt) -> Result<VerifiedPackage> {
        let (bytes, key, publisher): (Vec<u8>, String, String) = self.db.query_row("SELECT content.bytes,trust.key,trust.publisher FROM content,trust WHERE content.digest=?1 AND trust.package_id=?2 AND length(content.bytes)<=?3", params![item.digest,item.package_id,MAX_PACKAGE_BYTES as i64], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let package = verify_for_host(&bytes, &key, &self.supported_apis)?;
        if package.digest() != item.digest
            || package.manifest().package_id != item.package_id
            || package.manifest().publisher_id != publisher
        {
            return Err(StoreError::corrupt("package binding needs repair"));
        }
        Ok(package)
    }
    /// Fresh SQL material on every call. Only identical authenticated material
    /// may reuse parsing/signature work; no cache borrow survives this method's
    /// lookup/insert, and no installation or private-state fact is memoized.
    fn current_package_shared(&self, item: &Receipt) -> Result<std::sync::Arc<VerifiedPackage>> {
        let (bytes, key, publisher): (Vec<u8>, String, String) = self.db.query_row("SELECT content.bytes,trust.key,trust.publisher FROM content,trust WHERE content.digest=?1 AND trust.package_id=?2 AND length(content.bytes)<=?3", params![item.digest,item.package_id,MAX_PACKAGE_BYTES as i64], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let cached = {
            self.verified_cache
                .borrow_mut()
                .get(&bytes, &key, &self.supported_apis)
        };
        let was_cached = cached.is_some();
        let package = match cached {
            Some(package) => package,
            None => std::sync::Arc::new(verify_for_host(&bytes, &key, &self.supported_apis)?),
        };
        if package.digest() != item.digest
            || package.manifest().package_id != item.package_id
            || package.manifest().publisher_id != publisher
        {
            return Err(StoreError::corrupt("package binding needs repair"));
        }
        if !was_cached {
            self.verified_cache
                .borrow_mut()
                .insert(std::sync::Arc::clone(&package), &self.supported_apis);
        }
        Ok(package)
    }
    pub fn saved_grant(&self, item: &Receipt) -> Result<SavedGrant> {
        match self.saved_authorization(item)? {
            SavedAuthorization::V1(record) => Ok(record),
            SavedAuthorization::V2(_) => Err(StoreError::unsupported(
                "profile 2 cannot be projected into a legacy grant",
            )),
        }
    }
    fn check_grants(&self) -> Result<()> {
        let mut bindings = self
            .db
            .prepare("SELECT installation_id,library FROM installation_library")?;
        let rows = bindings.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let installations = self.status()?;
        for row in rows {
            let (id, library) = row?;
            if !installations.iter().any(|item| item.installation_id == id) {
                return Err(StoreError::corrupt("orphaned library binding"));
            }
            Resource::Vault { vault_id: library }.validate()?;
        }
        let mut query = self.db.prepare("SELECT installation_id FROM grants")?;
        let ids = query
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let items = self.status()?;
        for id in ids {
            self.saved_authorization(
                items
                    .iter()
                    .find(|r| r.installation_id == id)
                    .ok_or_else(|| StoreError::corrupt("orphaned grant"))?,
            )?;
        }
        Ok(())
    }

    fn replay(&self, operation: &str, input: &str) -> Result<Option<Receipt>> {
        let private_used: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM private_operations WHERE operation_id=?1) OR EXISTS(SELECT 1 FROM private_pending WHERE operation_id=?1)", [operation], |r|r.get(0))?;
        if private_used {
            return Err(StoreError::conflict(
                "operation ID reused across command types",
            ));
        }
        let saved: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT input_hash,receipt FROM operations WHERE operation_id=?1",
                [operation],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match saved {
            Some((hash, receipt)) if hash == input => Ok(Some(serde_json::from_str(&receipt)?)),
            Some(_) => Err(StoreError::conflict(
                "operation ID reused with different input",
            )),
            None => Ok(None),
        }
    }

    fn check_budget(&self, package: Option<&VerifiedPackage>) -> Result<()> {
        let (count, bytes, ops): (i64,i64,i64) = self.db.query_row(
            "SELECT (SELECT count(*) FROM installations),(SELECT coalesce(sum(length(bytes)),0) FROM content),(SELECT count(*) FROM operations)+(SELECT count(*) FROM private_operations)+(SELECT count(*) FROM private_pending)",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let (extra_count, extra_bytes) = if let Some(package) = package {
            let has_install: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM installations WHERE package_id=?1)",
                [&package.manifest().package_id],
                |r| r.get(0),
            )?;
            let has_content: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM content WHERE digest=?1)",
                [package.digest()],
                |r| r.get(0),
            )?;
            (
                i64::from(!has_install),
                if has_content {
                    0
                } else {
                    package.raw_bytes().len() as i64
                },
            )
        } else {
            (0, 0)
        };
        if count + extra_count > 1024 || bytes + extra_bytes > 64 * 1024 * 1024 || ops >= 8192 {
            return Err(StoreError::invalid("store budget exhausted"));
        }
        Ok(())
    }

    fn check_current(&self) -> Result<()> {
        for item in self.status()? {
            if item.revision < 1 || item.generation < 1 {
                return Err(StoreError::invalid("invalid installation counters"));
            }
            let (bytes, key, publisher): (Vec<u8>,String,String) = self.db.query_row(
                "SELECT content.bytes,trust.key,trust.publisher FROM content,trust WHERE content.digest=?1 AND trust.package_id=?2 AND length(content.bytes)<=?3",
                params![item.digest,item.package_id,MAX_PACKAGE_BYTES as i64], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let package = verify_for_host(&bytes, &key, &self.supported_apis)?;
            if package.digest() != item.digest
                || package.manifest().package_id != item.package_id
                || package.manifest().publisher_id != publisher
            {
                return Err(StoreError::corrupt("current package requires repair"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GrantScope {
    pub capability: Capability,
    pub library: String,
    pub document: String,
    pub version: String,
    pub expires_at: u64,
}
impl GrantScope {
    pub fn resource(&self) -> Resource {
        Resource::Document {
            vault_id: self.library.clone(),
            document_id: self.document.clone(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.capability != Capability::DocumentMetadataRead {
            return Err(StoreError::invalid(
                "capability cannot persist in this profile",
            ));
        }
        self.resource().validate()?;
        if self.version.is_empty()
            || self.version.len() > 128
            || self.version.chars().any(char::is_control)
        {
            return Err(StoreError::invalid("invalid document version"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SavedGrant {
    pub installation_id: i64,
    pub digest: String,
    pub generation: i64,
    pub policy_version: u32,
    pub scope: GrantScope,
}
impl SavedGrant {
    pub fn matches(&self, library: &str, document: &str, version: &str) -> Result<()> {
        if self.scope.expires_at <= now()? {
            return Err(StoreError::conflict("grant expired"));
        }
        if self.scope.library != library
            || self.scope.document != document
            || self.scope.version != version
        {
            return Err(StoreError::conflict("grant resource mismatch"));
        }
        Ok(())
    }
}
fn now() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}
fn save_management(
    tx: &rusqlite::Transaction<'_>,
    operation: &str,
    input: &str,
    receipt: &Receipt,
) -> Result<()> {
    tx.execute(
        "UPDATE installations SET revision=?1,generation=?2 WHERE id=?3",
        params![
            receipt.revision,
            receipt.generation,
            receipt.installation_id
        ],
    )?;
    tx.execute(
        "INSERT INTO operations VALUES (?1,?2,?3)",
        params![operation, input, serde_json::to_string(receipt)?],
    )?;
    Ok(())
}

pub fn read_package(path: &Path) -> Result<Vec<u8>> {
    reject_nonfile(path)?;
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?
    };
    #[cfg(not(unix))]
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(StoreError::invalid("package input is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PACKAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(StoreError::invalid("package input budget exceeded"));
    }
    // Verification and execution consume this buffer; never re-open the supplied path.
    Ok(bytes)
}
fn input_hash(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn checked_next(value: i64) -> Result<i64> {
    value
        .checked_add(1)
        .ok_or_else(|| StoreError::invalid("counter exhausted"))
}
fn validate_command(operation: &str, expected: i64) -> Result<()> {
    if operation.is_empty()
        || operation.len() > 128
        || !operation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        || expected < 0
    {
        return Err(StoreError::invalid(
            "invalid operation ID or expected revision",
        ));
    }
    Ok(())
}
fn reject_nonfile(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => Err(StoreError::invalid(
            "nonregular store or package file rejected",
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn private_directory(path: &Path) -> Result<()> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() && meta.permissions().mode() & 0o077 == 0 => {}
            Ok(_) => {
                return Err(StoreError::invalid(
                    "store must be a private directory (0700), without symlink",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(path)?;
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    File::open(parent)?.sync_all()?;
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod writer_lock_tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn has_fd_target(directory: &Path, target: &Path) -> bool {
        fs::read_dir(directory)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| fs::read_link(entry.path()).ok())
            .any(|path| path == target)
    }
    #[test]
    fn closed_store_unlocks_even_when_a_live_child_inherited_a_lock_clone() {
        let path = std::env::temp_dir().join(format!("qusor-writer-drop-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let store_path = path.join("store");
        let store = Store::open(&store_path).unwrap();
        assert!(
            Store::open(&store_path).is_err(),
            "a live owner must retain strict exclusion"
        );
        let inherited = store._lock.0.try_clone().unwrap();
        let mut child = OwnedChild(
            Command::new("/bin/sleep")
                .arg("10")
                .stdin(Stdio::from(inherited))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        // Observe the child image before asserting exec-time descriptor closure.
        use std::os::unix::fs::MetadataExt;
        let image = fs::metadata("/bin/sleep").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !fs::metadata(format!("/proc/{}/exe", child.0.id()))
            .is_ok_and(|m| m.dev() == image.dev() && m.ino() == image.ino())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "child never reached its native image"
            );
            std::thread::yield_now();
        }
        let db_path = store_path.join("state.sqlite3");
        assert!(has_fd_target(Path::new("/proc/self/fd"), &db_path));
        assert!(!has_fd_target(
            &std::path::PathBuf::from(format!("/proc/{}/fd", child.0.id())),
            &db_path
        ));
        drop(store);
        assert!(
            !has_fd_target(Path::new("/proc/self/fd"), &db_path),
            "the old Connection must close before lock release"
        );
        assert!(child.0.try_wait().unwrap().is_none());
        let next = Store::open(&store_path).unwrap();
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "unlock must not signal the inheriting child"
        );
        drop(next);
        drop(child);
        fs::remove_dir_all(path).unwrap();
    }
}

/// Map fresh durable facts to the single live Host's authorization source.
/// The store never constructs a Host, Session or capability permit.
impl extension_host::durable::AuthoritySource for Store {
    fn removed_installation(
        &self,
        package_id: &str,
    ) -> std::result::Result<extension_host::durable::RemovedInstallation, extension_api::ApiError>
    {
        let read = || -> Result<extension_host::durable::RemovedInstallation> {
            let item = self.current(package_id)?;
            if item.status != "uninstalled" {
                return Err(StoreError::conflict("installation is not uninstalled"));
            }
            self.ensure_no_pending(item.installation_id)?;
            self.ensure_worker_access(item.installation_id, None)?;
            self.current_package(&item)?;
            // Uninstall preserves the binding; never accept another vault's
            // numerically identical installation as this Host's tombstone.
            let vault_id: String = self.db.query_row(
                "SELECT library FROM installation_library WHERE installation_id=?1",
                [item.installation_id],
                |row| row.get(0),
            )?;
            Ok(extension_host::durable::RemovedInstallation {
                installation_id: item.installation_id,
                package_id: item.package_id,
                vault_id,
                revision: item.revision,
                generation: item.generation,
            })
        };
        read().map_err(|_| {
            extension_api::ApiError::new(
                extension_api::ErrorCode::PermissionDenied,
                "durable uninstall proof unavailable or invalid",
            )
        })
    }
    fn authority_snapshot(
        &self,
        package_id: &str,
        read: extension_host::durable::AuthorityRead<'_>,
    ) -> std::result::Result<extension_host::durable::AuthoritySnapshot, extension_api::ApiError>
    {
        use extension_host::durable::{AuthorityRead, AuthoritySnapshot};
        let snapshot = || -> Result<AuthoritySnapshot> {
            let run_id = match read {
                AuthorityRead::Activation => None,
                AuthorityRead::Current { run_id } => run_id,
            };
            let item = self.current(package_id)?;
            self.ensure_no_pending(item.installation_id)?;
            self.ensure_worker_access(item.installation_id, run_id)?;
            let package = self.current_package(&item)?;
            let authorization = self.saved_authorization_with_package(&item, &package)?;
            self.check_private_pair_with_package(
                &item,
                &package,
                matches!(read, AuthorityRead::Activation),
            )?;
            Ok(AuthoritySnapshot {
                installation_id: item.installation_id,
                revision: item.revision,
                generation: item.generation,
                manifest: package.manifest().clone(),
                digest: item.digest,
                vault_id: authorization.vault_id().to_owned(),
                enable_intent: true,
                required_api: match &authorization {
                    SavedAuthorization::V1(_) => None,
                    SavedAuthorization::V2(record) => Some(record.scope.api_version),
                },
                grants: authorization.durable_grants(),
            })
        };
        snapshot().map_err(|_| {
            extension_api::ApiError::new(
                extension_api::ErrorCode::PermissionDenied,
                "durable installation authority is unavailable or invalid",
            )
        })
    }
}
