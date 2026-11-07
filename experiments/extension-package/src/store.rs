//! Linux persistence experiment: bytes, disabled installation and receipt commit together.
use crate::package::{MAX_PACKAGE_BYTES, VerifiedPackage, verify};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
/// No grant or runtime session is persisted; every live installation remains disabled.
pub struct Store {
    db: Connection,
    _lock: File,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if !cfg!(target_os = "linux") {
            return Err("store profile is Linux only".into());
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
            .map_err(|_| "store has another writer")?;
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
                    return Err("unversioned nonempty store needs repair".into());
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
            _ => return Err("unknown store schema needs repair".into()),
        }
        let integrity: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err("store integrity check failed".into());
        }
        let store = Self { db, _lock: lock };
        store.check_current()?;
        Ok(store)
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
        crash: Option<&str>,
    ) -> Result<Receipt> {
        validate_command(operation, expected, crash)?;
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
            return Err("publisher key binding conflict".into());
        }
        let current = self
            .status()?
            .into_iter()
            .find(|r| r.package_id == manifest.package_id);
        let (revision, generation) = if let Some(old) = &current {
            if old.status == "uninstalled" {
                if expected != 0 {
                    return Err("reinstall requires revision zero".into());
                }
                (checked_next(old.revision)?, checked_next(old.generation)?)
            } else {
                if old.revision != expected {
                    return Err("installation revision conflict".into());
                }
                let bytes: Vec<u8> = self.db.query_row(
                    "SELECT bytes FROM content WHERE digest=?1 AND length(bytes)<=?2",
                    params![old.digest, MAX_PACKAGE_BYTES as i64],
                    |r| r.get(0),
                )?;
                let previous = verify(&bytes, package.publisher_key())?;
                if manifest.package_version <= previous.manifest().package_version
                    || old.digest == package.digest()
                {
                    return Err("update requires newer version and different digest".into());
                }
                (checked_next(old.revision)?, checked_next(old.generation)?)
            }
        } else {
            if expected != 0 {
                return Err("new installation requires revision zero".into());
            }
            (1, 1)
        };
        crash_at(crash, "before_transaction");
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
        let frozen = verify(&stored, package.publisher_key())?;
        if frozen.digest() != package.digest() {
            return Err("stored content digest mismatch".into());
        }
        crash_at(crash, "after_content");
        tx.execute(
            "INSERT OR IGNORE INTO trust VALUES (?1,?2,?3)",
            params![
                manifest.package_id,
                manifest.publisher_id,
                package.publisher_key()
            ],
        )?;
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
        crash_at(crash, "before_commit");
        tx.commit()?;
        crash_at(crash, "after_commit");
        Ok(receipt)
    }

    /// Tombstone only: retains content and history, has no access to Core knowledge.
    pub fn uninstall(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        crash: Option<&str>,
    ) -> Result<Receipt> {
        validate_command(operation, expected, crash)?;
        let input = input_hash(&("uninstall", expected, package_id))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        self.check_budget(None)?;
        let old = self
            .status()?
            .into_iter()
            .find(|r| r.package_id == package_id)
            .ok_or("installation not found")?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err("installation revision conflict".into());
        }
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            status: "uninstalled".into(),
            ..old
        };
        crash_at(crash, "before_transaction");
        let tx = self.db.transaction()?;
        crash_at(crash, "after_content");
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
        crash_at(crash, "before_commit");
        tx.commit()?;
        crash_at(crash, "after_commit");
        Ok(receipt)
    }

    fn replay(&self, operation: &str, input: &str) -> Result<Option<Receipt>> {
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
            Some(_) => Err("operation ID reused with different input".into()),
            None => Ok(None),
        }
    }

    fn check_budget(&self, package: Option<&VerifiedPackage>) -> Result<()> {
        let (count, bytes, ops): (i64,i64,i64) = self.db.query_row(
            "SELECT (SELECT count(*) FROM installations),(SELECT coalesce(sum(length(bytes)),0) FROM content),(SELECT count(*) FROM operations)",
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
            return Err("experiment store budget exhausted".into());
        }
        Ok(())
    }

    fn check_current(&self) -> Result<()> {
        for item in self.status()? {
            if item.revision < 1 || item.generation < 1 {
                return Err("invalid installation counters".into());
            }
            let (bytes, key, publisher): (Vec<u8>,String,String) = self.db.query_row(
                "SELECT content.bytes,trust.key,trust.publisher FROM content,trust WHERE content.digest=?1 AND trust.package_id=?2 AND length(content.bytes)<=?3",
                params![item.digest,item.package_id,MAX_PACKAGE_BYTES as i64], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let package = verify(&bytes, &key)?;
            if package.digest() != item.digest
                || package.manifest().package_id != item.package_id
                || package.manifest().publisher_id != publisher
            {
                return Err("current package requires repair".into());
            }
        }
        Ok(())
    }
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
        return Err("package input is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PACKAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package input budget exceeded".into());
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
        .ok_or_else(|| "counter exhausted".into())
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
fn crash_at(requested: Option<&str>, point: &str) {
    if requested == Some(point) {
        // No unwinding or destructors: exercises OS process death and journal recovery.
        let _ = std::io::stderr().write_all(format!("injected process exit: {point}\n").as_bytes());
        std::process::exit(86);
    }
}
fn reject_nonfile(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => {
            Err("nonregular store or package file rejected".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() && meta.permissions().mode() & 0o077 == 0 => {}
            Ok(_) => return Err("store must be a private directory (0700), without symlink".into()),
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
