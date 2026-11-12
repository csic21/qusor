#![cfg(unix)]
//! Exercise the real packaged Host initializers over one migrated Core.
//!
//! This is intentionally below the NSH window layer: it proves that the shipped
//! app profile, ordinary package verification, shared VaultStore, notes bridge
//! and vocabulary bridge agree after legacy split-store migration.
use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
    vocab::{NewWord, WordbookSeed},
};
use learning_application::{KnowledgeApplication, study::StudyApplication};
use nsh_core::prelude::System;
use qusor_app_native::{
    composition_native::CompositionHost, notes_native::NotesHost, vocab_native::VocabHost,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const NOW: i64 = 1_767_225_600_000;
const TEXT: &str = "迁移核心想法";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-migrated-hosts-{}-{}",
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

fn snapshot(original: &[u8]) -> PdfSnapshot {
    let characters = TEXT
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let byte = TEXT.char_indices().nth(index).unwrap().0;
            json!({
                "pdf_char_index": index,
                "unicode_value": character as u32,
                "mapping": "scalar",
                "generated": false,
                "scalar_start": index,
                "scalar_end": index + 1,
                "utf8_start": byte,
                "utf8_end": byte + character.len_utf8(),
                "utf16_start": index,
                "utf16_end": index + 1,
                "pdf_bounds": {
                    "left": index as f32,
                    "bottom": 1.0,
                    "right": index as f32 + 1.0,
                    "top": 2.0
                }
            })
        })
        .collect::<Vec<_>>();
    serde_json::from_value(json!({
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
            "normalization": PDF_NORMALIZATION,
        },
        "engine_report_sha256": "b".repeat(64),
        "pages": [{
            "page_index": 0,
            "engine_text": TEXT,
            "text_sha256": sha256(TEXT.as_bytes()),
            "rotation_degrees": 0,
            "media_box": {
                "left": 0.0,
                "bottom": 0.0,
                "right": 500.0,
                "top": 300.0
            },
            "crop_box": {
                "left": 0.0,
                "bottom": 0.0,
                "right": 500.0,
                "top": 300.0
            },
            "characters": characters,
        }],
    }))
    .unwrap()
}

fn seed_legacy_notes(root: &std::path::Path) {
    let original = b"%PDF-1.7\nMigrated notes fixture.\n".to_vec();
    let snapshot = snapshot(&original);
    let mut app = KnowledgeApplication::new(SqliteStore::open(root.join("notes")).unwrap());
    let imported = app
        .import_document(
            "legacy-import",
            "qusor-vault",
            "Legacy note",
            &original,
            &snapshot,
        )
        .unwrap();
    let draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(TEXT.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: TEXT.chars().count() as u64,
        exact: TEXT.into(),
    };
    app.capture_pdf(
        "legacy-capture",
        imported.document_id,
        &draft,
        &Provenance {
            package_id: "org.qusor.notes".into(),
            publisher_id: "org.qusor".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        },
        "迁移后的想法",
    )
    .unwrap();
}

fn seed_legacy_vocab(root: &std::path::Path) {
    let mut app = StudyApplication::new(SqliteStore::open(root.join("vocab")).unwrap());
    app.seed_wordbook(
        "qusor-vault",
        &WordbookSeed {
            title: "Legacy deck".into(),
            description: "Migrated vocabulary fixture.".into(),
            words: vec![NewWord {
                term: "core".into(),
                meaning: "核心".into(),
                example: "One Core serves both Hosts.".into(),
            }],
        },
        NOW,
    )
    .unwrap();
}

/// Manual companion for the packaged/dev-host migration check. It leaves the
/// split stores in place so a guarded application launch can migrate them.
#[test]
#[ignore]
fn seed_legacy_fixture_for_manual_app() {
    let root = std::env::var_os("QUSOR_SEED_LEGACY_ROOT")
        .map(PathBuf::from)
        .expect("set QUSOR_SEED_LEGACY_ROOT");
    fs::create_dir_all(&root).unwrap();
    seed_legacy_notes(&root);
    seed_legacy_vocab(&root);
}

fn value(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn packaged_host_initializers_read_one_migrated_core() {
    let scratch = Scratch::new();
    seed_legacy_notes(&scratch.0);
    seed_legacy_vocab(&scratch.0);

    let mut system = System::default();
    system.data_dir = Some(scratch.0.clone());

    let notes = value(NotesHost::page(system.clone()));
    assert_eq!(notes["ok"], true, "{notes}");
    let notes = notes["value"]["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["note_preview"], "迁移后的想法");

    let decks = value(VocabHost::decks(system.clone()));
    assert_eq!(decks["ok"], true, "{decks}");
    let decks = decks["value"]["decks"].as_array().unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0]["title"], "Legacy deck");

    let composition = value(CompositionHost::dump());
    assert_eq!(composition["ok"], true);
    assert!(
        composition["value"]["dump"]
            .as_str()
            .unwrap()
            .contains("app.qusor.core")
    );

    let workspaces = value(CompositionHost::workspaces(system.clone()));
    assert_eq!(workspaces["ok"], true, "{workspaces}");
    let workspaces = workspaces["value"]["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 4);
    assert!(workspaces.iter().any(|workspace| {
        workspace["id"] == "org.qusor.notes.workspace"
            && workspace["title"] == "想法"
            && workspace["kind"] == "notes_workspace"
    }));
    assert!(workspaces.iter().any(|workspace| {
        workspace["id"] == "org.qusor.vocab.workspace"
            && workspace["title"] == "背单词"
            && workspace["kind"] == "vocab_workspace"
    }));
    assert!(workspaces.iter().any(|workspace| {
        workspace["id"] == "org.qusor.reader.preview.workspace"
            && workspace["title"] == "阅读"
            && workspace["kind"] == "reader_workspace"
    }));
    assert!(workspaces.iter().any(|workspace| {
        workspace["id"] == "org.qusor.library.workspace"
            && workspace["title"] == "资料库"
            && workspace["kind"] == "library_workspace"
    }));
    let library = value(qusor_app_native::library_native::LibraryHost::page(
        system.clone(),
        String::new(),
        6,
    ));
    assert_eq!(library["ok"], true, "{library}");
    assert!(
        library["value"]["documents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|document| document["title"] == "Legacy note")
    );
    let packages = value(CompositionHost::packages(system.clone()));
    assert_eq!(packages["ok"], true, "{packages}");
    assert!(
        packages["value"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|package| package["enabled"] == true)
    );

    assert!(scratch.0.join("core/core.sqlite").is_file());
    assert!(scratch.0.join("notes/core.sqlite").is_file());
    assert!(scratch.0.join("vocab/core.sqlite").is_file());

    let disabled = value(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.vocab".into(),
        false,
    ));
    assert_eq!(disabled["ok"], true, "{disabled}");
    let state = fs::read_to_string(scratch.0.join("extensions/preferences.json")).unwrap();
    assert_eq!(
        disabled["value"]["workspaces"].as_array().unwrap().len(),
        3,
        "state={state} response={disabled}"
    );
    assert!(scratch.0.join("extensions/preferences.json").is_file());
    let packages_after_disable = value(CompositionHost::packages(system.clone()));
    assert_eq!(
        packages_after_disable["value"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["id"] == "org.qusor.vocab")
            .unwrap()["enabled"],
        false
    );
    let notes_after_disable = value(NotesHost::page(system.clone()));
    assert_eq!(notes_after_disable["ok"], true, "{notes_after_disable}");
    let vocab_after_disable = value(VocabHost::decks(system.clone()));
    assert_eq!(vocab_after_disable["ok"], false, "{vocab_after_disable}");
    assert!(
        vocab_after_disable["error"]
            .as_str()
            .unwrap()
            .contains("vocabulary runtime is not mounted")
    );

    let mut restarted = System::default();
    restarted.data_dir = Some(scratch.0.clone());
    let restarted_catalog = value(CompositionHost::workspaces(restarted));
    assert_eq!(
        restarted_catalog["value"]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let enabled = value(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.vocab".into(),
        true,
    ));
    assert_eq!(enabled["ok"], true, "{enabled}");
    assert_eq!(enabled["value"]["workspaces"].as_array().unwrap().len(), 4);
    let decks_after_enable = value(VocabHost::decks(system.clone()));
    assert_eq!(decks_after_enable["ok"], true, "{decks_after_enable}");
    assert_eq!(
        decks_after_enable["value"]["decks"][0]["title"],
        "Legacy deck"
    );

    let core_disable = value(CompositionHost::set_package_enabled(
        system,
        "org.qusor.core".into(),
        false,
    ));
    assert_eq!(core_disable["ok"], false, "{core_disable}");
}
