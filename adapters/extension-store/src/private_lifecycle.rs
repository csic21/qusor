//! Atomic private-state pairs; no Core knowledge or guest authority.
use super::*;

const MAX_STATE: usize = 64 * 1024;
const MAX_SNAPSHOTS: i64 = 8192;
const MAX_STATE_TOTAL: i64 = 64 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lifecycle {
    protocol: u32,
    private_schema: u32,
    migration_module: Option<String>,
    accepted_from_schemas: Vec<u32>,
}
impl Lifecycle {
    fn read(package: &VerifiedPackage) -> Result<Option<Self>> {
        let Some((mime, bytes)) = package.file("assets/lifecycle.json") else {
            return Ok(None);
        };
        if mime != "application/json" || bytes.len() > 4096 {
            return Err(StoreError::invalid(
                "invalid lifecycle descriptor budget or MIME",
            ));
        }
        let value: Self = serde_json::from_slice(bytes)?;
        if value.protocol != 1
            || value.private_schema == 0
            || value.private_schema > i32::MAX as u32
            || value.accepted_from_schemas.len() > 16
            || value
                .accepted_from_schemas
                .iter()
                .any(|schema| *schema == 0 || *schema > i32::MAX as u32)
            || value
                .accepted_from_schemas
                .iter()
                .enumerate()
                .any(|(i, s)| value.accepted_from_schemas[..i].contains(s))
            || value
                .migration_module
                .as_deref()
                .is_some_and(|name| name != "migration.wasm")
            || value.migration_module.is_none() && !value.accepted_from_schemas.is_empty()
        {
            return Err(StoreError::invalid("unsupported lifecycle descriptor"));
        }
        if let Some(name) = &value.migration_module {
            let (mime, _) = package
                .file(name)
                .ok_or_else(|| StoreError::invalid("missing migration module"))?;
            if mime != "application/wasm" {
                return Err(StoreError::invalid("invalid migration module MIME"));
            }
        }
        Ok(Some(value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PrivateReceipt {
    pub installation: Receipt,
    pub outcome: String,
    pub reason: Option<String>,
    pub snapshot_id: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct PrivateSnapshot {
    pub snapshot_id: i64,
    pub package_digest: String,
    pub schema: u32,
    pub state_digest: String,
    pub state_hex: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RetainedPair {
    pub pair_id: i64,
    pub snapshot_id: i64,
    pub package_digest: String,
    pub schema: u32,
    pub state_digest: String,
}
#[derive(Debug, Serialize)]
pub struct PrivateStatus {
    pub installation: Receipt,
    pub current: Option<PrivateSnapshot>,
    pub retained: Vec<RetainedPair>,
}
struct Snapshot {
    id: i64,
    installation: i64,
    digest: String,
    schema: u32,
    state_digest: String,
    bytes: Vec<u8>,
}
impl Snapshot {
    fn public(&self) -> PrivateSnapshot {
        PrivateSnapshot {
            snapshot_id: self.id,
            package_digest: self.digest.clone(),
            schema: self.schema,
            state_digest: self.state_digest.clone(),
            state_hex: self.bytes.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    operation: String,
    input: String,
    installation: Receipt,
    old_snapshot: i64,
    target_digest: String,
    target_snapshot: Option<i64>,
}

pub(super) fn create_schema(db: &mut Connection) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE private_snapshots (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
      digest TEXT NOT NULL REFERENCES content(digest), schema INTEGER NOT NULL,
      state_digest TEXT NOT NULL, bytes BLOB NOT NULL);
      CREATE TABLE private_heads (installation_id INTEGER PRIMARY KEY REFERENCES installations(id) ON DELETE CASCADE,
      snapshot_id INTEGER NOT NULL UNIQUE REFERENCES private_snapshots(id));
      CREATE TABLE retained_pairs (id INTEGER PRIMARY KEY AUTOINCREMENT,
      installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
      snapshot_id INTEGER NOT NULL REFERENCES private_snapshots(id), UNIQUE(installation_id,snapshot_id));
      CREATE TABLE private_pending (installation_id INTEGER PRIMARY KEY REFERENCES installations(id) ON DELETE CASCADE,
      operation_id TEXT NOT NULL UNIQUE, record TEXT NOT NULL);
      CREATE TABLE private_operations (operation_id TEXT PRIMARY KEY,input_hash TEXT NOT NULL,receipt TEXT NOT NULL);
      PRAGMA user_version=3;")?;
    tx.commit()?;
    Ok(())
}

impl Store {
    pub fn private_status(&self, package_id: &str) -> Result<PrivateStatus> {
        let item = self.current(package_id)?;
        let current = self.private_head(item.installation_id)?.map(|s| s.public());
        let mut stmt = self.db.prepare(
            "SELECT id,snapshot_id FROM retained_pairs WHERE installation_id=?1 ORDER BY id",
        )?;
        let pairs = stmt
            .query_map([item.installation_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut retained = Vec::new();
        for (id, snapshot) in pairs {
            let state = self.private_snapshot(snapshot)?;
            retained.push(RetainedPair {
                pair_id: id,
                snapshot_id: snapshot,
                package_digest: state.digest,
                schema: state.schema,
                state_digest: state.state_digest,
            });
        }
        Ok(PrivateStatus {
            installation: item,
            current,
            retained,
        })
    }

    pub fn initialize_private(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        input: &[u8],
    ) -> Result<PrivateReceipt> {
        validate_command(operation, expected)?;
        if input.len() > MAX_STATE {
            return Err(StoreError::invalid("private state budget exceeded"));
        }
        let hash = input_hash(&("initialize-private", expected, package_id, input))?;
        if let Some(receipt) = self.private_replay(operation, &hash)? {
            return Ok(receipt);
        }
        self.private_budget(input.len())?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status != "disabled"
            || old.revision != expected
            || self.private_head(old.installation_id)?.is_some()
        {
            return Err(StoreError::conflict(
                "private initialization requires current disabled installation without state",
            ));
        }
        let granted: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM grants WHERE installation_id=?1)",
            [old.installation_id],
            |r| r.get(0),
        )?;
        if granted {
            return Err(StoreError::conflict(
                "private initialization requires no authorization",
            ));
        }
        let package = self.current_package(&old)?;
        let descriptor = Lifecycle::read(&package)?
            .ok_or_else(|| StoreError::invalid("package has no private lifecycle"))?;
        self.services.private_executor.preflight(
            package.module_bytes(),
            descriptor.private_schema,
            input,
        )?;
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        self.services.observer.checkpoint(Checkpoint::BeforePrepare);
        let tx = self.db.transaction()?;
        let snapshot = insert_snapshot(
            &tx,
            receipt.installation_id,
            &receipt.digest,
            descriptor.private_schema,
            input,
        )?;
        self.services.observer.checkpoint(Checkpoint::AfterSnapshot);
        tx.execute(
            "INSERT INTO private_heads VALUES (?1,?2)",
            params![receipt.installation_id, snapshot],
        )?;
        let result = PrivateReceipt {
            installation: receipt,
            outcome: "succeeded".into(),
            reason: None,
            snapshot_id: snapshot,
        };
        finish_record(&tx, operation, &hash, &result)?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(result)
    }

    pub fn upgrade_private(
        &mut self,
        operation: &str,
        expected: i64,
        package: &VerifiedPackage,
    ) -> Result<PrivateReceipt> {
        validate_command(operation, expected)?;
        let hash = input_hash(&(
            "upgrade-private",
            expected,
            package.digest(),
            package.publisher_key(),
        ))?;
        if let Some(receipt) = self.private_replay(operation, &hash)? {
            return Ok(receipt);
        }
        self.check_budget(Some(package))?;
        self.private_budget(MAX_STATE)?;
        let old = self.current(&package.manifest().package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status != "disabled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let previous = self.current_package(&old)?;
        if previous.publisher_key() != package.publisher_key()
            || previous.manifest().publisher_id != package.manifest().publisher_id
        {
            return Err(StoreError::conflict("publisher key binding conflict"));
        }
        if package.manifest().package_version <= previous.manifest().package_version
            || package.digest() == old.digest
        {
            return Err(StoreError::conflict(
                "update requires newer version and different digest",
            ));
        }
        let state = self
            .private_head(old.installation_id)?
            .ok_or_else(|| StoreError::conflict("private state not initialized"))?;
        let descriptor = Lifecycle::read(package)?
            .ok_or_else(|| StoreError::invalid("package has no private lifecycle"))?;
        if !descriptor.accepted_from_schemas.contains(&state.schema) {
            return Err(StoreError::invalid("unsupported source private schema"));
        }
        let module = descriptor
            .migration_module
            .as_ref()
            .ok_or_else(|| StoreError::invalid("package has no migration module"))?;
        let wasm = package
            .file(module)
            .ok_or_else(|| StoreError::invalid("missing migration module"))?
            .1;
        let pending = self.prepare_private((operation, &hash), old, &state, package, None)?;
        let migrated = self
            .services
            .private_executor
            .migrate(wasm, state.schema, descriptor.private_schema, &state.bytes)
            .and_then(|bytes| {
                self.services
                    .private_executor
                    .preflight(package.module_bytes(), descriptor.private_schema, &bytes)
                    .map(|()| bytes)
            });
        let bytes = match migrated {
            Ok(bytes) => bytes,
            Err(error) => return self.fail_private(&pending, "failed", &error.to_string()),
        };
        self.services
            .observer
            .checkpoint(Checkpoint::AfterMigration);
        self.complete_private(pending, package, descriptor.private_schema, &bytes, None)
    }

    pub fn rollback_private(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        pair_id: i64,
    ) -> Result<PrivateReceipt> {
        validate_command(operation, expected)?;
        if pair_id < 1 {
            return Err(StoreError::invalid("invalid retained pair"));
        }
        let hash = input_hash(&("rollback-private", expected, package_id, pair_id))?;
        if let Some(receipt) = self.private_replay(operation, &hash)? {
            return Ok(receipt);
        }
        self.private_budget(0)?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status != "disabled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let state = self
            .private_head(old.installation_id)?
            .ok_or_else(|| StoreError::conflict("private state not initialized"))?;
        let snapshot: i64 = self.db.query_row(
            "SELECT snapshot_id FROM retained_pairs WHERE id=?1 AND installation_id=?2",
            params![pair_id, old.installation_id],
            |r| r.get(0),
        )?;
        let target = self.private_snapshot(snapshot)?;
        if target.id == state.id {
            return Err(StoreError::conflict("retained pair is already current"));
        }
        let package = self.package_for_snapshot(&target)?;
        let pending =
            self.prepare_private((operation, &hash), old, &state, &package, Some(snapshot))?;
        if let Err(error) = self.services.private_executor.preflight(
            package.module_bytes(),
            target.schema,
            &target.bytes,
        ) {
            return self.fail_private(&pending, "failed", &error.to_string());
        }
        self.services
            .observer
            .checkpoint(Checkpoint::AfterMigration);
        self.complete_private(
            pending,
            &package,
            target.schema,
            &target.bytes,
            Some(snapshot),
        )
    }

    fn prepare_private(
        &mut self,
        command: (&str, &str),
        old: Receipt,
        state: &Snapshot,
        package: &VerifiedPackage,
        target_snapshot: Option<i64>,
    ) -> Result<Pending> {
        let (operation, hash) = command;
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        let pending = Pending {
            operation: operation.into(),
            input: hash.into(),
            installation: receipt,
            old_snapshot: state.id,
            target_digest: package.digest().into(),
            target_snapshot,
        };
        self.services.observer.checkpoint(Checkpoint::BeforePrepare);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO content VALUES (?1,?2)",
            params![package.digest(), package.raw_bytes()],
        )?;
        let stored: Vec<u8> = tx.query_row(
            "SELECT bytes FROM content WHERE digest=?1 AND length(bytes)<=?2",
            params![package.digest(), MAX_PACKAGE_BYTES as i64],
            |r| r.get(0),
        )?;
        if verify_for_host(&stored, package.publisher_key(), &self.supported_apis)?.digest()
            != package.digest()
        {
            return Err(StoreError::corrupt("stored content digest mismatch"));
        }
        tx.execute(
            "DELETE FROM grants WHERE installation_id=?1",
            [pending.installation.installation_id],
        )?;
        update_installation(&tx, &pending.installation)?;
        tx.execute(
            "INSERT INTO private_pending VALUES (?1,?2,?3)",
            params![
                pending.installation.installation_id,
                operation,
                serde_json::to_string(&pending)?
            ],
        )?;
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterPrepare);
        Ok(pending)
    }
    fn complete_private(
        &mut self,
        pending: Pending,
        package: &VerifiedPackage,
        schema: u32,
        bytes: &[u8],
        existing: Option<i64>,
    ) -> Result<PrivateReceipt> {
        if bytes.len() > MAX_STATE {
            return self.fail_private(&pending, "failed", "private state budget exceeded");
        }
        let receipt = Receipt {
            digest: package.digest().into(),
            revision: checked_next(pending.installation.revision)?,
            generation: checked_next(pending.installation.generation)?,
            ..pending.installation.clone()
        };
        let tx = self.db.transaction()?;
        let snapshot = if let Some(id) = existing {
            id
        } else {
            insert_snapshot(&tx, receipt.installation_id, &receipt.digest, schema, bytes)?
        };
        self.services.observer.checkpoint(Checkpoint::AfterSnapshot);
        tx.execute(
            "INSERT OR IGNORE INTO retained_pairs(installation_id,snapshot_id) VALUES (?1,?2)",
            params![receipt.installation_id, pending.old_snapshot],
        )?;
        tx.execute(
            "UPDATE private_heads SET snapshot_id=?1 WHERE installation_id=?2",
            params![snapshot, receipt.installation_id],
        )?;
        let result = PrivateReceipt {
            installation: receipt,
            outcome: "succeeded".into(),
            reason: None,
            snapshot_id: snapshot,
        };
        finish_record(&tx, &pending.operation, &pending.input, &result)?;
        tx.execute(
            "DELETE FROM private_pending WHERE installation_id=?1",
            [result.installation.installation_id],
        )?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(result)
    }
    fn fail_private(
        &mut self,
        pending: &Pending,
        outcome: &str,
        reason: &str,
    ) -> Result<PrivateReceipt> {
        let result = PrivateReceipt {
            installation: pending.installation.clone(),
            outcome: outcome.into(),
            reason: Some(reason.chars().take(256).collect()),
            snapshot_id: pending.old_snapshot,
        };
        let tx = self.db.transaction()?;
        finish_record(&tx, &pending.operation, &pending.input, &result)?;
        tx.execute(
            "DELETE FROM private_pending WHERE installation_id=?1",
            [pending.installation.installation_id],
        )?;
        tx.commit()?;
        Ok(result)
    }

    fn private_replay(&self, operation: &str, input: &str) -> Result<Option<PrivateReceipt>> {
        let regular: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM operations WHERE operation_id=?1)",
            [operation],
            |r| r.get(0),
        )?;
        if regular {
            return Err(StoreError::conflict(
                "operation ID reused across command types",
            ));
        }
        let saved: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT input_hash,receipt FROM private_operations WHERE operation_id=?1",
                [operation],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match saved {
            Some((hash, value)) if hash == input => Ok(Some(serde_json::from_str(&value)?)),
            Some(_) => Err(StoreError::conflict(
                "operation ID reused with different input",
            )),
            None => {
                let pending: bool = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM private_pending WHERE operation_id=?1)",
                    [operation],
                    |r| r.get(0),
                )?;
                if pending {
                    return Err(StoreError::conflict("operation is pending recovery"));
                }
                Ok(None)
            }
        }
    }
    pub(super) fn ensure_no_pending(&self, id: i64) -> Result<()> {
        let has: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM private_pending WHERE installation_id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if has {
            return Err(StoreError::conflict(
                "installation needs private change recovery",
            ));
        }
        Ok(())
    }
    pub(super) fn has_private_head(&self, id: i64) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM private_heads WHERE installation_id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }
    pub fn check_private_ready(&self, item: &Receipt) -> Result<()> {
        self.check_private_pair(item, None, true)
    }
    fn check_private_pair(
        &self,
        item: &Receipt,
        run_id: Option<&str>,
        execute: bool,
    ) -> Result<()> {
        self.ensure_no_pending(item.installation_id)?;
        self.ensure_worker_access(item.installation_id, run_id)?;
        let package = self.current_package(item)?;
        self.check_private_pair_with_package(item, &package, execute)
    }

    /// The outer authority gate already checked pending/worker exclusion and
    /// authenticated these exact bytes. Reuse within that read, never across calls.
    pub(super) fn check_private_pair_with_package(
        &self,
        item: &Receipt,
        package: &VerifiedPackage,
        execute: bool,
    ) -> Result<()> {
        if let Some(descriptor) = Lifecycle::read(package)? {
            let state = self
                .private_head(item.installation_id)?
                .ok_or_else(|| StoreError::conflict("private state not initialized"))?;
            if state.installation != item.installation_id
                || state.digest != item.digest
                || state.schema != descriptor.private_schema
            {
                return Err(StoreError::conflict("private state package mismatch"));
            }
            if execute {
                self.services.private_executor.preflight(
                    package.module_bytes(),
                    state.schema,
                    &state.bytes,
                )?;
            }
        } else if self.has_private_head(item.installation_id)? {
            return Err(StoreError::conflict(
                "private state has no package contract",
            ));
        }
        Ok(())
    }
    fn private_head(&self, id: i64) -> Result<Option<Snapshot>> {
        let head: Option<i64> = self
            .db
            .query_row(
                "SELECT snapshot_id FROM private_heads WHERE installation_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        head.map(|snapshot| self.private_snapshot(snapshot))
            .transpose()
    }
    fn private_snapshot(&self, id: i64) -> Result<Snapshot> {
        let value=self.db.query_row("SELECT installation_id,digest,schema,state_digest,bytes FROM private_snapshots WHERE id=?1 AND length(bytes)<=?2",params![id,MAX_STATE as i64],|r|Ok(Snapshot{id,installation:r.get(0)?,digest:r.get(1)?,schema:r.get(2)?,state_digest:r.get(3)?,bytes:r.get(4)?}))?;
        if value.schema == 0
            || value.schema > i32::MAX as u32
            || value.state_digest != hash_bytes(&value.bytes)
        {
            return Err(StoreError::corrupt("private snapshot integrity mismatch"));
        }
        let package = self.package_for_snapshot(&value)?;
        let descriptor = Lifecycle::read(&package)?
            .ok_or_else(|| StoreError::invalid("snapshot package has no lifecycle"))?;
        if descriptor.private_schema != value.schema {
            return Err(StoreError::corrupt("snapshot schema package mismatch"));
        }
        Ok(value)
    }
    fn package_for_snapshot(&self, state: &Snapshot) -> Result<VerifiedPackage> {
        let item = self
            .status()?
            .into_iter()
            .find(|item| item.installation_id == state.installation)
            .ok_or_else(|| StoreError::corrupt("orphaned private snapshot"))?;
        let (bytes,key,publisher):(Vec<u8>,String,String)=self.db.query_row("SELECT content.bytes,trust.key,trust.publisher FROM content,trust WHERE content.digest=?1 AND trust.package_id=?2 AND length(content.bytes)<=?3",params![state.digest,item.package_id,MAX_PACKAGE_BYTES as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let package = verify_for_host(&bytes, &key, &self.supported_apis)?;
        if package.digest() != state.digest
            || package.manifest().package_id != item.package_id
            || package.manifest().publisher_id != publisher
        {
            return Err(StoreError::corrupt(
                "private snapshot publisher binding mismatch",
            ));
        }
        Ok(package)
    }
    fn private_budget(&self, extra: usize) -> Result<()> {
        self.check_budget(None)?;
        let (count, total): (i64, i64) = self.db.query_row(
            "SELECT count(*),coalesce(sum(length(bytes)),0) FROM private_snapshots",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if count >= MAX_SNAPSHOTS || total + extra as i64 > MAX_STATE_TOTAL {
            return Err(StoreError::invalid("private snapshot budget exceeded"));
        }
        Ok(())
    }
    pub(super) fn check_private(&self) -> Result<()> {
        let fk: Option<String> = self
            .db
            .query_row(
                "SELECT 'invalid' FROM pragma_foreign_key_check LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if fk.is_some() {
            return Err(StoreError::corrupt(
                "private foreign key integrity mismatch",
            ));
        }
        let ids = self
            .db
            .prepare("SELECT id FROM private_snapshots ORDER BY id")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if ids.len() > MAX_SNAPSHOTS as usize {
            return Err(StoreError::invalid("private snapshot budget exceeded"));
        }
        let total: i64 = self.db.query_row(
            "SELECT coalesce(sum(length(bytes)),0) FROM private_snapshots",
            [],
            |r| r.get(0),
        )?;
        if total > MAX_STATE_TOTAL {
            return Err(StoreError::invalid("private snapshot budget exceeded"));
        }
        let ops:i64=self.db.query_row("SELECT (SELECT count(*) FROM operations)+(SELECT count(*) FROM private_operations)+(SELECT count(*) FROM private_pending)",[],|r|r.get(0))?;
        if ops > 8192 {
            return Err(StoreError::invalid("store budget exhausted"));
        }
        for id in ids {
            let state = self.private_snapshot(id)?;
            if !self.has_private_head(state.installation)? {
                return Err(StoreError::corrupt("missing private state head"));
            }
            let referenced:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM private_heads WHERE snapshot_id=?1) OR EXISTS(SELECT 1 FROM retained_pairs WHERE snapshot_id=?1)",[id],|r|r.get(0))?;
            if !referenced {
                return Err(StoreError::corrupt("orphaned private snapshot"));
            }
        }
        let heads = self
            .db
            .prepare("SELECT installation_id,snapshot_id FROM private_heads")?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let items = self.status()?;
        for (id, snapshot) in heads {
            let state = self.private_snapshot(snapshot)?;
            let item = items
                .iter()
                .find(|item| item.installation_id == id)
                .ok_or_else(|| StoreError::corrupt("orphaned private head"))?;
            if state.installation != id || state.digest != item.digest {
                return Err(StoreError::corrupt("private head package mismatch"));
            }
        }
        let retained = self
            .db
            .prepare("SELECT installation_id,snapshot_id FROM retained_pairs")?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (id, snapshot) in retained {
            if self.private_snapshot(snapshot)?.installation != id {
                return Err(StoreError::invalid("retained pair installation mismatch"));
            }
        }
        let pending = self.pending_records()?;
        for value in pending {
            validate_command(&value.operation, 0)?;
            let item = items
                .iter()
                .find(|item| item.installation_id == value.installation.installation_id)
                .ok_or_else(|| StoreError::corrupt("orphaned pending change"))?;
            let mut expected = value.installation.clone();
            expected.operation_id.clear();
            if item != &expected
                || item.status != "disabled"
                || self.private_head(item.installation_id)?.map(|s| s.id)
                    != Some(value.old_snapshot)
            {
                return Err(StoreError::corrupt("pending change binding mismatch"));
            }
            let target = Snapshot {
                id: 0,
                installation: item.installation_id,
                digest: value.target_digest.clone(),
                schema: 1,
                state_digest: String::new(),
                bytes: Vec::new(),
            };
            self.package_for_snapshot(&target)?;
            if let Some(id) = value.target_snapshot {
                let state = self.private_snapshot(id)?;
                if state.installation != item.installation_id || state.digest != value.target_digest
                {
                    return Err(StoreError::corrupt("pending target mismatch"));
                }
            }
            let conflicts:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE operation_id=?1) OR EXISTS(SELECT 1 FROM private_operations WHERE operation_id=?1) OR EXISTS(SELECT 1 FROM grants WHERE installation_id=?2)",params![value.operation,item.installation_id],|r|r.get(0))?;
            if conflicts
                || value.input.len() != 64
                || !value
                    .input
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(StoreError::corrupt("pending operation conflict"));
            }
        }
        Ok(())
    }
    fn pending_records(&self) -> Result<Vec<Pending>> {
        let mut stmt = self
            .db
            .prepare("SELECT installation_id,operation_id,record FROM private_pending")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut result = Vec::new();
        for (id, op, record) in rows {
            if record.len() > 8192 {
                return Err(StoreError::invalid("pending record budget exceeded"));
            }
            let value: Pending = serde_json::from_str(&record)?;
            if value.installation.installation_id != id || value.operation != op {
                return Err(StoreError::corrupt("pending record identity mismatch"));
            }
            result.push(value)
        }
        Ok(result)
    }
    pub(super) fn recover_private(&mut self) -> Result<()> {
        for pending in self.pending_records()? {
            self.fail_private(
                &pending,
                "interrupted",
                "operation interrupted after preparation",
            )?;
        }
        Ok(())
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn insert_snapshot(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
    digest: &str,
    schema: u32,
    bytes: &[u8],
) -> Result<i64> {
    tx.execute("INSERT INTO private_snapshots(installation_id,digest,schema,state_digest,bytes) VALUES (?1,?2,?3,?4,?5)",params![id,digest,schema,hash_bytes(bytes),bytes])?;
    Ok(tx.last_insert_rowid())
}
fn update_installation(tx: &rusqlite::Transaction<'_>, receipt: &Receipt) -> Result<()> {
    tx.execute("UPDATE installations SET digest=?1,revision=?2,generation=?3,status='disabled' WHERE id=?4",params![receipt.digest,receipt.revision,receipt.generation,receipt.installation_id])?;
    Ok(())
}
fn finish_record(
    tx: &rusqlite::Transaction<'_>,
    operation: &str,
    input: &str,
    receipt: &PrivateReceipt,
) -> Result<()> {
    update_installation(tx, &receipt.installation)?;
    tx.execute(
        "INSERT INTO private_operations VALUES (?1,?2,?3)",
        params![operation, input, serde_json::to_string(receipt)?],
    )?;
    Ok(())
}
