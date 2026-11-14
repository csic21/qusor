#![cfg(unix)]
//! Real end-to-end integration test driving the compiled `reader.qpk` Rust Wasm
//! plugin (`org.qusor.reader.preview` / `org.qusor.reader.preview.workspace`)
//! through `PluginSlotHost` -> `WasmiSession` -> `Host::authorize_core_knowledge` ->
//! `OrthogonalKnowledgeBridge` -> real SQLite Core -> `ControlledSurfaceRef`.

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
use qusor_app_native::{composition_native::CompositionHost, plugin_slot::PluginSlotHost};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const PDF_TEXT: &str = "ReaderPdfE2ePassage.";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-plugin-slot-reader-e2e-{}-{}",
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
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("invalid JSON `{raw}`: {err}"))
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

fn seed_reader_documents(data_dir: &Path) -> (i64, i64) {
    let mut app = KnowledgeApplication::new(SqliteStore::open(data_dir.join("core")).unwrap());
    let provenance = Provenance {
        package_id: "org.qusor.library".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "e".repeat(64),
        installation_id: 1,
    };

    let pdf_original = b"%PDF-1.7\nReader E2E PDF fixture.\n".to_vec();
    let snapshot = pdf_snapshot(&pdf_original);
    let pdf = app
        .import_document(
            "e2e-reader-pdf-import",
            "qusor-vault",
            "E2E Reader PDF Handbook",
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
        end: 6,
        exact: "Reader".into(),
    };
    app.capture_pdf(
        "e2e-reader-pdf-cap",
        pdf.document_id,
        &pdf_draft,
        &provenance,
        "PDF page 1 note",
    )
    .unwrap();

    let epub_bytes = b"PK\x03\x04e2e-reader-epub-archive";
    let epub_snapshot = EpubSnapshot {
        vault: "qusor-vault".into(),
        title: "E2E Reader EPUB Book".into(),
        author: "Reader Author".into(),
        original_sha256: sha256(epub_bytes),
        original_bytes: epub_bytes.len() as u64,
        extraction_revision: "qusor-epub-rs-v1".into(),
        chapters: vec![
            EpubChapterSnapshot {
                chapter_index: 0,
                title: "Chapter 1: Intro".into(),
                href: "ch1.xhtml".into(),
                text: "Chapter one introduces the orthogonal reader architecture.".into(),
            },
            EpubChapterSnapshot {
                chapter_index: 1,
                title: "Chapter 2: Deep Dive".into(),
                href: "ch2.xhtml".into(),
                text: "Chapter two explores ControlledSurfaceRef in detail.".into(),
            },
        ],
        toc: vec![
            EpubTocEntry {
                title: "Chapter 1: Intro".into(),
                chapter_index: 0,
                depth: 0,
            },
            EpubTocEntry {
                title: "Chapter 2: Deep Dive".into(),
                chapter_index: 1,
                depth: 0,
            },
        ],
    };
    let epub_doc = app
        .import_epub("e2e-reader-epub-import", "qusor-vault", &epub_snapshot)
        .unwrap();
    app.capture_epub_selection(
        "e2e-reader-epub-cap",
        "qusor-vault",
        EpubSelectionRange {
            document_id: epub_doc.document_id,
            chapter_index: 1,
            start: 0,
            end: 30,
        },
        &provenance,
        "EPUB chapter 2 insight",
    )
    .unwrap();

    (pdf.document_id, epub_doc.document_id)
}

#[test]
fn real_reader_wasm_plugin_mounts_shelf_opens_controlled_surface_and_navigates_sections() {
    let scratch = Scratch::new();
    let (pdf_id, epub_id) = seed_reader_documents(&scratch.0);

    let mut system = System::default();
    system.data_dir = Some(scratch.0.clone());

    // 1. Mount real embedded `reader.qpk` via generic `PluginSlotHost`.
    let mounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.reader.preview.workspace".into(),
    ));
    assert_eq!(mounted["ok"], true, "{mounted}");
    let mount_id = mounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let generation = mounted["value"]["generation"].as_u64().unwrap();
    let shelf_tree = decode_tree(&mounted);
    let shelf_json = serde_json::to_string(&shelf_tree).unwrap();
    assert!(
        shelf_json.contains("E2E Reader PDF Handbook")
            && shelf_json.contains("E2E Reader EPUB Book"),
        "expected both PDF and EPUB documents on reader shelf: {shelf_json}"
    );

    // 2. Open EPUB document -> verify ControlledSurfaceRef(`reader-doc:epub:<id>:0`) and TOC.
    let opened_epub = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            &format!("doc_item_epub_{epub_id}"),
            &format!("open_doc:epub:{epub_id}"),
            UiAction::Press,
        ),
    ));
    assert_eq!(opened_epub["ok"], true, "{opened_epub}");
    let epub_tree = decode_tree(&opened_epub);
    assert!(
        has_surface_ref(
            &epub_tree.root,
            &format!("reader-doc:epub:{epub_id}:0"),
            ControlledSurfaceKind::ReaderDocument,
        ),
        "expected ControlledSurfaceRef for epub chapter 0"
    );
    let epub_view_json = serde_json::to_string(&epub_tree).unwrap();
    assert!(epub_view_json.contains("Chapter 2: Deep Dive"));

    // 3. Jump to TOC item 1 (Chapter 2) -> surface_id updates to `:1` and loads saved capture.
    let jumped = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "reader_toc_item_1",
            "jump_section:1",
            UiAction::Press,
        ),
    ));
    assert_eq!(jumped["ok"], true, "{jumped}");
    let jumped_tree = decode_tree(&jumped);
    assert!(
        has_surface_ref(
            &jumped_tree.root,
            &format!("reader-doc:epub:{epub_id}:1"),
            ControlledSurfaceKind::ReaderDocument,
        ),
        "expected ControlledSurfaceRef for epub chapter 1 after TOC jump"
    );
    let jumped_json = serde_json::to_string(&jumped_tree).unwrap();
    assert!(
        jumped_json.contains("EPUB chapter 2 insight"),
        "expected chapter 2 capture note in annotations card: {jumped_json}"
    );

    // 4. Return to shelf, then open PDF document -> verify `reader-doc:pdf:<id>:0`.
    let back = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            "reader_back_btn",
            "back_to_shelf",
            UiAction::Press,
        ),
    ));
    assert_eq!(back["ok"], true, "{back}");

    let opened_pdf = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id.clone(),
        envelope(
            &mount_id,
            generation,
            &format!("doc_item_pdf_{pdf_id}"),
            &format!("open_doc:pdf:{pdf_id}"),
            UiAction::Press,
        ),
    ));
    assert_eq!(opened_pdf["ok"], true, "{opened_pdf}");
    let pdf_tree = decode_tree(&opened_pdf);
    assert!(has_surface_ref(
        &pdf_tree.root,
        &format!("reader-doc:pdf:{pdf_id}:0"),
        ControlledSurfaceKind::ReaderDocument,
    ));

    // 5. Disabling `org.qusor.reader.preview` unmounts active slot and blocks remount.
    let disabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.reader.preview".into(),
        false,
    ));
    assert_eq!(disabled["ok"], true, "{disabled}");
    let after_disable = parse_json(PluginSlotHost::dispatch_event(
        system.clone(),
        mount_id,
        envelope(
            "stale",
            generation,
            "reader_back_btn",
            "back_to_shelf",
            UiAction::Press,
        ),
    ));
    assert_eq!(after_disable["ok"], false, "{after_disable}");

    let reenabled = parse_json(CompositionHost::set_package_enabled(
        system.clone(),
        "org.qusor.reader.preview".into(),
        true,
    ));
    assert_eq!(reenabled["ok"], true, "{reenabled}");
    let remounted = parse_json(PluginSlotHost::mount_slot(
        system.clone(),
        "org.qusor.reader.preview.workspace".into(),
    ));
    assert_eq!(remounted["ok"], true, "{remounted}");
    let remount_id = remounted["value"]["mount_id"].as_str().unwrap().to_owned();
    let unmounted = parse_json(PluginSlotHost::unmount_slot(system, remount_id));
    assert_eq!(unmounted["ok"], true, "{unmounted}");
}
