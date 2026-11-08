//! Durable execution exclusion, separate from authorization and task results.
use super::*;
use extension_runtime_api::ProcessIdentity;

const MAX_LEASES: usize = 16;
const MAX_RECORD: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkerLease {
    pub installation: Receipt,
    pub supervisor_boot: String,
    pub run_id: String,
    pub owner: ProcessIdentity,
    pub spawn_attempted: bool,
    pub worker: Option<ProcessIdentity>,
    pub fenced: Option<Receipt>,
}

pub(super) fn create_schema(db: &mut Connection) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute_batch(
        "CREATE TABLE worker_leases (
        installation_id INTEGER PRIMARY KEY REFERENCES installations(id),
        run_id TEXT NOT NULL UNIQUE, record TEXT NOT NULL, record_hash TEXT NOT NULL);
        PRAGMA user_version=4;",
    )?;
    tx.commit()?;
    Ok(())
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_identity(value: &ProcessIdentity) -> bool {
    let boot = value.boot_id.as_bytes();
    value.pid > 0
        && value.start_time_ticks > 0
        && boot.len() == 36
        && boot.iter().enumerate().all(|(index, b)| {
            if [8, 13, 18, 23].contains(&index) {
                *b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(b)
            }
        })
        && value.exe_inode > 0
}

impl Store {
    pub fn worker_leases(&self) -> Result<Vec<WorkerLease>> {
        let (count,size,min_hash,max_hash):(i64,i64,i64,i64)=self.db.query_row(
            "SELECT count(*),coalesce(max(length(record)),0),coalesce(min(length(record_hash)),64),coalesce(max(length(record_hash)),64) FROM worker_leases",[],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        if count > MAX_LEASES as i64 || size > MAX_RECORD as i64 || min_hash != 64 || max_hash != 64
        {
            return Err(StoreError::invalid(
                "worker lease storage budget or hash mismatch",
            ));
        }
        let mut query = self.db.prepare(
            "SELECT installation_id,run_id,record,record_hash FROM worker_leases ORDER BY installation_id",
        )?;
        let rows = query
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.len() > MAX_LEASES {
            return Err(StoreError::invalid("worker lease budget exceeded"));
        }
        let installations = self.status()?;
        let mut result = Vec::new();
        for (id, run_id, record, record_hash) in rows {
            if record.len() > MAX_RECORD {
                return Err(StoreError::invalid("worker lease record budget exceeded"));
            }
            if !hex(&record_hash, 64) || record_hash != lease_hash(&record) {
                return Err(StoreError::corrupt("worker lease integrity mismatch"));
            }
            let lease: WorkerLease = serde_json::from_str(&record)?;
            if serde_json::to_string(&lease)? != record {
                return Err(StoreError::corrupt("worker lease encoding mismatch"));
            }
            let current = installations
                .iter()
                .find(|item| item.installation_id == id)
                .ok_or_else(|| StoreError::corrupt("orphaned worker lease"))?;
            let mut expected = lease.fenced.as_ref().unwrap_or(&lease.installation).clone();
            expected.operation_id.clear();
            if lease.installation.installation_id < 1
                || lease.installation.revision < 1
                || lease.installation.generation < 1
                || !hex(&lease.installation.digest, 64)
                || current != &expected
                || lease.installation.installation_id != id
                || current.status != "disabled"
                || lease.run_id != run_id
                || !hex(&lease.supervisor_boot, 32)
                || !lease
                    .run_id
                    .strip_prefix(&(lease.supervisor_boot.clone() + "-"))
                    .is_some_and(|suffix| {
                        suffix
                            .parse::<u64>()
                            .is_ok_and(|number| number != 0 && number.to_string() == suffix)
                    })
                || (!lease.spawn_attempted && (lease.worker.is_some() || lease.fenced.is_some()))
                || lease.fenced.as_ref().is_some_and(|fenced| {
                    fenced.installation_id != id
                        || fenced.package_id != lease.installation.package_id
                        || fenced.digest != lease.installation.digest
                        || fenced.generation <= lease.installation.generation
                        || fenced.revision <= lease.installation.revision
                        || fenced.status != "disabled"
                })
                || !valid_identity(&lease.owner)
                || !lease.owner.run_id.is_empty()
                || lease.worker.as_ref().is_some_and(|worker| {
                    !valid_identity(worker)
                        || worker.pid == lease.owner.pid
                        || worker.start_time_ticks < lease.owner.start_time_ticks
                        || worker.run_id != lease.run_id
                        || worker.boot_id != lease.owner.boot_id
                        || worker.uid != lease.owner.uid
                        || worker.exe_device != lease.owner.exe_device
                        || worker.exe_inode != lease.owner.exe_inode
                })
            {
                return Err(StoreError::corrupt("worker lease binding needs repair"));
            }
            self.ensure_no_pending(id)?;
            result.push(lease);
        }
        Ok(result)
    }
    pub fn prepare_worker_lease(
        &mut self,
        installation: &Receipt,
        supervisor_boot: &str,
        run_id: &str,
        owner: &ProcessIdentity,
    ) -> Result<()> {
        self.ensure_worker_access(installation.installation_id, None)?;
        if self.worker_leases()?.len() >= MAX_LEASES {
            return Err(StoreError::invalid("worker lease budget exceeded"));
        }
        let lease = WorkerLease {
            installation: installation.clone(),
            supervisor_boot: supervisor_boot.into(),
            run_id: run_id.into(),
            owner: owner.clone(),
            spawn_attempted: false,
            worker: None,
            fenced: None,
        };
        let value = serde_json::to_string(&lease)?;
        if value.len() > MAX_RECORD {
            return Err(StoreError::invalid("worker lease record budget exceeded"));
        }
        self.db.execute(
            "INSERT INTO worker_leases VALUES (?1,?2,?3,?4)",
            params![
                installation.installation_id,
                run_id,
                value,
                lease_hash(&value)
            ],
        )?;
        // This is a single autocommit FULL-synchronous SQLite management write.
        if let Err(error) = self.worker_leases() {
            self.db.execute(
                "DELETE FROM worker_leases WHERE installation_id=?1",
                [installation.installation_id],
            )?;
            return Err(error);
        }
        Ok(())
    }
    pub fn mark_worker_spawn_attempted(&mut self, run_id: &str) -> Result<()> {
        let mut lease = self
            .worker_leases()?
            .into_iter()
            .find(|lease| lease.run_id == run_id)
            .ok_or_else(|| StoreError::conflict("worker lease not found"))?;
        if lease.spawn_attempted || lease.worker.is_some() {
            return Err(StoreError::conflict("worker spawn already attempted"));
        }
        lease.spawn_attempted = true;
        let text = serde_json::to_string(&lease)?;
        self.db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2 WHERE run_id=?3",
            params![text, lease_hash(&text), run_id],
        )?;
        Ok(())
    }
    pub fn bind_worker_lease(&mut self, run_id: &str, worker: ProcessIdentity) -> Result<()> {
        let mut lease = self
            .worker_leases()?
            .into_iter()
            .find(|lease| lease.run_id == run_id)
            .ok_or_else(|| StoreError::conflict("worker lease not found"))?;
        if lease.worker.is_some() || !lease.spawn_attempted {
            return Err(StoreError::conflict(
                "worker lease is not in unbound spawning state",
            ));
        }
        if !valid_identity(&worker)
            || worker.run_id != lease.run_id
            || worker.boot_id != lease.owner.boot_id
            || worker.uid != lease.owner.uid
            || worker.exe_device != lease.owner.exe_device
            || worker.exe_inode != lease.owner.exe_inode
        {
            return Err(StoreError::conflict(
                "worker lease process binding mismatch",
            ));
        }
        lease.worker = Some(worker);
        let value = serde_json::to_string(&lease)?;
        self.db.execute(
            "UPDATE worker_leases SET record=?1,record_hash=?2 WHERE run_id=?3",
            params![value, lease_hash(&value), run_id],
        )?;
        Ok(())
    }
    /// A trusted coordinator may clear only after confirmed termination, a
    /// definite spawn failure/unstarted intent, or proof of a prior kernel boot.
    /// An attempted spawn without identity is not cleared merely because no Boot was sent.
    pub fn finish_worker_lease(&mut self, run_id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM worker_leases WHERE run_id=?1", [run_id])?;
        Ok(())
    }
    pub(super) fn ensure_worker_control(&self, id: i64, run_id: Option<&str>) -> Result<()> {
        match run_id {
            None => self.ensure_worker_access(id, None),
            Some(run) => {
                let found = self.worker_leases()?.into_iter().any(|lease| {
                    lease.installation.installation_id == id
                        && lease.run_id == run
                        && lease.spawn_attempted
                });
                if found {
                    Ok(())
                } else {
                    Err(StoreError::conflict("worker control identity mismatch"))
                }
            }
        }
    }
    pub(super) fn ensure_worker_access(
        &self,
        installation_id: i64,
        run_id: Option<&str>,
    ) -> Result<()> {
        let lease = self
            .worker_leases()?
            .into_iter()
            .find(|lease| lease.installation.installation_id == installation_id);
        match (lease, run_id) {
            (None, None) => Ok(()),
            (Some(lease), Some(run)) if lease.run_id == run && lease.worker.is_some() => Ok(()),
            _ => Err(StoreError::conflict(
                "installation has unresolved worker lease",
            )),
        }
    }
}

pub(super) fn fence(tx: &rusqlite::Transaction<'_>, run_id: &str, receipt: &Receipt) -> Result<()> {
    let text: String = tx.query_row(
        "SELECT record FROM worker_leases WHERE run_id=?1",
        [run_id],
        |r| r.get(0),
    )?;
    let mut lease: WorkerLease = serde_json::from_str(&text)?;
    lease.fenced = Some(receipt.clone());
    let value = serde_json::to_string(&lease)?;
    tx.execute(
        "UPDATE worker_leases SET record=?1,record_hash=?2 WHERE run_id=?3",
        params![value, lease_hash(&value), run_id],
    )?;
    Ok(())
}

fn lease_hash(record: &str) -> String {
    format!("{:x}", Sha256::digest(record.as_bytes()))
}
