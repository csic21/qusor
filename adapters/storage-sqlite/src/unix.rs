//! SQLite relationship verification, schema migration and atomic knowledge commits.
use super::StorageStage;
use blob_store::{BlobStage, BlobStore};
use core_api::knowledge::*;
use learning_ports::{CommitPlan, KnowledgeStore};
use rusqlite::{Connection, params};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::{
    error::Error,
    fs::{self, File},
    path::Path,
    sync::Arc,
};
type Internal<T> = std::result::Result<T, Box<dyn Error>>;
type Observer = Arc<dyn Fn(StorageStage) + Send + Sync>;
// Declaration order closes SQLite before releasing filesystem writer ownership.
pub struct SqliteStore {
    db: Connection,
    blobs: BlobStore,
    observe: Observer,
}
fn fail<T>(message: &str) -> Internal<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Integrity, message).into())
}
fn storage(error: Box<dyn Error>) -> KnowledgeError {
    if let Some(value) = error.downcast_ref::<KnowledgeError>() {
        return if value.code == KnowledgeErrorCode::InvalidInput {
            KnowledgeError::new(
                KnowledgeErrorCode::Integrity,
                "stored knowledge encoding is invalid",
            )
        } else {
            value.clone()
        };
    }
    if error.is::<rusqlite::Error>() || error.is::<std::io::Error>() {
        return KnowledgeError::new(
            KnowledgeErrorCode::Storage,
            "knowledge storage operation failed",
        );
    }
    KnowledgeError::new(
        KnowledgeErrorCode::Integrity,
        "knowledge storage consistency check failed",
    )
}
impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_observer(path, Arc::new(|_| {}))
    }
    pub fn with_observer(path: impl AsRef<Path>, observe: Observer) -> Result<Self> {
        Self::open_inner(path.as_ref(), observe).map_err(storage)
    }
    pub fn set_observer(&mut self, observe: Observer) {
        self.observe = observe;
    }
    fn open_inner(path: &Path, observe: Observer) -> Internal<Self> {
        let root = path.to_owned();
        let blobs = BlobStore::open(&root)?;
        let dbpath = root.join("core.sqlite");
        for name in [
            "core.sqlite",
            "core.sqlite-journal",
            "core.sqlite-wal",
            "core.sqlite-shm",
        ] {
            if let Ok(m) = fs::symlink_metadata(root.join(name))
                && !m.is_file()
            {
                return fail("invalid database file");
            }
        }
        let db = Connection::open(&dbpath)?;
        fs::set_permissions(&dbpath, fs::Permissions::from_mode(0o600))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            let n: i64 = db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )?;
            if n != 0 {
                return fail("unrecognized existing schema");
            }
            db.execute_batch("BEGIN; CREATE TABLE source(id INTEGER PRIMARY KEY, descriptor TEXT NOT NULL); CREATE TABLE document_version(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),snapshot_hash TEXT NOT NULL); CREATE TABLE capture(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),version_id INTEGER NOT NULL REFERENCES document_version(id),record TEXT NOT NULL,anchor TEXT NOT NULL); CREATE TABLE operation(id TEXT PRIMARY KEY,capture_id INTEGER UNIQUE NOT NULL REFERENCES capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL); CREATE TABLE receipt(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL); CREATE TABLE outbox(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL); CREATE TABLE index_job(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL); PRAGMA user_version=1; COMMIT;")?;
            File::open(&root)?.sync_all()?;
        } else if version != 1 && version != 2 && version != 3 {
            return fail("unknown Core schema");
        }
        if version < 2 {
            migrate(&db, observe.as_ref())?;
        } else if version == 2 {
            migrate_compound(&db, observe.as_ref())?;
        }
        let core = Self { db, blobs, observe };
        let _: ArchiveV2 = Self::read_snapshot(&core.db, &core.blobs)?;
        Ok(core)
    }
    fn export_text(db: &Connection, cas: &BlobStore) -> Internal<ExportBundle> {
        for (table, payload) in [
            ("source", "length(CAST(descriptor AS BLOB))"),
            ("document_version", "length(CAST(snapshot_hash AS BLOB))"),
            (
                "capture",
                "length(CAST(record AS BLOB))+length(CAST(anchor AS BLOB))",
            ),
            (
                "operation",
                "length(CAST(id AS BLOB))+length(CAST(input_hash AS BLOB))+length(CAST(payload AS BLOB))",
            ),
            ("receipt", "length(CAST(payload AS BLOB))"),
            ("outbox", "length(CAST(payload AS BLOB))"),
            ("index_job", "length(CAST(payload AS BLOB))"),
        ] {
            let (count, bytes): (i64, i64) = db.query_row(
                &format!("SELECT count(*),coalesce(sum({payload}),0) FROM {table}"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if count > 4096 || bytes > LIMIT as i64 {
                return fail("text transaction budget exceeded");
            }
        }
        let count: i64 = db.query_row("SELECT count(*) FROM capture", [], |r| r.get(0))?;
        if count > 4096 {
            return fail("capture budget exceeded");
        }
        let bytes: i64 = db.query_row(
            "SELECT coalesce(sum(length(CAST(record AS BLOB))),0) FROM capture",
            [],
            |r| r.get(0),
        )?;
        if bytes > LIMIT as i64 {
            return fail("store byte budget exceeded");
        }
        let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return fail("database integrity failure");
        }
        let fk: bool = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if fk {
            return fail("broken relationships");
        }
        let mut entries = Vec::new();
        let mut blobs = std::collections::BTreeMap::new();
        let mut stmt=db.prepare("SELECT c.record,o.input_hash,o.payload,b.payload,j.payload,r.payload,s.descriptor,v.snapshot_hash,c.anchor FROM capture c JOIN operation o ON o.capture_id=c.id JOIN receipt r ON r.operation_id=o.id JOIN outbox b ON b.operation_id=o.id JOIN index_job j ON j.operation_id=o.id JOIN source s ON s.id=c.source_id JOIN document_version v ON v.id=c.version_id ORDER BY c.id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        for row in rows {
            let (s, input_hash, operation, outbox, index_job, receipt, source, hash, anchor) = row?;
            let record: CaptureRecord = serde_json::from_str(&s)?;
            let p = &record.receipt;
            let valid:i64=db.query_row("SELECT count(*) FROM capture c JOIN source s ON s.id=c.source_id JOIN document_version v ON v.id=c.version_id JOIN operation o ON o.capture_id=c.id JOIN receipt r ON r.operation_id=o.id JOIN outbox b ON b.operation_id=o.id JOIN index_job j ON j.operation_id=o.id WHERE c.id=?1 AND c.source_id=?2 AND c.version_id=?3 AND v.source_id=?2 AND o.id=?4 AND r.payload=?5 AND c.record=?6",params![p.capture_id,p.source_id,p.document_version_id,p.operation_id,receipt,s],|r|r.get(0))?;
            if valid != 1 {
                return fail("SQL identity relation mismatch");
            }
            if receipt != serde_json::to_string(&record.receipt)?
                || source != serde_json::to_string(&record.draft.source)?
                || hash != record.receipt.snapshot_hash
                || anchor
                    != serde_json::to_string(
                        &record
                            .draft
                            .anchor(&record.receipt.document_version_id.to_string())?,
                    )?
            {
                return fail("inconsistent Core relations");
            }
            blobs.insert(hash.clone(), String::from_utf8(cas.read(&hash)?)?);
            entries.push(ExportEntry {
                record,
                input_hash,
                operation,
                outbox,
                index_job,
            });
        }
        for table in [
            "capture",
            "source",
            "document_version",
            "operation",
            "receipt",
            "outbox",
            "index_job",
        ] {
            let count: i64 =
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            if count as usize != entries.len() {
                return fail("incomplete transaction records");
            }
        }
        let bundle = ExportBundle {
            schema_version: 1,
            entries,
            blobs: blobs
                .into_iter()
                .map(|(hash, text)| Blob { hash, text })
                .collect(),
        };
        validate_text_bundle(&bundle, false)?;
        Ok(bundle)
    }
    fn read_snapshot(db: &Connection, cas: &BlobStore) -> Internal<ArchiveV2> {
        for (table, payload) in [
            (
                "pdf_operation",
                "length(CAST(operation_id AS BLOB))+length(CAST(kind AS BLOB))+length(CAST(input_hash AS BLOB))+length(CAST(payload AS BLOB))",
            ),
            ("pdf_receipt", "length(CAST(payload AS BLOB))"),
            ("pdf_outbox", "length(CAST(payload AS BLOB))"),
            ("pdf_index_job", "length(CAST(payload AS BLOB))"),
            (
                "commit_log",
                "length(CAST(operation_id AS BLOB))+length(CAST(input_hash AS BLOB))+length(CAST(receipt AS BLOB))",
            ),
        ] {
            let (count, bytes): (i64, i64) = db.query_row(
                &format!("SELECT count(*),coalesce(sum({payload}),0) FROM {table}"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if count > 4096 || bytes > ARCHIVE_LIMIT as i64 {
                return fail("PDF operation or global log budget exceeded");
            }
        }
        let text = Self::export_text(db, cas)?;
        let mut archive = ArchiveV2 {
            schema_version: 2,
            text_entries: text.entries,
            pdf_documents: vec![],
            pdf_captures: vec![],
            pdf_operations: vec![],
            commits: vec![],
            blobs: text
                .blobs
                .iter()
                .map(|blob| BinaryBlob::new(blob.text.as_bytes()))
                .collect(),
        };
        for (table, target) in [("pdf_document", true), ("pdf_capture", false)] {
            let count: i64 =
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            let bytes: i64 = db.query_row(
                &format!("SELECT coalesce(sum(length(CAST(record AS BLOB))),0) FROM {table}"),
                [],
                |r| r.get(0),
            )?;
            if count > 4096 || bytes > ARCHIVE_LIMIT as i64 {
                return fail("PDF table budget exceeded");
            }
            let mut statement = db.prepare(&format!("SELECT record FROM {table} ORDER BY id"))?;
            for row in statement.query_map([], |r| r.get::<_, String>(0))? {
                let raw = row?;
                if target {
                    archive.pdf_documents.push(serde_json::from_str(&raw)?)
                } else {
                    archive.pdf_captures.push(serde_json::from_str(&raw)?)
                }
            }
        }
        let mut statement=db.prepare("SELECT o.operation_id,o.kind,o.document_id,o.capture_id,o.input_hash,o.payload,r.payload,b.payload,j.payload FROM pdf_operation o JOIN pdf_receipt r ON r.operation_id=o.operation_id JOIN pdf_outbox b ON b.operation_id=o.operation_id JOIN pdf_index_job j ON j.operation_id=o.operation_id ORDER BY o.operation_id")?;
        for row in statement.query_map([], |r| {
            Ok(PdfOperationEntry {
                operation_id: r.get(0)?,
                kind: r.get(1)?,
                document_id: r.get(2)?,
                capture_id: r.get(3)?,
                input_hash: r.get(4)?,
                operation: r.get(5)?,
                receipt: r.get(6)?,
                outbox: r.get(7)?,
                index_job: r.get(8)?,
            })
        })? {
            archive.pdf_operations.push(row?);
        }
        let mut statement=db.prepare("SELECT commit_rev,operation_id,kind,entity_id,input_hash,receipt FROM commit_log ORDER BY commit_rev")?;
        for row in statement.query_map([], |r| {
            Ok(CommitEntry {
                commit_rev: r.get(0)?,
                operation_id: r.get(1)?,
                kind: r.get(2)?,
                entity_id: r.get(3)?,
                input_hash: r.get(4)?,
                receipt: r.get(5)?,
            })
        })? {
            archive.commits.push(row?);
        }
        let mut original_hashes = BTreeSet::new();
        let mut original_bytes = 0u64;
        for doc in &archive.pdf_documents {
            if original_hashes.insert(&doc.receipt.original_sha256) {
                if doc.snapshot.original_bytes > MAX_ORIGINAL as u64 {
                    return fail("PDF original budget exceeded");
                }
                original_bytes = original_bytes
                    .checked_add(doc.snapshot.original_bytes)
                    .ok_or("original byte budget overflow")?;
                if original_bytes > ARCHIVE_LIMIT as u64 {
                    return fail("PDF originals total byte budget exceeded");
                }
            }
        }
        for doc in &archive.pdf_documents {
            let p = &doc.receipt;
            let valid:i64=db.query_row("SELECT count(*) FROM pdf_document d JOIN pdf_extraction e ON e.document_id=d.id WHERE d.id=?1 AND d.vault=?2 AND d.title=?3 AND d.original_hash=?4 AND d.original_bytes=?5 AND d.extraction_revision=?6 AND d.record=?7 AND e.snapshot=?8",params![p.document_id,doc.vault,doc.title,p.original_sha256,doc.snapshot.original_bytes,p.extraction_revision,serde_json::to_string(doc)?,serde_json::to_string(&doc.snapshot)?],|r|r.get(0))?;
            if valid != 1 {
                return fail("PDF document/extraction SQL identity mismatch");
            }
            if !archive
                .blobs
                .iter()
                .any(|blob| blob.hash == p.original_sha256)
            {
                archive
                    .blobs
                    .push(BinaryBlob::new(&cas.read(&p.original_sha256)?));
            }
        }
        for record in &archive.pdf_captures {
            let p = &record.receipt;
            let valid:i64=db.query_row("SELECT count(*) FROM pdf_capture WHERE id=?1 AND document_id=?2 AND record=?3 AND anchor=?4",params![p.capture_id,p.document_id,serde_json::to_string(record)?,capture_anchor(record)?],|r|r.get(0))?;
            if valid != 1 {
                return fail("PDF capture SQL identity mismatch");
            }
        }
        for (table, count) in [
            ("pdf_document", archive.pdf_documents.len()),
            ("pdf_extraction", archive.pdf_documents.len()),
            ("pdf_capture", archive.pdf_captures.len()),
            ("pdf_operation", archive.pdf_operations.len()),
            ("pdf_receipt", archive.pdf_operations.len()),
            ("pdf_outbox", archive.pdf_operations.len()),
            ("pdf_index_job", archive.pdf_operations.len()),
            ("commit_log", archive.commits.len()),
        ] {
            let actual: i64 =
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            if actual as usize != count {
                return fail("incomplete PDF transaction");
            }
        }
        archive
            .pdf_operations
            .sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        archive.blobs.sort_by(|a, b| a.hash.cmp(&b.hash));
        if archive
            .pdf_captures
            .iter()
            .any(|record| record.compound.is_some())
        {
            archive.schema_version = 3;
        }
        validate_archive(&archive)?;
        Ok(archive)
    }
}
fn insert(tx: &rusqlite::Transaction<'_>, e: &ExportEntry) -> Internal<()> {
    let r = &e.record;
    let p = &r.receipt;
    tx.execute(
        "INSERT INTO source VALUES(?,?)",
        params![p.source_id, serde_json::to_string(&r.draft.source)?],
    )?;
    tx.execute(
        "INSERT INTO document_version VALUES(?,?,?)",
        params![p.document_version_id, p.source_id, p.snapshot_hash],
    )?;
    tx.execute(
        "INSERT INTO capture VALUES(?,?,?,?,?)",
        params![
            p.capture_id,
            p.source_id,
            p.document_version_id,
            serde_json::to_string(r)?,
            serde_json::to_string(&r.draft.anchor(&p.document_version_id.to_string())?)?
        ],
    )?;
    tx.execute(
        "INSERT INTO operation VALUES(?,?,?,?)",
        params![p.operation_id, p.capture_id, e.input_hash, e.operation],
    )?;
    for (table, payload) in [
        ("receipt", serde_json::to_string(p)?),
        ("outbox", e.outbox.clone()),
        ("index_job", e.index_job.clone()),
    ] {
        tx.execute(
            &format!("INSERT INTO {table} VALUES(?,?)"),
            params![p.operation_id, payload],
        )?;
    }
    Ok(())
}
fn insert_commit(tx: &rusqlite::Transaction<'_>, entry: &CommitEntry) -> Internal<()> {
    tx.execute(
        "INSERT INTO commit_log VALUES(?,?,?,?,?,?)",
        params![
            entry.commit_rev,
            entry.operation_id,
            entry.kind,
            entry.entity_id,
            entry.input_hash,
            entry.receipt
        ],
    )?;
    Ok(())
}
fn insert_document(tx: &rusqlite::Transaction<'_>, record: &PdfDocumentRecord) -> Internal<()> {
    let p = &record.receipt;
    tx.execute(
        "INSERT INTO pdf_document VALUES(?,?,?,?,?,?,?)",
        params![
            p.document_id,
            record.vault,
            record.title,
            p.original_sha256,
            record.snapshot.original_bytes,
            p.extraction_revision,
            serde_json::to_string(record)?
        ],
    )?;
    tx.execute(
        "INSERT INTO pdf_extraction VALUES(?,?)",
        params![p.document_id, serde_json::to_string(&record.snapshot)?],
    )?;
    Ok(())
}
fn insert_pdf_capture(tx: &rusqlite::Transaction<'_>, record: &PdfCaptureRecord) -> Internal<()> {
    let p = &record.receipt;
    tx.execute(
        "INSERT INTO pdf_capture VALUES(?,?,?,?)",
        params![
            p.capture_id,
            p.document_id,
            serde_json::to_string(record)?,
            capture_anchor(record)?
        ],
    )?;
    Ok(())
}
fn insert_pdf_operation(
    tx: &rusqlite::Transaction<'_>,
    operation: &PdfOperationEntry,
) -> Internal<()> {
    tx.execute(
        "INSERT INTO pdf_operation VALUES(?,?,?,?,?,?)",
        params![
            operation.operation_id,
            operation.kind,
            operation.document_id,
            operation.capture_id,
            operation.input_hash,
            operation.operation
        ],
    )?;
    for (table, payload) in [
        ("pdf_receipt", &operation.receipt),
        ("pdf_outbox", &operation.outbox),
        ("pdf_index_job", &operation.index_job),
    ] {
        tx.execute(
            &format!("INSERT INTO {table} VALUES(?,?)"),
            params![operation.operation_id, payload],
        )?;
    }
    Ok(())
}
fn migrate(db: &Connection, observe: &dyn Fn(StorageStage)) -> Internal<()> {
    db.execute_batch("BEGIN IMMEDIATE;
        CREATE TABLE commit_log(commit_rev INTEGER PRIMARY KEY,operation_id TEXT UNIQUE NOT NULL,kind TEXT NOT NULL CHECK(kind IN('text','pdf_import','pdf_capture')),entity_id INTEGER NOT NULL,input_hash TEXT NOT NULL,receipt TEXT NOT NULL);
        CREATE TABLE pdf_document(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,original_hash TEXT NOT NULL,original_bytes INTEGER NOT NULL,extraction_revision TEXT NOT NULL,record TEXT NOT NULL);
        CREATE TABLE pdf_extraction(document_id INTEGER PRIMARY KEY REFERENCES pdf_document(id),snapshot TEXT NOT NULL);
        CREATE TABLE pdf_capture(id INTEGER PRIMARY KEY,document_id INTEGER NOT NULL REFERENCES pdf_document(id),record TEXT NOT NULL,anchor TEXT NOT NULL);
        CREATE TABLE pdf_operation(operation_id TEXT PRIMARY KEY REFERENCES commit_log(operation_id),kind TEXT NOT NULL,document_id INTEGER NOT NULL REFERENCES pdf_document(id),capture_id INTEGER UNIQUE REFERENCES pdf_capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL);
        CREATE TABLE pdf_receipt(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
        CREATE TABLE pdf_outbox(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
        CREATE TABLE pdf_index_job(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
        INSERT INTO commit_log SELECT json_extract(r.payload,'$.commit_rev'),o.id,'text',o.capture_id,o.input_hash,r.payload FROM operation o JOIN receipt r ON r.operation_id=o.id;
        PRAGMA user_version=3;")?;
    observe(StorageStage::BeforeMigrationCommit);
    db.execute_batch("COMMIT;")?;
    observe(StorageStage::AfterMigrationCommit);
    Ok(())
}
fn migrate_compound(db: &Connection, observe: &dyn Fn(StorageStage)) -> Internal<()> {
    // Existing columns contain strict tagged-by-presence JSON. Mark the new
    // reader requirement atomically, without rewriting historical data/hashes.
    db.execute_batch("BEGIN IMMEDIATE; PRAGMA user_version=3;")?;
    observe(StorageStage::BeforeMigrationCommit);
    db.execute_batch("COMMIT;")?;
    observe(StorageStage::AfterMigrationCommit);
    Ok(())
}

fn conflict(message: &str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::Conflict, message)
}
fn validate_transition(old: &ArchiveV2, plan: &CommitPlan) -> Result<()> {
    validate_archive(&plan.next)?;
    let next = &plan.next;
    if plan.expected_revision != old.commits.len() as i64 {
        return Err(conflict("stale knowledge commit plan"));
    }
    if next.commits.len() != old.commits.len() + 1
        || !next.commits.starts_with(&old.commits)
        || !next.text_entries.starts_with(&old.text_entries)
        || !next.pdf_documents.starts_with(&old.pdf_documents)
        || !next.pdf_captures.starts_with(&old.pdf_captures)
        || old
            .pdf_operations
            .iter()
            .any(|o| !next.pdf_operations.contains(o))
        || old.blobs.iter().any(|o| !next.blobs.contains(o))
    {
        return Err(conflict(
            "commit plan must append exactly one immutable transaction",
        ));
    }
    let commit = next
        .commits
        .last()
        .ok_or_else(|| conflict("commit plan has no operation"))?;
    let (text, document, capture, pdf_operation) = match commit.kind.as_str() {
        "text" => (1, 0, 0, 0),
        "pdf_import" => (0, 1, 0, 1),
        "pdf_capture" => (0, 0, 1, 1),
        _ => return Err(conflict("unknown transaction kind")),
    };
    if next.text_entries.len() != old.text_entries.len() + text
        || next.pdf_documents.len() != old.pdf_documents.len() + document
        || next.pdf_captures.len() != old.pdf_captures.len() + capture
        || next.pdf_operations.len() != old.pdf_operations.len() + pdf_operation
    {
        return Err(conflict("commit plan modifies unrelated knowledge"));
    }
    Ok(())
}
impl KnowledgeStore for SqliteStore {
    fn snapshot(&self) -> Result<ArchiveV2> {
        Self::read_snapshot(&self.db, &self.blobs).map_err(storage)
    }
    fn commit(&mut self, plan: CommitPlan) -> Result<()> {
        let old = self.snapshot()?;
        validate_transition(&old, &plan)?;
        let next = &plan.next;
        for blob in &next.blobs {
            if !old.blobs.iter().any(|b| b.hash == blob.hash) {
                self.blobs.publish(&blob.decode()?, &|stage| {
                    (self.observe)(match stage {
                        BlobStage::AfterWrite => StorageStage::AfterBlobWrite,
                        BlobStage::BeforePublish => StorageStage::BeforeBlobPublish,
                        BlobStage::AfterPublish => StorageStage::AfterBlobPublish,
                    })
                })?;
            }
        }
        self.commit_inner(&old, &plan).map_err(storage)
    }
    fn restore(&mut self, archive: &ArchiveV2) -> Result<()> {
        validate_archive(archive)?;
        if !self.snapshot()?.commits.is_empty() {
            return Err(conflict("restore requires empty Core"));
        }
        for blob in &archive.blobs {
            self.blobs.publish(&blob.decode()?, &|stage| {
                (self.observe)(match stage {
                    BlobStage::AfterWrite => StorageStage::AfterBlobWrite,
                    BlobStage::BeforePublish => StorageStage::BeforeBlobPublish,
                    BlobStage::AfterPublish => StorageStage::AfterBlobPublish,
                })
            })?;
        }
        self.restore_inner(archive).map_err(storage)
    }
}
impl SqliteStore {
    fn commit_inner(&mut self, old: &ArchiveV2, plan: &CommitPlan) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self.db.transaction()?;
        let rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        if rev != plan.expected_revision {
            return Err(conflict("stale knowledge commit plan").into());
        }
        let current = Self::read_snapshot(&tx, &self.blobs)?;
        if &current != old {
            return Err(conflict("knowledge changed before transaction").into());
        }
        validate_transition(&current, plan)?;
        let next = &plan.next;
        let commit = next.commits.last().ok_or("commit missing")?;
        if tx
            .prepare("SELECT 1 FROM commit_log WHERE operation_id=?")?
            .exists([&commit.operation_id])?
        {
            return Err(conflict("operation already committed").into());
        }
        for entry in &next.text_entries[old.text_entries.len()..] {
            insert(&tx, entry)?;
        }
        for record in &next.pdf_documents[old.pdf_documents.len()..] {
            insert_document(&tx, record)?;
        }
        for record in &next.pdf_captures[old.pdf_captures.len()..] {
            insert_pdf_capture(&tx, record)?;
        }
        insert_commit(&tx, commit)?;
        for operation in &next.pdf_operations {
            if !old
                .pdf_operations
                .iter()
                .any(|o| o.operation_id == operation.operation_id)
            {
                insert_pdf_operation(&tx, operation)?;
            }
        }
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }
    fn restore_inner(&mut self, archive: &ArchiveV2) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self.db.transaction()?;
        let count: i64 = tx.query_row("SELECT count(*) FROM commit_log", [], |r| r.get(0))?;
        if count != 0 {
            return Err(conflict("restore requires empty Core").into());
        }
        let current = Self::read_snapshot(&tx, &self.blobs)?;
        if !current.commits.is_empty() {
            return Err(conflict("restore requires empty Core").into());
        }
        validate_archive(archive)?;
        for e in &archive.text_entries {
            insert(&tx, e)?;
        }
        for d in &archive.pdf_documents {
            insert_document(&tx, d)?;
        }
        for c in &archive.pdf_captures {
            insert_pdf_capture(&tx, c)?;
        }
        for c in &archive.commits {
            insert_commit(&tx, c)?;
        }
        for o in &archive.pdf_operations {
            insert_pdf_operation(&tx, o)?;
        }
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }
}
