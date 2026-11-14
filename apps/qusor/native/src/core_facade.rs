//! Trusted Core service facade for the application shell.
//!
//! All non-bridge `KnowledgeApplication` and `StudyApplication` invocations in
//! `apps/qusor/native` are consolidated here and bound to `VaultStore::vault_id()`.
//! No other non-test application module may instantiate Core application objects
//! directly.
//!
//! Boundary classification:
//! 1. **Host universal capabilities**: archive export/restore, trusted PDF/EPUB
//!    original file import, native PDF document rendering/TOC reads, wordbook
//!    file import, and initial starter seeding. User file bytes and raw DB paths
//!    stay inside the trusted Host/app boundary.
//! 2. **Authorized Host bridges**: constructing `LibraryReadBridge`,
//!    `NotesReadBridge`, and `VocabBridge` backed by `Host::authorize_*` +
//!    `Host::dispatch`.
//! 3. **Legacy direct bypass (`legacy_plugin_bypass`, frozen baseline)**:
//!    feature-specific EPUB reading/capture, note edit/delete, document delete,
//!    and extended vocab queries that currently bypass Host wire protocols.
//!    These are frozen by `scripts/check_app_boundary.py` and exist solely to
//!    preserve current behavior until migrated into plugins and orthogonal Host
//!    capabilities.

use crate::vault::VaultStore;
use core_api::{
    epub_contract::{EpubSnapshot, EpubTocEntry},
    knowledge::{
        ArchiveV2, EpubDocumentReceipt, EpubReceipt, PdfDocumentReceipt, PdfDocumentRecord,
        Provenance, RestoreAllReceipt, Result as CoreResult,
    },
    pdf_contract::PdfSnapshot,
    vocab::{StudyArchive, Wordbook, WordbookSeed},
};
use learning_application::{
    KnowledgeApplication,
    epub_library::{
        EpubChapterTextChunk, EpubDocumentPage, EpubNoteChunk, EpubNotePage, EpubSavedSelection,
        EpubSelectionRange,
    },
    pdf_notes::NoteBodyField,
    study::StudyApplication,
};
use qusor_app_bridges::{
    library_bridge::LibraryReadBridge, notes_bridge::NotesReadBridge,
    orthogonal_bridge::OrthogonalKnowledgeBridge, vocab_bridge::VocabBridge,
};

// ---------------------------------------------------------------------------
// 1. Authorized Host Wire Bridges
// ---------------------------------------------------------------------------

/// Build the Host-gated orthogonal Core Knowledge wire adapter bound to `store.vault_id()`.
pub(crate) fn new_orthogonal_bridge(store: &VaultStore) -> OrthogonalKnowledgeBridge<VaultStore> {
    OrthogonalKnowledgeBridge::new(KnowledgeApplication::new(store.clone()), store.vault_id())
}

/// Build the Host-gated Library wire adapter bound to `store.vault_id()`.
pub(crate) fn new_library_bridge(store: &VaultStore) -> LibraryReadBridge<VaultStore> {
    LibraryReadBridge::new(KnowledgeApplication::new(store.clone()), store.vault_id())
}

/// Build the Host-gated Notes wire adapter bound to `store.vault_id()`.
pub(crate) fn new_notes_bridge(store: &VaultStore) -> NotesReadBridge<VaultStore> {
    NotesReadBridge::new(KnowledgeApplication::new(store.clone()), store.vault_id())
}

/// Build the Host-gated Vocabulary wire adapter bound to `store.vault_id()`.
pub(crate) fn new_vocab_bridge(
    store: &VaultStore,
    now_ms: i64,
    day_start_ms: i64,
) -> VocabBridge<VaultStore> {
    VocabBridge::new(
        StudyApplication::new(store.clone()),
        store.vault_id(),
        now_ms,
        day_start_ms,
    )
}

// ---------------------------------------------------------------------------
// 2. Host Universal Capabilities (Backup/Restore, Trusted Import, Native Docs)
// ---------------------------------------------------------------------------

/// Host universal capability: export full knowledge and study archives for backup.
pub(crate) fn export_backup_archives(store: &VaultStore) -> CoreResult<(ArchiveV2, StudyArchive)> {
    let knowledge = KnowledgeApplication::new(store.clone()).export_all()?;
    let study = StudyApplication::new(store.clone()).export_study()?;
    Ok((knowledge, study))
}

/// Host universal capability: inspect current archive state before restore.
pub(crate) fn inspect_backup_state(store: &VaultStore) -> CoreResult<(ArchiveV2, StudyArchive)> {
    export_backup_archives(store)
}

/// Host universal capability: restore knowledge archive into an empty Core.
pub(crate) fn restore_knowledge_archive(
    store: &VaultStore,
    archive: &ArchiveV2,
) -> CoreResult<RestoreAllReceipt> {
    KnowledgeApplication::new(store.clone()).restore_all(archive)
}

/// Host universal capability: replace starter wordbook (if needed) and restore study archive.
pub(crate) fn restore_study_archive(
    store: &VaultStore,
    replace_starter_wordbook_id: Option<i64>,
    archive: &StudyArchive,
) -> CoreResult<()> {
    let mut study = StudyApplication::new(store.clone());
    if let Some(starter_id) = replace_starter_wordbook_id {
        study.delete_wordbook(store.vault_id(), starter_id)?;
    }
    study.restore_study(archive)
}

/// Host universal capability (app bootstrap): seed starter wordbook if study store is empty.
pub(crate) fn ensure_starter_wordbook(
    store: &VaultStore,
    seed: &WordbookSeed,
    now_ms: i64,
) -> CoreResult<()> {
    let mut study = StudyApplication::new(store.clone());
    if !study.export_study()?.wordbooks.is_empty() {
        return Ok(());
    }
    study
        .seed_wordbook(store.vault_id(), seed, now_ms)
        .map(|_| ())
}

/// Host universal capability: commit user-selected PDF bytes and semantic snapshot to Core.
pub(crate) fn import_pdf_document(
    store: &VaultStore,
    operation_id: &str,
    title: &str,
    bytes: &[u8],
    snapshot: &PdfSnapshot,
) -> CoreResult<PdfDocumentReceipt> {
    KnowledgeApplication::new(store.clone()).import_document(
        operation_id,
        store.vault_id(),
        title,
        bytes,
        snapshot,
    )
}

/// Host native document service capability: query retained PDF snapshot metadata for rendering/TOC.
pub(crate) fn query_pdf_document(
    store: &VaultStore,
    document_id: i64,
) -> CoreResult<PdfDocumentRecord> {
    KnowledgeApplication::new(store.clone()).query_document(document_id)
}

/// Host native document service capability: read retained original PDF bytes for rendering/TOC.
pub(crate) fn read_original_pdf(store: &VaultStore, document_id: i64) -> CoreResult<Vec<u8>> {
    KnowledgeApplication::new(store.clone()).original_pdf(document_id)
}

/// Host universal capability: commit parsed EPUB snapshot from user-selected bytes to Core.
pub(crate) fn import_epub_document(
    store: &VaultStore,
    operation_id: &str,
    snapshot: &EpubSnapshot,
) -> CoreResult<EpubDocumentReceipt> {
    KnowledgeApplication::new(store.clone()).import_epub(operation_id, store.vault_id(), snapshot)
}

/// Host universal capability: import user-selected JSON wordbook into the bound vault.
pub(crate) fn import_wordbook(
    store: &VaultStore,
    seed: &WordbookSeed,
    now_ms: i64,
) -> Result<Wordbook, String> {
    let mut study = StudyApplication::new(store.clone());
    if study
        .decks(store.vault_id(), now_ms)
        .map_err(|e| e.to_string())?
        .iter()
        .any(|deck| deck.wordbook.title == seed.title)
    {
        return Err("a wordbook with this title already exists".into());
    }
    study
        .seed_wordbook(store.vault_id(), seed, now_ms)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// 3. Legacy Direct Bypass (Frozen Baseline — Pending Plugin Migration)
// ---------------------------------------------------------------------------

/// Frozen bypass methods pending migration out of `apps/qusor` into plugins.
/// `scripts/check_app_boundary.py` freezes this surface so no new bypasses can
/// be added.
pub(crate) mod legacy_plugin_bypass {
    use super::*;

    /// Pending Library plugin command: delete an imported PDF document.
    pub(crate) fn delete_pdf_document(store: &VaultStore, document_id: i64) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).delete_pdf_document(store.vault_id(), document_id)
    }

    /// Pending Library plugin command: delete an imported EPUB document.
    pub(crate) fn delete_epub_document(store: &VaultStore, document_id: i64) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).delete_epub_document(store.vault_id(), document_id)
    }

    /// Pending Library/Reader plugin query: paginated EPUB metadata list.
    pub(crate) fn epub_documents_page(
        store: &VaultStore,
        after: Option<&str>,
        limit: usize,
    ) -> CoreResult<EpubDocumentPage> {
        KnowledgeApplication::new(store.clone()).epub_documents_page(store.vault_id(), after, limit)
    }

    /// Pending Reader plugin query: EPUB table of contents.
    pub(crate) fn epub_toc(store: &VaultStore, document_id: i64) -> CoreResult<Vec<EpubTocEntry>> {
        KnowledgeApplication::new(store.clone()).epub_toc(store.vault_id(), document_id)
    }

    /// Pending Reader plugin query: chunked EPUB chapter text.
    pub(crate) fn epub_chapter_text(
        store: &VaultStore,
        document_id: i64,
        chapter_index: u32,
        offset: usize,
        max_bytes: usize,
    ) -> CoreResult<EpubChapterTextChunk> {
        KnowledgeApplication::new(store.clone()).epub_chapter_text(
            store.vault_id(),
            document_id,
            chapter_index,
            offset,
            max_bytes,
        )
    }

    /// Pending Reader plugin query: saved EPUB highlight ranges for a chapter.
    pub(crate) fn epub_saved_selections(
        store: &VaultStore,
        document_id: i64,
        chapter_index: u32,
    ) -> CoreResult<Vec<EpubSavedSelection>> {
        KnowledgeApplication::new(store.clone()).epub_saved_selections(
            store.vault_id(),
            document_id,
            chapter_index,
        )
    }

    /// Pending Reader plugin command: commit an EPUB text selection capture.
    pub(crate) fn capture_epub_selection(
        store: &VaultStore,
        operation_id: &str,
        range: EpubSelectionRange,
        provenance: &Provenance,
        note: &str,
    ) -> CoreResult<EpubReceipt> {
        KnowledgeApplication::new(store.clone()).capture_epub_selection(
            operation_id,
            store.vault_id(),
            range,
            provenance,
            note,
        )
    }

    /// Pending Notes plugin query: paginated EPUB capture notes.
    pub(crate) fn epub_notes_page(
        store: &VaultStore,
        after: Option<&str>,
        limit: usize,
    ) -> CoreResult<EpubNotePage> {
        KnowledgeApplication::new(store.clone()).epub_notes_page(
            store.vault_id(),
            None,
            after,
            limit,
        )
    }

    /// Pending Notes plugin query: chunked EPUB capture quote/note body.
    pub(crate) fn epub_note_body(
        store: &VaultStore,
        capture_id: i64,
        field: NoteBodyField,
        offset: usize,
        max_bytes: usize,
    ) -> CoreResult<EpubNoteChunk> {
        KnowledgeApplication::new(store.clone()).epub_note_body(
            store.vault_id(),
            capture_id,
            field,
            offset,
            max_bytes,
        )
    }

    /// Pending Notes plugin command: update a PDF capture's user note.
    pub(crate) fn update_pdf_capture_note(
        store: &VaultStore,
        capture_id: i64,
        note: &str,
    ) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).update_pdf_capture_note(
            store.vault_id(),
            capture_id,
            note,
        )
    }

    /// Pending Notes plugin command: update an EPUB capture's user note.
    pub(crate) fn update_epub_capture_note(
        store: &VaultStore,
        capture_id: i64,
        note: &str,
    ) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).update_epub_capture_note(
            store.vault_id(),
            capture_id,
            note,
        )
    }

    /// Pending Notes plugin command: delete a PDF capture.
    pub(crate) fn delete_pdf_capture(store: &VaultStore, capture_id: i64) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).delete_pdf_capture(store.vault_id(), capture_id)
    }

    /// Pending Notes plugin command: delete an EPUB capture.
    pub(crate) fn delete_epub_capture(store: &VaultStore, capture_id: i64) -> CoreResult<()> {
        KnowledgeApplication::new(store.clone()).delete_epub_capture(store.vault_id(), capture_id)
    }
}
