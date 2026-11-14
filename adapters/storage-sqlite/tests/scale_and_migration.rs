#![cfg(unix)]

use core_api::{
    contract::sha256,
    knowledge::{
        ArchiveV2, BinaryBlob, CommitEntry, KnowledgeErrorCode, PdfCaptureRecord,
        PdfDocumentReceipt, PdfDocumentRecord, PdfOperationEntry, PdfReceipt, Provenance,
        archive_schema_version, commit_for_pdf, operation_for_capture, operation_for_document,
    },
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
    vocab::{NewWord, StudyRating, WordbookSeed},
};
use learning_application::{
    KnowledgeApplication, pdf_notes::NoteBodyField, study::StudyApplication,
};
use learning_ports::KnowledgeArchive;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn origin() -> Provenance {
    Provenance {
        package_id: "third.party".into(),
        publisher_id: "publisher".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    }
}

fn pdf_snapshot_and_draft(original: &[u8]) -> (PdfSnapshot, PdfDraft) {
    let page_text = "Alphabetagammadelta";
    let chars: Vec<_> = page_text
        .chars()
        .enumerate()
        .map(|(i, c)| {
            serde_json::json!({
                "pdf_char_index": i,
                "unicode_value": c as u32,
                "mapping": "scalar",
                "generated": false,
                "scalar_start": i,
                "scalar_end": i + 1,
                "utf8_start": i,
                "utf8_end": i + 1,
                "utf16_start": i,
                "utf16_end": i + 1,
                "pdf_bounds": { "left": i as f32, "bottom": 1.0, "right": i as f32 + 1.0, "top": 2.0 }
            })
        })
        .collect();
    let snapshot: PdfSnapshot = serde_json::from_value(serde_json::json!({
        "protocol_version": 1,
        "original_sha256": sha256(original),
        "original_bytes": original.len(),
        "extraction": {
            "provider": "pdfium",
            "algorithm": "qusor-pdfium-char-map-v1",
            "wrapper": "pdfium-render-0.9.5",
            "abi": "pdfium_8125",
            "library_sha256": "a".repeat(64),
            "mapping_check": "unicode_scalar_validity_only",
            "normalization": PDF_NORMALIZATION
        },
        "engine_report_sha256": "b".repeat(64),
        "pages": [{
            "page_index": 0,
            "engine_text": page_text,
            "text_sha256": sha256(page_text.as_bytes()),
            "rotation_degrees": 0,
            "media_box": { "left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0 },
            "crop_box": { "left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0 },
            "characters": chars
        }]
    }))
    .unwrap();
    let draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(page_text.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 9,
        exact: "Alphabeta".into(),
    };
    (snapshot, draft)
}

fn build_scale_archive(pdf_bytes: &[u8], capture_count: usize) -> (ArchiveV2, PdfDraft) {
    let original_sha = sha256(pdf_bytes);
    let (snapshot, draft) = pdf_snapshot_and_draft(pdf_bytes);
    let extraction_revision = snapshot.extraction_revision().unwrap();
    let doc_receipt = PdfDocumentReceipt {
        operation_id: "op-scale-doc-1".into(),
        document_id: 1,
        commit_rev: 1,
        original_sha256: original_sha.clone(),
        extraction_revision: extraction_revision.clone(),
    };
    let doc_record = PdfDocumentRecord {
        receipt: doc_receipt,
        vault: "main".into(),
        title: "Scale 3MiB PDF".into(),
        snapshot,
    };
    let doc_op = operation_for_document(&doc_record).unwrap();
    let doc_commit = commit_for_pdf(&doc_op, 1);
    let draft_hash = draft.draft_hash().unwrap();

    let mut pdf_captures = Vec::with_capacity(capture_count);
    let mut pdf_operations: Vec<PdfOperationEntry> = Vec::with_capacity(capture_count + 1);
    let mut commits: Vec<CommitEntry> = Vec::with_capacity(capture_count + 1);

    pdf_operations.push(doc_op);
    commits.push(doc_commit);

    for idx in 1..=capture_count {
        let cap_id = idx as i64;
        let rev = (idx + 1) as i64;
        let op_id = format!("op-scale-cap-{idx}");
        let receipt = PdfReceipt {
            operation_id: op_id.clone(),
            capture_id: cap_id,
            document_id: 1,
            commit_rev: rev,
            original_sha256: original_sha.clone(),
            extraction_revision: extraction_revision.clone(),
            draft_hash: draft_hash.clone(),
        };
        let cap = PdfCaptureRecord {
            receipt,
            draft: draft.clone(),
            compound: None,
            provenance: origin(),
            user_note: format!("Scale capture note #{idx}"),
        };
        let op = operation_for_capture(&cap, &doc_record).unwrap();
        let commit = commit_for_pdf(&op, rev);
        pdf_captures.push(cap);
        pdf_operations.push(op);
        commits.push(commit);
    }

    let mut archive = ArchiveV2 {
        schema_version: 2,
        text_entries: vec![],
        pdf_documents: vec![doc_record],
        pdf_captures,
        pdf_operations,
        epub_documents: vec![],
        epub_captures: vec![],
        commits,
        blobs: vec![BinaryBlob::new(pdf_bytes)],
    };
    archive.schema_version = archive_schema_version(&archive);
    (archive, draft)
}

#[test]
fn sqlite_scale_read_write_avoids_blob_reads_and_bounds_queries() {
    let scratch = Scratch::new("qusor-sqlite-scale");
    let mut pdf_bytes = vec![b' '; 3 * 1024 * 1024]; // 3 MiB PDF blob
    pdf_bytes[..9].copy_from_slice(b"%PDF-1.7\n");

    const BASE_CAPTURES: usize = 2_000;
    let (archive, draft) = build_scale_archive(&pdf_bytes, BASE_CAPTURES);

    {
        let mut store = SqliteStore::open(&scratch.0).unwrap();
        store.core_mut().restore(&archive).unwrap();

        let mut study = StudyApplication::new(store);
        let seed = WordbookSeed {
            title: "Scale Vocab".into(),
            description: "2000 cards".into(),
            words: (0..2_000)
                .map(|i| NewWord {
                    term: format!("word-{i}"),
                    pos: "n.".into(),
                    phonetic: "/w/".into(),
                    meaning: format!("meaning-{i}"),
                    example: format!("Example sentence {i}."),
                })
                .collect(),
        };
        let now = 1_767_225_600_000i64;
        study.seed_wordbook("main", &seed, now).unwrap();
    }

    // Reopen fresh from disk and assert 0 CAS blob reads during all normal reads and incremental writes.
    let store = SqliteStore::open(&scratch.0).unwrap();
    store.reset_blob_read_count();
    let mut app = KnowledgeApplication::new(store);

    let docs = app.pdf_documents_page("main", None, 20).unwrap();
    assert_eq!(docs.documents.len(), 1);
    assert_eq!(docs.documents[0].original_bytes, pdf_bytes.len() as u64);

    // Bounded keyset pagination across 2,000 captures.
    let page1 = app.pdf_notes_page("main", Some(1), None, 50).unwrap();
    assert_eq!(page1.notes.len(), 50);
    assert!(page1.next_cursor.is_some());
    let page2 = app
        .pdf_notes_page("main", Some(1), page1.next_cursor.as_deref(), 50)
        .unwrap();
    assert_eq!(page2.notes.len(), 50);
    assert!(page2.notes[0].capture_id < page1.notes[49].capture_id);

    // Point reads, resolution, note update, and live incremental capture.
    assert_eq!(
        app.pdf_note_body("main", 1, NoteBodyField::Note, 0, 512)
            .unwrap()
            .text,
        "Scale capture note #1"
    );
    assert!(
        app.pdf_page_text("main", 1, 0, 0, 512)
            .unwrap()
            .text
            .starts_with("Alphabeta")
    );
    assert_eq!(app.pdf_saved_selections("main", 1, 0).unwrap().len(), 1);
    assert_eq!(app.query_document(1).unwrap().title, "Scale 3MiB PDF");
    assert_eq!(
        app.query_pdf_complete(2_000).unwrap().user_note,
        "Scale capture note #2000"
    );
    assert_eq!(app.resolve_pdf(1, &draft).unwrap().exact, "Alphabeta");
    assert!(app.pdf_receipt("op-scale-cap-1").unwrap().is_some());
    assert!(app.document_receipt("op-scale-doc-1").unwrap().is_some());
    assert!(app.query_operation("op-scale-cap-2000").unwrap().is_some());

    app.update_pdf_capture_note("main", 1, "Updated under 2000-capture scale")
        .unwrap();
    let added = app
        .capture_pdf(
            "op-scale-cap-live-1",
            1,
            &draft,
            &origin(),
            "live incremental note",
        )
        .unwrap();
    assert_eq!(added.capture_id, 2_001);
    assert_eq!(added.commit_rev, 2_002);

    let store = app.into_store();
    assert_eq!(
        store.blob_read_count(),
        0,
        "normal knowledge reads/writes must not read original CAS blobs"
    );

    let mut study = StudyApplication::new(store);
    let now = 1_767_225_600_000i64;
    let decks = study.decks("main", now).unwrap();
    assert_eq!(decks[0].total, 2_000);
    let cards = study
        .wordbook_cards("main", decks[0].wordbook.id, 50, 0)
        .unwrap();
    assert_eq!(cards.len(), 50);
    let q = study
        .queue("main", decks[0].wordbook.id, now, now - 1_000, 10)
        .unwrap();
    assert!(!q.cards.is_empty());
    study
        .review(
            "main",
            q.cards[0].card.id,
            StudyRating::Good,
            "op-scale-study-rev",
            now,
            now - 1_000,
        )
        .unwrap();

    let store = study.into_store();
    assert_eq!(
        store.blob_read_count(),
        0,
        "study operations must not read original CAS blobs"
    );

    // Explicit original PDF read touches the CAS blob exactly once.
    let app = KnowledgeApplication::new(store);
    let bytes = app.original_pdf(1).unwrap();
    assert_eq!(bytes.len(), pdf_bytes.len());
    assert_eq!(app.into_store().blob_read_count(), 1);
}

#[test]
fn schema_v5_migrates_cleanly_to_v6_with_backfill_and_indexes() {
    let dir = Scratch::new("qusor-migrate-v5-v6");
    let db_path = dir.0.join("core.sqlite");
    let pdf_bytes = b"%PDF-1.7\nlegacy v5 pdf\n";
    let (archive, draft) = build_scale_archive(pdf_bytes, 1);
    let doc = &archive.pdf_documents[0];
    let doc_json = serde_json::to_string(doc).unwrap();
    let doc_op = &archive.pdf_operations[0];
    let doc_commit = &archive.commits[0];
    let cap1 = &archive.pdf_captures[0];
    let cap1_json = serde_json::to_string(cap1).unwrap();
    let cap1_anchor = serde_json::to_string(&cap1.draft).unwrap();
    let cap1_op = &archive.pdf_operations[1];
    let cap1_commit = &archive.commits[1];
    let doc_snapshot_json = serde_json::to_string(&doc.snapshot).unwrap();

    // Write the original CAS blob in BlobStore layout blobs/<sha256> with private permissions.
    let blobs_dir = dir.0.join("blobs");
    fs::create_dir_all(&blobs_dir).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&blobs_dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let blob_path = blobs_dir.join(&doc.receipt.original_sha256);
    fs::write(&blob_path, pdf_bytes).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&blob_path, fs::Permissions::from_mode(0o400)).unwrap();
    }

    {
        let db = rusqlite::Connection::open(&db_path).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;
            BEGIN;
            CREATE TABLE source(id INTEGER PRIMARY KEY, descriptor TEXT NOT NULL);
            CREATE TABLE document_version(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),snapshot_hash TEXT NOT NULL);
            CREATE TABLE capture(id INTEGER PRIMARY KEY,source_id INTEGER NOT NULL REFERENCES source(id),version_id INTEGER NOT NULL REFERENCES document_version(id),record TEXT NOT NULL,anchor TEXT NOT NULL);
            CREATE TABLE operation(id TEXT PRIMARY KEY,capture_id INTEGER UNIQUE NOT NULL REFERENCES capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE receipt(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE outbox(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE index_job(operation_id TEXT PRIMARY KEY REFERENCES operation(id),payload TEXT NOT NULL);
            CREATE TABLE commit_log(commit_rev INTEGER PRIMARY KEY,operation_id TEXT UNIQUE NOT NULL,kind TEXT NOT NULL CHECK(kind IN('text','pdf_import','pdf_capture','epub_import','epub_capture')),entity_id INTEGER NOT NULL,input_hash TEXT NOT NULL,receipt TEXT NOT NULL);
            CREATE TABLE pdf_document(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,original_hash TEXT NOT NULL,original_bytes INTEGER NOT NULL,extraction_revision TEXT NOT NULL,record TEXT NOT NULL);
            CREATE TABLE pdf_extraction(document_id INTEGER PRIMARY KEY REFERENCES pdf_document(id),snapshot TEXT NOT NULL);
            CREATE TABLE pdf_capture(id INTEGER PRIMARY KEY,document_id INTEGER NOT NULL REFERENCES pdf_document(id),record TEXT NOT NULL,anchor TEXT NOT NULL);
            CREATE TABLE pdf_operation(operation_id TEXT PRIMARY KEY REFERENCES commit_log(operation_id),kind TEXT NOT NULL,document_id INTEGER NOT NULL REFERENCES pdf_document(id),capture_id INTEGER UNIQUE REFERENCES pdf_capture(id),input_hash TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE pdf_receipt(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE pdf_outbox(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE pdf_index_job(operation_id TEXT PRIMARY KEY REFERENCES pdf_operation(operation_id),payload TEXT NOT NULL);
            CREATE TABLE vocab_wordbook(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,description TEXT NOT NULL,created_at_ms INTEGER NOT NULL,UNIQUE(vault,title));
            CREATE TABLE vocab_card(id INTEGER PRIMARY KEY,wordbook_id INTEGER NOT NULL REFERENCES vocab_wordbook(id),position INTEGER NOT NULL,term TEXT NOT NULL,meaning TEXT NOT NULL,example TEXT NOT NULL,pos TEXT NOT NULL DEFAULT '',phonetic TEXT NOT NULL DEFAULT '',UNIQUE(wordbook_id,position));
            CREATE TABLE vocab_schedule(card_id INTEGER PRIMARY KEY REFERENCES vocab_card(id),due_at_ms INTEGER NOT NULL,stage TEXT NOT NULL CHECK(stage IN('learning','review')),stability REAL NOT NULL,difficulty REAL NOT NULL,reps INTEGER NOT NULL CHECK(reps > 0),lapses INTEGER NOT NULL CHECK(lapses >= 0 AND lapses <= reps),last_review_at_ms INTEGER NOT NULL,last_rating INTEGER NOT NULL CHECK(last_rating BETWEEN 1 AND 4));
            CREATE TABLE vocab_review(id INTEGER PRIMARY KEY,card_id INTEGER NOT NULL REFERENCES vocab_card(id),operation_id TEXT NOT NULL UNIQUE,rating INTEGER NOT NULL CHECK(rating BETWEEN 1 AND 4),reviewed_at_ms INTEGER NOT NULL,stage_before TEXT NOT NULL,stage_after TEXT NOT NULL CHECK(stage_after IN('learning','review')),stability REAL NOT NULL,difficulty REAL NOT NULL,scheduled_minutes INTEGER NOT NULL CHECK(scheduled_minutes > 0),due_at_ms INTEGER NOT NULL,algorithm TEXT NOT NULL);
            CREATE TABLE epub_documents(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,title TEXT NOT NULL,author TEXT NOT NULL,original_hash TEXT NOT NULL,original_bytes INTEGER NOT NULL,extraction_revision TEXT NOT NULL,operation_id TEXT NOT NULL UNIQUE REFERENCES commit_log(operation_id),commit_rev INTEGER NOT NULL,receipt TEXT NOT NULL);
            CREATE TABLE epub_chapters(document_id INTEGER NOT NULL REFERENCES epub_documents(id),chapter_index INTEGER NOT NULL,title TEXT NOT NULL,href TEXT NOT NULL,text TEXT NOT NULL,PRIMARY KEY(document_id, chapter_index));
            CREATE TABLE epub_toc(document_id INTEGER NOT NULL REFERENCES epub_documents(id),position INTEGER NOT NULL,title TEXT NOT NULL,chapter_index INTEGER NOT NULL,depth INTEGER NOT NULL,PRIMARY KEY(document_id, position));
            CREATE TABLE epub_captures(id INTEGER PRIMARY KEY,document_id INTEGER NOT NULL REFERENCES epub_documents(id),operation_id TEXT NOT NULL UNIQUE REFERENCES commit_log(operation_id),commit_rev INTEGER NOT NULL,draft TEXT NOT NULL,provenance TEXT NOT NULL,user_note TEXT NOT NULL,receipt TEXT NOT NULL);
            CREATE TABLE knowledge_audit(id INTEGER PRIMARY KEY,vault TEXT NOT NULL,action TEXT NOT NULL,entity_kind TEXT NOT NULL,entity_id INTEGER NOT NULL,detail TEXT NOT NULL);
            PRAGMA user_version=5;
            COMMIT;",
        )
        .unwrap();
        db.execute(
            "INSERT INTO commit_log(commit_rev,operation_id,kind,entity_id,input_hash,receipt) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![doc_commit.commit_rev, doc_commit.operation_id, doc_commit.kind, doc_commit.entity_id, doc_commit.input_hash, doc_commit.receipt],
        ).unwrap();
        db.execute(
            "INSERT INTO pdf_document(id,vault,title,original_hash,original_bytes,extraction_revision,record) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params![1i64, doc.vault, doc.title, doc.receipt.original_sha256, pdf_bytes.len() as i64, doc.receipt.extraction_revision, doc_json],
        ).unwrap();
        db.execute(
            "INSERT INTO pdf_extraction(document_id,snapshot) VALUES(1,?1)",
            [&doc_snapshot_json],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_operation(operation_id,kind,document_id,capture_id,input_hash,payload) VALUES(?1,?2,?3,NULL,?4,?5)",
            rusqlite::params![doc_op.operation_id, doc_op.kind, doc_op.document_id, doc_op.input_hash, doc_op.operation],
        ).unwrap();
        db.execute(
            "INSERT INTO pdf_receipt(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![doc_op.operation_id, doc_op.receipt],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_outbox(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![doc_op.operation_id, doc_op.outbox],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_index_job(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![doc_op.operation_id, doc_op.index_job],
        )
        .unwrap();

        db.execute(
            "INSERT INTO commit_log(commit_rev,operation_id,kind,entity_id,input_hash,receipt) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![cap1_commit.commit_rev, cap1_commit.operation_id, cap1_commit.kind, cap1_commit.entity_id, cap1_commit.input_hash, cap1_commit.receipt],
        ).unwrap();
        db.execute(
            "INSERT INTO pdf_capture(id,document_id,record,anchor) VALUES(?1,?2,?3,?4)",
            rusqlite::params![1i64, 1i64, cap1_json, cap1_anchor],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_operation(operation_id,kind,document_id,capture_id,input_hash,payload) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![cap1_op.operation_id, cap1_op.kind, cap1_op.document_id, cap1_op.capture_id, cap1_op.input_hash, cap1_op.operation],
        ).unwrap();
        db.execute(
            "INSERT INTO pdf_receipt(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![cap1_op.operation_id, cap1_op.receipt],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_outbox(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![cap1_op.operation_id, cap1_op.outbox],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pdf_index_job(operation_id,payload) VALUES(?1,?2)",
            rusqlite::params![cap1_op.operation_id, cap1_op.index_job],
        )
        .unwrap();
    }
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let store = SqliteStore::open(&dir.0).unwrap();
    store.reset_blob_read_count();
    let mut app = KnowledgeApplication::new(store);
    let docs = app.pdf_documents_page("main", None, 10).unwrap();
    assert_eq!(docs.documents.len(), 1);
    assert_eq!(docs.documents[0].page_count, 1);
    let notes = app.pdf_notes_page("main", Some(1), None, 10).unwrap();
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].note_preview, "Scale capture note #1");

    // Incremental commit after v5->v6 migration works cleanly.
    let cap2 = app
        .capture_pdf("op-post-v6-cap", 1, &draft, &origin(), "after v6")
        .unwrap();
    assert_eq!(cap2.capture_id, 2);
    assert_eq!(app.into_store().blob_read_count(), 0);

    let db = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 6);
    let doc_meta_count: i64 = db
        .query_row("SELECT count(*) FROM pdf_document_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(doc_meta_count, 1);
    let cap_meta_count: i64 = db
        .query_row("SELECT count(*) FROM pdf_capture_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cap_meta_count, 2);
    let index_count: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 12);
}

#[test]
fn sqlite_and_blob_errors_keep_structured_context_without_path_leaks() {
    let dir = Scratch::new("qusor-secret-canary-dir-99412");
    fs::write(dir.0.join("core.sqlite"), b"not-a-valid-sqlite-database").unwrap();

    let err = match SqliteStore::open(&dir.0) {
        Ok(_) => panic!("corrupt core.sqlite must fail"),
        Err(err) => err,
    };
    assert_eq!(err.code, KnowledgeErrorCode::Storage);
    assert!(
        err.message.contains("[category=sqlite"),
        "expected structured category context, got: {}",
        err.message
    );
    assert!(
        !err.message.contains("qusor-secret-canary-dir-99412"),
        "error message leaked filesystem path: {}",
        err.message
    );
    assert!(
        !err.message.contains("core.sqlite"),
        "error message leaked database filename: {}",
        err.message
    );
}
