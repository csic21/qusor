//! SQLite relationship verification, schema migration and atomic split-port persistence.
use super::StorageStage;
use blob_store::{BlobStage, BlobStore};
use core_api::contract::{CaptureDraft, sha256};
use core_api::epub_contract::{
    EPUB_PROTOCOL_VERSION, EPUB_TEXT_UNIT, EpubDraft, EpubSnapshot, resolve_epub,
};
use core_api::knowledge::*;
use core_api::pdf_compound::PdfCompoundDraft;
use core_api::pdf_contract::{PdfDraft, PdfSnapshot, resolve_pdf};
use core_api::vocab::{
    CardSchedule, DAILY_NEW_LIMIT, ReviewEntry, ReviewGuard, STUDY_SCHEMA_VERSION, StudyArchive,
    StudyCard, StudyRating, StudyStage, Wordbook, WordbookSeed,
    validate_archive as validate_study_archive, validate_review as validate_study_review,
    validate_schedule as validate_study_schedule, validate_seed as validate_study_seed,
};
#[allow(deprecated)]
use learning_ports::{CommitPlan, KnowledgeStore, StudyStore};
use learning_ports::{
    EpubCaptureWithVault, EpubChapterLookup, EpubDocumentSummaryRow, EpubDocumentsPageData,
    EpubNoteSummaryRow, EpubNotesPageData, EpubSavedSelectionRaw, EpubSavedSelectionsLookup,
    KnowledgeArchive, KnowledgeCommand, KnowledgeQuery, OperationLookup, PdfCaptureWithVault,
    PdfDocumentSummaryRow, PdfDocumentsPageData, PdfNoteSummaryRow, PdfNotesPageData,
    PdfPageTextLookup, PdfSavedSelectionRaw, PdfSavedSelectionsLookup, StudyArchivePort,
    StudyCommand, StudyDeckCounts, StudyQuery, StudyQueueData, StudyQueueEntry, StudyReviewContext,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::{
    error::Error,
    fs::{self, File},
    path::Path,
    sync::Arc,
};

type Internal<T> = std::result::Result<T, Box<dyn Error>>;
type Observer = Arc<dyn Fn(StorageStage) + Send + Sync>;

/// Split-port SQLite core implementation. Owned by [`SqliteStore`] and exposed directly for
/// callers migrating off deprecated whole-snapshot `KnowledgeStore`/`StudyStore` bounds.
// Declaration order closes SQLite before releasing filesystem writer ownership.
pub struct SqliteCore {
    db: Connection,
    blobs: BlobStore,
    observe: Observer,
}

/// Concrete SQLite + CAS store implementing legacy [`KnowledgeStore`]/[`StudyStore`] traits
/// via delegation to [`SqliteCore`] split ports.
pub struct SqliteStore {
    core: SqliteCore,
}

fn fail<T>(message: &str) -> Internal<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Integrity, message).into())
}

fn conflict(message: &str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::Conflict, message)
}

fn sqlite_error_kind(err: &rusqlite::Error) -> &'static str {
    match err {
        rusqlite::Error::SqliteFailure(_, _) => "SqliteFailure",
        rusqlite::Error::SqliteSingleThreadedMode => "SqliteSingleThreadedMode",
        rusqlite::Error::FromSqlConversionFailure(_, _, _) => "FromSqlConversionFailure",
        rusqlite::Error::IntegralValueOutOfRange(_, _) => "IntegralValueOutOfRange",
        rusqlite::Error::Utf8Error(_) => "Utf8Error",
        rusqlite::Error::NulError(_) => "NulError",
        rusqlite::Error::InvalidParameterName(_) => "InvalidParameterName",
        rusqlite::Error::InvalidPath(_) => "InvalidPath",
        rusqlite::Error::ExecuteReturnedResults => "ExecuteReturnedResults",
        rusqlite::Error::QueryReturnedNoRows => "QueryReturnedNoRows",
        rusqlite::Error::InvalidColumnIndex(_) => "InvalidColumnIndex",
        rusqlite::Error::InvalidColumnName(_) => "InvalidColumnName",
        rusqlite::Error::InvalidColumnType(_, _, _) => "InvalidColumnType",
        rusqlite::Error::StatementChangedRows(_) => "StatementChangedRows",
        rusqlite::Error::ToSqlConversionFailure(_) => "ToSqlConversionFailure",
        rusqlite::Error::InvalidQuery => "InvalidQuery",
        rusqlite::Error::MultipleStatement => "MultipleStatement",
        rusqlite::Error::InvalidParameterCount(_, _) => "InvalidParameterCount",
        _ => "Other",
    }
}

fn storage(error: Box<dyn Error>) -> KnowledgeError {
    if let Some(value) = error.downcast_ref::<KnowledgeError>() {
        return if value.code == KnowledgeErrorCode::InvalidInput {
            KnowledgeError::new(
                KnowledgeErrorCode::Integrity,
                "stored knowledge encoding is invalid [category=integrity]",
            )
        } else {
            value.clone()
        };
    }
    if let Some(sqlite_err) = error.downcast_ref::<rusqlite::Error>() {
        let detail = match sqlite_err {
            rusqlite::Error::SqliteFailure(ffi, _) => format!(
                "knowledge storage operation failed [category=sqlite code={:?} extended={}]",
                ffi.code, ffi.extended_code
            ),
            other => format!(
                "knowledge storage operation failed [category=sqlite kind={}]",
                sqlite_error_kind(other)
            ),
        };
        return KnowledgeError::new(KnowledgeErrorCode::Storage, detail);
    }
    if let Some(io_err) = error.downcast_ref::<std::io::Error>() {
        let os_detail = io_err
            .raw_os_error()
            .map_or_else(String::new, |code| format!(" os={code}"));
        return KnowledgeError::new(
            KnowledgeErrorCode::Storage,
            format!(
                "knowledge storage operation failed [category=io kind={:?}{os_detail}]",
                io_err.kind()
            ),
        );
    }
    if error.is::<serde_json::Error>() {
        return KnowledgeError::new(
            KnowledgeErrorCode::Integrity,
            "stored knowledge encoding is invalid [category=json]",
        );
    }
    if error.is::<std::string::FromUtf8Error>() || error.is::<std::str::Utf8Error>() {
        return KnowledgeError::new(
            KnowledgeErrorCode::Integrity,
            "stored knowledge encoding is invalid [category=utf8]",
        );
    }
    KnowledgeError::new(
        KnowledgeErrorCode::Integrity,
        "knowledge storage consistency check failed [category=integrity]",
    )
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_observer(path, Arc::new(|_| {}))
    }

    pub fn with_observer(path: impl AsRef<Path>, observe: Observer) -> Result<Self> {
        let core = SqliteCore::open_inner(path.as_ref(), observe).map_err(storage)?;
        Ok(Self { core })
    }

    pub fn set_observer(&mut self, observe: Observer) {
        self.core.set_observer(observe);
    }

    pub fn core(&self) -> &SqliteCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut SqliteCore {
        &mut self.core
    }

    pub fn blob_read_count(&self) -> u64 {
        self.core.blob_read_count()
    }

    pub fn reset_blob_read_count(&self) {
        self.core.reset_blob_read_count();
    }
}

impl SqliteCore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_observer(path, Arc::new(|_| {}))
    }

    pub fn with_observer(path: impl AsRef<Path>, observe: Observer) -> Result<Self> {
        Self::open_inner(path.as_ref(), observe).map_err(storage)
    }

    pub fn set_observer(&mut self, observe: Observer) {
        self.observe = observe;
    }

    pub fn blob_read_count(&self) -> u64 {
        self.blobs.read_count()
    }

    pub fn reset_blob_read_count(&self) {
        self.blobs.reset_read_count();
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
        } else if !(1..=6).contains(&version) {
            return fail("unknown Core schema");
        }
        if version < 2 {
            migrate(&db, observe.as_ref())?;
        } else if version == 2 {
            migrate_compound(&db, observe.as_ref())?;
        }
        if version < 4 {
            migrate_study(&db, observe.as_ref())?;
        }
        if version < 5 {
            migrate_v5(&db, observe.as_ref())?;
        }
        if version < 6 {
            migrate_v6(&db, observe.as_ref())?;
        }
        let expected_tables: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('vocab_wordbook','vocab_card','vocab_schedule','vocab_review','epub_documents','epub_chapters','epub_toc','epub_captures','knowledge_audit','pdf_document_meta','pdf_capture_meta')",
            [],
            |r| r.get(0),
        )?;
        if expected_tables != 11 {
            return fail("missing v6 schema tables");
        }
        let check: String = db.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if check != "ok" {
            return fail("database integrity failure");
        }
        let fk: bool = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if fk {
            return fail("broken relationships");
        }
        Ok(Self { db, blobs, observe })
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
        let mut blobs = BTreeMap::new();
        let mut stmt = db.prepare("SELECT c.record,o.input_hash,o.payload,b.payload,j.payload,r.payload,s.descriptor,v.snapshot_hash,c.anchor FROM capture c JOIN operation o ON o.capture_id=c.id JOIN receipt r ON r.operation_id=o.id JOIN outbox b ON b.operation_id=o.id JOIN index_job j ON j.operation_id=o.id JOIN source s ON s.id=c.source_id JOIN document_version v ON v.id=c.version_id ORDER BY c.id")?;
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
            let valid: i64 = db.query_row(
                "SELECT count(*) FROM capture c JOIN source s ON s.id=c.source_id JOIN document_version v ON v.id=c.version_id JOIN operation o ON o.capture_id=c.id JOIN receipt r ON r.operation_id=o.id JOIN outbox b ON b.operation_id=o.id JOIN index_job j ON j.operation_id=o.id WHERE c.id=?1 AND c.source_id=?2 AND c.version_id=?3 AND v.source_id=?2 AND o.id=?4 AND r.payload=?5 AND c.record=?6",
                params![
                    p.capture_id,
                    p.source_id,
                    p.document_version_id,
                    p.operation_id,
                    receipt,
                    s
                ],
                |r| r.get(0),
            )?;
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
            epub_documents: vec![],
            epub_captures: vec![],
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
        let mut statement = db.prepare("SELECT o.operation_id,o.kind,o.document_id,o.capture_id,o.input_hash,o.payload,r.payload,b.payload,j.payload FROM pdf_operation o JOIN pdf_receipt r ON r.operation_id=o.operation_id JOIN pdf_outbox b ON b.operation_id=o.operation_id JOIN pdf_index_job j ON j.operation_id=o.operation_id ORDER BY o.operation_id")?;
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
        let mut statement = db.prepare("SELECT commit_rev,operation_id,kind,entity_id,input_hash,receipt FROM commit_log ORDER BY commit_rev")?;
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
            let valid: i64 = db.query_row("SELECT count(*) FROM pdf_document d JOIN pdf_extraction e ON e.document_id=d.id WHERE d.id=?1 AND d.vault=?2 AND d.title=?3 AND d.original_hash=?4 AND d.original_bytes=?5 AND d.extraction_revision=?6 AND d.record=?7 AND e.snapshot=?8", params![p.document_id, doc.vault, doc.title, p.original_sha256, doc.snapshot.original_bytes, p.extraction_revision, serde_json::to_string(doc)?, serde_json::to_string(&doc.snapshot)?], |r| r.get(0))?;
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
            let valid: i64 = db.query_row("SELECT count(*) FROM pdf_capture WHERE id=?1 AND document_id=?2 AND record=?3 AND anchor=?4", params![p.capture_id, p.document_id, serde_json::to_string(record)?, capture_anchor(record)?], |r| r.get(0))?;
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
        {
            let doc_count: i64 =
                db.query_row("SELECT count(*) FROM epub_documents", [], |r| r.get(0))?;
            let cap_count: i64 =
                db.query_row("SELECT count(*) FROM epub_captures", [], |r| r.get(0))?;
            let chapter_bytes: i64 = db.query_row(
                "SELECT coalesce(sum(length(CAST(text AS BLOB))),0) FROM epub_chapters",
                [],
                |r| r.get(0),
            )?;
            if doc_count > 4096 || cap_count > 4096 || chapter_bytes > ARCHIVE_LIMIT as i64 {
                return fail("EPUB table budget exceeded");
            }
            let mut stmt = db.prepare(
                "SELECT id,vault,title,author,original_hash,original_bytes,extraction_revision,operation_id,commit_rev,receipt FROM epub_documents ORDER BY id",
            )?;
            let doc_rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, String>(9)?,
                ))
            })?;
            for row in doc_rows {
                let (
                    id,
                    vault,
                    title,
                    author,
                    original_sha256,
                    original_bytes,
                    extraction_revision,
                    operation_id,
                    commit_rev,
                    receipt_raw,
                ) = row?;
                let receipt: EpubDocumentReceipt = serde_json::from_str(&receipt_raw)?;
                if receipt.document_id != id
                    || receipt.operation_id != operation_id
                    || receipt.commit_rev != commit_rev
                    || receipt.original_sha256 != original_sha256
                    || receipt.extraction_revision != extraction_revision
                {
                    return fail("EPUB document SQL identity mismatch");
                }
                let mut chapters = Vec::new();
                let mut ch_stmt = db.prepare(
                    "SELECT chapter_index,title,href,text FROM epub_chapters WHERE document_id=?1 ORDER BY chapter_index",
                )?;
                for ch_row in ch_stmt.query_map([id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })? {
                    let (chapter_index, ch_title, href, text) = ch_row?;
                    chapters.push(EpubChapterRecord {
                        chapter_index: u32::try_from(chapter_index)?,
                        title: ch_title,
                        href,
                        text,
                    });
                }
                let mut toc = Vec::new();
                let mut toc_stmt = db.prepare(
                    "SELECT title,chapter_index,depth FROM epub_toc WHERE document_id=?1 ORDER BY position",
                )?;
                for toc_row in toc_stmt.query_map([id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })? {
                    let (toc_title, chapter_index, depth) = toc_row?;
                    toc.push(EpubTocRecord {
                        title: toc_title,
                        chapter_index: u32::try_from(chapter_index)?,
                        depth: u8::try_from(depth)?,
                    });
                }
                archive.epub_documents.push(EpubDocumentRecord {
                    receipt,
                    vault,
                    title,
                    author,
                    original_bytes: u64::try_from(original_bytes)?,
                    chapters,
                    toc,
                });
            }
            let mut cap_stmt = db.prepare(
                "SELECT id,document_id,operation_id,commit_rev,draft,provenance,user_note,receipt FROM epub_captures ORDER BY id",
            )?;
            for row in cap_stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })? {
                let (
                    id,
                    document_id,
                    operation_id,
                    commit_rev,
                    draft_raw,
                    prov_raw,
                    user_note,
                    receipt_raw,
                ) = row?;
                let receipt: EpubReceipt = serde_json::from_str(&receipt_raw)?;
                if receipt.capture_id != id
                    || receipt.document_id != document_id
                    || receipt.operation_id != operation_id
                    || receipt.commit_rev != commit_rev
                {
                    return fail("EPUB capture SQL identity mismatch");
                }
                let draft: EpubDraft = serde_json::from_str(&draft_raw)?;
                let provenance: Provenance = serde_json::from_str(&prov_raw)?;
                archive.epub_captures.push(EpubCaptureRecord {
                    receipt,
                    draft,
                    provenance,
                    user_note,
                });
            }
            let total_chapters: usize = archive
                .epub_documents
                .iter()
                .map(|doc| doc.chapters.len())
                .sum();
            let total_toc: usize = archive.epub_documents.iter().map(|doc| doc.toc.len()).sum();
            let actual_chapters: i64 =
                db.query_row("SELECT count(*) FROM epub_chapters", [], |r| r.get(0))?;
            let actual_toc: i64 =
                db.query_row("SELECT count(*) FROM epub_toc", [], |r| r.get(0))?;
            if actual_chapters as usize != total_chapters || actual_toc as usize != total_toc {
                return fail("incomplete EPUB chapters or TOC");
            }
        }
        archive
            .pdf_operations
            .sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        archive.blobs.sort_by(|a, b| a.hash.cmp(&b.hash));
        archive.schema_version = archive_schema_version(&archive);
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
    tx.execute(
        "INSERT INTO pdf_document_meta(document_id, commit_rev, page_count) VALUES(?1,?2,?3)",
        params![
            p.document_id,
            p.commit_rev,
            i64::try_from(record.snapshot.pages.len())?
        ],
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
    tx.execute(
        "INSERT INTO pdf_capture_meta(capture_id, document_id, commit_rev) VALUES(?1,?2,?3)",
        params![p.capture_id, p.document_id, p.commit_rev],
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

fn insert_epub_document(
    tx: &rusqlite::Transaction<'_>,
    record: &EpubDocumentRecord,
) -> Internal<()> {
    let p = &record.receipt;
    tx.execute(
        "INSERT INTO epub_documents(id,vault,title,author,original_hash,original_bytes,extraction_revision,operation_id,commit_rev,receipt) VALUES(?,?,?,?,?,?,?,?,?,?)",
        params![
            p.document_id,
            record.vault,
            record.title,
            record.author,
            p.original_sha256,
            i64::try_from(record.original_bytes)?,
            p.extraction_revision,
            p.operation_id,
            p.commit_rev,
            serde_json::to_string(p)?
        ],
    )?;
    for chapter in &record.chapters {
        tx.execute(
            "INSERT INTO epub_chapters(document_id,chapter_index,title,href,text) VALUES(?,?,?,?,?)",
            params![
                p.document_id,
                i64::from(chapter.chapter_index),
                chapter.title,
                chapter.href,
                chapter.text
            ],
        )?;
    }
    for (position, entry) in record.toc.iter().enumerate() {
        tx.execute(
            "INSERT INTO epub_toc(document_id,position,title,chapter_index,depth) VALUES(?,?,?,?,?)",
            params![
                p.document_id,
                i64::try_from(position)?,
                entry.title,
                i64::from(entry.chapter_index),
                i64::from(entry.depth)
            ],
        )?;
    }
    Ok(())
}

fn insert_epub_capture(tx: &rusqlite::Transaction<'_>, record: &EpubCaptureRecord) -> Internal<()> {
    let p = &record.receipt;
    tx.execute(
        "INSERT INTO epub_captures(id,document_id,operation_id,commit_rev,draft,provenance,user_note,receipt) VALUES(?,?,?,?,?,?,?,?)",
        params![
            p.capture_id,
            p.document_id,
            p.operation_id,
            p.commit_rev,
            serde_json::to_string(&record.draft)?,
            serde_json::to_string(&record.provenance)?,
            record.user_note,
            serde_json::to_string(p)?
        ],
    )?;
    Ok(())
}

fn record_audit(
    tx: &rusqlite::Transaction<'_>,
    vault: &str,
    action: &str,
    entity_kind: &str,
    entity_id: i64,
    detail: &str,
) -> Internal<()> {
    tx.execute(
        "INSERT INTO knowledge_audit(vault,action,entity_kind,entity_id,detail) VALUES(?,?,?,?,?)",
        params![vault, action, entity_kind, entity_id, detail],
    )?;
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

fn has_column(db: &Connection, table: &str, column: &str) -> Internal<bool> {
    let mut stmt = db.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_v5(db: &Connection, observe: &dyn Fn(StorageStage)) -> Internal<()> {
    let has_pos = has_column(db, "vocab_card", "pos")?;
    let has_phonetic = has_column(db, "vocab_card", "phonetic")?;
    db.execute_batch("PRAGMA foreign_keys=OFF;")?;
    let migration_result: Internal<()> = (|| {
        db.execute_batch("BEGIN IMMEDIATE;")?;
        if !has_pos {
            db.execute_batch("ALTER TABLE vocab_card ADD COLUMN pos TEXT NOT NULL DEFAULT '';")?;
        }
        if !has_phonetic {
            db.execute_batch(
                "ALTER TABLE vocab_card ADD COLUMN phonetic TEXT NOT NULL DEFAULT '';",
            )?;
        }
        db.execute_batch(
            "CREATE TABLE commit_log_v5(
                commit_rev INTEGER PRIMARY KEY,
                operation_id TEXT UNIQUE NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN('text','pdf_import','pdf_capture','epub_import','epub_capture')),
                entity_id INTEGER NOT NULL,
                input_hash TEXT NOT NULL,
                receipt TEXT NOT NULL
            );
            INSERT INTO commit_log_v5 SELECT commit_rev,operation_id,kind,entity_id,input_hash,receipt FROM commit_log;
            DROP TABLE commit_log;
            ALTER TABLE commit_log_v5 RENAME TO commit_log;
            CREATE TABLE IF NOT EXISTS epub_documents(
                id INTEGER PRIMARY KEY,
                vault TEXT NOT NULL,
                title TEXT NOT NULL,
                author TEXT NOT NULL,
                original_hash TEXT NOT NULL,
                original_bytes INTEGER NOT NULL,
                extraction_revision TEXT NOT NULL,
                operation_id TEXT NOT NULL UNIQUE REFERENCES commit_log(operation_id),
                commit_rev INTEGER NOT NULL,
                receipt TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS epub_chapters(
                document_id INTEGER NOT NULL REFERENCES epub_documents(id),
                chapter_index INTEGER NOT NULL,
                title TEXT NOT NULL,
                href TEXT NOT NULL,
                text TEXT NOT NULL,
                PRIMARY KEY(document_id, chapter_index)
            );
            CREATE TABLE IF NOT EXISTS epub_toc(
                document_id INTEGER NOT NULL REFERENCES epub_documents(id),
                position INTEGER NOT NULL,
                title TEXT NOT NULL,
                chapter_index INTEGER NOT NULL,
                depth INTEGER NOT NULL,
                PRIMARY KEY(document_id, position)
            );
            CREATE TABLE IF NOT EXISTS epub_captures(
                id INTEGER PRIMARY KEY,
                document_id INTEGER NOT NULL REFERENCES epub_documents(id),
                operation_id TEXT NOT NULL UNIQUE REFERENCES commit_log(operation_id),
                commit_rev INTEGER NOT NULL,
                draft TEXT NOT NULL,
                provenance TEXT NOT NULL,
                user_note TEXT NOT NULL,
                receipt TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS knowledge_audit(
                id INTEGER PRIMARY KEY,
                vault TEXT NOT NULL,
                action TEXT NOT NULL,
                entity_kind TEXT NOT NULL,
                entity_id INTEGER NOT NULL,
                detail TEXT NOT NULL
            );
            PRAGMA user_version=5;",
        )?;
        observe(StorageStage::BeforeMigrationCommit);
        db.execute_batch("COMMIT;")?;
        observe(StorageStage::AfterMigrationCommit);
        Ok(())
    })();
    let _ = db.execute_batch("PRAGMA foreign_keys=ON;");
    migration_result?;
    let fk: bool = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
    if fk {
        return fail("broken relationships after v5 migration");
    }
    Ok(())
}

fn migrate_v6(db: &Connection, observe: &dyn Fn(StorageStage)) -> Internal<()> {
    db.execute_batch(
        "BEGIN IMMEDIATE;
        CREATE TABLE IF NOT EXISTS pdf_document_meta(
            document_id INTEGER PRIMARY KEY REFERENCES pdf_document(id),
            commit_rev INTEGER NOT NULL,
            page_count INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO pdf_document_meta(document_id, commit_rev, page_count)
            SELECT id,
                   CAST(json_extract(record, '$.receipt.commit_rev') AS INTEGER),
                   COALESCE(json_array_length(record, '$.snapshot.pages'), 0)
            FROM pdf_document;
        CREATE TABLE IF NOT EXISTS pdf_capture_meta(
            capture_id INTEGER PRIMARY KEY REFERENCES pdf_capture(id),
            document_id INTEGER NOT NULL REFERENCES pdf_document(id),
            commit_rev INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO pdf_capture_meta(capture_id, document_id, commit_rev)
            SELECT id,
                   document_id,
                   CAST(json_extract(record, '$.receipt.commit_rev') AS INTEGER)
            FROM pdf_capture;
        CREATE INDEX IF NOT EXISTS idx_commit_log_kind_rev_entity
            ON commit_log(kind, commit_rev DESC, entity_id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_document_vault_id
            ON pdf_document(vault, id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_document_meta_rev_doc
            ON pdf_document_meta(commit_rev DESC, document_id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_capture_doc_id
            ON pdf_capture(document_id, id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_capture_meta_rev_cap
            ON pdf_capture_meta(commit_rev DESC, capture_id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_capture_meta_doc_rev_cap
            ON pdf_capture_meta(document_id, commit_rev DESC, capture_id DESC);
        CREATE INDEX IF NOT EXISTS idx_pdf_operation_doc_kind
            ON pdf_operation(document_id, kind);
        CREATE INDEX IF NOT EXISTS idx_epub_documents_vault_rev_id
            ON epub_documents(vault, commit_rev DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_epub_captures_doc_rev_id
            ON epub_captures(document_id, commit_rev DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_vocab_card_book_pos_id
            ON vocab_card(wordbook_id, position, id);
        CREATE INDEX IF NOT EXISTS idx_vocab_schedule_due_card
            ON vocab_schedule(due_at_ms, card_id);
        CREATE INDEX IF NOT EXISTS idx_vocab_review_card_stage_time
            ON vocab_review(card_id, stage_before, reviewed_at_ms);
        PRAGMA user_version=6;",
    )?;
    observe(StorageStage::BeforeMigrationCommit);
    db.execute_batch("COMMIT;")?;
    observe(StorageStage::AfterMigrationCommit);
    let fk: bool = db.prepare("PRAGMA foreign_key_check")?.exists([])?;
    if fk {
        return fail("broken relationships after v6 migration");
    }
    Ok(())
}

fn validate_transition(old: &ArchiveV2, plan: &CommitPlan) -> Result<()> {
    validate_archive(&plan.next)?;
    let next = &plan.next;
    let expected_rev = old.commits.iter().map(|c| c.commit_rev).max().unwrap_or(0);
    if plan.expected_revision != expected_rev {
        return Err(conflict("stale knowledge commit plan"));
    }
    if next.commits.len() != old.commits.len() + 1
        || !next.commits.starts_with(&old.commits)
        || !next.text_entries.starts_with(&old.text_entries)
        || !next.pdf_documents.starts_with(&old.pdf_documents)
        || !next.pdf_captures.starts_with(&old.pdf_captures)
        || !next.epub_documents.starts_with(&old.epub_documents)
        || !next.epub_captures.starts_with(&old.epub_captures)
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
    let (text, document, capture, pdf_operation, epub_doc, epub_cap) = match commit.kind.as_str() {
        "text" => (1, 0, 0, 0, 0, 0),
        "pdf_import" => (0, 1, 0, 1, 0, 0),
        "pdf_capture" => (0, 0, 1, 1, 0, 0),
        "epub_import" => (0, 0, 0, 0, 1, 0),
        "epub_capture" => (0, 0, 0, 0, 0, 1),
        _ => return Err(conflict("unknown transaction kind")),
    };
    if next.text_entries.len() != old.text_entries.len() + text
        || next.pdf_documents.len() != old.pdf_documents.len() + document
        || next.pdf_captures.len() != old.pdf_captures.len() + capture
        || next.pdf_operations.len() != old.pdf_operations.len() + pdf_operation
        || next.epub_documents.len() != old.epub_documents.len() + epub_doc
        || next.epub_captures.len() != old.epub_captures.len() + epub_cap
    {
        return Err(conflict("commit plan modifies unrelated knowledge"));
    }
    Ok(())
}

fn validate_vault_arg(vault: &str) -> Result<()> {
    if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
        return Err(KnowledgeError::new(
            KnowledgeErrorCode::InvalidInput,
            "invalid vault",
        ));
    }
    Ok(())
}

fn not_found_err(message: &str) -> KnowledgeError {
    KnowledgeError::new(KnowledgeErrorCode::NotFound, message)
}

fn publish_blob_with_stage(
    blobs: &BlobStore,
    bytes: &[u8],
    observe: &dyn Fn(StorageStage),
) -> Result<()> {
    blobs.publish(bytes, &|stage| {
        observe(match stage {
            BlobStage::AfterWrite => StorageStage::AfterBlobWrite,
            BlobStage::BeforePublish => StorageStage::BeforeBlobPublish,
            BlobStage::AfterPublish => StorageStage::AfterBlobPublish,
        })
    })
}

impl SqliteCore {
    fn text_capture_inner(&self, id: i64) -> Internal<Option<CaptureRecord>> {
        let row: Option<(String, String, String, String, String)> = self
            .db
            .query_row(
                "SELECT c.record, c.anchor, s.descriptor, v.snapshot_hash, r.payload
                 FROM capture c
                 JOIN source s ON s.id = c.source_id
                 JOIN document_version v ON v.id = c.version_id
                 JOIN operation o ON o.capture_id = c.id
                 JOIN receipt r ON r.operation_id = o.id
                 WHERE c.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let Some((raw_record, anchor, source_desc, hash, raw_receipt)) = row else {
            return Ok(None);
        };
        let record: CaptureRecord = serde_json::from_str(&raw_record)?;
        if raw_receipt != serde_json::to_string(&record.receipt)?
            || source_desc != serde_json::to_string(&record.draft.source)?
            || hash != record.receipt.snapshot_hash
            || anchor
                != serde_json::to_string(
                    &record
                        .draft
                        .anchor(&record.receipt.document_version_id.to_string())?,
                )?
        {
            return fail("inconsistent Core text capture relations");
        }
        let blob_bytes = self.blobs.read(&hash)?;
        let blob_text = String::from_utf8(blob_bytes)?;
        if blob_text != record.draft.snapshot_text {
            return fail("referenced immutable text blob mismatch");
        }
        Ok(Some(record))
    }

    fn text_receipt_by_operation_inner(&self, op: &str) -> Internal<Option<Receipt>> {
        let payload: Option<String> = self
            .db
            .query_row(
                "SELECT payload FROM receipt WHERE operation_id = ?1",
                [op],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .map(|raw| Ok(serde_json::from_str(&raw)?))
            .transpose()
    }

    fn commit_by_operation_inner(&self, op: &str) -> Internal<Option<CommitEntry>> {
        let row: Option<CommitEntry> = self
            .db
            .query_row(
                "SELECT commit_rev,operation_id,kind,entity_id,input_hash,receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| {
                    Ok(CommitEntry {
                        commit_rev: r.get(0)?,
                        operation_id: r.get(1)?,
                        kind: r.get(2)?,
                        entity_id: r.get(3)?,
                        input_hash: r.get(4)?,
                        receipt: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    fn pdf_document_inner(&self, id: i64) -> Internal<Option<PdfDocumentRecord>> {
        let raw: Option<String> = self
            .db
            .query_row("SELECT record FROM pdf_document WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }

    fn pdf_document_receipt_by_operation_inner(
        &self,
        op: &str,
    ) -> Internal<Option<PdfDocumentReceipt>> {
        let payload: Option<String> = self
            .db
            .query_row(
                "SELECT r.payload FROM pdf_operation o JOIN pdf_receipt r ON r.operation_id = o.operation_id WHERE o.operation_id = ?1 AND o.kind = 'pdf_import'",
                [op],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .map(|raw| Ok(serde_json::from_str(&raw)?))
            .transpose()
    }

    fn pdf_capture_inner(&self, id: i64) -> Internal<Option<PdfCaptureRecord>> {
        let raw: Option<String> = self
            .db
            .query_row("SELECT record FROM pdf_capture WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }

    fn pdf_capture_with_vault_inner(&self, id: i64) -> Internal<Option<PdfCaptureWithVault>> {
        let row: Option<(String, Option<String>)> = self
            .db
            .query_row(
                "SELECT c.record, d.vault FROM pdf_capture c LEFT JOIN pdf_document d ON d.id = c.document_id WHERE c.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((cap_raw, doc_vault)) = row else {
            return Ok(None);
        };
        let Some(document_vault) = doc_vault else {
            return fail("note document is missing from Core");
        };
        Ok(Some(PdfCaptureWithVault {
            record: serde_json::from_str(&cap_raw)?,
            document_vault,
        }))
    }

    fn pdf_capture_by_operation_inner(
        &self,
        op: &str,
    ) -> Internal<OperationLookup<PdfCaptureWithVault>> {
        let found: Option<(String, Option<String>)> = self
            .db
            .query_row(
                "SELECT c.record, d.vault FROM pdf_operation o JOIN pdf_capture c ON c.id = o.capture_id LEFT JOIN pdf_document d ON d.id = c.document_id WHERE o.operation_id = ?1 AND o.kind = 'pdf_capture'",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((cap_raw, doc_vault)) = found {
            let Some(document_vault) = doc_vault else {
                return fail("note document is missing from Core");
            };
            return Ok(OperationLookup::Match(PdfCaptureWithVault {
                record: serde_json::from_str(&cap_raw)?,
                document_vault,
            }));
        }
        let exists = self
            .db
            .prepare("SELECT 1 FROM commit_log WHERE operation_id = ?1")?
            .exists([op])?;
        Ok(if exists {
            OperationLookup::OtherOperationKind
        } else {
            OperationLookup::Absent
        })
    }

    fn pdf_documents_page_inner(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Internal<PdfDocumentsPageData> {
        let fetch_limit = i64::try_from(limit.saturating_add(1))?;
        let mut rows = Vec::new();
        if let Some((cursor_rev, cursor_id)) = after {
            let cursor_in_vault = self
                .db
                .prepare(
                    "SELECT 1 FROM pdf_document d JOIN pdf_document_meta m ON m.document_id = d.id WHERE d.id = ?1 AND m.commit_rev = ?2 AND d.vault = ?3",
                )?
                .exists(params![cursor_id, cursor_rev, vault])?;
            if !cursor_in_vault {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::InvalidInput,
                    "library cursor does not belong to this vault",
                )
                .into());
            }
            let mut stmt = self.db.prepare(
                "SELECT d.id, m.commit_rev, d.title, d.original_hash, d.original_bytes, d.extraction_revision, m.page_count
                 FROM pdf_document d
                 JOIN pdf_document_meta m ON m.document_id = d.id
                 WHERE d.vault = ?1
                   AND (m.commit_rev < ?2 OR (m.commit_rev = ?2 AND d.id < ?3))
                 ORDER BY m.commit_rev DESC, d.id DESC
                 LIMIT ?4",
            )?;
            for row in stmt.query_map(params![vault, cursor_rev, cursor_id, fetch_limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })? {
                rows.push(row?);
            }
        } else {
            let mut stmt = self.db.prepare(
                "SELECT d.id, m.commit_rev, d.title, d.original_hash, d.original_bytes, d.extraction_revision, m.page_count
                 FROM pdf_document d
                 JOIN pdf_document_meta m ON m.document_id = d.id
                 WHERE d.vault = ?1
                 ORDER BY m.commit_rev DESC, d.id DESC
                 LIMIT ?2",
            )?;
            for row in stmt.query_map(params![vault, fetch_limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })? {
                rows.push(row?);
            }
        }
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor_key = if has_more {
            rows.last().map(|r| (r.1, r.0))
        } else {
            None
        };
        let documents = rows
            .into_iter()
            .map(
                |(
                    document_id,
                    commit_rev,
                    title,
                    original_sha256,
                    original_bytes,
                    extraction_revision,
                    page_count,
                )| {
                    Ok(PdfDocumentSummaryRow {
                        document_id,
                        commit_rev,
                        title,
                        original_sha256,
                        original_bytes: u64::try_from(original_bytes)?,
                        extraction_revision,
                        page_count: usize::try_from(page_count)?,
                    })
                },
            )
            .collect::<Internal<Vec<_>>>()?;
        Ok(PdfDocumentsPageData {
            documents,
            next_cursor_key,
        })
    }

    fn pdf_single_page_engine_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Internal<PdfPageTextLookup> {
        let json_path_idx = format!("$.pages[{page_index}].page_index");
        let json_path_text = format!("$.pages[{page_index}].engine_text");
        let row: Option<(Option<i64>, Option<String>)> = self
            .db
            .query_row(
                "SELECT json_extract(e.snapshot, ?3), json_extract(e.snapshot, ?4)
                 FROM pdf_document d
                 JOIN pdf_extraction e ON e.document_id = d.id
                 WHERE d.id = ?1 AND d.vault = ?2",
                params![document_id, vault, json_path_idx, json_path_text],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((found_idx, found_text)) = row else {
            return Ok(PdfPageTextLookup::DocumentMissing);
        };
        if found_idx == Some(i64::from(page_index))
            && let Some(engine_text) = found_text
        {
            return Ok(PdfPageTextLookup::Found { engine_text });
        }
        let snapshot_raw: String = self.db.query_row(
            "SELECT e.snapshot FROM pdf_document d JOIN pdf_extraction e ON e.document_id = d.id WHERE d.id = ?1 AND d.vault = ?2",
            params![document_id, vault],
            |r| r.get(0),
        )?;
        let snapshot: PdfSnapshot = serde_json::from_str(&snapshot_raw)?;
        let Some(page) = snapshot.pages.get(page_index as usize) else {
            return Ok(PdfPageTextLookup::PageMissing);
        };
        Ok(PdfPageTextLookup::Found {
            engine_text: page.engine_text.clone(),
        })
    }

    fn pdf_saved_selections_inner(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Internal<PdfSavedSelectionsLookup> {
        let page_scalar_count =
            match self.pdf_single_page_engine_text(vault, document_id, page_index)? {
                PdfPageTextLookup::DocumentMissing => {
                    return Ok(PdfSavedSelectionsLookup::DocumentMissing);
                }
                PdfPageTextLookup::PageMissing => return Ok(PdfSavedSelectionsLookup::PageMissing),
                PdfPageTextLookup::Found { engine_text } => engine_text.chars().count(),
            };
        let mut ranges = Vec::new();
        let mut stmt = self
            .db
            .prepare("SELECT record FROM pdf_capture WHERE document_id = ?1 ORDER BY id")?;
        for row in stmt.query_map([document_id], |r| r.get::<_, String>(0))? {
            let raw = row?;
            let capture: PdfCaptureRecord = serde_json::from_str(&raw)?;
            let drafts: Vec<&PdfDraft> = match &capture.compound {
                Some(compound) => compound.segments.iter().collect(),
                None => vec![&capture.draft],
            };
            for draft in drafts {
                if draft.page_index == page_index {
                    ranges.push(PdfSavedSelectionRaw {
                        capture_id: capture.receipt.capture_id,
                        start: draft.start,
                        end: draft.end,
                    });
                }
            }
        }
        Ok(PdfSavedSelectionsLookup::Found {
            page_scalar_count,
            ranges,
        })
    }

    fn pdf_notes_page_inner(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Internal<PdfNotesPageData> {
        if let Some((cursor_rev, cursor_id)) = after {
            let valid_cursor = match document_id {
                Some(doc_id) => self
                    .db
                    .prepare(
                        "SELECT 1 FROM pdf_capture_meta m JOIN pdf_document d ON d.id = m.document_id
                         WHERE m.capture_id = ?1 AND m.commit_rev = ?2 AND d.vault = ?3 AND m.document_id = ?4",
                    )?
                    .exists(params![cursor_id, cursor_rev, vault, doc_id])?,
                None => self
                    .db
                    .prepare(
                        "SELECT 1 FROM pdf_capture_meta m JOIN pdf_document d ON d.id = m.document_id
                         WHERE m.capture_id = ?1 AND m.commit_rev = ?2 AND d.vault = ?3",
                    )?
                    .exists(params![cursor_id, cursor_rev, vault])?,
            };
            if !valid_cursor {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::InvalidInput,
                    "note cursor does not belong to this query scope",
                )
                .into());
            }
        }
        let fetch_limit = i64::try_from(limit.saturating_add(1))?;
        let mut rows: Vec<(String, String, i64, i64)> = Vec::new();
        match (document_id, after) {
            (Some(doc_id), Some((cursor_rev, cursor_id))) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.record, d.title, m.commit_rev, c.id
                     FROM pdf_capture c
                     JOIN pdf_capture_meta m ON m.capture_id = c.id
                     JOIN pdf_document d ON d.id = c.document_id
                     WHERE d.vault = ?1 AND c.document_id = ?2
                       AND (m.commit_rev < ?3 OR (m.commit_rev = ?3 AND c.id < ?4))
                     ORDER BY m.commit_rev DESC, c.id DESC
                     LIMIT ?5",
                )?;
                for row in stmt.query_map(
                    params![vault, doc_id, cursor_rev, cursor_id, fetch_limit],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )? {
                    rows.push(row?);
                }
            }
            (Some(doc_id), None) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.record, d.title, m.commit_rev, c.id
                     FROM pdf_capture c
                     JOIN pdf_capture_meta m ON m.capture_id = c.id
                     JOIN pdf_document d ON d.id = c.document_id
                     WHERE d.vault = ?1 AND c.document_id = ?2
                     ORDER BY m.commit_rev DESC, c.id DESC
                     LIMIT ?3",
                )?;
                for row in stmt.query_map(params![vault, doc_id, fetch_limit], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })? {
                    rows.push(row?);
                }
            }
            (None, Some((cursor_rev, cursor_id))) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.record, d.title, m.commit_rev, c.id
                     FROM pdf_capture c
                     JOIN pdf_capture_meta m ON m.capture_id = c.id
                     JOIN pdf_document d ON d.id = c.document_id
                     WHERE d.vault = ?1
                       AND (m.commit_rev < ?2 OR (m.commit_rev = ?2 AND c.id < ?3))
                     ORDER BY m.commit_rev DESC, c.id DESC
                     LIMIT ?4",
                )?;
                for row in stmt
                    .query_map(params![vault, cursor_rev, cursor_id, fetch_limit], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })?
                {
                    rows.push(row?);
                }
            }
            (None, None) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.record, d.title, m.commit_rev, c.id
                     FROM pdf_capture c
                     JOIN pdf_capture_meta m ON m.capture_id = c.id
                     JOIN pdf_document d ON d.id = c.document_id
                     WHERE d.vault = ?1
                     ORDER BY m.commit_rev DESC, c.id DESC
                     LIMIT ?2",
                )?;
                for row in stmt.query_map(params![vault, fetch_limit], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })? {
                    rows.push(row?);
                }
            }
        }
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor_key = if has_more {
            rows.last().map(|r| (r.2, r.3))
        } else {
            None
        };
        let notes = rows
            .into_iter()
            .map(|(cap_raw, document_title, _, _)| {
                Ok(PdfNoteSummaryRow {
                    record: serde_json::from_str(&cap_raw)?,
                    document_title,
                })
            })
            .collect::<Internal<Vec<_>>>()?;
        Ok(PdfNotesPageData {
            notes,
            next_cursor_key,
        })
    }

    fn read_original_pdf_inner(&self, id: i64) -> Internal<Vec<u8>> {
        let hash: Option<String> = self
            .db
            .query_row(
                "SELECT original_hash FROM pdf_document WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(hash) = hash else {
            return Err(not_found_err("PDF document not found").into());
        };
        Ok(self.blobs.read(&hash)?)
    }

    fn epub_documents_page_inner(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Internal<EpubDocumentsPageData> {
        let fetch_limit = i64::try_from(limit.saturating_add(1))?;
        let mut rows = Vec::new();
        if let Some((cursor_rev, cursor_id)) = after {
            let cursor_ok = self
                .db
                .prepare(
                    "SELECT 1 FROM epub_documents WHERE id = ?1 AND commit_rev = ?2 AND vault = ?3",
                )?
                .exists(params![cursor_id, cursor_rev, vault])?;
            if !cursor_ok {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::InvalidInput,
                    "EPUB library cursor does not belong to this vault",
                )
                .into());
            }
            let mut stmt = self.db.prepare(
                "SELECT d.id, d.commit_rev, d.title, d.author, d.original_hash, d.original_bytes, d.extraction_revision,
                        (SELECT count(*) FROM epub_chapters c WHERE c.document_id = d.id)
                 FROM epub_documents d
                 WHERE d.vault = ?1
                   AND (d.commit_rev < ?2 OR (d.commit_rev = ?2 AND d.id < ?3))
                 ORDER BY d.commit_rev DESC, d.id DESC
                 LIMIT ?4",
            )?;
            for row in stmt.query_map(params![vault, cursor_rev, cursor_id, fetch_limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })? {
                rows.push(row?);
            }
        } else {
            let mut stmt = self.db.prepare(
                "SELECT d.id, d.commit_rev, d.title, d.author, d.original_hash, d.original_bytes, d.extraction_revision,
                        (SELECT count(*) FROM epub_chapters c WHERE c.document_id = d.id)
                 FROM epub_documents d
                 WHERE d.vault = ?1
                 ORDER BY d.commit_rev DESC, d.id DESC
                 LIMIT ?2",
            )?;
            for row in stmt.query_map(params![vault, fetch_limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })? {
                rows.push(row?);
            }
        }
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor_key = if has_more {
            rows.last().map(|r| (r.1, r.0))
        } else {
            None
        };
        let documents = rows
            .into_iter()
            .map(
                |(
                    document_id,
                    commit_rev,
                    title,
                    author,
                    original_sha256,
                    original_bytes,
                    extraction_revision,
                    chapter_count,
                )| {
                    Ok(EpubDocumentSummaryRow {
                        document_id,
                        commit_rev,
                        title,
                        author,
                        original_sha256,
                        original_bytes: u64::try_from(original_bytes)?,
                        extraction_revision,
                        chapter_count: usize::try_from(chapter_count)?,
                    })
                },
            )
            .collect::<Internal<Vec<_>>>()?;
        Ok(EpubDocumentsPageData {
            documents,
            next_cursor_key,
        })
    }

    fn epub_chapter_text_inner(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Internal<EpubChapterLookup> {
        let exists = self
            .db
            .prepare("SELECT 1 FROM epub_documents WHERE id = ?1 AND vault = ?2")?
            .exists(params![document_id, vault])?;
        if !exists {
            return Ok(EpubChapterLookup::DocumentMissing);
        }
        let row: Option<(String, String, String)> = self
            .db
            .query_row(
                "SELECT title, href, text FROM epub_chapters WHERE document_id = ?1 AND chapter_index = ?2",
                params![document_id, i64::from(chapter_index)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(match row {
            Some((title, href, text)) => EpubChapterLookup::Found { title, href, text },
            None => EpubChapterLookup::ChapterMissing,
        })
    }

    fn epub_toc_inner(
        &self,
        vault: &str,
        document_id: i64,
    ) -> Internal<Option<Vec<EpubTocRecord>>> {
        let exists = self
            .db
            .prepare("SELECT 1 FROM epub_documents WHERE id = ?1 AND vault = ?2")?
            .exists(params![document_id, vault])?;
        if !exists {
            return Ok(None);
        }
        let mut toc = Vec::new();
        let mut stmt = self.db.prepare(
            "SELECT title, chapter_index, depth FROM epub_toc WHERE document_id = ?1 ORDER BY position",
        )?;
        for row in stmt.query_map([document_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (title, chapter_index, depth) = row?;
            toc.push(EpubTocRecord {
                title,
                chapter_index: u32::try_from(chapter_index)?,
                depth: u8::try_from(depth)?,
            });
        }
        Ok(Some(toc))
    }

    fn epub_saved_selections_inner(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Internal<EpubSavedSelectionsLookup> {
        let text = match self.epub_chapter_text_inner(vault, document_id, chapter_index)? {
            EpubChapterLookup::DocumentMissing => {
                return Ok(EpubSavedSelectionsLookup::DocumentMissing);
            }
            EpubChapterLookup::ChapterMissing => {
                return Ok(EpubSavedSelectionsLookup::ChapterMissing);
            }
            EpubChapterLookup::Found { text, .. } => text,
        };
        let chapter_scalar_count = text.chars().count() as u64;
        let mut ranges = Vec::new();
        let mut stmt = self
            .db
            .prepare("SELECT id, draft FROM epub_captures WHERE document_id = ?1 ORDER BY id")?;
        for row in stmt.query_map([document_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })? {
            let (capture_id, draft_raw) = row?;
            let draft: EpubDraft = serde_json::from_str(&draft_raw)?;
            if draft.chapter_index == chapter_index {
                ranges.push(EpubSavedSelectionRaw {
                    capture_id,
                    start: draft.start,
                    end: draft.end,
                });
            }
        }
        Ok(EpubSavedSelectionsLookup::Found {
            chapter_scalar_count,
            ranges,
        })
    }

    fn epub_notes_page_inner(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Internal<EpubNotesPageData> {
        if let Some((cursor_rev, cursor_id)) = after {
            let valid = match document_id {
                Some(doc_id) => self
                    .db
                    .prepare(
                        "SELECT 1 FROM epub_captures c JOIN epub_documents d ON d.id = c.document_id
                         WHERE c.id = ?1 AND c.commit_rev = ?2 AND d.vault = ?3 AND c.document_id = ?4",
                    )?
                    .exists(params![cursor_id, cursor_rev, vault, doc_id])?,
                None => self
                    .db
                    .prepare(
                        "SELECT 1 FROM epub_captures c JOIN epub_documents d ON d.id = c.document_id
                         WHERE c.id = ?1 AND c.commit_rev = ?2 AND d.vault = ?3",
                    )?
                    .exists(params![cursor_id, cursor_rev, vault])?,
            };
            if !valid {
                return Err(KnowledgeError::new(
                    KnowledgeErrorCode::InvalidInput,
                    "EPUB note cursor does not belong to this query scope",
                )
                .into());
            }
        }
        let fetch_limit = i64::try_from(limit.saturating_add(1))?;
        let mut rows: Vec<(String, String, String, String, String, i64, i64)> = Vec::new();
        match (document_id, after) {
            (Some(doc_id), Some((cursor_rev, cursor_id))) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.receipt, c.draft, c.provenance, c.user_note, d.title, c.commit_rev, c.id
                     FROM epub_captures c
                     JOIN epub_documents d ON d.id = c.document_id
                     WHERE d.vault = ?1 AND c.document_id = ?2
                       AND (c.commit_rev < ?3 OR (c.commit_rev = ?3 AND c.id < ?4))
                     ORDER BY c.commit_rev DESC, c.id DESC
                     LIMIT ?5",
                )?;
                for row in stmt.query_map(
                    params![vault, doc_id, cursor_rev, cursor_id, fetch_limit],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )? {
                    rows.push(row?);
                }
            }
            (Some(doc_id), None) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.receipt, c.draft, c.provenance, c.user_note, d.title, c.commit_rev, c.id
                     FROM epub_captures c
                     JOIN epub_documents d ON d.id = c.document_id
                     WHERE d.vault = ?1 AND c.document_id = ?2
                     ORDER BY c.commit_rev DESC, c.id DESC
                     LIMIT ?3",
                )?;
                for row in stmt.query_map(params![vault, doc_id, fetch_limit], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })? {
                    rows.push(row?);
                }
            }
            (None, Some((cursor_rev, cursor_id))) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.receipt, c.draft, c.provenance, c.user_note, d.title, c.commit_rev, c.id
                     FROM epub_captures c
                     JOIN epub_documents d ON d.id = c.document_id
                     WHERE d.vault = ?1
                       AND (c.commit_rev < ?2 OR (c.commit_rev = ?2 AND c.id < ?3))
                     ORDER BY c.commit_rev DESC, c.id DESC
                     LIMIT ?4",
                )?;
                for row in
                    stmt.query_map(params![vault, cursor_rev, cursor_id, fetch_limit], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    })?
                {
                    rows.push(row?);
                }
            }
            (None, None) => {
                let mut stmt = self.db.prepare(
                    "SELECT c.receipt, c.draft, c.provenance, c.user_note, d.title, c.commit_rev, c.id
                     FROM epub_captures c
                     JOIN epub_documents d ON d.id = c.document_id
                     WHERE d.vault = ?1
                     ORDER BY c.commit_rev DESC, c.id DESC
                     LIMIT ?2",
                )?;
                for row in stmt.query_map(params![vault, fetch_limit], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })? {
                    rows.push(row?);
                }
            }
        }
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor_key = if has_more {
            rows.last().map(|r| (r.5, r.6))
        } else {
            None
        };
        let notes = rows
            .into_iter()
            .map(
                |(receipt_raw, draft_raw, prov_raw, user_note, document_title, _, _)| {
                    Ok(EpubNoteSummaryRow {
                        record: EpubCaptureRecord {
                            receipt: serde_json::from_str(&receipt_raw)?,
                            draft: serde_json::from_str(&draft_raw)?,
                            provenance: serde_json::from_str(&prov_raw)?,
                            user_note,
                        },
                        document_title,
                    })
                },
            )
            .collect::<Internal<Vec<_>>>()?;
        Ok(EpubNotesPageData {
            notes,
            next_cursor_key,
        })
    }

    fn epub_capture_with_vault_inner(&self, id: i64) -> Internal<Option<EpubCaptureWithVault>> {
        let row: Option<(String, String, String, String, Option<String>)> = self
            .db
            .query_row(
                "SELECT c.receipt, c.draft, c.provenance, c.user_note, d.vault
                 FROM epub_captures c
                 LEFT JOIN epub_documents d ON d.id = c.document_id
                 WHERE c.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let Some((receipt_raw, draft_raw, prov_raw, user_note, doc_vault)) = row else {
            return Ok(None);
        };
        let Some(document_vault) = doc_vault else {
            return fail("EPUB note document is missing from Core");
        };
        Ok(Some(EpubCaptureWithVault {
            record: EpubCaptureRecord {
                receipt: serde_json::from_str(&receipt_raw)?,
                draft: serde_json::from_str(&draft_raw)?,
                provenance: serde_json::from_str(&prov_raw)?,
                user_note,
            },
            document_vault,
        }))
    }

    fn commit_text_capture_inner(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Internal<Receipt> {
        let probe_record = CaptureRecord {
            receipt: Receipt {
                operation_id: op.into(),
                capture_id: 1,
                source_id: 1,
                document_version_id: 1,
                commit_rev: 1,
                snapshot_hash: draft.snapshot_hash(),
                draft_hash: draft.draft_hash()?,
            },
            vault: vault.into(),
            draft: draft.clone(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let expected_hash = input_hash(&probe_record)?;
        if let Some(existing) = self.commit_by_operation_inner(op)? {
            if existing.kind != "text" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if existing.input_hash != expected_hash {
                return Err(conflict("operation id payload conflict").into());
            }
            let receipt: Receipt = serde_json::from_str(&existing.receipt)?;
            let blob_bytes = self.blobs.read(&receipt.snapshot_hash)?;
            let blob_text = String::from_utf8(blob_bytes)?;
            if blob_text != draft.snapshot_text {
                return fail("referenced immutable text blob mismatch");
            }
            return Ok(receipt);
        }
        publish_blob_with_stage(
            &self.blobs,
            draft.snapshot_text.as_bytes(),
            self.observe.as_ref(),
        )?;
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_in_tx: Option<(String, String, String)> = tx
            .query_row(
                "SELECT kind, input_hash, receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((kind, prior_hash, prior_receipt_raw)) = existing_in_tx {
            if kind != "text" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if prior_hash != expected_hash {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&prior_receipt_raw)?);
        }
        let max_rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        let id: i64 = tx.query_row("SELECT coalesce(max(id),0)+1 FROM capture", [], |r| {
            r.get(0)
        })?;
        let receipt = Receipt {
            operation_id: op.into(),
            capture_id: id,
            source_id: id,
            document_version_id: id,
            commit_rev: max_rev + 1,
            snapshot_hash: draft.snapshot_hash(),
            draft_hash: draft.draft_hash()?,
        };
        let record = CaptureRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            draft: draft.clone(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let ev = event(&record)?;
        let entry = ExportEntry {
            record,
            input_hash: expected_hash,
            operation: ev.clone(),
            outbox: ev.clone(),
            index_job: ev,
        };
        let commit = text_commit(&entry)?;
        insert(&tx, &entry)?;
        insert_commit(&tx, &commit)?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(receipt)
    }

    fn import_pdf_document_inner(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Internal<PdfDocumentReceipt> {
        let orig_hash = sha256(original);
        let extraction_revision = snapshot.extraction_revision()?;
        let probe_record = PdfDocumentRecord {
            receipt: PdfDocumentReceipt {
                operation_id: op.into(),
                document_id: 1,
                commit_rev: 1,
                original_sha256: orig_hash.clone(),
                extraction_revision: extraction_revision.clone(),
            },
            vault: vault.into(),
            title: title.into(),
            snapshot: snapshot.clone(),
        };
        let expected_input = document_input(&probe_record)?;
        if let Some(existing) = self.commit_by_operation_inner(op)? {
            if existing.kind != "pdf_import" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if existing.input_hash != expected_input {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&existing.receipt)?);
        }
        publish_blob_with_stage(&self.blobs, original, self.observe.as_ref())?;
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_in_tx: Option<(String, String, String)> = tx
            .query_row(
                "SELECT kind, input_hash, receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((kind, prior_hash, prior_receipt_raw)) = existing_in_tx {
            if kind != "pdf_import" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if prior_hash != expected_input {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&prior_receipt_raw)?);
        }
        let max_rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        let id: i64 = tx.query_row("SELECT coalesce(max(id),0)+1 FROM pdf_document", [], |r| {
            r.get(0)
        })?;
        let receipt = PdfDocumentReceipt {
            operation_id: op.into(),
            document_id: id,
            commit_rev: max_rev + 1,
            original_sha256: orig_hash,
            extraction_revision,
        };
        let record = PdfDocumentRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            title: title.into(),
            snapshot: snapshot.clone(),
        };
        let operation = operation_for_document(&record)?;
        let commit = commit_for_pdf(&operation, receipt.commit_rev);
        insert_document(&tx, &record)?;
        insert_commit(&tx, &commit)?;
        insert_pdf_operation(&tx, &operation)?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(receipt)
    }

    fn capture_pdf_inner(
        &mut self,
        op: &str,
        document_id: i64,
        single: Option<&PdfDraft>,
        compound: Option<&PdfCompoundDraft>,
        origin: &Provenance,
        user_note: &str,
    ) -> Internal<PdfReceipt> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let doc_raw: Option<String> = tx
            .query_row(
                "SELECT record FROM pdf_document WHERE id = ?1",
                [document_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(doc_raw) = doc_raw else {
            return Err(not_found_err("PDF document not found").into());
        };
        let document: PdfDocumentRecord = serde_json::from_str(&doc_raw)?;
        validate_native(op, &document.vault, origin, user_note)?;
        let (draft, draft_hash) = match (single, compound) {
            (Some(d), None) => {
                resolve_pdf(&document.snapshot, d)?;
                (d, d.draft_hash()?)
            }
            (None, Some(c)) => {
                c.resolve(&document.snapshot)?;
                let first = c.segments.first().ok_or("compound PDF has no segments")?;
                (first, c.draft_hash()?)
            }
            _ => return Err(conflict("invalid PDF capture payload").into()),
        };
        let probe_record = PdfCaptureRecord {
            receipt: PdfReceipt {
                operation_id: op.into(),
                capture_id: 1,
                document_id,
                commit_rev: 1,
                original_sha256: document.receipt.original_sha256.clone(),
                extraction_revision: document.receipt.extraction_revision.clone(),
                draft_hash: draft_hash.clone(),
            },
            draft: draft.clone(),
            compound: compound.cloned(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let expected_input = capture_input(&probe_record, &document)?;
        let existing_in_tx: Option<(String, String, String)> = tx
            .query_row(
                "SELECT kind, input_hash, receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((kind, prior_hash, prior_receipt_raw)) = existing_in_tx {
            if kind != "pdf_capture" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if prior_hash != expected_input {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&prior_receipt_raw)?);
        }
        let max_rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        let next_capture_id: i64 =
            tx.query_row("SELECT coalesce(max(id),0)+1 FROM pdf_capture", [], |r| {
                r.get(0)
            })?;
        let receipt = PdfReceipt {
            operation_id: op.into(),
            capture_id: next_capture_id,
            document_id,
            commit_rev: max_rev + 1,
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            draft_hash,
        };
        let record = PdfCaptureRecord {
            receipt: receipt.clone(),
            draft: draft.clone(),
            compound: compound.cloned(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let operation = operation_for_capture(&record, &document)?;
        let commit = commit_for_pdf(&operation, receipt.commit_rev);
        insert_pdf_capture(&tx, &record)?;
        insert_commit(&tx, &commit)?;
        insert_pdf_operation(&tx, &operation)?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(receipt)
    }

    fn import_epub_inner(
        &mut self,
        op: &str,
        vault: &str,
        snapshot: &EpubSnapshot,
    ) -> Internal<EpubDocumentReceipt> {
        let probe_record = EpubDocumentRecord {
            receipt: EpubDocumentReceipt {
                operation_id: op.into(),
                document_id: 1,
                commit_rev: 1,
                original_sha256: snapshot.original_sha256.clone(),
                extraction_revision: snapshot.extraction_revision.clone(),
            },
            vault: vault.into(),
            title: snapshot.title.clone(),
            author: snapshot.author.clone(),
            original_bytes: snapshot.original_bytes,
            chapters: snapshot
                .chapters
                .iter()
                .map(|c| EpubChapterRecord {
                    chapter_index: c.chapter_index,
                    title: c.title.clone(),
                    href: c.href.clone(),
                    text: c.text.clone(),
                })
                .collect(),
            toc: snapshot
                .toc
                .iter()
                .map(|t| EpubTocRecord {
                    title: t.title.clone(),
                    chapter_index: t.chapter_index,
                    depth: t.depth,
                })
                .collect(),
        };
        let expected_input_hash = epub_document_input(&probe_record)?;
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_in_tx: Option<(String, String, String)> = tx
            .query_row(
                "SELECT kind, input_hash, receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((kind, prior_hash, prior_receipt_raw)) = existing_in_tx {
            if kind != "epub_import" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if prior_hash != expected_input_hash {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&prior_receipt_raw)?);
        }
        let max_rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        let id: i64 = tx.query_row(
            "SELECT coalesce(max(id),0)+1 FROM epub_documents",
            [],
            |r| r.get(0),
        )?;
        let receipt = EpubDocumentReceipt {
            operation_id: op.into(),
            document_id: id,
            commit_rev: max_rev + 1,
            original_sha256: snapshot.original_sha256.clone(),
            extraction_revision: snapshot.extraction_revision.clone(),
        };
        let mut record = probe_record;
        record.receipt = receipt.clone();
        let commit = commit_for_epub_document(&record)?;
        insert_commit(&tx, &commit)?;
        insert_epub_document(&tx, &record)?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(receipt)
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_epub_selection_inner(
        &mut self,
        op: &str,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
        start: u64,
        end: u64,
        origin: &Provenance,
        user_note: &str,
    ) -> Internal<EpubReceipt> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let doc_row: Option<(String, String, i64, String, String, String)> = tx
            .query_row(
                "SELECT title, author, original_bytes, original_hash, extraction_revision, receipt FROM epub_documents WHERE id = ?1 AND vault = ?2",
                params![document_id, vault],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            title,
            author,
            original_bytes,
            original_hash,
            extraction_revision,
            doc_receipt_raw,
        )) = doc_row
        else {
            return Err(not_found_err("EPUB document not found").into());
        };
        let doc_receipt: EpubDocumentReceipt = serde_json::from_str(&doc_receipt_raw)?;
        validate_native(op, vault, origin, user_note)?;
        let ch_row: Option<(String, String, String)> = tx
            .query_row(
                "SELECT title, href, text FROM epub_chapters WHERE document_id = ?1 AND chapter_index = ?2",
                params![document_id, i64::from(chapter_index)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((ch_title, ch_href, ch_text)) = ch_row else {
            return Err(not_found_err("EPUB chapter not found").into());
        };
        let scalar_count = ch_text.chars().count() as u64;
        if end > scalar_count {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "EPUB selection exceeds the stored chapter",
            )
            .into());
        }
        let exact = ch_text
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect::<String>();
        let draft = EpubDraft {
            protocol_version: EPUB_PROTOCOL_VERSION,
            original_sha256: original_hash.clone(),
            extraction_revision: extraction_revision.clone(),
            chapter_index,
            chapter_text_sha256: sha256(ch_text.as_bytes()),
            unit: EPUB_TEXT_UNIT.into(),
            start,
            end,
            exact,
        };
        let document = EpubDocumentRecord {
            receipt: doc_receipt,
            vault: vault.into(),
            title,
            author,
            original_bytes: u64::try_from(original_bytes)?,
            chapters: vec![EpubChapterRecord {
                chapter_index,
                title: ch_title,
                href: ch_href,
                text: ch_text,
            }],
            toc: vec![],
        };
        // Resolve draft against this document chapter without loading unrelated chapters.
        let sparse_chapters: Vec<EpubChapterRecord> = (0..=chapter_index)
            .map(|idx| {
                if idx == chapter_index {
                    document.chapters[0].clone()
                } else {
                    EpubChapterRecord {
                        chapter_index: idx,
                        title: String::new(),
                        href: format!("chapter-{idx}.xhtml"),
                        text: String::new(),
                    }
                }
            })
            .collect();
        let resolve_doc = EpubDocumentRecord {
            chapters: sparse_chapters,
            ..document.clone()
        };
        resolve_epub(&resolve_doc.to_snapshot(), &draft)
            .map_err(|e| KnowledgeError::new(KnowledgeErrorCode::InvalidInput, e.message))?;
        let probe_record = EpubCaptureRecord {
            receipt: EpubReceipt {
                operation_id: op.into(),
                capture_id: 1,
                document_id,
                commit_rev: 1,
                original_sha256: original_hash.clone(),
                extraction_revision: extraction_revision.clone(),
                draft_hash: draft.draft_hash()?,
            },
            draft: draft.clone(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let expected_input_hash = epub_capture_input(&probe_record, &document)?;
        let existing_in_tx: Option<(String, String, String)> = tx
            .query_row(
                "SELECT kind, input_hash, receipt FROM commit_log WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((kind, prior_hash, prior_receipt_raw)) = existing_in_tx {
            if kind != "epub_capture" {
                return Err(conflict(
                    "operation id already belongs to a different payload or kind",
                )
                .into());
            }
            if prior_hash != expected_input_hash {
                return Err(conflict("operation id payload conflict").into());
            }
            return Ok(serde_json::from_str(&prior_receipt_raw)?);
        }
        let max_rev: i64 = tx.query_row(
            "SELECT coalesce(max(commit_rev),0) FROM commit_log",
            [],
            |r| r.get(0),
        )?;
        let capture_id: i64 =
            tx.query_row("SELECT coalesce(max(id),0)+1 FROM epub_captures", [], |r| {
                r.get(0)
            })?;
        let receipt = EpubReceipt {
            operation_id: op.into(),
            capture_id,
            document_id,
            commit_rev: max_rev + 1,
            original_sha256: original_hash,
            extraction_revision,
            draft_hash: draft.draft_hash()?,
        };
        let mut record = probe_record;
        record.receipt = receipt.clone();
        let commit = commit_for_epub_capture(&record, &document)?;
        insert_commit(&tx, &commit)?;
        insert_epub_capture(&tx, &record)?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(receipt)
    }

    fn is_empty_inner(&self) -> Internal<bool> {
        let count: i64 = self
            .db
            .query_row("SELECT count(*) FROM commit_log", [], |r| r.get(0))?;
        Ok(count == 0)
    }

    fn commit_inner(&mut self, old: &ArchiveV2, plan: &CommitPlan) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
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
        for record in &next.epub_documents[old.epub_documents.len()..] {
            insert_epub_document(&tx, record)?;
        }
        for record in &next.epub_captures[old.epub_captures.len()..] {
            insert_epub_capture(&tx, record)?;
        }
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn restore_inner(&mut self, archive: &ArchiveV2) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
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
        for d in &archive.epub_documents {
            insert_epub_document(&tx, d)?;
        }
        for c in &archive.epub_captures {
            insert_epub_capture(&tx, c)?;
        }
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn update_pdf_capture_note_inner(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row: Option<(String, String)> = tx
            .query_row(
                "SELECT c.record, d.record FROM pdf_capture c JOIN pdf_document d ON d.id = c.document_id WHERE c.id=?1 AND d.vault=?2",
                params![capture_id, vault],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((cap_raw, doc_raw)) = row else {
            return Err(not_found_err("PDF capture not found").into());
        };
        let mut record: PdfCaptureRecord = serde_json::from_str(&cap_raw)?;
        let doc: PdfDocumentRecord = serde_json::from_str(&doc_raw)?;
        record.user_note = note.to_owned();
        let new_record = serde_json::to_string(&record)?;
        let new_op = operation_for_capture(&record, &doc)?;
        tx.execute(
            "UPDATE pdf_capture SET record=?1 WHERE id=?2",
            params![new_record, capture_id],
        )?;
        tx.execute(
            "UPDATE pdf_operation SET input_hash=?1 WHERE operation_id=?2",
            params![new_op.input_hash, new_op.operation_id],
        )?;
        tx.execute(
            "UPDATE commit_log SET input_hash=?1 WHERE operation_id=?2",
            params![new_op.input_hash, new_op.operation_id],
        )?;
        record_audit(
            &tx,
            vault,
            "update_pdf_capture_note",
            "pdf_capture",
            capture_id,
            &serde_json::to_string(&serde_json::json!({
                "capture_id": capture_id,
                "note_bytes": note.len()
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn delete_pdf_capture_inner(&mut self, vault: &str, capture_id: i64) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let op_id: Option<String> = tx
            .query_row(
                "SELECT o.operation_id FROM pdf_capture c JOIN pdf_document d ON d.id = c.document_id JOIN pdf_operation o ON o.capture_id = c.id WHERE c.id=?1 AND d.vault=?2",
                params![capture_id, vault],
                |r| r.get(0),
            )
            .optional()?;
        let Some(op_id) = op_id else {
            return Err(not_found_err("PDF capture not found").into());
        };
        for table in [
            "pdf_receipt",
            "pdf_outbox",
            "pdf_index_job",
            "pdf_operation",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE operation_id=?1"),
                [&op_id],
            )?;
        }
        tx.execute(
            "DELETE FROM pdf_capture_meta WHERE capture_id=?1",
            [capture_id],
        )?;
        tx.execute("DELETE FROM pdf_capture WHERE id=?1", [capture_id])?;
        tx.execute("DELETE FROM commit_log WHERE operation_id=?1", [&op_id])?;
        record_audit(
            &tx,
            vault,
            "delete_pdf_capture",
            "pdf_capture",
            capture_id,
            &serde_json::to_string(&serde_json::json!({
                "capture_id": capture_id,
                "operation_id": op_id
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn delete_pdf_document_inner(&mut self, vault: &str, document_id: i64) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM pdf_document WHERE id=?1 AND vault=?2",
                params![document_id, vault],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Err(not_found_err("PDF document not found").into());
        }
        let mut ops = Vec::new();
        {
            let mut stmt =
                tx.prepare("SELECT operation_id FROM pdf_operation WHERE document_id=?1")?;
            for row in stmt.query_map([document_id], |r| r.get::<_, String>(0))? {
                ops.push(row?);
            }
        }
        for op_id in &ops {
            for table in ["pdf_receipt", "pdf_outbox", "pdf_index_job"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE operation_id=?1"),
                    [op_id],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM pdf_operation WHERE document_id=?1",
            [document_id],
        )?;
        for op_id in &ops {
            tx.execute("DELETE FROM commit_log WHERE operation_id=?1", [op_id])?;
        }
        tx.execute(
            "DELETE FROM pdf_capture_meta WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute(
            "DELETE FROM pdf_capture WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute(
            "DELETE FROM pdf_extraction WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute(
            "DELETE FROM pdf_document_meta WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute(
            "DELETE FROM pdf_document WHERE id=?1 AND vault=?2",
            params![document_id, vault],
        )?;
        record_audit(
            &tx,
            vault,
            "delete_pdf_document",
            "pdf_document",
            document_id,
            &serde_json::to_string(&serde_json::json!({
                "document_id": document_id,
                "deleted_operations": ops.len()
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn update_epub_capture_note_inner(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row: Option<(i64, String, i64, String, String, String)> = tx
            .query_row(
                "SELECT c.document_id, c.operation_id, c.commit_rev, c.draft, c.provenance, c.receipt FROM epub_captures c JOIN epub_documents d ON d.id = c.document_id WHERE c.id=?1 AND d.vault=?2",
                params![capture_id, vault],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((document_id, operation_id, _commit_rev, draft_raw, prov_raw, receipt_raw)) = row
        else {
            return Err(not_found_err("EPUB capture not found").into());
        };
        let doc_row: (String, String, String, i64, String, String, i64, String) = tx.query_row(
            "SELECT title, author, original_hash, original_bytes, extraction_revision, operation_id, commit_rev, receipt FROM epub_documents WHERE id=?1",
            [document_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )?;
        let doc = EpubDocumentRecord {
            receipt: serde_json::from_str(&doc_row.7)?,
            vault: vault.to_owned(),
            title: doc_row.0,
            author: doc_row.1,
            original_bytes: u64::try_from(doc_row.3)?,
            chapters: vec![],
            toc: vec![],
        };
        let record = EpubCaptureRecord {
            receipt: serde_json::from_str(&receipt_raw)?,
            draft: serde_json::from_str(&draft_raw)?,
            provenance: serde_json::from_str(&prov_raw)?,
            user_note: note.to_owned(),
        };
        let new_hash = epub_capture_input(&record, &doc)?;
        tx.execute(
            "UPDATE epub_captures SET user_note=?1 WHERE id=?2",
            params![note, capture_id],
        )?;
        tx.execute(
            "UPDATE commit_log SET input_hash=?1 WHERE operation_id=?2",
            params![new_hash, operation_id],
        )?;
        record_audit(
            &tx,
            vault,
            "update_epub_capture_note",
            "epub_capture",
            capture_id,
            &serde_json::to_string(&serde_json::json!({
                "capture_id": capture_id,
                "note_bytes": note.len()
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn delete_epub_capture_inner(&mut self, vault: &str, capture_id: i64) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let op_id: Option<String> = tx
            .query_row(
                "SELECT c.operation_id FROM epub_captures c JOIN epub_documents d ON d.id = c.document_id WHERE c.id=?1 AND d.vault=?2",
                params![capture_id, vault],
                |r| r.get(0),
            )
            .optional()?;
        let Some(op_id) = op_id else {
            return Err(not_found_err("EPUB capture not found").into());
        };
        tx.execute("DELETE FROM epub_captures WHERE id=?1", [capture_id])?;
        tx.execute("DELETE FROM commit_log WHERE operation_id=?1", [&op_id])?;
        record_audit(
            &tx,
            vault,
            "delete_epub_capture",
            "epub_capture",
            capture_id,
            &serde_json::to_string(&serde_json::json!({
                "capture_id": capture_id,
                "operation_id": op_id
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn delete_epub_document_inner(&mut self, vault: &str, document_id: i64) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let doc_op: Option<String> = tx
            .query_row(
                "SELECT operation_id FROM epub_documents WHERE id=?1 AND vault=?2",
                params![document_id, vault],
                |r| r.get(0),
            )
            .optional()?;
        let Some(doc_op) = doc_op else {
            return Err(not_found_err("EPUB document not found").into());
        };
        let mut ops = vec![doc_op];
        {
            let mut stmt =
                tx.prepare("SELECT operation_id FROM epub_captures WHERE document_id=?1")?;
            for row in stmt.query_map([document_id], |r| r.get::<_, String>(0))? {
                ops.push(row?);
            }
        }
        tx.execute(
            "DELETE FROM epub_captures WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute("DELETE FROM epub_toc WHERE document_id=?1", [document_id])?;
        tx.execute(
            "DELETE FROM epub_chapters WHERE document_id=?1",
            [document_id],
        )?;
        tx.execute(
            "DELETE FROM epub_documents WHERE id=?1 AND vault=?2",
            params![document_id, vault],
        )?;
        for op_id in &ops {
            tx.execute("DELETE FROM commit_log WHERE operation_id=?1", [op_id])?;
        }
        record_audit(
            &tx,
            vault,
            "delete_epub_document",
            "epub_document",
            document_id,
            &serde_json::to_string(&serde_json::json!({
                "document_id": document_id,
                "deleted_operations": ops.len()
            }))?,
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }
}

impl KnowledgeQuery for SqliteCore {
    fn text_capture(&self, id: i64) -> Result<Option<CaptureRecord>> {
        self.text_capture_inner(id).map_err(storage)
    }

    fn text_receipt_by_operation(&self, op: &str) -> Result<Option<Receipt>> {
        self.text_receipt_by_operation_inner(op).map_err(storage)
    }

    fn commit_by_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        self.commit_by_operation_inner(op).map_err(storage)
    }

    fn pdf_document(&self, id: i64) -> Result<Option<PdfDocumentRecord>> {
        self.pdf_document_inner(id).map_err(storage)
    }

    fn pdf_document_receipt_by_operation(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        self.pdf_document_receipt_by_operation_inner(op)
            .map_err(storage)
    }

    fn pdf_capture(&self, id: i64) -> Result<Option<PdfCaptureRecord>> {
        self.pdf_capture_inner(id).map_err(storage)
    }

    fn pdf_capture_with_vault(&self, id: i64) -> Result<Option<PdfCaptureWithVault>> {
        self.pdf_capture_with_vault_inner(id).map_err(storage)
    }

    fn pdf_capture_by_operation(&self, op: &str) -> Result<OperationLookup<PdfCaptureWithVault>> {
        self.pdf_capture_by_operation_inner(op).map_err(storage)
    }

    fn pdf_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfDocumentsPageData> {
        self.pdf_documents_page_inner(vault, after, limit)
            .map_err(storage)
    }

    fn pdf_page_text(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfPageTextLookup> {
        self.pdf_single_page_engine_text(vault, document_id, page_index)
            .map_err(storage)
    }

    fn pdf_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        page_index: u32,
    ) -> Result<PdfSavedSelectionsLookup> {
        self.pdf_saved_selections_inner(vault, document_id, page_index)
            .map_err(storage)
    }

    fn pdf_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<PdfNotesPageData> {
        self.pdf_notes_page_inner(vault, document_id, after, limit)
            .map_err(storage)
    }

    fn read_original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        self.read_original_pdf_inner(id).map_err(storage)
    }

    fn epub_documents_page(
        &self,
        vault: &str,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubDocumentsPageData> {
        self.epub_documents_page_inner(vault, after, limit)
            .map_err(storage)
    }

    fn epub_chapter_text(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubChapterLookup> {
        self.epub_chapter_text_inner(vault, document_id, chapter_index)
            .map_err(storage)
    }

    fn epub_toc(&self, vault: &str, document_id: i64) -> Result<Option<Vec<EpubTocRecord>>> {
        self.epub_toc_inner(vault, document_id).map_err(storage)
    }

    fn epub_saved_selections(
        &self,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
    ) -> Result<EpubSavedSelectionsLookup> {
        self.epub_saved_selections_inner(vault, document_id, chapter_index)
            .map_err(storage)
    }

    fn epub_notes_page(
        &self,
        vault: &str,
        document_id: Option<i64>,
        after: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<EpubNotesPageData> {
        self.epub_notes_page_inner(vault, document_id, after, limit)
            .map_err(storage)
    }

    fn epub_capture_with_vault(&self, id: i64) -> Result<Option<EpubCaptureWithVault>> {
        self.epub_capture_with_vault_inner(id).map_err(storage)
    }
}

impl KnowledgeCommand for SqliteCore {
    fn commit_text_capture(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<Receipt> {
        draft.validate()?;
        validate_native(op, vault, origin, user_note)?;
        self.commit_text_capture_inner(op, vault, draft, origin, user_note)
            .map_err(storage)
    }

    fn import_pdf_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt> {
        native_import(op, vault, title)?;
        if original.len() > MAX_ORIGINAL {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "PDF original budget exceeded",
            ));
        }
        snapshot.validate_original(original)?;
        self.import_pdf_document_inner(op, vault, title, original, snapshot)
            .map_err(storage)
    }

    fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.capture_pdf_inner(op, document_id, Some(draft), None, origin, user_note)
            .map_err(storage)
    }

    fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.capture_pdf_inner(op, document_id, None, Some(draft), origin, user_note)
            .map_err(storage)
    }

    fn import_epub(
        &mut self,
        op: &str,
        vault: &str,
        snapshot: &EpubSnapshot,
    ) -> Result<EpubDocumentReceipt> {
        native_import(op, vault, &snapshot.title)?;
        snapshot.validate()?;
        if snapshot.vault != vault {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "EPUB snapshot vault does not match target vault",
            ));
        }
        self.import_epub_inner(op, vault, snapshot).map_err(storage)
    }

    fn capture_epub_selection(
        &mut self,
        op: &str,
        vault: &str,
        document_id: i64,
        chapter_index: u32,
        start: u64,
        end: u64,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<EpubReceipt> {
        validate_vault_arg(vault)?;
        if start >= end {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "EPUB selection range is empty or inverted",
            ));
        }
        self.capture_epub_selection_inner(
            op,
            vault,
            document_id,
            chapter_index,
            start,
            end,
            origin,
            user_note,
        )
        .map_err(storage)
    }

    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        validate_vault_arg(vault)?;
        validate_note_input(note)?;
        self.update_pdf_capture_note_inner(vault, capture_id, note)
            .map_err(storage)
    }

    fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        validate_vault_arg(vault)?;
        self.delete_pdf_capture_inner(vault, capture_id)
            .map_err(storage)
    }

    fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        validate_vault_arg(vault)?;
        self.delete_pdf_document_inner(vault, document_id)
            .map_err(storage)
    }

    fn update_epub_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        validate_vault_arg(vault)?;
        validate_note_input(note)?;
        self.update_epub_capture_note_inner(vault, capture_id, note)
            .map_err(storage)
    }

    fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        validate_vault_arg(vault)?;
        self.delete_epub_capture_inner(vault, capture_id)
            .map_err(storage)
    }

    fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        validate_vault_arg(vault)?;
        self.delete_epub_document_inner(vault, document_id)
            .map_err(storage)
    }
}

impl KnowledgeArchive for SqliteCore {
    fn is_empty(&self) -> Result<bool> {
        self.is_empty_inner().map_err(storage)
    }

    fn snapshot(&self) -> Result<ArchiveV2> {
        Self::read_snapshot(&self.db, &self.blobs).map_err(storage)
    }

    fn restore(&mut self, archive: &ArchiveV2) -> Result<()> {
        validate_archive(archive)?;
        if !self.is_empty()? {
            return Err(conflict("restore requires empty Core"));
        }
        for blob in &archive.blobs {
            publish_blob_with_stage(&self.blobs, &blob.decode()?, self.observe.as_ref())?;
        }
        self.restore_inner(archive).map_err(storage)
    }
}

#[allow(deprecated)]
impl KnowledgeStore for SqliteStore {
    fn snapshot(&self) -> Result<ArchiveV2> {
        self.core.snapshot()
    }

    fn commit(&mut self, plan: CommitPlan) -> Result<()> {
        let old = self.core.snapshot()?;
        validate_transition(&old, &plan)?;
        let next = &plan.next;
        for blob in &next.blobs {
            if !old.blobs.iter().any(|b| b.hash == blob.hash) {
                publish_blob_with_stage(
                    &self.core.blobs,
                    &blob.decode()?,
                    self.core.observe.as_ref(),
                )?;
            }
        }
        self.core.commit_inner(&old, &plan).map_err(storage)
    }

    fn restore(&mut self, archive: &ArchiveV2) -> Result<()> {
        self.core.restore(archive)
    }

    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        self.core.update_pdf_capture_note(vault, capture_id, note)
    }

    fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        self.core.delete_pdf_capture(vault, capture_id)
    }

    fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        self.core.delete_pdf_document(vault, document_id)
    }

    fn update_epub_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        self.core.update_epub_capture_note(vault, capture_id, note)
    }

    fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        self.core.delete_epub_capture(vault, capture_id)
    }

    fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        self.core.delete_epub_document(vault, document_id)
    }

    fn query_port(&self) -> Option<&dyn KnowledgeQuery> {
        Some(&self.core)
    }

    fn command_port(&mut self) -> Option<&mut dyn KnowledgeCommand> {
        Some(&mut self.core)
    }

    fn archive_port(&self) -> Option<&dyn KnowledgeArchive> {
        Some(&self.core)
    }

    fn archive_port_mut(&mut self) -> Option<&mut dyn KnowledgeArchive> {
        Some(&mut self.core)
    }
}

fn migrate_study(db: &Connection, observe: &dyn Fn(StorageStage)) -> Internal<()> {
    db.execute_batch(
        "BEGIN IMMEDIATE;
        CREATE TABLE vocab_wordbook(
            id INTEGER PRIMARY KEY,
            vault TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(vault,title));
        CREATE TABLE vocab_card(
            id INTEGER PRIMARY KEY,
            wordbook_id INTEGER NOT NULL REFERENCES vocab_wordbook(id),
            position INTEGER NOT NULL,
            term TEXT NOT NULL,
            meaning TEXT NOT NULL,
            example TEXT NOT NULL,
            pos TEXT NOT NULL DEFAULT '',
            phonetic TEXT NOT NULL DEFAULT '',
            UNIQUE(wordbook_id,term),
            UNIQUE(wordbook_id,position));
        CREATE TABLE vocab_schedule(
            card_id INTEGER PRIMARY KEY REFERENCES vocab_card(id),
            due_at_ms INTEGER NOT NULL,
            stage TEXT NOT NULL CHECK(stage IN('learning','review')),
            stability REAL NOT NULL,
            difficulty REAL NOT NULL,
            reps INTEGER NOT NULL CHECK(reps > 0),
            lapses INTEGER NOT NULL CHECK(lapses >= 0 AND lapses <= reps),
            last_review_at_ms INTEGER NOT NULL,
            last_rating INTEGER NOT NULL CHECK(last_rating BETWEEN 1 AND 4));
        CREATE TABLE vocab_review(
            id INTEGER PRIMARY KEY,
            card_id INTEGER NOT NULL REFERENCES vocab_card(id),
            operation_id TEXT NOT NULL UNIQUE,
            rating INTEGER NOT NULL CHECK(rating BETWEEN 1 AND 4),
            reviewed_at_ms INTEGER NOT NULL,
            stage_before TEXT NOT NULL,
            stage_after TEXT NOT NULL CHECK(stage_after IN('learning','review')),
            stability REAL NOT NULL,
            difficulty REAL NOT NULL,
            scheduled_minutes INTEGER NOT NULL CHECK(scheduled_minutes > 0),
            due_at_ms INTEGER NOT NULL,
            algorithm TEXT NOT NULL);
        PRAGMA user_version=4;",
    )?;
    observe(StorageStage::BeforeMigrationCommit);
    db.execute_batch("COMMIT;")?;
    observe(StorageStage::AfterMigrationCommit);
    Ok(())
}

fn stored_stage(value: &str) -> Internal<StudyStage> {
    StudyStage::from_name(value).ok_or_else(|| "invalid stored study stage".into())
}

fn stored_rating(code: i64) -> Internal<StudyRating> {
    let code = u8::try_from(code).map_err(|_| "invalid stored study rating")?;
    StudyRating::from_code(code).ok_or_else(|| "invalid stored study rating".into())
}

fn read_study(db: &Connection) -> Internal<StudyArchive> {
    let mut wordbooks = Vec::new();
    {
        let mut stmt = db.prepare(
            "SELECT id,vault,title,description,created_at_ms FROM vocab_wordbook ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Wordbook {
                id: r.get(0)?,
                vault: r.get(1)?,
                title: r.get(2)?,
                description: r.get(3)?,
                created_at_ms: r.get(4)?,
            })
        })?;
        for row in rows {
            wordbooks.push(row?);
        }
    }
    let mut cards = Vec::new();
    {
        let mut stmt = db.prepare(
            "SELECT id,wordbook_id,position,term,meaning,example,pos,phonetic FROM vocab_card ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        for row in rows {
            let (id, wordbook_id, position, term, meaning, example, pos, phonetic) = row?;
            cards.push(StudyCard {
                id,
                wordbook_id,
                position: u32::try_from(position)?,
                term,
                meaning,
                example,
                pos,
                phonetic,
            });
        }
    }
    let mut schedules = Vec::new();
    {
        let mut stmt = db.prepare(
            "SELECT card_id,due_at_ms,stage,stability,difficulty,reps,lapses,last_review_at_ms,last_rating FROM vocab_schedule ORDER BY card_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })?;
        for row in rows {
            let (
                card_id,
                due_at_ms,
                stage,
                stability,
                difficulty,
                reps,
                lapses,
                last_review_at_ms,
                last_rating,
            ) = row?;
            schedules.push(CardSchedule {
                card_id,
                due_at_ms,
                stage: stored_stage(&stage)?,
                stability,
                difficulty,
                reps: u32::try_from(reps)?,
                lapses: u32::try_from(lapses)?,
                last_review_at_ms,
                last_rating: stored_rating(last_rating)?,
            });
        }
    }
    let mut reviews = Vec::new();
    {
        let mut stmt = db.prepare(
            "SELECT id,card_id,operation_id,rating,reviewed_at_ms,stage_before,stage_after,stability,difficulty,scheduled_minutes,due_at_ms,algorithm FROM vocab_review ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, f64>(7)?,
                r.get::<_, f64>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, String>(11)?,
            ))
        })?;
        for row in rows {
            let (
                id,
                card_id,
                operation_id,
                rating,
                reviewed_at_ms,
                stage_before,
                stage_after,
                stability,
                difficulty,
                scheduled_minutes,
                due_at_ms,
                algorithm,
            ) = row?;
            reviews.push(ReviewEntry {
                id,
                card_id,
                operation_id,
                rating: stored_rating(rating)?,
                reviewed_at_ms,
                stage_before: stored_stage(&stage_before)?,
                stage_after: stored_stage(&stage_after)?,
                stability,
                difficulty,
                scheduled_minutes: u32::try_from(scheduled_minutes)?,
                due_at_ms,
                algorithm,
            });
        }
    }
    let archive = StudyArchive {
        schema_version: STUDY_SCHEMA_VERSION,
        wordbooks,
        cards,
        schedules,
        reviews,
    };
    validate_study_archive(&archive)?;
    Ok(archive)
}

fn select_review_by_operation(
    db: &Connection,
    operation_id: &str,
) -> Internal<Option<ReviewEntry>> {
    let mut stmt = db.prepare(
        "SELECT id,card_id,operation_id,rating,reviewed_at_ms,stage_before,stage_after,stability,difficulty,scheduled_minutes,due_at_ms,algorithm FROM vocab_review WHERE operation_id=?1",
    )?;
    let mut rows = stmt.query_map([operation_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, f64>(7)?,
            r.get::<_, f64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, i64>(10)?,
            r.get::<_, String>(11)?,
        ))
    })?;
    if let Some(row) = rows.next() {
        let (
            id,
            card_id,
            operation_id,
            rating,
            reviewed_at_ms,
            stage_before,
            stage_after,
            stability,
            difficulty,
            scheduled_minutes,
            due_at_ms,
            algorithm,
        ) = row?;
        return Ok(Some(ReviewEntry {
            id,
            card_id,
            operation_id,
            rating: stored_rating(rating)?,
            reviewed_at_ms,
            stage_before: stored_stage(&stage_before)?,
            stage_after: stored_stage(&stage_after)?,
            stability,
            difficulty,
            scheduled_minutes: u32::try_from(scheduled_minutes)?,
            due_at_ms,
            algorithm,
        }));
    }
    Ok(None)
}

struct RawStudyCardRow {
    id: i64,
    wordbook_id: i64,
    position: i64,
    term: String,
    meaning: String,
    example: String,
    pos: String,
    phonetic: String,
}

fn study_card_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawStudyCardRow> {
    Ok(RawStudyCardRow {
        id: r.get(0)?,
        wordbook_id: r.get(1)?,
        position: r.get(2)?,
        term: r.get(3)?,
        meaning: r.get(4)?,
        example: r.get(5)?,
        pos: r.get(6)?,
        phonetic: r.get(7)?,
    })
}

fn decode_study_card(row: RawStudyCardRow) -> Internal<StudyCard> {
    Ok(StudyCard {
        id: row.id,
        wordbook_id: row.wordbook_id,
        position: u32::try_from(row.position)?,
        term: row.term,
        meaning: row.meaning,
        example: row.example,
        pos: row.pos,
        phonetic: row.phonetic,
    })
}

impl SqliteCore {
    fn study_decks_inner(&self, vault: &str, now_ms: i64) -> Internal<Vec<StudyDeckCounts>> {
        let mut stmt = self.db.prepare(
            "SELECT w.id, w.vault, w.title, w.description, w.created_at_ms,
                    COUNT(c.id),
                    COALESCE(SUM(CASE WHEN s.card_id IS NOT NULL AND s.due_at_ms <= ?2 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN c.id IS NOT NULL AND s.card_id IS NULL THEN 1 ELSE 0 END), 0)
             FROM vocab_wordbook w
             LEFT JOIN vocab_card c ON c.wordbook_id = w.id
             LEFT JOIN vocab_schedule s ON s.card_id = c.id
             WHERE w.vault = ?1
             GROUP BY w.id
             ORDER BY w.id ASC",
        )?;
        let mut decks = Vec::new();
        for row in stmt.query_map(params![vault, now_ms], |r| {
            Ok((
                Wordbook {
                    id: r.get(0)?,
                    vault: r.get(1)?,
                    title: r.get(2)?,
                    description: r.get(3)?,
                    created_at_ms: r.get(4)?,
                },
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })? {
            let (wordbook, total, due, new_count) = row?;
            decks.push(StudyDeckCounts {
                wordbook,
                total: u32::try_from(total).unwrap_or(u32::MAX),
                due: u32::try_from(due).unwrap_or(u32::MAX),
                new_count: u32::try_from(new_count).unwrap_or(u32::MAX),
            });
        }
        Ok(decks)
    }

    fn wordbook_cards_inner(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Internal<Vec<StudyCard>> {
        let exists = self
            .db
            .prepare("SELECT 1 FROM vocab_wordbook WHERE id=?1 AND vault=?2")?
            .exists(params![wordbook_id, vault])?;
        if !exists {
            return Err(not_found_err("wordbook not found in this vault").into());
        }
        let mut stmt = self.db.prepare(
            "SELECT id, wordbook_id, position, term, meaning, example, pos, phonetic
             FROM vocab_card
             WHERE wordbook_id = ?1
             ORDER BY position ASC, id ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let mut cards = Vec::new();
        for row in stmt.query_map(
            params![wordbook_id, i64::try_from(limit)?, i64::try_from(offset)?],
            study_card_from_row,
        )? {
            cards.push(decode_study_card(row?)?);
        }
        Ok(cards)
    }

    fn remaining_counts_inner(
        &self,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        daily_new_limit: u32,
    ) -> Internal<(u32, u32)> {
        let due: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_card c JOIN vocab_schedule s ON s.card_id = c.id
             WHERE c.wordbook_id = ?1 AND s.due_at_ms <= ?2",
            params![wordbook_id, now_ms],
            |r| r.get(0),
        )?;
        let introduced_today: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_review r JOIN vocab_card c ON c.id = r.card_id
             WHERE c.wordbook_id = ?1 AND r.stage_before = 'new'
               AND r.reviewed_at_ms >= ?2 AND r.reviewed_at_ms <= ?3",
            params![wordbook_id, day_start_ms, now_ms],
            |r| r.get(0),
        )?;
        let new_cards: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_card c LEFT JOIN vocab_schedule s ON s.card_id = c.id
             WHERE c.wordbook_id = ?1 AND s.card_id IS NULL",
            [wordbook_id],
            |r| r.get(0),
        )?;
        let allowance = daily_new_limit
            .min(DAILY_NEW_LIMIT)
            .saturating_sub(u32::try_from(introduced_today).unwrap_or(u32::MAX));
        Ok((
            u32::try_from(due).unwrap_or(u32::MAX),
            allowance.min(u32::try_from(new_cards).unwrap_or(u32::MAX)),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn study_queue_inner(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Internal<StudyQueueData> {
        let exists = self
            .db
            .prepare("SELECT 1 FROM vocab_wordbook WHERE id=?1 AND vault=?2")?
            .exists(params![wordbook_id, vault])?;
        if !exists {
            return Err(not_found_err("wordbook not found in this vault").into());
        }
        let total_due: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_card c JOIN vocab_schedule s ON s.card_id = c.id
             WHERE c.wordbook_id = ?1 AND s.due_at_ms <= ?2",
            params![wordbook_id, now_ms],
            |r| r.get(0),
        )?;
        let total_new: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_card c LEFT JOIN vocab_schedule s ON s.card_id = c.id
             WHERE c.wordbook_id = ?1 AND s.card_id IS NULL",
            [wordbook_id],
            |r| r.get(0),
        )?;
        let introduced_today: i64 = self.db.query_row(
            "SELECT count(*) FROM vocab_review r JOIN vocab_card c ON c.id = r.card_id
             WHERE c.wordbook_id = ?1 AND r.stage_before = 'new'
               AND r.reviewed_at_ms >= ?2 AND r.reviewed_at_ms <= ?3",
            params![wordbook_id, day_start_ms, now_ms],
            |r| r.get(0),
        )?;
        let total_due_usize = usize::try_from(total_due)?;
        let total_new_usize = usize::try_from(total_new)?;
        let due_take = limit.min(total_due_usize);
        let new_allowance =
            daily_new_limit.saturating_sub(u32::try_from(introduced_today).unwrap_or(u32::MAX));
        let take_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(total_new_usize)
            .min(limit.saturating_sub(due_take));

        let mut cards = Vec::with_capacity(due_take + take_new);
        if due_take > 0 {
            let mut stmt = self.db.prepare(
                "SELECT c.id, c.wordbook_id, c.position, c.term, c.meaning, c.example, c.pos, c.phonetic, s.stage
                 FROM vocab_card c
                 JOIN vocab_schedule s ON s.card_id = c.id
                 WHERE c.wordbook_id = ?1 AND s.due_at_ms <= ?2
                 ORDER BY s.due_at_ms ASC, c.position ASC, c.id ASC
                 LIMIT ?3",
            )?;
            for row in stmt.query_map(
                params![wordbook_id, now_ms, i64::try_from(due_take)?],
                |r| Ok((study_card_from_row(r)?, r.get::<_, String>(8)?)),
            )? {
                let (card_tuple, stage_str) = row?;
                cards.push(StudyQueueEntry {
                    card: decode_study_card(card_tuple)?,
                    stage: stored_stage(&stage_str)?,
                });
            }
        }
        if take_new > 0 {
            let mut stmt = self.db.prepare(
                "SELECT c.id, c.wordbook_id, c.position, c.term, c.meaning, c.example, c.pos, c.phonetic
                 FROM vocab_card c
                 LEFT JOIN vocab_schedule s ON s.card_id = c.id
                 WHERE c.wordbook_id = ?1 AND s.card_id IS NULL
                 ORDER BY c.position ASC, c.id ASC
                 LIMIT ?2",
            )?;
            for row in stmt.query_map(
                params![wordbook_id, i64::try_from(take_new)?],
                study_card_from_row,
            )? {
                cards.push(StudyQueueEntry {
                    card: decode_study_card(row?)?,
                    stage: StudyStage::New,
                });
            }
        }
        let remaining_due = total_due_usize.saturating_sub(due_take);
        let remaining_new = usize::try_from(new_allowance)
            .unwrap_or(0)
            .min(total_new_usize)
            .saturating_sub(take_new);
        Ok(StudyQueueData {
            cards,
            remaining_due: u32::try_from(remaining_due).unwrap_or(u32::MAX),
            remaining_new: u32::try_from(remaining_new).unwrap_or(u32::MAX),
        })
    }

    fn card_review_context_inner(
        &self,
        vault: &str,
        card_id: i64,
        operation_id: &str,
    ) -> Internal<StudyReviewContext> {
        let card_row = self
            .db
            .query_row(
                "SELECT id, wordbook_id, position, term, meaning, example, pos, phonetic
                 FROM vocab_card
                 WHERE id = ?1",
                [card_id],
                study_card_from_row,
            )
            .optional()?;
        let Some(card_row) = card_row else {
            return Err(not_found_err("card not found").into());
        };
        let card = decode_study_card(card_row)?;
        let book_in_vault = self
            .db
            .prepare("SELECT 1 FROM vocab_wordbook WHERE id = ?1 AND vault = ?2")?
            .exists(params![card.wordbook_id, vault])?;
        if !book_in_vault {
            return Err(not_found_err("card not found in this vault").into());
        }
        let existing_review = select_review_by_operation(&self.db, operation_id)?;
        struct RawScheduleRow {
            card_id: i64,
            due_at_ms: i64,
            stage: String,
            stability: f64,
            difficulty: f64,
            reps: i64,
            lapses: i64,
            last_review_at_ms: i64,
            last_rating: i64,
        }
        let sched_row: Option<RawScheduleRow> = self
            .db
            .query_row(
                "SELECT card_id, due_at_ms, stage, stability, difficulty, reps, lapses, last_review_at_ms, last_rating
                 FROM vocab_schedule
                 WHERE card_id = ?1",
                [card_id],
                |r| {
                    Ok(RawScheduleRow {
                        card_id: r.get(0)?,
                        due_at_ms: r.get(1)?,
                        stage: r.get(2)?,
                        stability: r.get(3)?,
                        difficulty: r.get(4)?,
                        reps: r.get(5)?,
                        lapses: r.get(6)?,
                        last_review_at_ms: r.get(7)?,
                        last_rating: r.get(8)?,
                    })
                },
            )
            .optional()?;
        let current_schedule = match sched_row {
            Some(row) => Some(CardSchedule {
                card_id: row.card_id,
                due_at_ms: row.due_at_ms,
                stage: stored_stage(&row.stage)?,
                stability: row.stability,
                difficulty: row.difficulty,
                reps: u32::try_from(row.reps)?,
                lapses: u32::try_from(row.lapses)?,
                last_review_at_ms: row.last_review_at_ms,
                last_rating: stored_rating(row.last_rating)?,
            }),
            None => None,
        };
        Ok(StudyReviewContext {
            wordbook_id: card.wordbook_id,
            card,
            existing_review,
            current_schedule,
        })
    }

    fn ensure_wordbook_inner(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Internal<Wordbook> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(i64, String, String, i64)> = tx
            .query_row(
                "SELECT id,title,description,created_at_ms FROM vocab_wordbook WHERE vault=?1 AND title=?2",
                params![vault, seed.title],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        if let Some((id, title, description, created_at_ms)) = existing {
            return Ok(Wordbook {
                id,
                vault: vault.to_owned(),
                title,
                description,
                created_at_ms,
            });
        }
        tx.execute(
            "INSERT INTO vocab_wordbook(vault,title,description,created_at_ms) VALUES(?1,?2,?3,?4)",
            params![vault, seed.title, seed.description, now_ms],
        )?;
        let id = tx.last_insert_rowid();
        for (position, word) in seed.words.iter().enumerate() {
            let position = i64::try_from(position)?;
            tx.execute(
                "INSERT INTO vocab_card(wordbook_id,position,term,meaning,example,pos,phonetic) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    position,
                    word.term,
                    word.meaning,
                    word.example,
                    word.pos,
                    word.phonetic
                ],
            )?;
        }
        tx.commit()?;
        Ok(Wordbook {
            id,
            vault: vault.to_owned(),
            title: seed.title.clone(),
            description: seed.description.clone(),
            created_at_ms: now_ms,
        })
    }

    fn commit_review_inner(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Internal<ReviewEntry> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = select_review_by_operation(&tx, &review.operation_id)? {
            if existing.card_id != review.card_id || existing.rating != review.rating {
                return Err(
                    conflict("review operation id is already bound to another grade").into(),
                );
            }
            return Ok(existing);
        }
        if !tx
            .prepare("SELECT 1 FROM vocab_card WHERE id=?1")?
            .exists([review.card_id])?
        {
            return Err(not_found_err("review card not found").into());
        }
        let current: Option<(i64, i64)> = tx
            .query_row(
                "SELECT reps,last_review_at_ms FROM vocab_schedule WHERE card_id=?1",
                [review.card_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let guard_matches = match (current, guard.expected_reps) {
            (None, 0) => guard.expected_last_review_at_ms.is_none(),
            (Some((reps, last)), expected) => {
                u32::try_from(reps).ok() == Some(expected)
                    && guard.expected_last_review_at_ms == Some(last)
            }
            (None, _) => false,
        };
        if !guard_matches {
            return Err(conflict("card schedule changed before this review").into());
        }
        tx.execute(
            "INSERT INTO vocab_review(card_id,operation_id,rating,reviewed_at_ms,stage_before,stage_after,stability,difficulty,scheduled_minutes,due_at_ms,algorithm) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                review.card_id,
                review.operation_id,
                review.rating.code(),
                review.reviewed_at_ms,
                review.stage_before.as_str(),
                review.stage_after.as_str(),
                review.stability,
                review.difficulty,
                review.scheduled_minutes,
                review.due_at_ms,
                review.algorithm,
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO vocab_schedule(card_id,due_at_ms,stage,stability,difficulty,reps,lapses,last_review_at_ms,last_rating) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(card_id) DO UPDATE SET due_at_ms=excluded.due_at_ms,stage=excluded.stage,stability=excluded.stability,difficulty=excluded.difficulty,reps=excluded.reps,lapses=excluded.lapses,last_review_at_ms=excluded.last_review_at_ms,last_rating=excluded.last_rating",
            params![
                schedule.card_id,
                schedule.due_at_ms,
                schedule.stage.as_str(),
                schedule.stability,
                schedule.difficulty,
                i64::from(schedule.reps),
                i64::from(schedule.lapses),
                schedule.last_review_at_ms,
                schedule.last_rating.code(),
            ],
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(ReviewEntry {
            id,
            ..review.clone()
        })
    }

    fn restore_study_inner(&mut self, archive: &StudyArchive) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row(
            "SELECT (SELECT count(*) FROM vocab_wordbook)+(SELECT count(*) FROM vocab_card)+(SELECT count(*) FROM vocab_schedule)+(SELECT count(*) FROM vocab_review)",
            [],
            |r| r.get(0),
        )?;
        if count != 0 {
            return Err(conflict("restore requires empty study data").into());
        }
        for book in &archive.wordbooks {
            tx.execute(
                "INSERT INTO vocab_wordbook(id,vault,title,description,created_at_ms) VALUES(?1,?2,?3,?4,?5)",
                params![
                    book.id,
                    book.vault,
                    book.title,
                    book.description,
                    book.created_at_ms
                ],
            )?;
        }
        for card in &archive.cards {
            tx.execute(
                "INSERT INTO vocab_card(id,wordbook_id,position,term,meaning,example,pos,phonetic) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    card.id,
                    card.wordbook_id,
                    i64::from(card.position),
                    card.term,
                    card.meaning,
                    card.example,
                    card.pos,
                    card.phonetic
                ],
            )?;
        }
        for schedule in &archive.schedules {
            tx.execute(
                "INSERT INTO vocab_schedule(card_id,due_at_ms,stage,stability,difficulty,reps,lapses,last_review_at_ms,last_rating) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    schedule.card_id,
                    schedule.due_at_ms,
                    schedule.stage.as_str(),
                    schedule.stability,
                    schedule.difficulty,
                    i64::from(schedule.reps),
                    i64::from(schedule.lapses),
                    schedule.last_review_at_ms,
                    schedule.last_rating.code()
                ],
            )?;
        }
        for review in &archive.reviews {
            tx.execute(
                "INSERT INTO vocab_review(id,card_id,operation_id,rating,reviewed_at_ms,stage_before,stage_after,stability,difficulty,scheduled_minutes,due_at_ms,algorithm) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    review.id,
                    review.card_id,
                    review.operation_id,
                    review.rating.code(),
                    review.reviewed_at_ms,
                    review.stage_before.as_str(),
                    review.stage_after.as_str(),
                    review.stability,
                    review.difficulty,
                    review.scheduled_minutes,
                    review.due_at_ms,
                    review.algorithm
                ],
            )?;
        }
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }

    fn delete_wordbook_inner(&mut self, vault: &str, wordbook_id: i64) -> Internal<()> {
        (self.observe)(StorageStage::BeforeTransaction);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM vocab_wordbook WHERE id=?1 AND vault=?2",
                params![wordbook_id, vault],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Err(not_found_err("wordbook not found in this vault").into());
        }
        tx.execute(
            "DELETE FROM vocab_review WHERE card_id IN (SELECT id FROM vocab_card WHERE wordbook_id=?1)",
            [wordbook_id],
        )?;
        tx.execute(
            "DELETE FROM vocab_schedule WHERE card_id IN (SELECT id FROM vocab_card WHERE wordbook_id=?1)",
            [wordbook_id],
        )?;
        tx.execute("DELETE FROM vocab_card WHERE wordbook_id=?1", [wordbook_id])?;
        tx.execute(
            "DELETE FROM vocab_wordbook WHERE id=?1 AND vault=?2",
            params![wordbook_id, vault],
        )?;
        (self.observe)(StorageStage::AfterEntities);
        (self.observe)(StorageStage::BeforeCommit);
        tx.commit()?;
        (self.observe)(StorageStage::AfterCommit);
        Ok(())
    }
}

impl StudyQuery for SqliteCore {
    fn study_decks(&self, vault: &str, now_ms: i64) -> Result<Vec<StudyDeckCounts>> {
        self.study_decks_inner(vault, now_ms).map_err(storage)
    }

    fn wordbook_cards(
        &self,
        vault: &str,
        wordbook_id: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StudyCard>> {
        self.wordbook_cards_inner(vault, wordbook_id, limit, offset)
            .map_err(storage)
    }

    fn study_queue(
        &self,
        vault: &str,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        limit: usize,
        daily_new_limit: u32,
    ) -> Result<StudyQueueData> {
        self.study_queue_inner(
            vault,
            wordbook_id,
            now_ms,
            day_start_ms,
            limit,
            daily_new_limit,
        )
        .map_err(storage)
    }

    fn card_review_context(
        &self,
        vault: &str,
        card_id: i64,
        operation_id: &str,
    ) -> Result<StudyReviewContext> {
        self.card_review_context_inner(vault, card_id, operation_id)
            .map_err(storage)
    }

    fn study_remaining_counts(
        &self,
        wordbook_id: i64,
        now_ms: i64,
        day_start_ms: i64,
        daily_new_limit: u32,
    ) -> Result<(u32, u32)> {
        self.remaining_counts_inner(wordbook_id, now_ms, day_start_ms, daily_new_limit)
            .map_err(storage)
    }
}

impl StudyCommand for SqliteCore {
    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook> {
        validate_study_seed(vault, seed)?;
        if now_ms <= 0 {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "invalid study timestamp",
            ));
        }
        self.ensure_wordbook_inner(vault, seed, now_ms)
            .map_err(storage)
    }

    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        validate_study_schedule(schedule)?;
        validate_study_review(review)?;
        if review.id != 0 || schedule.card_id != review.card_id {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "invalid review commit draft",
            ));
        }
        self.commit_review_inner(guard, schedule, review)
            .map_err(storage)
    }

    fn delete_wordbook(&mut self, vault: &str, wordbook_id: i64) -> Result<()> {
        validate_vault_arg(vault)?;
        if wordbook_id <= 0 {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "invalid wordbook id",
            ));
        }
        self.delete_wordbook_inner(vault, wordbook_id)
            .map_err(storage)
    }
}

impl StudyArchivePort for SqliteCore {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        read_study(&self.db).map_err(storage)
    }

    fn restore_study(&mut self, archive: &StudyArchive) -> Result<()> {
        validate_study_archive(archive)?;
        self.restore_study_inner(archive).map_err(storage)
    }
}

#[allow(deprecated)]
impl StudyStore for SqliteStore {
    fn study_snapshot(&self) -> Result<StudyArchive> {
        self.core.study_snapshot()
    }

    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> Result<Wordbook> {
        self.core.ensure_wordbook(vault, seed, now_ms)
    }

    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> Result<ReviewEntry> {
        self.core.commit_review(guard, schedule, review)
    }

    fn restore_study(&mut self, archive: &StudyArchive) -> Result<()> {
        self.core.restore_study(archive)
    }

    fn delete_wordbook(&mut self, vault: &str, wordbook_id: i64) -> Result<()> {
        self.core.delete_wordbook(vault, wordbook_id)
    }

    fn study_query_port(&self) -> Option<&dyn StudyQuery> {
        Some(&self.core)
    }

    fn study_command_port(&mut self) -> Option<&mut dyn StudyCommand> {
        Some(&mut self.core)
    }

    fn study_archive_port(&self) -> Option<&dyn StudyArchivePort> {
        Some(&self.core)
    }

    fn study_archive_port_mut(&mut self) -> Option<&mut dyn StudyArchivePort> {
        Some(&mut self.core)
    }
}

#[cfg(test)]
mod tests {
    use super::storage;
    use core_api::knowledge::KnowledgeErrorCode;
    use std::io;

    #[test]
    fn structured_error_mapper_preserves_codes_without_path_leaks() {
        let io_err = io::Error::new(
            io::ErrorKind::PermissionDenied,
            "/Users/secret/vault/core.sqlite: permission denied",
        );
        let mapped = storage(Box::new(io_err));
        assert_eq!(mapped.code, KnowledgeErrorCode::Storage);
        assert!(mapped.message.contains("category=io"));
        assert!(mapped.message.contains("kind=PermissionDenied"));
        assert!(!mapped.message.contains("/Users/secret"));
        assert!(!mapped.message.contains("core.sqlite"));

        let sqlite_err = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ffi::ErrorCode::DatabaseBusy,
                extended_code: 5,
            },
            Some("/tmp/sensitive_path/core.sqlite is locked".into()),
        );
        let mapped_sqlite = storage(Box::new(sqlite_err));
        assert_eq!(mapped_sqlite.code, KnowledgeErrorCode::Storage);
        assert!(mapped_sqlite.message.contains("category=sqlite"));
        assert!(mapped_sqlite.message.contains("code=DatabaseBusy"));
        assert!(mapped_sqlite.message.contains("extended=5"));
        assert!(!mapped_sqlite.message.contains("sensitive_path"));
    }
}
