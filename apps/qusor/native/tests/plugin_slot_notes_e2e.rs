#![cfg(unix)]
//! Real end-to-end integration test driving the compiled `notes.qpk` Rust Wasm
//! plugin through `PluginSlotHost` -> `WasmiSession` -> `Host::authorize_core_knowledge`
//! -> `OrthogonalKnowledgeBridge` -> real SQLite Core -> `PluginUiHost` generation fencing.

use core_api::{
    contract::sha256,
    epub_contract::{EpubChapterSnapshot, EpubSnapshot},
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::ui::{PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiEventEnvelope, UiTree};
use learning_application::{KnowledgeApplication, epub_library::EpubSelectionRange};
use nsh_core::prelude::System;
use qusor_app_native::{
    app_info::AppInfo, composition_native::CompositionHost, plugin_slot::PluginSlotHost,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

static NEXT: AtomicU64 = AtomicU64::new(0);
const PDF_TEXT: &str = "Local-first storage keeps user notes independent.";
const EPUB_TEXT: &str = "Alpha Orthogonal capabilities decouple read from write.";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-plugin-slot-notes-e2e-{}-{}",
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

fn parse_json(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap()
}

fn envelope(
    mount_id: &str,
    generation: u64,
    node_id: &str,
    action_id: &str,
    action: UiAction,
) -> String {
    serde_json::to_string(&UiEventEnvelope {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: mount_id.to_owned(),
        generation,
        node_id: node_id.to_owned(),
        action_id: action_id.to_owned(),
        action,
    })
    .unwrap()
}

fn pdf_snapshot(original: &[u8]) -> PdfSnapshot {
    let characters = PDF_TEXT
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let byte = PDF_TEXT.char_indices().nth(index).unwrap().0;
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
                "pdf_bounds": if character.is_whitespace() {
                    Value::Null
                } else {
                    json!({
                        "left": index as f32,
                        "bottom": 1.0,
                        "right": index as f32 + 1.0,
                        "top": 2.0
                    })
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
            "engine_text": PDF_TEXT,
            "text_sha256": sha256(PDF_TEXT.as_bytes()),
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

fn seed_pdf_and_epub_captures(data_dir: &std::path::Path) -> (i64, i64) {
    let mut app = KnowledgeApplication::new(SqliteStore::open(data_dir.join("core")).unwrap());
    let provenance = Provenance {
        package_id: "org.qusor.notes".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    };

    let pdf_bytes = b"%PDF-1.7\nE2E Notes PDF fixture.\n".to_vec();
    let pdf_snap = pdf_snapshot(&pdf_bytes);
    let pdf_doc = app
        .import_document(
            "e2e-notes-pdf-import",
            "qusor-vault",
            "E2E PDF Spec",
            &pdf_bytes,
            &pdf_snap,
        )
        .unwrap();
    let pdf_draft = PdfDraft {
        protocol_version: 1,
        original_sha256: pdf_snap.original_sha256.clone(),
        extraction_revision: pdf_snap.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(PDF_TEXT.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: PDF_TEXT.chars().count() as u64,
        exact: PDF_TEXT.into(),
    };
    let pdf_cap = app
        .capture_pdf(
            "e2e-notes-pdf-cap",
            pdf_doc.document_id,
            &pdf_draft,
            &provenance,
            "Initial PDF note",
        )
        .unwrap();

    let epub_snap = EpubSnapshot {
        vault: "qusor-vault".into(),
        title: "E2E EPUB Guide".into(),
        author: "Tester".into(),
        original_sha256: sha256(b"e2e-notes-epub-bytes"),
        original_bytes: 20,
        extraction_revision: "qusor-epub-rs-v1".into(),
        chapters: vec![EpubChapterSnapshot {
            chapter_index: 0,
            title: "Chapter 1".into(),
            href: "ch1.xhtml".into(),
            text: EPUB_TEXT.into(),
        }],
        toc: vec![],
    };
    let epub_doc = app
        .import_epub("e2e-notes-epub-import", "qusor-vault", &epub_snap)
        .unwrap();
    let epub_cap = app
        .capture_epub_selection(
            "e2e-notes-epub-cap",
            "qusor-vault",
            EpubSelectionRange {
                document_id: epub_doc.document_id,
                chapter_index: 0,
                start: 6,
                end: 55,
            },
            &provenance,
            "Initial EPUB note",
        )
        .unwrap();

    (pdf_cap.capture_id, epub_cap.capture_id)
}

#[test]
fn real_notes_wasm_plugin_queries_updates_deletes_and_persists_to_sqlite() {
    let scratch = Scratch::new();
    let (pdf_cap_id, epub_cap_id) = seed_pdf_and_epub_captures(&scratch.0);

    let mut system = System::default();
    system.data_dir = Some(scratch.0.clone());

    // 1. Mount real embedded `notes.qpk` via generic `PluginSlotHost`.
    let mounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.notes.workspace".into(),
    ));
    assert_eq!(mounted["ok"], true, "{mounted}");
    let mount_id = mounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let generation = mounted["value"]["generation"].as_u64().unwrap();
    let initial_tree_bytes = serde_json::to_vec(&mounted["value"]["tree"]).unwrap();
    let initial_tree =
        UiTree::from_slice_with_budgets(&initial_tree_bytes, UiBudgets::default()).unwrap();
    let initial_text = serde_json::to_string(&initial_tree).unwrap();
    assert!(
        initial_text.contains("E2E PDF Spec") && initial_text.contains("E2E EPUB Guide"),
        "expected mixed PDF + EPUB notes in initial UiTree: {initial_text}"
    );
    assert!(
        initial_text.contains(&format!("select_note:pdf:{pdf_cap_id}"))
            && initial_text.contains(&format!("select_note:epub:{epub_cap_id}")),
        "expected selection actions for both captures: {initial_text}"
    );

    // 2. Scope tab switch to PDF filters out EPUB, then switch back to All.
    let pdf_tab = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "notes_scope_tabs",
            "switch_scope",
            UiAction::TabChanged {
                tab_id: "pdf".into(),
            },
        ),
    ));
    assert_eq!(pdf_tab["ok"], true, "{pdf_tab}");
    let pdf_tab_text = pdf_tab["value"]["tree"].to_string();
    assert!(pdf_tab_text.contains("E2E PDF Spec"), "{pdf_tab_text}");
    assert!(!pdf_tab_text.contains("E2E EPUB Guide"), "{pdf_tab_text}");

    let all_tab = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "notes_scope_tabs",
            "switch_scope",
            UiAction::TabChanged {
                tab_id: "all".into(),
            },
        ),
    ));
    assert_eq!(all_tab["ok"], true, "{all_tab}");

    // 3. Select EPUB note -> loads separate immutable quote and editable user note via NoteBody.
    let selected_epub = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            &format!("note_item_epub_{epub_cap_id}"),
            &format!("select_note:epub:{epub_cap_id}"),
            UiAction::Press,
        ),
    ));
    assert_eq!(selected_epub["ok"], true, "{selected_epub}");
    let selected_text = selected_epub["value"]["tree"].to_string();
    assert!(
        selected_text.contains("Orthogonal capabilities decouple read from write."),
        "{selected_text}"
    );
    assert!(
        selected_text.contains("Initial EPUB note"),
        "{selected_text}"
    );

    // 4. Edit draft and submit UpdateCaptureNote -> persists into SQLite Core.
    let drafted = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "note_draft_input",
            "draft_changed",
            UiAction::TextChanged {
                value: "Updated E2E EPUB insight".into(),
            },
        ),
    ));
    assert_eq!(drafted["ok"], true, "{drafted}");

    let saved = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "save_note_btn",
            "save_note",
            UiAction::Press,
        ),
    ));
    assert_eq!(saved["ok"], true, "{saved}");
    let saved_tree_bytes = serde_json::to_vec(&saved["value"]["tree"]).unwrap();
    let saved_tree =
        UiTree::from_slice_with_budgets(&saved_tree_bytes, UiBudgets::default()).unwrap();
    let saved_text = serde_json::to_string(&saved_tree).unwrap();
    assert!(saved_text.contains("已保存个人想法"), "{saved_text}");
    assert!(
        saved_text.contains("Updated E2E EPUB insight"),
        "{saved_text}"
    );

    // 5. Select PDF note and execute two-step DeleteCapture confirmation.
    let selected_pdf = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            &format!("note_item_pdf_{pdf_cap_id}"),
            &format!("select_note:pdf:{pdf_cap_id}"),
            UiAction::Press,
        ),
    ));
    assert_eq!(selected_pdf["ok"], true, "{selected_pdf}");

    let ask_delete = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "delete_note_btn",
            "request_delete",
            UiAction::Press,
        ),
    ));
    assert_eq!(ask_delete["ok"], true, "{ask_delete}");
    assert!(
        ask_delete["value"]["tree"]
            .to_string()
            .contains("confirm_delete")
    );

    let confirmed_delete = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "confirm_delete_btn",
            "confirm_delete",
            UiAction::Press,
        ),
    ));
    assert_eq!(confirmed_delete["ok"], true, "{confirmed_delete}");
    let after_delete_text = confirmed_delete["value"]["tree"].to_string();
    assert!(
        !after_delete_text.contains("E2E PDF Spec"),
        "deleted PDF capture should leave notes list: {after_delete_text}"
    );
    assert!(
        after_delete_text.contains("E2E EPUB Guide"),
        "EPUB note should remain: {after_delete_text}"
    );

    // 6. Verify SQLite Core persistence via backup export.
    let backup = parse_json(AppInfo::export_backup(system.clone()));
    assert_eq!(backup["ok"], true, "{backup}");
    let backup_path = backup["value"]["path"].as_str().unwrap();
    let backup_doc: Value =
        serde_json::from_str(&fs::read_to_string(backup_path).unwrap()).unwrap();
    let pdf_captures = backup_doc["knowledge"]["pdf_captures"].as_array().unwrap();
    assert!(
        pdf_captures.is_empty(),
        "expected deleted PDF capture removed from SQLite Core, got {pdf_captures:?}"
    );
    let epub_captures = backup_doc["knowledge"]["epub_captures"].as_array().unwrap();
    assert_eq!(epub_captures.len(), 1);
    assert_eq!(epub_captures[0]["user_note"], "Updated E2E EPUB insight");

    // 7. Reject stale generation events (generation fencing).
    let stale = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation.saturating_sub(1),
            "notes_refresh_btn",
            "refresh_notes",
            UiAction::Press,
        ),
    ));
    assert_eq!(stale["ok"], false, "{stale}");

    // 8. Disabling the package immediately unmounts its live slot and blocks re-mount.
    let disabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.notes".into(),
        false,
    ));
    assert_eq!(disabled["ok"], true, "{disabled}");
    let after_disable = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "notes_refresh_btn",
            "refresh_notes",
            UiAction::Press,
        ),
    ));
    assert_eq!(after_disable["ok"], false, "{after_disable}");
    let remount_while_disabled = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.notes.workspace".into(),
    ));
    assert_eq!(
        remount_while_disabled["ok"], false,
        "{remount_while_disabled}"
    );

    // 9. Re-enabling mounts a fresh Wasm instance that reads persisted Core state.
    let reenabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.notes".into(),
        true,
    ));
    assert_eq!(reenabled["ok"], true, "{reenabled}");
    let remounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.notes.workspace".into(),
    ));
    assert_eq!(remounted["ok"], true, "{remounted}");
    let remount_mount_id = remounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let remount_text = remounted["value"]["tree"].to_string();
    assert!(
        remount_text.contains("Updated E2E EPUB insight") && !remount_text.contains("E2E PDF Spec"),
        "remounted notes plugin should reflect persisted Core updates and deletions: {remount_text}"
    );

    let unmounted = parse_json(PluginSlotHost::unmount_slot(system, remount_mount_id));
    assert_eq!(unmounted["ok"], true, "{unmounted}");
}
