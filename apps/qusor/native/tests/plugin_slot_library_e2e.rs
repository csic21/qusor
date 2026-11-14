#![cfg(unix)]
//! Real end-to-end integration test driving the compiled `library.qpk` Rust Wasm
//! plugin through `PluginSlotHost` -> `WasmiSession` -> `Host::authorize_*` ->
//! `OrthogonalKnowledgeBridge` -> real SQLite Core -> `PluginUiHost` generation fencing.

use core_api::{
    contract::sha256,
    epub_contract::{EpubChapterSnapshot, EpubSnapshot, EpubTocEntry},
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::ui::{
    ControlledSurfaceKind, PLUGIN_UI_PROTOCOL, UiAction, UiBudgets, UiComponent, UiEventEnvelope,
    UiNode, UiTree,
};
use learning_application::{KnowledgeApplication, epub_library::EpubSelectionRange};
use nsh_core::prelude::System;
use qusor_app_native::{
    app_info::AppInfo, composition_native::CompositionHost, plugin_slot::PluginSlotHost,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const PDF_TEXT: &str = "LibraryPdfE2ePassage.";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-plugin-slot-library-e2e-{}-{}",
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

fn press_envelope(mount_id: &str, generation: u64, node_id: &str, action_id: &str) -> String {
    serde_json::to_string(&UiEventEnvelope {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: mount_id.to_owned(),
        generation,
        node_id: node_id.to_owned(),
        action_id: action_id.to_owned(),
        action: UiAction::Press,
    })
    .unwrap()
}

fn tab_envelope(mount_id: &str, generation: u64, tab_id: &str) -> String {
    serde_json::to_string(&UiEventEnvelope {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: mount_id.to_owned(),
        generation,
        node_id: "library_format_tabs".to_owned(),
        action_id: "filter_format".to_owned(),
        action: UiAction::TabChanged {
            tab_id: tab_id.to_owned(),
        },
    })
    .unwrap()
}

fn decode_tree(response: &Value) -> UiTree {
    let bytes = serde_json::to_vec(&response["value"]["tree"]).unwrap();
    UiTree::from_slice_with_budgets(&bytes, UiBudgets::default()).unwrap()
}

fn has_surface_ref(node: &UiNode, expected_id: &str, expected_kind: ControlledSurfaceKind) -> bool {
    let hit = matches!(
        &node.component,
        UiComponent::ControlledSurfaceRef {
            surface_id,
            surface_kind,
        } if surface_id == expected_id && *surface_kind == expected_kind
    );
    hit || node
        .children
        .iter()
        .any(|child| has_surface_ref(child, expected_id, expected_kind))
}

fn has_list_item(node: &UiNode, title_needle: &str) -> bool {
    let hit = matches!(
        &node.component,
        UiComponent::ListItem { title, .. } if title.contains(title_needle)
    );
    hit || node
        .children
        .iter()
        .any(|child| has_list_item(child, title_needle))
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

fn seed_library_documents(data_dir: &Path) -> (i64, i64, i64) {
    let mut app = KnowledgeApplication::new(SqliteStore::open(data_dir.join("core")).unwrap());
    let provenance = Provenance {
        package_id: "org.qusor.library".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "c".repeat(64),
        installation_id: 1,
    };

    let pdf_original = b"%PDF-1.7\nLibrary E2E PDF fixture.\n".to_vec();
    let snapshot = pdf_snapshot(&pdf_original);
    let pdf = app
        .import_document(
            "e2e-lib-pdf-import",
            "qusor-vault",
            "E2E Library PDF",
            &pdf_original,
            &snapshot,
        )
        .unwrap();
    let pdf_draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(PDF_TEXT.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 7,
        exact: "Library".into(),
    };
    let _pdf_cap = app
        .capture_pdf(
            "e2e-lib-pdf-cap",
            pdf.document_id,
            &pdf_draft,
            &provenance,
            "PDF E2E highlight",
        )
        .unwrap();

    let epub_bytes = b"PK\x03\x04e2e-epub-archive";
    let epub_snapshot = EpubSnapshot {
        vault: "qusor-vault".into(),
        title: "E2E Library EPUB".into(),
        author: "QUSOR".into(),
        original_sha256: sha256(epub_bytes),
        original_bytes: epub_bytes.len() as u64,
        extraction_revision: "qusor-epub-rs-v1".into(),
        chapters: vec![
            EpubChapterSnapshot {
                chapter_index: 0,
                title: "Chapter One".into(),
                href: "ch1.xhtml".into(),
                text: "Orthogonal knowledge plugins stay isolated.".into(),
            },
            EpubChapterSnapshot {
                chapter_index: 1,
                title: "Chapter Two".into(),
                href: "ch2.xhtml".into(),
                text: "Controlled surfaces render safely.".into(),
            },
        ],
        toc: vec![
            EpubTocEntry {
                title: "Chapter One".into(),
                chapter_index: 0,
                depth: 0,
            },
            EpubTocEntry {
                title: "Chapter Two".into(),
                chapter_index: 1,
                depth: 0,
            },
        ],
    };
    let epub = app
        .import_epub("e2e-lib-epub-import", "qusor-vault", &epub_snapshot)
        .unwrap();
    let epub_cap = app
        .capture_epub_selection(
            "e2e-lib-epub-cap",
            "qusor-vault",
            EpubSelectionRange {
                document_id: epub.document_id,
                chapter_index: 0,
                start: 0,
                end: 10,
            },
            &provenance,
            "EPUB E2E highlight",
        )
        .unwrap();

    (pdf.document_id, epub.document_id, epub_cap.capture_id)
}

#[test]
fn real_library_wasm_plugin_lists_opens_controlled_surface_and_deletes_in_sqlite() {
    let scratch = Scratch::new();
    let (pdf_id, epub_id, epub_capture_id) = seed_library_documents(&scratch.0);

    let mut system = System::default();
    system.data_dir = Some(scratch.0.clone());

    // 1. Mount real embedded `library.qpk` via generic `PluginSlotHost`.
    let mounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.library.workspace".into(),
    ));
    assert_eq!(mounted["ok"], true, "{mounted}");
    let mount_id = mounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let generation = mounted["value"]["generation"].as_u64().unwrap();
    let initial_tree = decode_tree(&mounted);
    assert!(has_list_item(&initial_tree.root, "E2E Library PDF"));
    assert!(has_list_item(&initial_tree.root, "E2E Library EPUB"));
    let initial_text = serde_json::to_string(&initial_tree).unwrap();
    assert!(initial_text.contains("host_import_pdf"), "{initial_text}");
    assert!(initial_text.contains("host_import_epub"), "{initial_text}");

    // 2. Open EPUB document -> emits ControlledSurfaceRef("library-doc:epub:<id>:0") + TOC + saved selection.
    let opened_epub = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation,
            &format!("doc_item_epub_{epub_id}"),
            &format!("open_doc:epub:{epub_id}"),
        ),
    ));
    assert_eq!(opened_epub["ok"], true, "{opened_epub}");
    let epub_tree = decode_tree(&opened_epub);
    assert!(
        has_surface_ref(
            &epub_tree.root,
            &format!("library-doc:epub:{epub_id}:0"),
            ControlledSurfaceKind::ReaderDocument,
        ),
        "expected EPUB ControlledSurfaceRef in tree: {opened_epub}"
    );
    let epub_text = serde_json::to_string(&epub_tree).unwrap();
    assert!(epub_text.contains("Chapter One"), "{epub_text}");
    assert!(epub_text.contains("Chapter Two"), "{epub_text}");
    assert!(
        epub_text.contains(&format!("摘录 #{epub_capture_id}")),
        "expected saved selection #{epub_capture_id} in tree: {epub_text}"
    );

    // 3. Switch to chapter 1 -> ControlledSurfaceRef updates to section_index 1.
    let next_sec = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "next_section_btn", "next_section"),
    ));
    assert_eq!(next_sec["ok"], true, "{next_sec}");
    let sec1_tree = decode_tree(&next_sec);
    assert!(has_surface_ref(
        &sec1_tree.root,
        &format!("library-doc:epub:{epub_id}:1"),
        ControlledSurfaceKind::ReaderDocument,
    ));

    // 4. Return to list, filter by PDF tab, open PDF document -> ControlledSurfaceRef("library-doc:pdf:<id>:0").
    let back = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "detail_back_btn", "back_to_list"),
    ));
    assert_eq!(back["ok"], true, "{back}");

    let filtered_pdf = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        tab_envelope(&mount_id, generation, "pdf"),
    ));
    assert_eq!(filtered_pdf["ok"], true, "{filtered_pdf}");
    let filtered_tree = decode_tree(&filtered_pdf);
    assert!(has_list_item(&filtered_tree.root, "E2E Library PDF"));
    assert!(!has_list_item(&filtered_tree.root, "E2E Library EPUB"));

    let opened_pdf = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation,
            &format!("doc_item_pdf_{pdf_id}"),
            &format!("open_doc:pdf:{pdf_id}"),
        ),
    ));
    assert_eq!(opened_pdf["ok"], true, "{opened_pdf}");
    let pdf_tree = decode_tree(&opened_pdf);
    assert!(has_surface_ref(
        &pdf_tree.root,
        &format!("library-doc:pdf:{pdf_id}:0"),
        ControlledSurfaceKind::ReaderDocument,
    ));

    // 5. Two-step DeleteDocument: step 1 prompts without mutating SQLite; step 2 deletes in SQLite.
    let req_del = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "detail_delete_btn", "request_delete"),
    ));
    assert_eq!(req_del["ok"], true, "{req_del}");
    assert!(req_del["value"]["tree"].to_string().contains("确认删除"));

    let backup_before = parse_json(AppInfo::export_backup(system.clone()));
    assert_eq!(backup_before["ok"], true, "{backup_before}");
    let before_doc: Value = serde_json::from_str(
        &fs::read_to_string(backup_before["value"]["path"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        before_doc["knowledge"]["pdf_documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let confirm_del = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation,
            "confirm_delete_btn",
            "confirm_delete",
        ),
    ));
    assert_eq!(confirm_del["ok"], true, "{confirm_del}");
    let after_del_tree = decode_tree(&confirm_del);
    assert!(!has_list_item(&after_del_tree.root, "E2E Library PDF"));

    let backup_after = parse_json(AppInfo::export_backup(system.clone()));
    assert_eq!(backup_after["ok"], true, "{backup_after}");
    let after_doc: Value = serde_json::from_str(
        &fs::read_to_string(backup_after["value"]["path"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        after_doc["knowledge"]["pdf_documents"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "PDF document should be deleted from SQLite Core: {after_doc}"
    );
    assert_eq!(
        after_doc["knowledge"]["epub_documents"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "EPUB document should remain in SQLite Core: {after_doc}"
    );

    // 6. Reject stale generation events (generation fencing).
    let stale = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(
            &mount_id,
            generation.saturating_sub(1),
            "refresh_docs_btn",
            "refresh_docs",
        ),
    ));
    assert_eq!(stale["ok"], false, "{stale}");

    // 7. Disabling package immediately unmounts live slot & blocks remount.
    let disabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.library".into(),
        false,
    ));
    assert_eq!(disabled["ok"], true, "{disabled}");
    let after_disable = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        press_envelope(&mount_id, generation, "refresh_docs_btn", "refresh_docs"),
    ));
    assert_eq!(after_disable["ok"], false, "{after_disable}");
    let remount_disabled = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.library.workspace".into(),
    ));
    assert_eq!(remount_disabled["ok"], false, "{remount_disabled}");

    // 8. Re-enabling lets a fresh slot remount and still sees persisted SQLite state (only EPUB remains).
    let reenabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.library".into(),
        true,
    ));
    assert_eq!(reenabled["ok"], true, "{reenabled}");
    let remounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.library.workspace".into(),
    ));
    assert_eq!(remounted["ok"], true, "{remounted}");
    let remount_mount_id = remounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let remount_tree = decode_tree(&remounted);
    assert!(has_list_item(&remount_tree.root, "E2E Library EPUB"));
    assert!(!has_list_item(&remount_tree.root, "E2E Library PDF"));

    let unmounted = parse_json(PluginSlotHost::unmount_slot(system, remount_mount_id));
    assert_eq!(unmounted["ok"], true, "{unmounted}");
}
