//! The workbench notes module: ordinary package + public notes.read wire over a
//! seeded real Core database, verified without a window.
use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::{Manifest, SUPPORTED_HOST_APIS};
use extension_package::package::{PackageFile, build_signed_for_host, publisher_key};
use learning_application::KnowledgeApplication;
use qusor_nsh_native_host_probe::notes_native::{self, NotesHost};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const TEXT: &str = "插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。";
const SEED: [u8; 32] = [0x33; 32];
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-notes-native-{}-{}",
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
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn snapshot(original: &[u8]) -> PdfSnapshot {
    let characters = TEXT
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let byte = TEXT
                .char_indices()
                .nth(index)
                .map(|(byte, _)| byte)
                .unwrap();
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
                "pdf_bounds": {"left": index as f32, "bottom": 1.0,
                               "right": index as f32 + 1.0, "top": 2.0},
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
            "media_box": {"left": 0.0, "bottom": 0.0, "right": 500.0, "top": 300.0},
            "crop_box": {"left": 0.0, "bottom": 0.0, "right": 500.0, "top": 300.0},
            "characters": characters,
        }],
    }))
    .unwrap()
}

fn draft(snapshot: &PdfSnapshot, start: usize, end: usize) -> PdfDraft {
    PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(TEXT.as_bytes()),
        unit: PDF_TEXT_UNIT.into(),
        start: start as u64,
        end: end as u64,
        exact: TEXT.chars().skip(start).take(end - start).collect(),
    }
}

fn seed_core(scratch: &Scratch) -> i64 {
    let original = b"%PDF-1.7\nNative notes module fixture.\n".to_vec();
    let snapshot = snapshot(&original);
    let mut app = KnowledgeApplication::new(SqliteStore::open(scratch.0.join("core")).unwrap());
    let imported = app
        .import_document(
            "import",
            "workbench-vault",
            "准原生应用的秩序",
            &original,
            &snapshot,
        )
        .unwrap();
    let origin = Provenance {
        package_id: "org.qusor.notes".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    };
    let count = TEXT.chars().count();
    for (index, (start, end, note)) in [
        (0, count, "第一条本机想法。"),
        (6, count, "第二条本机想法，用来验证有界预览与分块读取。工具应该像书架一样，需要时出现，不需要时退到一旁；但退到一旁不等于消失，用户仍要能从工作区身份和返回路径知道自己在哪。列表用于浏览，详情用于阅读与复制，两者不要同时抢注意力。复制出来的引用必须带上资料标题与页码，否则离开应用之后就无法再核对来源。"),
    ]
    .into_iter()
    .enumerate()
    {
        app.capture_pdf(
            &format!("capture-{index}"),
            imported.document_id,
            &draft(&snapshot, start, end),
            &origin,
            note,
        )
        .unwrap();
    }
    imported.document_id
}

fn signed_package(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../plugins/notes");
    let manifest = Manifest::from_slice(&fs::read(plugins.join("manifest.json")).unwrap()).unwrap();
    let wasm = wat::parse_str(fs::read_to_string(plugins.join("guest.wat")).unwrap()).unwrap();
    let bytes = build_signed_for_host(
        manifest,
        "notes.wasm",
        vec![PackageFile {
            path: "notes.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &SEED,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    let package = scratch.0.join("notes.qpk");
    let key = scratch.0.join("notes-key.txt");
    fs::write(&package, bytes).unwrap();
    fs::write(&key, publisher_key(&SEED)).unwrap();
    (package, key)
}

#[test]
fn the_workbench_module_serves_a_real_core_page_and_body() {
    let scratch = Scratch::new();
    let document = seed_core(&scratch);
    let (package, key) = signed_package(&scratch);
    notes_native::initialize(&scratch.0.join("core"), &package, &key, document).unwrap();

    let page: Value = serde_json::from_str(&NotesHost::page()).unwrap();
    assert_eq!(page["ok"], true, "{page}");
    assert_eq!(page["value"]["current_document_id"], document);
    let notes = page["value"]["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0]["document_title"], "准原生应用的秩序");
    assert_eq!(notes[1]["capture_id"], 1);
    assert!(notes[0]["note_preview"].as_str().unwrap().chars().count() <= 96);
    assert!(notes[0]["note_truncated"].as_bool().unwrap());

    let capture_id = notes[0]["capture_id"].as_i64().unwrap();
    let body: Value =
        serde_json::from_str(&NotesHost::body(capture_id, "note".into(), 0, 16)).unwrap();
    assert_eq!(body["ok"], true, "{body}");
    assert!(!body["value"]["text"].as_str().unwrap().is_empty());
    assert!(body["value"]["next_offset"].as_u64().unwrap() <= 16);

    // An unsupported field cannot widen the wire.
    let refused: Value =
        serde_json::from_str(&NotesHost::body(capture_id, "title".into(), 0, 16)).unwrap();
    assert_eq!(refused["ok"], false);
}
