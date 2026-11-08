use qusor_pdf_provider_probe::{DocumentReport, MAX_PDF_BYTES, ResolveRequest, resolve};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-pdf-provider-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn library() -> PathBuf {
    std::env::var_os("QUSOR_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| base().join("target/pdfium/lib/libpdfium.so"))
}
fn run(args: &[&std::ffi::OsStr], success: bool) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_qusor-pdf-provider-probe"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], success);
    value
}
fn inspect(pdf: &Path, output: &Path, success: bool) -> Value {
    run(
        &[
            "inspect".as_ref(),
            library().as_os_str(),
            pdf.as_os_str(),
            output.as_os_str(),
        ],
        success,
    )
}
fn text_report(scratch: &Scratch) -> DocumentReport {
    serde_json::from_value(
        inspect(
            &base().join("fixtures/text-two-pages.pdf"),
            &scratch.join("render"),
            true,
        )["report"]
            .clone(),
    )
    .unwrap()
}
fn reseal(report: &mut DocumentReport) {
    report.extraction_sha256 = qusor_pdf_provider_probe::sha256(
        &serde_json::to_vec(&(&report.original_sha256, &report.extractor, &report.pages)).unwrap(),
    );
}

#[test]
fn real_pages_render_and_map_crop_rotation_to_actual_pixels() {
    let tmp = Scratch::new();
    let report = text_report(&tmp);
    assert_eq!(report.pages.len(), 2);
    assert_eq!(
        report.original_sha256,
        "7fd4515748d1b14265ab2af851fe02fc231493e9bf443feb978f1dcff605fd9c"
    );
    assert_eq!(report.pages[0].crop_box.left, 18.0);
    assert_eq!(report.pages[1].crop_box.left, 30.0);
    assert_eq!(report.pages[0].rotation, "None");
    assert_eq!(report.pages[1].rotation, "Degrees90");
    for page in &report.pages {
        assert_eq!(page.text_status, "mapped");
        assert!(
            page.pixel_width > 0
                && page.pixel_width <= 2048
                && page.pixel_height > 0
                && page.pixel_height <= 2048
        );
        let ppm = fs::read(tmp.join("render").join(&page.rendered_file)).unwrap();
        assert!(ppm.starts_with(b"P6\n"));
        let pixels = ppm.splitn(4, |byte| *byte == b'\n').nth(3).unwrap();
        assert_eq!(qusor_pdf_provider_probe::sha256(&ppm), page.rendered_sha256);
        for (index, c) in page.characters.iter().enumerate() {
            assert_eq!(c.pdf_char_index, index);
            assert_eq!(c.scalar_start, index);
            assert_eq!(c.scalar_end, index + 1);
            let text = &page.page_text[c.utf8_start..c.utf8_end];
            assert_eq!(text.chars().next().unwrap() as u32, c.unicode_value);
            assert_eq!(text.encode_utf16().count(), c.utf16_end - c.utf16_start);
            if let Some(quad) = c.pixel_quad {
                for [x, y] in quad {
                    assert!(x >= 0 && x <= page.pixel_width && y >= 0 && y <= page.pixel_height);
                }
                let left = (quad.iter().map(|p| p[0]).min().unwrap() - 1).max(0);
                let right =
                    (quad.iter().map(|p| p[0]).max().unwrap() + 1).min(page.pixel_width - 1);
                let top = (quad.iter().map(|p| p[1]).min().unwrap() - 1).max(0);
                let bottom =
                    (quad.iter().map(|p| p[1]).max().unwrap() + 1).min(page.pixel_height - 1);
                let ink = (top..=bottom).any(|y| {
                    (left..=right).any(|x| {
                        let offset = ((y * page.pixel_width + x) * 3) as usize;
                        pixels[offset..offset + 3].iter().any(|value| *value < 128)
                    })
                });
                assert!(
                    ink,
                    "page {} char {} has no raster ink inside projected glyph",
                    page.page_index, c.pdf_char_index
                );
            }
        }
    }
    // A 90-degree display rotates a character's horizontal PDF edge vertically.
    let q = report.pages[1].characters[0].pixel_quad.unwrap();
    assert_eq!(q[0][0], q[1][0]);
    assert_ne!(q[0][1], q[1][1]);
}

#[test]
fn exact_selection_reopens_pdf_and_checks_all_three_versions() {
    let tmp = Scratch::new();
    let report = text_report(&tmp);
    let page = &report.pages[0];
    let quote = "Selection target: immutable originals.";
    let start = page.page_text[..page.page_text.find(quote).unwrap()]
        .chars()
        .count();
    let end = start + quote.chars().count();
    let result = run(
        &[
            "resolve".as_ref(),
            library().as_os_str(),
            base().join("fixtures/text-two-pages.pdf").as_os_str(),
            "0".as_ref(),
            start.to_string().as_ref(),
            end.to_string().as_ref(),
            report.original_sha256.as_ref(),
            report.extraction_sha256.as_ref(),
            page.page_text_sha256.as_ref(),
            "unicode_scalar".as_ref(),
            quote.as_ref(),
            tmp.join("reopen").as_os_str(),
        ],
        true,
    );
    assert_eq!(result["selection"]["exact"], quote);
    assert_eq!(result["selection"]["page_index"], 0);
    assert!(
        result["selection"]["characters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| !c["pixel_quad"].is_null())
    );
    let request = |original_hash, extraction_hash, page_hash, unit, quote| ResolveRequest {
        page: 0,
        start,
        end,
        original_hash,
        extraction_hash,
        page_hash,
        unit,
        quote,
    };
    assert!(
        resolve(
            &report,
            request(
                "bad",
                &report.extraction_sha256,
                &page.page_text_sha256,
                "unicode_scalar",
                quote
            )
        )
        .unwrap_err()
        .to_string()
        .contains("original")
    );
    assert!(
        resolve(
            &report,
            request(
                &report.original_sha256,
                "bad",
                &page.page_text_sha256,
                "unicode_scalar",
                quote
            )
        )
        .unwrap_err()
        .to_string()
        .contains("extraction")
    );
    assert!(
        resolve(
            &report,
            request(
                &report.original_sha256,
                &report.extraction_sha256,
                "bad",
                "unicode_scalar",
                quote
            )
        )
        .unwrap_err()
        .to_string()
        .contains("page text")
    );
    assert!(
        resolve(
            &report,
            request(
                &report.original_sha256,
                &report.extraction_sha256,
                &page.page_text_sha256,
                "utf16",
                quote
            )
        )
        .unwrap_err()
        .to_string()
        .contains("unit")
    );
    assert!(
        resolve(
            &report,
            request(
                &report.original_sha256,
                &report.extraction_sha256,
                &page.page_text_sha256,
                "unicode_scalar",
                "similar text"
            )
        )
        .unwrap_err()
        .to_string()
        .contains("quote")
    );
}

#[test]
fn scanned_page_is_renderable_but_not_selectable_and_bad_pdf_fails() {
    let tmp = Scratch::new();
    let value = inspect(
        &base().join("fixtures/image-only.pdf"),
        &tmp.join("scan"),
        true,
    );
    let report: DocumentReport = serde_json::from_value(value["report"].clone()).unwrap();
    assert_eq!(report.pages[0].text_status, "no_text");
    assert!(report.pages[0].characters.is_empty());
    let request = ResolveRequest {
        page: 0,
        start: 0,
        end: 1,
        original_hash: &report.original_sha256,
        extraction_hash: &report.extraction_sha256,
        page_hash: &report.pages[0].page_text_sha256,
        unit: "unicode_scalar",
        quote: "invented OCR",
    };
    assert!(
        resolve(&report, request)
            .unwrap_err()
            .to_string()
            .contains("no reliable text")
    );
    inspect(
        &base().join("fixtures/malformed.pdf"),
        &tmp.join("bad"),
        false,
    );
}

#[test]
fn file_budget_symlink_and_existing_output_are_rejected() {
    let tmp = Scratch::new();
    let oversized = tmp.join("oversized.pdf");
    let file = fs::File::create(&oversized).unwrap();
    file.set_len(MAX_PDF_BYTES as u64 + 1).unwrap();
    assert!(
        inspect(&oversized, &tmp.join("oversized-out"), false)["error"]
            .as_str()
            .unwrap()
            .contains("bounded")
    );
    let link = tmp.join("linked.pdf");
    std::os::unix::fs::symlink(base().join("fixtures/text-two-pages.pdf"), &link).unwrap();
    inspect(&link, &tmp.join("link-out"), false);
    inspect(
        &base().join("fixtures/text-two-pages.pdf"),
        &tmp.join("existing"),
        true,
    );
    inspect(
        &base().join("fixtures/text-two-pages.pdf"),
        &tmp.join("existing"),
        false,
    );
}

#[test]
fn scalar_selection_cannot_split_graphemes_or_accept_unknown_mapping() {
    let tmp = Scratch::new();
    let mut report = text_report(&tmp);
    let original = report.pages[0].characters[0].clone();
    let text = "e\u{301}🙂";
    let page = &mut report.pages[0];
    page.page_text = text.into();
    page.page_text_sha256 = qusor_pdf_provider_probe::sha256(text.as_bytes());
    page.characters.clear();
    let mut utf16 = 0;
    for (index, (byte, c)) in text.char_indices().enumerate() {
        let mut mapped = original.clone();
        mapped.pdf_char_index = index;
        mapped.scalar_start = index;
        mapped.scalar_end = index + 1;
        mapped.utf8_start = byte;
        mapped.utf8_end = byte + c.len_utf8();
        mapped.utf16_start = utf16;
        utf16 += c.len_utf16();
        mapped.utf16_end = utf16;
        mapped.unicode_value = c as u32;
        page.characters.push(mapped);
    }
    let hash = page.page_text_sha256.clone();
    reseal(&mut report);
    let request = |start, end, quote| ResolveRequest {
        page: 0,
        start,
        end,
        original_hash: report.original_sha256.as_str(),
        extraction_hash: report.extraction_sha256.as_str(),
        page_hash: hash.as_str(),
        unit: "unicode_scalar",
        quote,
    };
    assert!(
        resolve(&report, request(0, 1, "e"))
            .unwrap_err()
            .to_string()
            .contains("grapheme")
    );
    assert!(resolve(&report, request(1, 2, "\u{301}")).is_err());
    assert_eq!(
        resolve(&report, request(0, 2, "e\u{301}")).unwrap().exact,
        "e\u{301}"
    );
    assert_eq!(
        resolve(&report, request(2, 3, "🙂")).unwrap().characters[0].utf16_end - 2,
        2
    );
    report.pages[0].text_status = "mapping_incomplete".into();
    assert!(
        resolve(
            &report,
            ResolveRequest {
                page: 0,
                start: 0,
                end: 2,
                original_hash: &report.original_sha256,
                extraction_hash: &report.extraction_sha256,
                page_hash: &hash,
                unit: "unicode_scalar",
                quote: "e\u{301}"
            }
        )
        .is_err()
    );
}

#[test]
fn damaged_serialized_offsets_fail_without_panics_or_slicing_similar_text() {
    let tmp = Scratch::new();
    let baseline = text_report(&tmp);
    for damage in ["offset", "text", "scalar", "geometry", "digest"] {
        let mut report = baseline.clone();
        match damage {
            "offset" => report.pages[0].characters[0].utf8_start = usize::MAX,
            "text" => report.pages[0].page_text.push('!'),
            "scalar" => report.pages[0].characters[0].scalar_end = 99,
            "geometry" => {
                report.pages[0].characters[0]
                    .pdf_bounds
                    .as_mut()
                    .unwrap()
                    .left = f32::NAN
            }
            _ => report.extraction_sha256 = "0".repeat(64),
        }
        assert!(
            resolve(
                &report,
                ResolveRequest {
                    page: 0,
                    start: 0,
                    end: 1,
                    original_hash: &report.original_sha256,
                    extraction_hash: &report.extraction_sha256,
                    page_hash: &report.pages[0].page_text_sha256,
                    unit: "unicode_scalar",
                    quote: "Q"
                }
            )
            .is_err(),
            "{damage}"
        );
    }
}
