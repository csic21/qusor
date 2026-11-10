//! Seed a local Core database for the notes workbench window.
//! Trusted experiment input: synthetic snapshot and three saved Chinese notes.
use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use learning_application::KnowledgeApplication;
use serde_json::json;
use std::{env, fs, os::unix::fs::PermissionsExt, path::Path, process::ExitCode};
use storage_sqlite::SqliteStore;

const TEXT: &str = "插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。";

fn snapshot(original: &[u8]) -> PdfSnapshot {
    let mut characters = Vec::new();
    let mut utf16 = 0usize;
    for (index, (byte, character)) in TEXT.char_indices().enumerate() {
        let utf8_end = byte + character.len_utf8();
        let wide = character.len_utf16();
        characters.push(json!({
            "pdf_char_index": index,
            "unicode_value": character as u32,
            "mapping": "scalar",
            "generated": false,
            "scalar_start": index,
            "scalar_end": index + 1,
            "utf8_start": byte,
            "utf8_end": utf8_end,
            "utf16_start": utf16,
            "utf16_end": utf16 + wide,
            "pdf_bounds": {"left": index as f32, "bottom": 1.0,
                           "right": index as f32 + 1.0, "top": 2.0},
        }));
        utf16 += wide;
    }
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

fn selection(snapshot: &PdfSnapshot, start: usize, end: usize) -> PdfDraft {
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

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let data = arguments
        .next()
        .ok_or("usage: seed_workbench_notes DATA_DIR RECEIPT")?;
    let receipt_path = arguments.next().ok_or("receipt path required")?;
    let data = Path::new(&data);
    fs::create_dir_all(data).map_err(|error| error.to_string())?;
    fs::set_permissions(data, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let original = b"%PDF-1.7\nWorkbench notes seed fixture.\n".to_vec();
    let snapshot = snapshot(&original);
    let mut app =
        KnowledgeApplication::new(SqliteStore::open(data).map_err(|error| error.to_string())?);
    let imported = app
        .import_document(
            "seed-workbench-import",
            "workbench-vault",
            "准原生应用的秩序",
            &original,
            &snapshot,
        )
        .map_err(|error| error.to_string())?;
    let origin = Provenance {
        package_id: "org.qusor.notes".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    };
    let count = TEXT.chars().count();
    let notes = [
        (
            selection(&snapshot, 0, count),
            "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
        ),
        (
            selection(&snapshot, 10, count),
            "阅读时不让表单常驻。只有选文后，才把保存和想法动作放到正文附近。工具应该像书架一样，需要时出现，不需要时退到一旁；但退到一旁不等于消失，用户仍要能从工作区身份和返回路径知道自己在哪。列表用于浏览，详情用于阅读与复制，两者不要同时抢注意力。复制出来的引用必须带上资料标题与页码，否则离开应用之后就无法再核对来源。",
        ),
        (
            selection(&snapshot, 0, 6),
            "共同的是工作区，变化的是看待内容的方式。",
        ),
    ];
    let mut capture_ids = Vec::new();
    for (index, (draft, note)) in notes.iter().enumerate() {
        capture_ids.push(
            app.capture_pdf(
                &format!("seed-workbench-capture-{index}"),
                imported.document_id,
                draft,
                &origin,
                note,
            )
            .map_err(|error| error.to_string())?
            .capture_id,
        );
    }
    let receipt = json!({
        "vault": "workbench-vault",
        "document_id": imported.document_id,
        "capture_ids": capture_ids,
        "notes": notes.len(),
    });
    fs::write(
        receipt_path,
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .map_err(|error| error.to_string())?;
    println!("{receipt}");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("seed_workbench_notes: {error}");
            ExitCode::FAILURE
        }
    }
}
