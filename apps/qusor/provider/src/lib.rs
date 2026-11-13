//! PDFium 页渲染与字符映射提供者：串行执行原生 PDF 渲染并产出字符映射报告。
//! 不持有 UI、Core 或插件授权。
pub mod selection;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use unicode_segmentation::UnicodeSegmentation;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const PROFILE: &str = "qusor-pdfium-char-map-v1";
pub const UNIT: &str = "unicode_scalar";
pub const MAX_PDF_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PAGES: usize = 16;
pub const MAX_CHARS: usize = 100_000;
pub const MAX_RENDER_EDGE: i32 = 2048;
pub const MAX_TOC_ENTRIES: usize = 512;
pub const MAX_TOC_DEPTH: u8 = 8;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorProfile {
    pub algorithm: String,
    pub wrapper: String,
    pub abi: String,
    pub library_sha256: String,
    pub mapping_check: String,
    pub normalization: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}
impl Rect {
    fn valid(&self) -> bool {
        [self.left, self.bottom, self.right, self.top]
            .iter()
            .all(|v| v.is_finite())
            && self.left <= self.right
            && self.bottom <= self.top
    }
    fn from_pdf(rect: PdfRect) -> Result<Self> {
        let result = Self {
            left: rect.left().value,
            bottom: rect.bottom().value,
            right: rect.right().value,
            top: rect.top().value,
        };
        if !result.valid() {
            return Err("invalid PDF geometry".into());
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub pdf_char_index: usize,
    pub unicode_value: u32,
    pub mapping: String,
    pub generated: bool,
    pub scalar_start: usize,
    pub scalar_end: usize,
    pub utf8_start: usize,
    pub utf8_end: usize,
    pub utf16_start: usize,
    pub utf16_end: usize,
    pub pdf_bounds: Option<Rect>,
    pub crop_normalized_quad: Option<[[f32; 2]; 4]>,
    pub pixel_quad: Option<[[i32; 2]; 4]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageReport {
    pub page_index: usize,
    pub rotation: String,
    pub media_box: Rect,
    pub crop_box: Rect,
    pub displayed_width_points: f32,
    pub displayed_height_points: f32,
    pub pixel_width: i32,
    pub pixel_height: i32,
    pub page_text: String,
    pub page_text_sha256: String,
    pub text_status: String,
    pub characters: Vec<Character>,
    pub rendered_file: String,
    pub rendered_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub title: String,
    pub page_index: u32,
    pub depth: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentReport {
    pub schema_version: u16,
    pub original_sha256: String,
    pub original_bytes: usize,
    pub extractor: ExtractorProfile,
    pub extraction_sha256: String,
    pub pages: Vec<PageReport>,
    #[serde(default)]
    pub toc: Vec<BookmarkEntry>,
}

fn bookmark_page_index(bookmark: &PdfBookmark<'_>) -> Option<u32> {
    if let Some(dest) = bookmark.destination() {
        return dest.page_index().ok().and_then(|idx| u32::try_from(idx).ok());
    }
    match bookmark.action() {
        Some(PdfAction::LocalDestination(local)) => local
            .destination()
            .ok()
            .and_then(|dest| dest.page_index().ok())
            .and_then(|idx| u32::try_from(idx).ok()),
        _ => None,
    }
}

fn extract_toc(document: &PdfDocument<'_>, pages_count: usize) -> Vec<BookmarkEntry> {
    let mut toc = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    if let Some(root) = document.bookmarks().root() {
        stack.push((root, 0u8));
    }
    while let Some((bookmark, depth)) = stack.pop() {
        if toc.len() >= MAX_TOC_ENTRIES {
            break;
        }
        if visited.contains(&bookmark) {
            continue;
        }
        let next_sibling = bookmark.next_sibling();
        let first_child = if depth < MAX_TOC_DEPTH {
            bookmark.first_child()
        } else {
            None
        };
        if let Some(title) = bookmark
            .title()
            .map(|raw| raw.trim().to_string())
            .filter(|trimmed| !trimmed.is_empty())
            && let Some(page_index) = bookmark_page_index(&bookmark)
            && (page_index as usize) < pages_count
        {
            toc.push(BookmarkEntry {
                title,
                page_index,
                depth,
            });
        }
        visited.insert(bookmark);
        if let Some(sibling) = next_sibling {
            stack.push((sibling, depth));
        }
        if let Some(child) = first_child {
            stack.push((child, depth.saturating_add(1)));
        }
    }
    toc
}

fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > max as u64 {
        return Err("input must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err("input budget exceeded".into());
    }
    Ok(bytes)
}

/// Call only in the dedicated serial worker. The parent owns its deadline.
pub fn inspect(library: &Path, input: &Path, output: &Path) -> Result<DocumentReport> {
    let library_bytes = read_bounded(library, 64 * 1024 * 1024)?;
    inspect_with_library_sha256(library, &sha256(&library_bytes), input, output)
}

/// Inspect with an already verified runtime digest. Android loads PDFium from
/// the APK's native-library directory, where a plain filesystem path is not the
/// loader identity; the application supplies the pinned digest it shipped.
pub fn inspect_with_library_sha256(
    library: &Path,
    library_sha256: &str,
    input: &Path,
    output: &Path,
) -> Result<DocumentReport> {
    let bytes = read_bounded(input, MAX_PDF_BYTES)?;
    if !bytes.starts_with(b"%PDF-") {
        return Err("not a PDF header".into());
    }
    if library_sha256.len() != 64
        || !library_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid PDFium runtime digest".into());
    }
    let extractor = ExtractorProfile {
        algorithm: PROFILE.into(),
        wrapper: "pdfium-render-0.9.5".into(),
        abi: "pdfium_8125".into(),
        library_sha256: library_sha256.into(),
        mapping_check: "unicode_scalar_validity_only".into(),
        normalization: "pdfium-char-stream-raw-v1".into(),
    };
    let pdfium = match Pdfium::bind_to_library(library) {
        Ok(bindings) => Pdfium::new(bindings),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
        Err(error) => return Err(error.into()),
    };
    let document = pdfium.load_pdf_from_byte_slice(&bytes, None)?;
    let pages_count = usize::try_from(document.pages().len())?;
    if pages_count == 0 || pages_count > MAX_PAGES {
        return Err("PDF page budget exceeded or empty document".into());
    }
    fs::create_dir_all(output)?;
    let mut pages = Vec::with_capacity(pages_count);
    let mut total_characters = 0usize;
    for page_index in 0..pages_count {
        let page = document.pages().get(page_index as i32)?;
        let width = page.width().value;
        let height = page.height().value;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || width > 200_000.0
            || height > 200_000.0
        {
            return Err("page geometry budget exceeded".into());
        }
        let scale = (MAX_RENDER_EDGE as f32 / width.max(height)).min(2.0);
        let pixel_width = (width * scale).round().max(1.0) as i32;
        let pixel_height = (height * scale).round().max(1.0) as i32;
        let config = PdfRenderConfig::new()
            .set_target_size(pixel_width, pixel_height)
            .render_form_data(false)
            .render_annotations(false);
        let media = Rect::from_pdf(page.boundaries().media()?.bounds)?;
        let crop = Rect::from_pdf(page.boundaries().crop()?.bounds)?;
        if crop.right <= crop.left || crop.top <= crop.bottom {
            return Err("empty crop box".into());
        }
        let text = page.text()?;
        let count = text.len();
        if count < 0 || count as usize > MAX_CHARS {
            return Err("page character budget exceeded".into());
        }
        total_characters = total_characters
            .checked_add(count as usize)
            .ok_or("character overflow")?;
        if total_characters > MAX_CHARS {
            return Err("document character budget exceeded".into());
        }
        let chars = text.chars();
        let mut page_text = String::new();
        let mut characters = Vec::with_capacity(count as usize);
        let mut utf16 = 0usize;
        let mut complete = true;
        for index in 0..count {
            let character = chars.get(index as usize)?;
            let unicode = character.unicode_value();
            let valid = unicode != 0 && char::from_u32(unicode).is_some();
            let scalar = if valid {
                char::from_u32(unicode).unwrap()
            } else {
                complete = false;
                '\u{fffd}'
            };
            let start = page_text.len();
            page_text.push(scalar);
            let generated = character.is_generated()?;
            let bounds = if generated || scalar.is_whitespace() {
                None
            } else {
                Some(Rect::from_pdf(character.tight_bounds()?)?)
            };
            let mut pixel_quad = None;
            let mut normalized = None;
            if let Some(rect) = &bounds {
                let points = [
                    [rect.left, rect.bottom],
                    [rect.right, rect.bottom],
                    [rect.right, rect.top],
                    [rect.left, rect.top],
                ];
                let mut pixels = [[0; 2]; 4];
                let mut normalized_points = [[0.0; 2]; 4];
                for (i, [x, y]) in points.into_iter().enumerate() {
                    let (px, py) =
                        page.points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), &config)?;
                    pixels[i] = [px, py];
                    normalized_points[i] = [
                        (x - crop.left) / (crop.right - crop.left),
                        (crop.top - y) / (crop.top - crop.bottom),
                    ];
                }
                pixel_quad = Some(pixels);
                normalized = Some(normalized_points);
            }
            characters.push(Character {
                pdf_char_index: character.index(),
                unicode_value: unicode,
                mapping: if valid { "scalar" } else { "unknown" }.into(),
                generated,
                scalar_start: index as usize,
                scalar_end: index as usize + 1,
                utf8_start: start,
                utf8_end: page_text.len(),
                utf16_start: utf16,
                utf16_end: utf16 + scalar.len_utf16(),
                pdf_bounds: bounds,
                crop_normalized_quad: normalized,
                pixel_quad,
            });
            utf16 += scalar.len_utf16();
        }
        let bitmap = page.render_with_config(&config)?;
        if bitmap.width() != pixel_width || bitmap.height() != pixel_height {
            return Err("render geometry changed".into());
        }
        let rgba = bitmap.as_rgba_bytes();
        if rgba.len() != pixel_width as usize * pixel_height as usize * 4 {
            return Err("unexpected bitmap layout".into());
        }
        let mut ppm = format!("P6\n{pixel_width} {pixel_height}\n255\n").into_bytes();
        for pixel in rgba.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        let filename = format!("page-{page_index}.ppm");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(&filename))?;
        file.write_all(&ppm)?;
        pages.push(PageReport {
            page_index,
            rotation: format!("{:?}", page.rotation()?),
            media_box: media,
            crop_box: crop,
            displayed_width_points: width,
            displayed_height_points: height,
            pixel_width,
            pixel_height,
            page_text_sha256: sha256(page_text.as_bytes()),
            page_text,
            text_status: if count == 0 {
                "no_text"
            } else if complete {
                "mapped"
            } else {
                "mapping_incomplete"
            }
            .into(),
            characters,
            rendered_file: filename,
            rendered_sha256: sha256(&ppm),
        });
    }
    let toc = extract_toc(&document, pages_count);
    let original_sha256 = sha256(&bytes);
    let extraction_sha256 = sha256(&serde_json::to_vec(&(
        &original_sha256,
        &extractor,
        &pages,
    ))?);
    Ok(DocumentReport {
        schema_version: 1,
        original_sha256,
        original_bytes: bytes.len(),
        extractor,
        extraction_sha256,
        pages,
        toc,
    })
}

#[derive(Debug, Serialize)]
pub struct Selection {
    pub original_sha256: String,
    pub extraction_sha256: String,
    pub page_text_sha256: String,
    pub page_index: usize,
    pub unit: String,
    pub start: usize,
    pub end: usize,
    pub exact: String,
    pub characters: Vec<Character>,
}
pub struct ResolveRequest<'a> {
    pub page: usize,
    pub start: usize,
    pub end: usize,
    pub original_hash: &'a str,
    pub extraction_hash: &'a str,
    pub page_hash: &'a str,
    pub unit: &'a str,
    pub quote: &'a str,
}
pub(crate) fn validate_report(report: &DocumentReport) -> Result<()> {
    if report.schema_version != 1
        || report.original_bytes > MAX_PDF_BYTES
        || report.pages.is_empty()
        || report.pages.len() > MAX_PAGES
        || report.extractor.algorithm != PROFILE
        || report.extractor.wrapper != "pdfium-render-0.9.5"
        || report.extractor.abi != "pdfium_8125"
        || report.extractor.normalization != "pdfium-char-stream-raw-v1"
        || report.extractor.mapping_check != "unicode_scalar_validity_only"
    {
        return Err("unsupported or oversized extraction report".into());
    }
    let mut count = 0usize;
    for (index, page) in report.pages.iter().enumerate() {
        count = count
            .checked_add(page.characters.len())
            .ok_or("character overflow")?;
        if count > MAX_CHARS
            || page.page_index != index
            || page.page_text.len() > MAX_CHARS * 4
            || page.page_text_sha256 != sha256(page.page_text.as_bytes())
            || page.pixel_width <= 0
            || page.pixel_width > MAX_RENDER_EDGE
            || page.pixel_height <= 0
            || page.pixel_height > MAX_RENDER_EDGE
            || !page.media_box.valid()
            || !page.crop_box.valid()
            || page.crop_box.left >= page.crop_box.right
            || page.crop_box.bottom >= page.crop_box.top
            || !page.displayed_width_points.is_finite()
            || page.displayed_width_points <= 0.0
            || !page.displayed_height_points.is_finite()
            || page.displayed_height_points <= 0.0
            || page.rendered_file != format!("page-{index}.ppm")
        {
            return Err("invalid page report".into());
        }
        let mut utf16 = 0usize;
        let mut complete = true;
        let scalar_count = page.page_text.chars().count();
        if scalar_count != page.characters.len() {
            return Err("incomplete character map".into());
        }
        for (i, ((byte, scalar), mapped)) in page
            .page_text
            .char_indices()
            .zip(&page.characters)
            .enumerate()
        {
            let valid = mapped.unicode_value != 0 && char::from_u32(mapped.unicode_value).is_some();
            let expected = if valid {
                char::from_u32(mapped.unicode_value).unwrap()
            } else {
                complete = false;
                '\u{fffd}'
            };
            if scalar != expected
                || mapped.pdf_char_index != i
                || mapped.scalar_start != i
                || mapped.scalar_end != i + 1
                || mapped.utf8_start != byte
                || mapped.utf8_end != byte + scalar.len_utf8()
                || mapped.utf16_start != utf16
                || mapped.utf16_end != utf16 + scalar.len_utf16()
                || mapped.mapping != if valid { "scalar" } else { "unknown" }
            {
                return Err("invalid character offset or Unicode map".into());
            }
            if let Some(bounds) = &mapped.pdf_bounds {
                if !bounds.valid()
                    || mapped.pixel_quad.is_none()
                    || mapped
                        .crop_normalized_quad
                        .is_none_or(|quad| quad.iter().flatten().any(|v| !v.is_finite()))
                {
                    return Err("invalid character geometry".into());
                }
            } else if mapped.pixel_quad.is_some() || mapped.crop_normalized_quad.is_some() {
                return Err("geometry without PDF bounds".into());
            }
            utf16 += scalar.len_utf16();
        }
        let expected_status = if scalar_count == 0 {
            "no_text"
        } else if complete {
            "mapped"
        } else {
            "mapping_incomplete"
        };
        if page.text_status != expected_status {
            return Err("inconsistent mapping status".into());
        }
    }
    if report.toc.len() > MAX_TOC_ENTRIES
        || report.toc.iter().any(|entry| {
            entry.title.trim().is_empty()
                || entry.depth > MAX_TOC_DEPTH
                || (entry.page_index as usize) >= report.pages.len()
        })
    {
        return Err("invalid or oversized TOC".into());
    }
    let hash = sha256(&serde_json::to_vec(&(
        &report.original_sha256,
        &report.extractor,
        &report.pages,
    ))?);
    if hash != report.extraction_sha256 {
        return Err("extraction report digest mismatch".into());
    }
    Ok(())
}
pub fn resolve(report: &DocumentReport, request: ResolveRequest<'_>) -> Result<Selection> {
    validate_report(report)?;
    if request.original_hash != report.original_sha256 {
        return Err("original version/hash mismatch".into());
    }
    if request.extraction_hash != report.extraction_sha256 {
        return Err("extraction version/hash mismatch".into());
    }
    if request.unit != UNIT {
        return Err("unsupported selection unit".into());
    }
    let page = report.pages.get(request.page).ok_or("page missing")?;
    if page.page_text_sha256 != request.page_hash {
        return Err("page text version/hash mismatch".into());
    }
    if page.text_status != "mapped" {
        return Err("no reliable text map; exact text selection unavailable".into());
    }
    if request.start >= request.end || request.end > page.characters.len() {
        return Err("selection range invalid".into());
    }
    let byte_start = page.characters[request.start].utf8_start;
    let byte_end = page.characters[request.end - 1].utf8_end;
    let boundary = |offset| {
        offset == page.page_text.len()
            || page
                .page_text
                .grapheme_indices(true)
                .any(|(i, _)| i == offset)
    };
    if !boundary(byte_start) || !boundary(byte_end) {
        return Err("selection splits a grapheme".into());
    }
    let exact = page
        .page_text
        .get(byte_start..byte_end)
        .ok_or("invalid UTF-8 character map")?;
    if exact != request.quote {
        return Err("exact quote mismatch".into());
    }
    let characters = page.characters[request.start..request.end].to_vec();
    if !characters.iter().any(|c| c.pixel_quad.is_some()) {
        return Err("selection has no visible glyph geometry".into());
    }
    Ok(Selection {
        original_sha256: report.original_sha256.clone(),
        extraction_sha256: report.extraction_sha256.clone(),
        page_text_sha256: page.page_text_sha256.clone(),
        page_index: request.page,
        unit: UNIT.into(),
        start: request.start,
        end: request.end,
        exact: exact.into(),
        characters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn minimal_page(page_index: usize) -> PageReport {
        PageReport {
            page_index,
            rotation: "None".into(),
            media_box: Rect {
                left: 0.0,
                bottom: 0.0,
                right: 612.0,
                top: 792.0,
            },
            crop_box: Rect {
                left: 0.0,
                bottom: 0.0,
                right: 612.0,
                top: 792.0,
            },
            displayed_width_points: 612.0,
            displayed_height_points: 792.0,
            pixel_width: 612,
            pixel_height: 792,
            page_text: String::new(),
            page_text_sha256: sha256(b""),
            text_status: "no_text".into(),
            characters: Vec::new(),
            rendered_file: format!("page-{page_index}.ppm"),
            rendered_sha256: sha256(b"stub"),
        }
    }

    fn minimal_report(toc: Vec<BookmarkEntry>) -> DocumentReport {
        let original_sha256 = sha256(b"%PDF-1.4");
        let extractor = ExtractorProfile {
            algorithm: PROFILE.into(),
            wrapper: "pdfium-render-0.9.5".into(),
            abi: "pdfium_8125".into(),
            library_sha256: "0".repeat(64),
            mapping_check: "unicode_scalar_validity_only".into(),
            normalization: "pdfium-char-stream-raw-v1".into(),
        };
        let pages = vec![minimal_page(0), minimal_page(1)];
        let extraction_sha256 =
            sha256(&serde_json::to_vec(&(&original_sha256, &extractor, &pages)).unwrap());
        DocumentReport {
            schema_version: 1,
            original_sha256,
            original_bytes: 8,
            extractor,
            extraction_sha256,
            pages,
            toc,
        }
    }

    #[test]
    fn document_report_toc_roundtrip_and_bounds() {
        let mut value = serde_json::to_value(minimal_report(Vec::new())).unwrap();
        value.as_object_mut().unwrap().remove("toc");
        let legacy: DocumentReport = serde_json::from_value(value).unwrap();
        assert!(legacy.toc.is_empty());
        assert!(validate_report(&legacy).is_ok());

        let with_toc = minimal_report(vec![
            BookmarkEntry {
                title: "Chapter 1".into(),
                page_index: 0,
                depth: 0,
            },
            BookmarkEntry {
                title: "Section 1.1".into(),
                page_index: 1,
                depth: 8,
            },
        ]);
        let roundtrip: DocumentReport =
            serde_json::from_str(&serde_json::to_string(&with_toc).unwrap()).unwrap();
        assert_eq!(roundtrip.toc, with_toc.toc);
        assert!(validate_report(&roundtrip).is_ok());

        let bad_depth = minimal_report(vec![BookmarkEntry {
            title: "Too deep".into(),
            page_index: 0,
            depth: MAX_TOC_DEPTH + 1,
        }]);
        assert!(validate_report(&bad_depth).is_err());

        let bad_page = minimal_report(vec![BookmarkEntry {
            title: "Out of range".into(),
            page_index: 2,
            depth: 0,
        }]);
        assert!(validate_report(&bad_page).is_err());

        let empty_title = minimal_report(vec![BookmarkEntry {
            title: "   ".into(),
            page_index: 0,
            depth: 0,
        }]);
        assert!(validate_report(&empty_title).is_err());

        let oversized = minimal_report(
            (0..=MAX_TOC_ENTRIES)
                .map(|i| BookmarkEntry {
                    title: format!("Item {i}"),
                    page_index: 0,
                    depth: 0,
                })
                .collect(),
        );
        assert!(validate_report(&oversized).is_err());
    }

    fn build_pdf_with_outlines() -> Vec<u8> {
        let objects = [
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /CropBox [0 0 200 200] >>\nendobj\n",
            "4 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /CropBox [0 0 200 200] >>\nendobj\n",
            "5 0 obj\n<< /Type /Outlines /First 6 0 R /Last 10 0 R /Count 4 >>\nendobj\n",
            "6 0 obj\n<< /Title (Chapter 1) /Parent 5 0 R /Next 8 0 R /First 7 0 R /Last 7 0 R /Count 1 /Dest [3 0 R /Fit] >>\nendobj\n",
            "7 0 obj\n<< /Title (Section 1.1) /Parent 6 0 R /Dest [4 0 R /Fit] >>\nendobj\n",
            "8 0 obj\n<< /Title (   ) /Parent 5 0 R /Prev 6 0 R /Next 9 0 R /Dest [3 0 R /Fit] >>\nendobj\n",
            "9 0 obj\n<< /Title (Chapter 2) /Parent 5 0 R /Prev 8 0 R /Next 10 0 R /A << /S /GoTo /D [4 0 R /Fit] >> >>\nendobj\n",
            "10 0 obj\n<< /Title (External Link) /Parent 5 0 R /Prev 9 0 R /A << /S /URI /URI (https://example.com) >> >>\nendobj\n",
        ];
        let mut out = Vec::from(&b"%PDF-1.4\n"[..]);
        let mut offsets = Vec::with_capacity(objects.len());
        for obj in &objects {
            offsets.push(out.len());
            out.extend_from_slice(obj.as_bytes());
        }
        let xref_start = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn inspect_extracts_bounded_pdf_bookmarks() {
        let dylib = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../native/assets/pdfium/macos-aarch64/libpdfium.dylib");
        if !dylib.is_file() {
            return;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("qusor-pdf-toc-test-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let pdf_path = dir.join("sample.pdf");
        let out_dir = dir.join("out");
        fs::write(&pdf_path, build_pdf_with_outlines()).unwrap();

        let report = inspect(&dylib, &pdf_path, &out_dir).unwrap();
        assert_eq!(
            report.toc,
            vec![
                BookmarkEntry {
                    title: "Chapter 1".into(),
                    page_index: 0,
                    depth: 0,
                },
                BookmarkEntry {
                    title: "Section 1.1".into(),
                    page_index: 1,
                    depth: 1,
                },
                BookmarkEntry {
                    title: "Chapter 2".into(),
                    page_index: 1,
                    depth: 0,
                },
            ]
        );
        assert!(validate_report(&report).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
