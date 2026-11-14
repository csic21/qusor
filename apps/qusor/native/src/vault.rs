//! One app-owned Core writer shared by every authorized Host adapter.
//!
//! The composition declares one `vault` provider. This module is the trusted
//! implementation of that provider: it opens one SQLite/CAS directory, serializes
//! access, and exposes the existing persistence ports without changing their
//! contracts. Plugins never receive this handle or a database path.
use core_api::{
    knowledge::{ArchiveV2, KnowledgeError, KnowledgeErrorCode},
    vocab::{
        CardSchedule, NewWord, ReviewEntry, ReviewGuard, StudyArchive, Wordbook, WordbookSeed,
    },
};
use learning_ports::{CommitPlan, KnowledgeStore, StudyStore};
use nsh_core::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use storage_sqlite::SqliteStore;

const CORE_DIRECTORY: &str = "core";
const LEGACY_DIRECTORIES: [&str; 2] = ["notes", "vocab"];
const DEFAULT_VAULT_ID: &str = "qusor-vault";

#[derive(Clone)]
pub struct VaultStore {
    vault_id: String,
    inner: Arc<Mutex<SqliteStore>>,
}

impl VaultStore {
    pub fn open_app(data_dir: &Path) -> Result<Self, String> {
        Self::open_app_for_vault(data_dir, DEFAULT_VAULT_ID)
    }

    pub fn open_app_for_vault(
        data_dir: &Path,
        vault_id: impl Into<String>,
    ) -> Result<Self, String> {
        let vault_id = vault_id.into();
        if vault_id.trim().is_empty() {
            return Err("vault id cannot be empty".into());
        }
        fs::create_dir_all(data_dir).map_err(text)?;
        let core = data_dir.join(CORE_DIRECTORY);
        if core.join("core.sqlite").is_file() {
            return Self::open_path(&core, vault_id);
        }
        if core.exists() {
            return Err("the Core directory exists without core.sqlite".into());
        }

        let legacy = LegacyArchives::read(data_dir)?;
        if legacy.knowledge.is_none() && legacy.study.is_none() {
            return Self::open_path(&core, vault_id);
        }
        migrate_legacy(data_dir, &core, legacy)?;
        Self::open_path(&core, vault_id)
    }

    pub fn vault_id(&self) -> &str {
        &self.vault_id
    }

    pub fn ensure_starter_wordbook(&self) -> Result<(), String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis();
        let now_ms = i64::try_from(now_ms).map_err(|_| "system time is unsupported")?;
        crate::core_facade::ensure_starter_wordbook(self, &starter_wordbook(), now_ms)
            .map_err(|error| error.to_string())
    }

    fn open_path(path: &Path, vault_id: String) -> Result<Self, String> {
        Ok(Self {
            vault_id,
            inner: Arc::new(Mutex::new(SqliteStore::open(path).map_err(text)?)),
        })
    }

    fn with_store<T>(
        &self,
        operation: impl FnOnce(&mut SqliteStore) -> core_api::knowledge::Result<T>,
    ) -> core_api::knowledge::Result<T> {
        let mut store = self.inner.lock().map_err(|_| {
            KnowledgeError::new(
                KnowledgeErrorCode::Unavailable,
                "the shared Core writer is unavailable",
            )
        })?;
        operation(&mut store)
    }
}

fn starter_wordbook() -> WordbookSeed {
    let word = |term: &str, pos: &str, phonetic: &str, meaning: &str, example: &str| NewWord {
        term: term.into(),
        pos: pos.into(),
        phonetic: phonetic.into(),
        meaning: meaning.into(),
        example: example.into(),
    };
    WordbookSeed {
        title: "QUSOR 入门 12 词".into(),
        description: "随应用提供的入门词书；用于首次学习，不代表考试词表。".into(),
        words: vec![
            word(
                "abandon",
                "v.",
                "/əˈbændən/",
                "放弃；抛弃",
                "They had to abandon the plan when the funding was withdrawn.",
            ),
            word(
                "benefit",
                "n./v.",
                "/ˈbenɪfɪt/",
                "好处；使受益",
                "Daily review benefits long-term memory more than one long session.",
            ),
            word(
                "candidate",
                "n.",
                "/ˈkændɪdət/",
                "候选人；应试者",
                "Each candidate read the passage twice before answering.",
            ),
            word(
                "coherent",
                "adj.",
                "/kəʊˈhɪərənt/",
                "连贯的；一致的",
                "A coherent argument states its claim before the evidence.",
            ),
            word(
                "cite",
                "v.",
                "/saɪt/",
                "引用；举例",
                "Cite the original page instead of a summary of it.",
            ),
            word(
                "context",
                "n.",
                "/ˈkɒntekst/",
                "上下文；语境",
                "The surrounding context changes how a sentence is understood.",
            ),
            word(
                "deliberate",
                "adj.",
                "/dɪˈlɪbərət/",
                "深思熟虑的；故意的",
                "A deliberate pause can make room for a better answer.",
            ),
            word(
                "evidence",
                "n.",
                "/ˈevɪdəns/",
                "证据",
                "Keep the evidence next to the claim it supports.",
            ),
            word(
                "interpret",
                "v.",
                "/ɪnˈtɜːprɪt/",
                "解释；理解",
                "Readers may interpret the same passage in different ways.",
            ),
            word(
                "method",
                "n.",
                "/ˈmeθəd/",
                "方法",
                "A clear method makes the result easier to reproduce.",
            ),
            word(
                "primary",
                "adj.",
                "/ˈpraɪməri/",
                "主要的；最初的",
                "The primary source is more reliable than a later summary.",
            ),
            word(
                "source",
                "n.",
                "/sɔːs/",
                "来源；出处",
                "Every quotation should preserve its source and page.",
            ),
        ],
    }
}

impl KnowledgeStore for VaultStore {
    fn snapshot(&self) -> core_api::knowledge::Result<ArchiveV2> {
        self.with_store(|store| store.snapshot())
    }

    fn commit(&mut self, plan: CommitPlan) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.commit(plan))
    }

    fn restore(&mut self, archive: &ArchiveV2) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.restore(archive))
    }

    fn update_pdf_capture_note(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.update_pdf_capture_note(vault, capture_id, note))
    }

    fn delete_pdf_capture(
        &mut self,
        vault: &str,
        capture_id: i64,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.delete_pdf_capture(vault, capture_id))
    }

    fn delete_pdf_document(
        &mut self,
        vault: &str,
        document_id: i64,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.delete_pdf_document(vault, document_id))
    }

    fn update_epub_capture_note(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.update_epub_capture_note(vault, capture_id, note))
    }

    fn delete_epub_capture(
        &mut self,
        vault: &str,
        capture_id: i64,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.delete_epub_capture(vault, capture_id))
    }

    fn delete_epub_document(
        &mut self,
        vault: &str,
        document_id: i64,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.delete_epub_document(vault, document_id))
    }
}

impl StudyStore for VaultStore {
    fn study_snapshot(&self) -> core_api::knowledge::Result<StudyArchive> {
        self.with_store(|store| store.study_snapshot())
    }

    fn ensure_wordbook(
        &mut self,
        vault: &str,
        seed: &WordbookSeed,
        now_ms: i64,
    ) -> core_api::knowledge::Result<Wordbook> {
        self.with_store(|store| store.ensure_wordbook(vault, seed, now_ms))
    }

    fn commit_review(
        &mut self,
        guard: &ReviewGuard,
        schedule: &CardSchedule,
        review: &ReviewEntry,
    ) -> core_api::knowledge::Result<ReviewEntry> {
        self.with_store(|store| store.commit_review(guard, schedule, review))
    }

    fn restore_study(&mut self, archive: &StudyArchive) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.restore_study(archive))
    }

    fn delete_wordbook(
        &mut self,
        vault: &str,
        wordbook_id: i64,
    ) -> core_api::knowledge::Result<()> {
        self.with_store(|store| store.delete_wordbook(vault, wordbook_id))
    }
}

/// Resolve the one Core handle owned by the application runtime.
pub fn open(sys: &System) -> Result<VaultStore, String> {
    crate::profile::require_service_active(crate::profile::VAULT_READ_SERVICE)?;
    let data_dir = sys
        .data_dir
        .clone()
        .ok_or("the host gave no app data directory")?;
    VaultStore::open_app(&data_dir)
}

struct LegacyArchives {
    knowledge: Option<ArchiveV2>,
    study: Option<StudyArchive>,
}

impl LegacyArchives {
    fn read(data_dir: &Path) -> Result<Self, String> {
        let mut knowledge = None;
        let mut study = None;
        for name in LEGACY_DIRECTORIES {
            let path = data_dir.join(name);
            if !path.join("core.sqlite").is_file() {
                continue;
            }
            let store = SqliteStore::open(&path)
                .map_err(|error| format!("cannot read legacy Core {name}: {error}"))?;
            let next_knowledge = store.snapshot().map_err(text)?;
            let next_study = store.study_snapshot().map_err(text)?;
            drop(store);
            if !knowledge_is_empty(&next_knowledge) {
                merge_once(&mut knowledge, next_knowledge, "captured knowledge")?;
            }
            if !study_is_empty(&next_study) {
                merge_once(&mut study, next_study, "vocabulary study data")?;
            }
        }
        Ok(Self { knowledge, study })
    }
}

fn merge_once<T>(slot: &mut Option<T>, value: T, label: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!(
            "more than one legacy Core contains {label}; refusing to merge implicitly"
        ));
    }
    Ok(())
}

fn knowledge_is_empty(archive: &ArchiveV2) -> bool {
    archive.commits.is_empty()
        && archive.text_entries.is_empty()
        && archive.pdf_documents.is_empty()
        && archive.pdf_captures.is_empty()
        && archive.pdf_operations.is_empty()
        && archive.epub_documents.is_empty()
        && archive.epub_captures.is_empty()
        && archive.blobs.is_empty()
}

fn study_is_empty(archive: &StudyArchive) -> bool {
    archive.wordbooks.is_empty()
        && archive.cards.is_empty()
        && archive.schedules.is_empty()
        && archive.reviews.is_empty()
}

fn migrate_legacy(data_dir: &Path, core: &Path, legacy: LegacyArchives) -> Result<(), String> {
    let staging = data_dir.join(format!(".core-migration-{}", std::process::id()));
    if staging.exists() {
        return Err("a previous Core migration directory is still present".into());
    }
    let cleanup = StagingDirectory {
        path: staging.clone(),
        armed: true,
    };
    let mut store = SqliteStore::open(&staging).map_err(text)?;
    if let Some(knowledge) = &legacy.knowledge {
        store.restore(knowledge).map_err(text)?;
    }
    if let Some(study) = &legacy.study {
        store.restore_study(study).map_err(text)?;
    }
    drop(store);
    fs::rename(&staging, core).map_err(text)?;
    let mut cleanup = cleanup;
    cleanup.armed = false;
    Ok(())
}

struct StagingDirectory {
    path: PathBuf,
    armed: bool,
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use core_api::{
        contract::CaptureDraft,
        knowledge::{Provenance, Receipt},
        vocab::{NewWord, StudyRating},
    };
    use learning_application::{KnowledgeApplication, study::StudyApplication};
    use std::{
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    const NOW: i64 = 1_767_225_600_000;
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-shared-core-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn draft() -> CaptureDraft {
        serde_json::from_value(serde_json::json!({
            "protocol_version": 1,
            "source": {
                "namespace": "test",
                "reference": "local",
                "title": "Shared Core",
                "public_uri": null,
                "external_return": "none"
            },
            "snapshot_text": "One vault",
            "selector": {
                "unit": "unicode_scalar",
                "normalization": "qusor-raw-text-v1",
                "start": 4,
                "end": 9,
                "exact": "vault"
            }
        }))
        .unwrap()
    }

    fn origin() -> Provenance {
        Provenance {
            package_id: "org.qusor.notes".into(),
            publisher_id: "org.qusor".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        }
    }

    fn commit(store: &VaultStore, operation: &str) -> Receipt {
        let mut app = KnowledgeApplication::new(store.clone());
        app.commit(operation, "qusor-vault", &draft(), &origin(), "shared")
            .unwrap()
    }

    fn seed(store: &VaultStore) {
        let mut app = StudyApplication::new(store.clone());
        app.seed_wordbook(
            "qusor-vault",
            &WordbookSeed {
                title: "Shared Core".into(),
                description: "Synthetic shared-Core fixture.".into(),
                words: vec![NewWord {
                    term: "vault".into(),
                    pos: String::new(),
                    phonetic: String::new(),
                    meaning: "本地库".into(),
                    example: "One Core serves both plugins.".into(),
                }],
            },
            NOW,
        )
        .unwrap();
    }

    #[test]
    fn notes_and_vocab_share_one_writer_across_reopen() {
        let scratch = Scratch::new();
        {
            let store = VaultStore::open_app(&scratch.0).unwrap();
            commit(&store, "shared-notes");
            seed(&store);
            let notes = KnowledgeApplication::new(store.clone())
                .export_all()
                .unwrap();
            let study = StudyApplication::new(store.clone()).export_study().unwrap();
            assert_eq!(notes.text_entries.len(), 1);
            assert_eq!(study.wordbooks.len(), 1);
        }
        assert!(scratch.0.join("core/core.sqlite").is_file());
        assert!(!scratch.0.join("notes").exists());
        assert!(!scratch.0.join("vocab").exists());

        let reopened = VaultStore::open_app(&scratch.0).unwrap();
        assert_eq!(
            KnowledgeApplication::new(reopened.clone())
                .export_all()
                .unwrap()
                .text_entries
                .len(),
            1
        );
        assert_eq!(
            StudyApplication::new(reopened)
                .export_study()
                .unwrap()
                .wordbooks
                .len(),
            1
        );
    }

    #[test]
    fn starter_wordbook_is_installed_once_and_reopens() {
        let scratch = Scratch::new();
        {
            let store = VaultStore::open_app(&scratch.0).unwrap();
            store.ensure_starter_wordbook().unwrap();
            store.ensure_starter_wordbook().unwrap();
            let study = StudyApplication::new(store).export_study().unwrap();
            assert_eq!(study.wordbooks.len(), 1);
            assert_eq!(study.wordbooks[0].title, "QUSOR 入门 12 词");
            assert_eq!(study.cards.len(), 12);
            assert!(study.schedules.is_empty());
            assert!(study.reviews.is_empty());
        }
        let reopened = VaultStore::open_app(&scratch.0).unwrap();
        let study = StudyApplication::new(reopened).export_study().unwrap();
        assert_eq!(study.wordbooks.len(), 1);
        assert_eq!(study.cards.len(), 12);
    }

    #[test]
    fn legacy_split_cores_are_copied_into_one_new_core() {
        let scratch = Scratch::new();
        {
            let notes = SqliteStore::open(scratch.0.join("notes")).unwrap();
            let mut app = KnowledgeApplication::new(notes);
            app.commit("legacy-notes", "qusor-vault", &draft(), &origin(), "legacy")
                .unwrap();
        }
        {
            let vocab = SqliteStore::open(scratch.0.join("vocab")).unwrap();
            let mut app = StudyApplication::new(vocab);
            app.seed_wordbook(
                "qusor-vault",
                &WordbookSeed {
                    title: "Legacy vocab".into(),
                    description: "Synthetic migration fixture.".into(),
                    words: vec![NewWord {
                        term: "legacy".into(),
                        pos: String::new(),
                        phonetic: String::new(),
                        meaning: "旧的".into(),
                        example: "The split Core is preserved.".into(),
                    }],
                },
                NOW,
            )
            .unwrap();
            let card_id = app.decks("qusor-vault", NOW).unwrap()[0].wordbook.id;
            let queue = app
                .queue("qusor-vault", card_id, NOW, NOW - 86_400_000, 1)
                .unwrap();
            app.review(
                "qusor-vault",
                queue.cards[0].card.id,
                StudyRating::Good,
                "legacy-review",
                NOW,
                NOW - 86_400_000,
            )
            .unwrap();
        }

        let migrated = VaultStore::open_app(&scratch.0).unwrap();
        let knowledge = KnowledgeApplication::new(migrated.clone())
            .export_all()
            .unwrap();
        let study = StudyApplication::new(migrated).export_study().unwrap();
        assert_eq!(knowledge.text_entries.len(), 1);
        assert_eq!(study.wordbooks.len(), 1);
        assert_eq!(study.reviews.len(), 1);
        assert!(scratch.0.join("core/core.sqlite").is_file());
        assert!(scratch.0.join("notes/core.sqlite").is_file());
        assert!(scratch.0.join("vocab/core.sqlite").is_file());
    }

    #[test]
    fn conflicting_legacy_knowledge_is_rejected_without_publishing_core() {
        let scratch = Scratch::new();
        {
            let notes = SqliteStore::open(scratch.0.join("notes")).unwrap();
            let mut app = KnowledgeApplication::new(notes);
            app.commit("notes-one", "qusor-vault", &draft(), &origin(), "one")
                .unwrap();
        }
        {
            let vocab = SqliteStore::open(scratch.0.join("vocab")).unwrap();
            let mut app = KnowledgeApplication::new(vocab);
            app.commit("vocab-two", "qusor-vault", &draft(), &origin(), "two")
                .unwrap();
        }

        let error = match VaultStore::open_app(&scratch.0) {
            Ok(_) => panic!("conflicting legacy knowledge must not open"),
            Err(error) => error,
        };
        assert!(error.contains("more than one legacy Core"));
        assert!(!scratch.0.join("core").exists());
        assert!(
            !scratch
                .0
                .join(format!(".core-migration-{}", std::process::id()))
                .exists()
        );
    }
}
