use qusor_pdf_provider_probe::{DocumentReport, selection::*, sha256};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "qusor-select-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn report() -> (Scratch, DocumentReport) {
    let tmp = Scratch::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let library = std::env::var_os("QUSOR_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/pdfium/lib/libpdfium.so"));
    let out = Command::new(env!("CARGO_BIN_EXE_qusor-pdf-provider-probe"))
        .arg("inspect")
        .arg(library)
        .arg(root.join("fixtures/text-two-pages.pdf"))
        .arg(tmp.0.join("render"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let report = serde_json::from_value(v["report"].clone()).unwrap();
    (tmp, report)
}
fn point(report: &DocumentReport, page: usize, index: usize) -> Point {
    let q = report.pages[page].characters[index].pixel_quad.unwrap();
    Point {
        x: q.iter().map(|p| f64::from(p[0])).sum::<f64>() / 4.0,
        y: q.iter().map(|p| f64::from(p[1])).sum::<f64>() / 4.0,
    }
}
fn index(report: &DocumentReport, page: usize, quote: &str) -> usize {
    let text = &report.pages[page].page_text;
    text[..text.find(quote).unwrap()].chars().count()
}
fn rehash(report: &mut DocumentReport) {
    report.extraction_sha256 = sha256(
        &serde_json::to_vec(&(&report.original_sha256, &report.extractor, &report.pages)).unwrap(),
    );
}
fn gesture(outcome: PointerOutcome) -> u64 {
    if let PointerOutcome::Dragging { gesture_id, .. } = outcome {
        gesture_id
    } else {
        panic!("expected dragging")
    }
}
#[test]
fn actual_native_points_select_forward_backward_and_viewport_transform() {
    let (_tmp, r) = report();
    let b = VersionBinding::from_report(&r, 0).unwrap();
    let v = Viewport::default();
    let quote = "immutable originals";
    let start = index(&r, 0, quote);
    let end = start + quote.chars().count() - 1;
    let a = point(&r, 0, start);
    let f = point(&r, 0, end);
    let h = hit_test(&r, &b, &v, a).unwrap();
    assert_eq!(h.char_index, start);
    let result = drag_selection(&r, &b, &v, a, f).unwrap();
    assert_eq!(result.joined_text, quote);
    assert!(result.committable_single_segment);
    assert_eq!(result.segments[0].start, start);
    assert_eq!(result.direction, Direction::Forward);
    let reverse = drag_selection(&r, &b, &v, f, a).unwrap();
    assert_eq!(reverse.joined_text, quote);
    assert_eq!(reverse.direction, Direction::Backward);
    let transformed = Viewport {
        page_origin_dip: [17.0, 29.0],
        scroll_dip: [23.0, 51.0],
        zoom: 1.75,
        device_scale: 2.0,
        generation: 7,
    };
    let to_dip = |p: Point| Point {
        x: p.x * transformed.zoom / transformed.device_scale - transformed.scroll_dip[0]
            + transformed.page_origin_dip[0],
        y: p.y * transformed.zoom / transformed.device_scale - transformed.scroll_dip[1]
            + transformed.page_origin_dip[1],
    };
    assert_eq!(
        drag_selection(&r, &b, &transformed, to_dip(a), to_dip(f))
            .unwrap()
            .joined_text,
        quote
    );
    assert_eq!(
        hit_test(&r, &b, &v, Point { x: 800.0, y: 900.0 })
            .unwrap_err()
            .code,
        SelectionErrorCode::NoHit
    );
    assert_eq!(
        hit_test(&r, &b, &v, Point { x: -1.0, y: 1.0 })
            .unwrap_err()
            .code,
        SelectionErrorCode::NoHit
    );
}
#[test]
fn actual_native_multiline_preserves_segments_and_rotated_engine_indices() {
    let (_tmp, r) = report();
    let v = Viewport::default();
    for page in 0..2 {
        let b = VersionBinding::from_report(&r, page).unwrap();
        let first = if page == 0 {
            "QUSOR native PDF qualification"
        } else {
            "QUSOR second source page"
        };
        let last = "Selection target: immutable originals.";
        let start = index(&r, page, first);
        let end = index(&r, page, last) + last.len() - 1;
        let result =
            drag_selection(&r, &b, &v, point(&r, page, start), point(&r, page, end)).unwrap();
        assert!(!result.committable_single_segment);
        assert_eq!(result.segments.len(), if page == 0 { 5 } else { 4 });
        assert!(result.joined_text.starts_with(first));
        assert!(result.joined_text.ends_with(last));
        for segment in &result.segments {
            let p = &r.pages[page];
            let a = p.characters[segment.start].utf8_start;
            let b = p.characters[segment.end - 1].utf8_end;
            assert_eq!(segment.exact, &p.page_text[a..b]);
        }
        if page == 1 {
            assert!(
                result
                    .segments
                    .windows(2)
                    .all(|pair| pair[0].start > pair[1].start)
            );
        }
        let reverse =
            drag_selection(&r, &b, &v, point(&r, page, end), point(&r, page, start)).unwrap();
        assert_eq!(reverse.joined_text, result.joined_text);
        assert_eq!(reverse.direction, Direction::Backward);
    }
}
#[test]
fn damaged_map_overlap_columns_and_versions_are_explicit_failures() {
    let (_tmp, r) = report();
    let v = Viewport::default();
    let b = VersionBinding::from_report(&r, 0).unwrap();
    let p = point(&r, 0, 0);
    let mut bad = r.clone();
    bad.pages[0].characters[0].utf8_end = usize::MAX;
    assert_eq!(
        hit_test(&bad, &b, &v, p).unwrap_err().code,
        SelectionErrorCode::InvalidReport
    );
    let mut bad = r.clone();
    bad.pages[0].characters[1].pixel_quad = bad.pages[0].characters[0].pixel_quad;
    rehash(&mut bad);
    let binding = VersionBinding::from_report(&bad, 0).unwrap();
    assert_eq!(
        hit_test(&bad, &binding, &v, p).unwrap_err().code,
        SelectionErrorCode::Ambiguous
    );
    let mut wrong = b.clone();
    wrong.frame_report_sha256 = "0".repeat(64);
    assert_eq!(
        hit_test(&r, &wrong, &v, p).unwrap_err().code,
        SelectionErrorCode::VersionMismatch
    );
    let mut invalid = v.clone();
    invalid.zoom = f64::NAN;
    assert_eq!(
        hit_test(&r, &b, &invalid, p).unwrap_err().code,
        SelectionErrorCode::InvalidViewport
    );
    let mut columns = r.clone();
    let second = index(&r, 0, "Repeated evidence stays local.");
    let end = second + "Repeated evidence stays local.".len();
    for c in &mut columns.pages[0].characters[second..end] {
        if let Some(rect) = &mut c.pdf_bounds {
            rect.left += 300.0;
            rect.right += 300.0;
        }
    }
    rehash(&mut columns);
    let binding = VersionBinding::from_report(&columns, 0).unwrap();
    let last = index(&columns, 0, "Selection target: immutable originals.")
        + "Selection target: immutable originals.".len()
        - 1;
    assert_eq!(
        drag_selection(&columns, &binding, &v, p, point(&columns, 0, last))
            .unwrap_err()
            .code,
        SelectionErrorCode::UnsupportedLayout
    );
}
#[test]
fn pointer_ownership_cancel_blank_and_stale_tokens_never_commit() {
    let (_tmp, r) = report();
    let b = VersionBinding::from_report(&r, 0).unwrap();
    let v = Viewport::default();
    let a = point(&r, 0, 0);
    let f = point(&r, 0, 4);
    let blank = Point { x: 800.0, y: 900.0 };
    let mut s = PointerSession::new(b.clone(), v.clone()).unwrap();
    assert_eq!(
        s.pointer_up(&r, &b, &v, 1, 0, f).unwrap_err().code,
        SelectionErrorCode::NotDragging
    );
    let g = gesture(s.pointer_down(&r, &b, &v, 1, a).unwrap());
    assert_eq!(
        s.pointer_down(&r, &b, &v, 2, f).unwrap_err().code,
        SelectionErrorCode::WrongPointer
    );
    assert_eq!(
        s.pointer_down(&r, &b, &v, 1, f).unwrap_err().code,
        SelectionErrorCode::WrongPointer
    );
    assert_eq!(
        s.pointer_up(&r, &b, &v, 2, g, f).unwrap_err().code,
        SelectionErrorCode::WrongPointer
    );
    s.pointer_move(&r, &b, &v, 1, g, f).unwrap();
    assert!(s.preview().is_some());
    assert_eq!(
        s.pointer_move(&r, &b, &v, 1, g, blank).unwrap_err().code,
        SelectionErrorCode::NoHit
    );
    assert!(s.preview().is_none());
    assert_eq!(
        s.pointer_up(&r, &b, &v, 1, g, blank).unwrap_err().code,
        SelectionErrorCode::NoHit
    );
    assert_eq!(
        s.pointer_up(&r, &b, &v, 1, g, f).unwrap_err().code,
        SelectionErrorCode::NotDragging
    );
    let g2 = gesture(s.pointer_down(&r, &b, &v, 1, a).unwrap());
    assert_ne!(g, g2);
    let mut old_view = v.clone();
    old_view.generation = 99;
    assert_eq!(
        s.pointer_up(&r, &b, &old_view, 1, g, f).unwrap_err().code,
        SelectionErrorCode::WrongPointer
    );
    assert!(matches!(
        s.pointer_up(&r, &b, &v, 1, g2, f).unwrap(),
        PointerOutcome::Ready { .. }
    ));
    let g3 = gesture(s.pointer_down(&r, &b, &v, 1, a).unwrap());
    s.cancel();
    assert_eq!(
        s.pointer_up(&r, &b, &v, 1, g3, f).unwrap_err().code,
        SelectionErrorCode::NotDragging
    );
    let g4 = gesture(s.pointer_down(&r, &b, &v, 1, a).unwrap());
    let mut changed = v.clone();
    changed.generation = 1;
    assert_eq!(
        s.pointer_move(&r, &b, &changed, 1, g4, f).unwrap_err().code,
        SelectionErrorCode::Cancelled
    );
    assert!(s.preview().is_none());
    assert_eq!(
        s.pointer_up(&r, &b, &v, 1, g4, f).unwrap_err().code,
        SelectionErrorCode::NotDragging
    );
    let mut s = PointerSession::new(b.clone(), v.clone()).unwrap();
    let g = gesture(s.pointer_down(&r, &b, &v, 1, a).unwrap());
    assert!(matches!(
        s.pointer_up(&r, &b, &v, 1, g, a).unwrap(),
        PointerOutcome::Click
    ));
}

fn synthetic_text(mut report: DocumentReport, values: &[u32]) -> DocumentReport {
    let originals = report.pages[0].characters.clone();
    let page = &mut report.pages[0];
    page.page_text.clear();
    page.characters.clear();
    let mut utf16 = 0;
    for (index, value) in values.iter().copied().enumerate() {
        let c = char::from_u32(value)
            .filter(|_| value != 0)
            .unwrap_or('\u{fffd}');
        let mut mapped = originals[index.min(originals.len() - 1)].clone();
        if index == 1 && values.get(1) == Some(&0x301) {
            mapped = originals[0].clone();
        }
        mapped.pdf_char_index = index;
        mapped.unicode_value = value;
        mapped.mapping = if value != 0 && char::from_u32(value).is_some() {
            "scalar"
        } else {
            "unknown"
        }
        .into();
        mapped.generated = false;
        mapped.scalar_start = index;
        mapped.scalar_end = index + 1;
        mapped.utf8_start = page.page_text.len();
        page.page_text.push(c);
        mapped.utf8_end = page.page_text.len();
        mapped.utf16_start = utf16;
        utf16 += c.len_utf16();
        mapped.utf16_end = utf16;
        if c.is_whitespace() {
            mapped.pdf_bounds = None;
            mapped.pixel_quad = None;
            mapped.crop_normalized_quad = None;
        }
        page.characters.push(mapped);
    }
    page.page_text_sha256 = sha256(page.page_text.as_bytes());
    page.text_status = if values.is_empty() {
        "no_text"
    } else if values
        .iter()
        .any(|v| *v == 0 || char::from_u32(*v).is_none())
    {
        "mapping_incomplete"
    } else {
        "mapped"
    }
    .into();
    rehash(&mut report);
    report
}
#[test]
fn synthetic_graphemes_unknown_maps_and_no_text_never_invent_ranges() {
    let (_tmp, r) = report();
    let s = synthetic_text(r.clone(), &[b'e' as u32, 0x301, 0x1f642, 32, b'z' as u32]);
    let b = VersionBinding::from_report(&s, 0).unwrap();
    let v = Viewport::default();
    let a = point(&s, 0, 0);
    let h = hit_test(&s, &b, &v, a).unwrap();
    assert_eq!((h.grapheme_start, h.grapheme_end), (0, 2));
    let selected = drag_selection(&s, &b, &v, a, point(&s, 0, 4)).unwrap();
    assert_eq!(selected.joined_text, "e\u{301}🙂 z");
    assert_eq!(
        selected.segments[0].characters[2].utf16_end
            - selected.segments[0].characters[2].utf16_start,
        2
    );
    let unknown = synthetic_text(r.clone(), &[b'e' as u32, 0]);
    let b = VersionBinding::from_report(&unknown, 0).unwrap();
    assert_eq!(
        hit_test(&unknown, &b, &v, point(&unknown, 0, 0))
            .unwrap_err()
            .code,
        SelectionErrorCode::UnknownMapping
    );
    let empty = synthetic_text(r, &[]);
    let b = VersionBinding::from_report(&empty, 0).unwrap();
    assert_eq!(
        hit_test(&empty, &b, &v, Point { x: 10.0, y: 10.0 })
            .unwrap_err()
            .code,
        SelectionErrorCode::NoText
    );
    let b = VersionBinding::from_report(&s, 0).unwrap();
    assert_eq!(
        hit_test(
            &s,
            &b,
            &v,
            Point {
                x: f64::NAN,
                y: 10.0
            }
        )
        .unwrap_err()
        .code,
        SelectionErrorCode::InvalidViewport
    );
}
