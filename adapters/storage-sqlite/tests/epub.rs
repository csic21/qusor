#![cfg(unix)]
//! Real SQLite integration tests for EPUB import, chapter reading, TOC,
//! selection captures, note editing/deletion, document deletion, audit logging,
//! and export/restore roundtrip.
use core_api::{
    contract::sha256,
    epub_contract::{EpubChapterSnapshot, EpubSnapshot, EpubTocEntry},
    knowledge::{KnowledgeErrorCode, Provenance},
};
use learning_application::{
    KnowledgeApplication,
    epub_library::{EpubSavedSelection, EpubSelectionRange},
    pdf_notes::{MAX_NOTE_CHUNK_BYTES, NoteBodyField},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-epub-sqlite-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn store(&self) -> SqliteStore {
        SqliteStore::open(&self.0).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn origin() -> Provenance {
    Provenance {
        package_id: "third.party.epub".into(),
        publisher_id: "publisher".into(),
        package_digest: "b".repeat(64),
        installation_id: 2,
    }
}

fn sample_epub(vault: &str, title: &str, ch0_text: &str, ch1_text: &str) -> EpubSnapshot {
    EpubSnapshot {
        vault: vault.into(),
        title: title.into(),
        author: "Ada Lovelace".into(),
        original_sha256: sha256(format!("{vault}:{title}:{ch0_text}").as_bytes()),
        original_bytes: 2048,
        extraction_revision: "epub-v1".into(),
        chapters: vec![
            EpubChapterSnapshot {
                chapter_index: 0,
                title: "Chapter 1".into(),
                href: "OPS/ch1.xhtml".into(),
                text: ch0_text.into(),
            },
            EpubChapterSnapshot {
                chapter_index: 1,
                title: "Chapter 2".into(),
                href: "OPS/ch2.xhtml".into(),
                text: ch1_text.into(),
            },
        ],
        toc: vec![
            EpubTocEntry {
                title: "Part I".into(),
                chapter_index: 0,
                depth: 0,
            },
            EpubTocEntry {
                title: "Chapter 2".into(),
                chapter_index: 1,
                depth: 1,
            },
        ],
    }
}

#[test]
fn epub_full_lifecycle_read_capture_edit_export_restore() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());

    let snap1 = sample_epub(
        "main",
        "Analytical Engine Notes",
        "The engine might compose elaborate pieces of music😀.",
        "Second chapter content with plenty of prose.",
    );
    let snap2 = sample_epub(
        "main",
        "Second Book",
        "Another EPUB book first chapter.",
        "Second book chapter two.",
    );
    let other_snap = sample_epub(
        "other",
        "Other Vault Book",
        "Isolated vault chapter.",
        "Other chapter two.",
    );

    // Import + idempotent retry + conflict on mismatched payload
    let doc1 = app.import_epub("op-epub-1", "main", &snap1).unwrap();
    assert_eq!(app.import_epub("op-epub-1", "main", &snap1).unwrap(), doc1);
    assert_eq!(
        app.import_epub("op-epub-1", "main", &snap2)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::Conflict
    );

    let doc2 = app.import_epub("op-epub-2", "main", &snap2).unwrap();
    let _other_doc = app
        .import_epub("op-epub-other", "other", &other_snap)
        .unwrap();

    // Library listing pagination (newest-first, scoped by vault)
    let page1 = app.epub_documents_page("main", None, 1).unwrap();
    assert_eq!(page1.documents.len(), 1);
    assert_eq!(page1.documents[0].document_id, doc2.document_id);
    assert_eq!(page1.documents[0].chapter_count, 2);
    let page2 = app
        .epub_documents_page("main", page1.next_cursor.as_deref(), 10)
        .unwrap();
    assert_eq!(page2.documents.len(), 1);
    assert_eq!(page2.documents[0].document_id, doc1.document_id);
    assert_eq!(page2.next_cursor, None);

    // TOC & wrong vault check
    let toc = app.epub_toc("main", doc1.document_id).unwrap();
    assert_eq!(toc, snap1.toc);
    assert_eq!(
        app.epub_toc("other", doc1.document_id).unwrap_err().code,
        KnowledgeErrorCode::NotFound
    );

    // Chunked chapter reading across multi-byte UTF-8 boundary
    let ch0_chunk1 = app
        .epub_chapter_text("main", doc1.document_id, 0, 0, 32)
        .unwrap();
    let ch0_chunk2 = app
        .epub_chapter_text(
            "main",
            doc1.document_id,
            0,
            ch0_chunk1.next_offset.unwrap(),
            1024,
        )
        .unwrap();
    assert_eq!(
        format!("{}{}", ch0_chunk1.text, ch0_chunk2.text),
        snap1.chapters[0].text
    );
    assert_eq!(ch0_chunk2.next_offset, None);

    // Capture scalar selection covering "compose elaborate"
    let full = &snap1.chapters[0].text;
    let start = full[..full.find("compose").unwrap()].chars().count() as u64;
    let end = start + "compose elaborate".chars().count() as u64;
    let cap = app
        .capture_epub_selection(
            "op-epub-cap-1",
            "main",
            EpubSelectionRange {
                document_id: doc1.document_id,
                chapter_index: 0,
                start,
                end,
            },
            &origin(),
            "Original EPUB note",
        )
        .unwrap();
    // Idempotent retry
    assert_eq!(
        app.capture_epub_selection(
            "op-epub-cap-1",
            "main",
            EpubSelectionRange {
                document_id: doc1.document_id,
                chapter_index: 0,
                start,
                end,
            },
            &origin(),
            "Original EPUB note",
        )
        .unwrap(),
        cap
    );

    let saved = app
        .epub_saved_selections("main", doc1.document_id, 0)
        .unwrap();
    assert_eq!(
        saved,
        vec![EpubSavedSelection {
            capture_id: cap.capture_id,
            start,
            end,
        }]
    );

    // Edit note + validation + audit row
    assert_eq!(
        app.update_epub_capture_note("other", cap.capture_id, "Nope")
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    assert_eq!(
        app.update_epub_capture_note("main", cap.capture_id, "bad\u{0000}")
            .unwrap_err()
            .code,
        KnowledgeErrorCode::InvalidInput
    );
    app.update_epub_capture_note("main", cap.capture_id, "Edited EPUB note")
        .unwrap();

    let note_body = app
        .epub_note_body(
            "main",
            cap.capture_id,
            NoteBodyField::Note,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(note_body.text, "Edited EPUB note");
    let quote_body = app
        .epub_note_body(
            "main",
            cap.capture_id,
            NoteBodyField::Quote,
            0,
            MAX_NOTE_CHUNK_BYTES,
        )
        .unwrap();
    assert_eq!(quote_body.text, "compose elaborate");

    // Reopen SQLite store and verify persistence before export/restore
    drop(app);
    let reopened = KnowledgeApplication::new(scratch.store());
    let notes = reopened
        .epub_notes_page("main", Some(doc1.document_id), None, 10)
        .unwrap();
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].note_preview, "Edited EPUB note");

    // Export and restore into a brand-new empty Core
    let exported = reopened.export_all().unwrap();
    assert_eq!(exported.epub_documents.len(), 3);
    assert_eq!(exported.epub_captures.len(), 1);
    assert_eq!(exported.epub_captures[0].user_note, "Edited EPUB note");

    let scratch2 = Scratch::new();
    let mut restored = KnowledgeApplication::new(scratch2.store());
    let receipt = restored.restore_all(&exported).unwrap();
    assert_eq!(receipt.epub_documents, 3);
    assert_eq!(receipt.epub_captures, 1);

    let restored_toc = restored.epub_toc("main", doc1.document_id).unwrap();
    assert_eq!(restored_toc, snap1.toc);
    let restored_ch1 = restored
        .epub_chapter_text("main", doc1.document_id, 1, 0, 1024)
        .unwrap();
    assert_eq!(restored_ch1.text, snap1.chapters[1].text);
    let restored_notes = restored.epub_notes_page("main", None, None, 10).unwrap();
    assert_eq!(restored_notes.notes.len(), 1);
    assert_eq!(restored_notes.notes[0].note_preview, "Edited EPUB note");

    // Subsequent capture after restore works
    let cap2 = restored
        .capture_epub_selection(
            "op-epub-cap-2",
            "main",
            EpubSelectionRange {
                document_id: doc1.document_id,
                chapter_index: 1,
                start: 0,
                end: 6,
            },
            &origin(),
            "Chapter two note",
        )
        .unwrap();
    assert!(cap2.capture_id > cap.capture_id);
}

#[test]
fn epub_delete_capture_and_delete_document_audit_and_restore() {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(scratch.store());
    let snap1 = sample_epub(
        "main",
        "Book One",
        "Alpha beta gamma delta.",
        "Epsilon zeta eta.",
    );
    let snap2 = sample_epub("main", "Book Two", "Second book text.", "More text.");
    let doc1 = app.import_epub("op-e-1", "main", &snap1).unwrap();
    let doc2 = app.import_epub("op-e-2", "main", &snap2).unwrap();

    let cap1 = app
        .capture_epub_selection(
            "op-ec-1",
            "main",
            EpubSelectionRange {
                document_id: doc1.document_id,
                chapter_index: 0,
                start: 0,
                end: 5,
            },
            &origin(),
            "Note 1",
        )
        .unwrap();
    let _cap2 = app
        .capture_epub_selection(
            "op-ec-2",
            "main",
            EpubSelectionRange {
                document_id: doc1.document_id,
                chapter_index: 1,
                start: 0,
                end: 7,
            },
            &origin(),
            "Note 2",
        )
        .unwrap();
    let cap3 = app
        .capture_epub_selection(
            "op-ec-3",
            "main",
            EpubSelectionRange {
                document_id: doc2.document_id,
                chapter_index: 0,
                start: 0,
                end: 6,
            },
            &origin(),
            "Book 2 note",
        )
        .unwrap();

    // Delete single capture
    assert_eq!(
        app.delete_epub_capture("other", cap1.capture_id)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    app.delete_epub_capture("main", cap1.capture_id).unwrap();
    assert!(
        app.epub_saved_selections("main", doc1.document_id, 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.epub_notes_page("main", Some(doc1.document_id), None, 10)
            .unwrap()
            .notes
            .len(),
        1
    );

    // Delete doc1 cascades cap2 while keeping doc2 & cap3 intact
    assert_eq!(
        app.delete_epub_document("other", doc1.document_id)
            .unwrap_err()
            .code,
        KnowledgeErrorCode::NotFound
    );
    app.delete_epub_document("main", doc1.document_id).unwrap();
    let docs = app.epub_documents_page("main", None, 10).unwrap();
    assert_eq!(docs.documents.len(), 1);
    assert_eq!(docs.documents[0].document_id, doc2.document_id);
    let notes = app.epub_notes_page("main", None, None, 10).unwrap();
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].capture_id, cap3.capture_id);

    // Verify knowledge_audit rows
    let db = rusqlite::Connection::open(scratch.0.join("core.sqlite")).unwrap();
    let del_cap_audit: i64 = db
        .query_row(
            "SELECT count(*) FROM knowledge_audit WHERE vault='main' AND action='delete_epub_capture' AND entity_kind='epub_capture' AND entity_id=?1",
            [cap1.capture_id],
            |r| r.get(0),
        )
        .unwrap();
    let del_doc_audit: i64 = db
        .query_row(
            "SELECT count(*) FROM knowledge_audit WHERE vault='main' AND action='delete_epub_document' AND entity_kind='epub_document' AND entity_id=?1",
            [doc1.document_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(del_cap_audit, 1);
    assert_eq!(del_doc_audit, 1);

    // Export and restore after deletions into empty Core
    let exported = app.export_all().unwrap();
    let scratch2 = Scratch::new();
    let mut restored = KnowledgeApplication::new(scratch2.store());
    restored.restore_all(&exported).unwrap();
    let restored_docs = restored.epub_documents_page("main", None, 10).unwrap();
    assert_eq!(restored_docs.documents.len(), 1);
    assert_eq!(restored_docs.documents[0].document_id, doc2.document_id);
    let restored_notes = restored.epub_notes_page("main", None, None, 10).unwrap();
    assert_eq!(restored_notes.notes.len(), 1);
    assert_eq!(restored_notes.notes[0].capture_id, cap3.capture_id);
}
