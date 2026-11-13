//! Trusted PDF import and page-image rendering for the native Library surface.
//!
//! User-selected bytes never cross into a plugin. This module writes one
//! private job directory, asks the pinned PDFium provider for a bounded report,
//! commits the immutable original and semantic snapshot through Core, and can
//! later render a page from the retained original as native `ImageData`.

use crate::pdf_page::PdfPageData;
use crate::vault::VaultStore;
use core_api::contract::sha256;
use core_api::pdf_contract::PdfSnapshot;
use learning_application::KnowledgeApplication;
use nsh_core::{ImageData, element::System};
use qusor_pdf_provider::{DocumentReport, MAX_PDF_BYTES, PageReport};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PDFIUM_VERSION: &str = "chromium/8125";
const MAX_TITLE_BYTES: usize = 1024;
const MAX_GEOMETRY_BYTES: usize = 512 * 1024;
const MAX_GEOMETRY_GLYPHS: usize = 20_000;
static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct PdfImport {
    pub document_id: i64,
    pub title: String,
    pub page_count: u32,
}

struct JobDirectory(PathBuf);

impl JobDirectory {
    fn create(sys: &System) -> Result<Self, String> {
        let data = sys
            .data_dir
            .as_ref()
            .ok_or_else(|| "the host gave no app data directory".to_string())?;
        let jobs = data.join("pdf-jobs");
        fs::create_dir_all(&jobs).map_err(text)?;
        for _ in 0..8 {
            let sequence = JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(text)?
                .as_nanos();
            let path = jobs.join(format!("{}-{time}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not allocate a private PDF job directory".into())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for JobDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RuntimeLibrary {
    path: PathBuf,
    sha256: String,
    verify_file: bool,
}

fn asset_candidates(file: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("QUSOR_PDFIUM_LIBRARY") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.push(directory.join("assets").join("pdfium").join(file));
        candidates.push(
            directory
                .join("..")
                .join("Resources")
                .join("nsh")
                .join("assets")
                .join("pdfium")
                .join(file),
        );
        candidates.push(
            directory
                .join("nsh")
                .join("assets")
                .join("pdfium")
                .join(file),
        );
    }
    #[cfg(debug_assertions)]
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("pdfium")
            .join(file),
    );
    candidates
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn runtime_library() -> Result<RuntimeLibrary, String> {
    const SHA256: &str = "a45c44645523968525c710462691dd974f4efb1013ce1c249919a0939cfb373a";
    let candidates = asset_candidates("macos-aarch64/libpdfium.dylib");
    let path = candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or_else(|| format!("PDFium {PDFIUM_VERSION} macOS arm64 runtime is missing"))?;
    Ok(RuntimeLibrary {
        path,
        sha256: SHA256.into(),
        verify_file: true,
    })
}

#[cfg(all(target_os = "android", target_arch = "aarch64"))]
fn runtime_library() -> Result<RuntimeLibrary, String> {
    const SHA256: &str = "de19db1c0fb33579bca6ace903af7ccb9d03ad1d3f6c5202970d8d79d7538303";
    Ok(RuntimeLibrary {
        path: PathBuf::from("libpdfium.so"),
        sha256: SHA256.into(),
        verify_file: false,
    })
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "android", target_arch = "aarch64")
)))]
fn runtime_library() -> Result<RuntimeLibrary, String> {
    Err(format!(
        "PDFium {PDFIUM_VERSION} runtime is not packaged for this platform yet"
    ))
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn write_original(job: &JobDirectory, bytes: &[u8]) -> Result<PathBuf, String> {
    if bytes.is_empty() || bytes.len() > MAX_PDF_BYTES || !bytes.starts_with(b"%PDF-") {
        return Err("the selected file is not a PDF within the 16 MiB import budget".into());
    }
    let path = job.path().join("original.pdf");
    let mut file = fs::File::create(&path).map_err(text)?;
    file.write_all(bytes).map_err(text)?;
    file.sync_all().map_err(text)?;
    Ok(path)
}

fn inspect(
    job: &JobDirectory,
    original: &Path,
) -> Result<(DocumentReport, RuntimeLibrary), String> {
    let library = runtime_library()?;
    if library.verify_file {
        let bytes = fs::read(&library.path).map_err(text)?;
        if sha256(&bytes) != library.sha256 {
            return Err("the packaged PDFium runtime does not match its pinned digest".into());
        }
    }
    let output = job.path().join("pages");
    let report = qusor_pdf_provider::inspect_with_library_sha256(
        &library.path,
        &library.sha256,
        original,
        &output,
    )
    .map_err(text)?;
    Ok((report, library))
}

fn snapshot_from_report(report: &DocumentReport) -> Result<PdfSnapshot, String> {
    let mut extraction = serde_json::to_value(&report.extractor).map_err(text)?;
    extraction
        .as_object_mut()
        .ok_or("invalid PDF extractor profile")?
        .insert("provider".into(), json!("pdfium"));
    let pages = report
        .pages
        .iter()
        .map(|page| {
            let rotation = match page.rotation.as_str() {
                "None" => 0,
                "Degrees90" => 90,
                "Degrees180" => 180,
                "Degrees270" => 270,
                _ => return Err("unsupported PDF page rotation".to_string()),
            };
            let characters = page
                .characters
                .iter()
                .map(|character| {
                    let mut value = serde_json::to_value(character).map_err(text)?;
                    let object = value.as_object_mut().ok_or("invalid PDF character map")?;
                    object.remove("crop_normalized_quad");
                    object.remove("pixel_quad");
                    Ok(value)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(json!({
                "page_index": page.page_index,
                "engine_text": page.page_text,
                "text_sha256": page.page_text_sha256,
                "rotation_degrees": rotation,
                "media_box": page.media_box,
                "crop_box": page.crop_box,
                "characters": characters,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::from_value(json!({
        "protocol_version": 1,
        "original_sha256": report.original_sha256,
        "original_bytes": report.original_bytes,
        "extraction": extraction,
        "engine_report_sha256": report.extraction_sha256,
        "pages": pages,
    }))
    .map_err(text)
}

fn valid_title(title: &str) -> bool {
    let title = title.trim();
    !title.is_empty() && title.len() <= MAX_TITLE_BYTES && !title.chars().any(char::is_control)
}

pub(crate) fn import(
    sys: &System,
    store: VaultStore,
    title: String,
    bytes: Vec<u8>,
) -> Result<PdfImport, String> {
    let title = title.trim().to_string();
    if !valid_title(&title) {
        return Err("the PDF title is empty or too long".into());
    }
    let job = JobDirectory::create(sys)?;
    let original = write_original(&job, &bytes)?;
    let (report, _) = inspect(&job, &original)?;
    let snapshot = snapshot_from_report(&report)?;
    snapshot.validate_original(&bytes).map_err(text)?;
    let operation = format!(
        "library-pdf-{}",
        &sha256(format!("{title}\0{}", report.original_sha256).as_bytes())[..32]
    );
    let receipt = KnowledgeApplication::new(store)
        .import_document(&operation, "qusor-vault", &title, &bytes, &snapshot)
        .map_err(text)?;
    Ok(PdfImport {
        document_id: receipt.document_id,
        title,
        page_count: u32::try_from(snapshot.pages.len()).map_err(text)?,
    })
}

fn ppm_dimensions(bytes: &[u8]) -> Result<(usize, usize, &[u8]), String> {
    let mut cursor = 0usize;
    fn token<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a str, String> {
        while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        let start = *cursor;
        while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        std::str::from_utf8(&bytes[start..*cursor]).map_err(text)
    }
    if token(bytes, &mut cursor)? != "P6" {
        return Err("the PDF provider returned an unsupported bitmap".into());
    }
    let width = token(bytes, &mut cursor)?.parse::<usize>().map_err(text)?;
    let height = token(bytes, &mut cursor)?.parse::<usize>().map_err(text)?;
    let maximum = token(bytes, &mut cursor)?.parse::<usize>().map_err(text)?;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || maximum != 255
        || cursor >= bytes.len()
    {
        return Err("the PDF provider returned an invalid bitmap".into());
    }
    cursor += 1;
    let pixels = &bytes[cursor..];
    if pixels.len() != width * height * 3 {
        return Err("the PDF provider bitmap has an unexpected size".into());
    }
    Ok((width, height, pixels))
}

fn png_from_ppm(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (width, height, pixels) = ppm_dimensions(bytes)?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in pixels.chunks_exact(3) {
        rgba.extend_from_slice(pixel);
        rgba.push(255);
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(text)?;
        writer.write_image_data(&rgba).map_err(text)?;
    }
    Ok(png)
}

fn page_geometry(page: &PageReport) -> String {
    if page.rotation != "None"
        || page.text_status != "mapped"
        || page.characters.len() > MAX_GEOMETRY_GLYPHS
    {
        return String::new();
    }
    let glyphs = page
        .characters
        .iter()
        .filter_map(|character| {
            let quad = character.pixel_quad?;
            let left = quad.iter().map(|point| point[0]).min()?;
            let top = quad.iter().map(|point| point[1]).min()?;
            let right = quad.iter().map(|point| point[0]).max()?;
            let bottom = quad.iter().map(|point| point[1]).max()?;
            (right > left && bottom > top).then_some(json!([
                character.scalar_start,
                left,
                top,
                right - left,
                bottom - top
            ]))
        })
        .collect::<Vec<_>>();
    let geometry = json!({
        "width": page.pixel_width,
        "height": page.pixel_height,
        "glyphs": glyphs,
    })
    .to_string();
    if geometry.len() <= MAX_GEOMETRY_BYTES {
        geometry
    } else {
        String::new()
    }
}

pub(crate) fn render_page(
    sys: &System,
    store: VaultStore,
    document_id: i64,
    page_index: u32,
) -> Result<PdfPageData, String> {
    let application = KnowledgeApplication::new(store);
    let document = application.query_document(document_id).map_err(text)?;
    let page_count = u32::try_from(document.snapshot.pages.len()).map_err(text)?;
    if page_index >= page_count {
        return Err("the requested PDF page does not exist".into());
    }
    let original = application.original_pdf(document_id).map_err(text)?;
    let job = JobDirectory::create(sys)?;
    let original_path = write_original(&job, &original)?;
    let (report, _) = inspect(&job, &original_path)?;
    if report.original_sha256 != document.snapshot.original_sha256
        || report.extraction_sha256 != document.snapshot.engine_report_sha256
    {
        return Err("the retained PDF no longer matches its Core snapshot".into());
    }
    let page = report
        .pages
        .get(page_index as usize)
        .ok_or_else(|| "the requested PDF page is missing from the report".to_string())?;
    let rendered = fs::read(job.path().join("pages").join(&page.rendered_file)).map_err(text)?;
    if sha256(&rendered) != page.rendered_sha256 {
        return Err("the rendered PDF page failed its digest check".into());
    }
    let png = png_from_ppm(&rendered)?;
    Ok(PdfPageData {
        info: json!({
            "ok": true,
            "document_id": document_id,
            "title": document.title,
            "page_index": page_index,
            "page_count": page_count,
            "width": page.pixel_width,
            "height": page.pixel_height,
        })
        .to_string(),
        image: Some(ImageData::new(png)),
        geometry: page_geometry(page),
    })
}

pub(crate) fn import_value(value: PdfImport) -> Value {
    json!({
        "document_id": value.document_id,
        "title": value.title,
        "page_count": value.page_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-pdf-reader-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../experiments/pdf-provider/fixtures/text-two-pages.pdf"),
        )
        .unwrap()
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn imports_and_renders_a_real_pdf_through_core() {
        let scratch = Scratch::new();
        let mut sys = System::default();
        sys.data_dir = Some(scratch.0.join("data"));
        let store = VaultStore::open_app(&scratch.0.join("data")).unwrap();
        let imported = import(&sys, store.clone(), "text-two-pages.pdf".into(), fixture()).unwrap();
        assert_eq!(imported.page_count, 2);
        let page = render_page(&sys, store, imported.document_id, 0).unwrap();
        let info: Value = serde_json::from_str(&page.info).unwrap();
        assert_eq!(info["ok"], true);
        assert_eq!(info["page_index"], 0);
        assert!(page.image.is_some());
        assert!(page.geometry.contains("\"glyphs\""));
    }

    #[test]
    fn rejects_bytes_that_are_not_a_pdf() {
        let scratch = Scratch::new();
        let mut sys = System::default();
        sys.data_dir = Some(scratch.0.join("data"));
        let store = VaultStore::open_app(&scratch.0.join("data")).unwrap();
        let error = import(&sys, store, "not-a-pdf".into(), b"plain text".to_vec())
            .err()
            .unwrap();
        assert!(error.contains("not a PDF"));
    }
}
