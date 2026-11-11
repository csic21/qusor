//! Native pointer geometry over a frozen PDF report. No nearest-text fallback.
use crate::{Character, DocumentReport, PageReport, validate_report};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};
use unicode_segmentation::UnicodeSegmentation;

pub const LAYOUT_PROFILE: &str = "qusor-plain-ltr-lines-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionErrorCode {
    InvalidReport,
    InvalidViewport,
    VersionMismatch,
    NoText,
    UnknownMapping,
    NoHit,
    Ambiguous,
    UnsupportedLayout,
    ClickOnly,
    NotDragging,
    WrongPointer,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionError {
    pub code: SelectionErrorCode,
    pub message: String,
}
impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl Error for SelectionError {}
pub type Result<T> = std::result::Result<T, SelectionError>;
fn fail<T>(code: SelectionErrorCode, message: &str) -> Result<T> {
    Err(SelectionError {
        code,
        message: message.into(),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
/// Pointer coordinates are native logical DIPs. Bitmap pixels are converted by
/// `(point + scroll_dip - page_origin_dip) * device_scale / zoom`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub page_origin_dip: [f64; 2],
    pub scroll_dip: [f64; 2],
    pub zoom: f64,
    pub device_scale: f64,
    pub generation: u64,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            page_origin_dip: [0.0; 2],
            scroll_dip: [0.0; 2],
            zoom: 1.0,
            device_scale: 1.0,
            generation: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionBinding {
    pub original_sha256: String,
    pub frame_report_sha256: String,
    pub page_text_sha256: String,
    pub page_index: usize,
}
impl VersionBinding {
    pub fn from_report(report: &DocumentReport, page_index: usize) -> Result<Self> {
        let page = report.pages.get(page_index).ok_or_else(|| SelectionError {
            code: SelectionErrorCode::VersionMismatch,
            message: "page missing".into(),
        })?;
        Ok(Self {
            original_sha256: report.original_sha256.clone(),
            frame_report_sha256: report.extraction_sha256.clone(),
            page_text_sha256: page.page_text_sha256.clone(),
            page_index,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub char_index: usize,
    pub grapheme_start: usize,
    pub grapheme_end: usize,
    pub render_point: Point,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub exact: String,
    pub characters: Vec<Character>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Forward,
    Backward,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DragSelection {
    pub binding: VersionBinding,
    pub layout_profile: String,
    pub order_basis: String,
    pub committable_single_segment: bool,
    pub segments: Vec<Segment>,
    pub separator: String,
    pub joined_text: String,
    pub direction: Direction,
}

fn valid_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn page<'a>(report: &'a DocumentReport, binding: &VersionBinding) -> Result<&'a PageReport> {
    validate_report(report).map_err(|e| SelectionError {
        code: SelectionErrorCode::InvalidReport,
        message: e.to_string(),
    })?;
    if !valid_digest(&report.original_sha256) || !valid_digest(&report.extraction_sha256) {
        return fail(SelectionErrorCode::InvalidReport, "invalid version digest");
    }
    let page = report
        .pages
        .get(binding.page_index)
        .ok_or_else(|| SelectionError {
            code: SelectionErrorCode::VersionMismatch,
            message: "page missing".into(),
        })?;
    if binding.original_sha256 != report.original_sha256
        || binding.frame_report_sha256 != report.extraction_sha256
        || binding.page_text_sha256 != page.page_text_sha256
    {
        return fail(
            SelectionErrorCode::VersionMismatch,
            "source/extraction/page version changed",
        );
    }
    if page.text_status == "no_text" {
        return fail(
            SelectionErrorCode::NoText,
            "page has no embedded text layer",
        );
    }
    if page.text_status != "mapped" {
        return fail(
            SelectionErrorCode::UnknownMapping,
            "page contains unknown Unicode mapping",
        );
    }
    Ok(page)
}
fn validate_viewport(viewport: &Viewport) -> Result<()> {
    if !viewport
        .page_origin_dip
        .iter()
        .chain(&viewport.scroll_dip)
        .all(|v| v.is_finite() && v.abs() <= 1e9)
        || !viewport.zoom.is_finite()
        || !(0.0625..=64.0).contains(&viewport.zoom)
        || !viewport.device_scale.is_finite()
        || !(0.5..=8.0).contains(&viewport.device_scale)
    {
        return fail(
            SelectionErrorCode::InvalidViewport,
            "nonfinite or out-of-budget viewport transform",
        );
    }
    Ok(())
}
fn render_point(viewport: &Viewport, point: Point, page: &PageReport) -> Result<Point> {
    validate_viewport(viewport)?;
    if !point.x.is_finite() || !point.y.is_finite() {
        return fail(
            SelectionErrorCode::InvalidViewport,
            "pointer must be finite",
        );
    }
    let p = Point {
        x: (point.x + viewport.scroll_dip[0] - viewport.page_origin_dip[0]) * viewport.device_scale
            / viewport.zoom,
        y: (point.y + viewport.scroll_dip[1] - viewport.page_origin_dip[1]) * viewport.device_scale
            / viewport.zoom,
    };
    if p.x < 0.0
        || p.y < 0.0
        || p.x >= f64::from(page.pixel_width)
        || p.y >= f64::from(page.pixel_height)
    {
        return fail(
            SelectionErrorCode::NoHit,
            "pointer lies outside the rendered page",
        );
    }
    Ok(p)
}
fn inside(quad: [[i32; 2]; 4], p: Point) -> Result<bool> {
    if quad.iter().flatten().any(|v| v.abs_diff(0) > 8192) {
        return fail(
            SelectionErrorCode::InvalidReport,
            "glyph pixel geometry exceeds budget",
        );
    }
    let mut positive = false;
    let mut negative = false;
    let mut area = 0.0;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let cross = (f64::from(b[0]) - f64::from(a[0])) * (p.y - f64::from(a[1]))
            - (f64::from(b[1]) - f64::from(a[1])) * (p.x - f64::from(a[0]));
        positive |= cross > 1e-8;
        negative |= cross < -1e-8;
        area += f64::from(a[0]) * f64::from(b[1]) - f64::from(b[0]) * f64::from(a[1]);
    }
    Ok(area.abs() > 1e-8 && !(positive && negative))
}
fn grapheme(page: &PageReport, index: usize) -> Result<(usize, usize)> {
    let mapped = page.characters.get(index).ok_or_else(|| SelectionError {
        code: SelectionErrorCode::InvalidReport,
        message: "missing glyph map".into(),
    })?;
    for (byte, text) in page.page_text.grapheme_indices(true) {
        if byte <= mapped.utf8_start && mapped.utf8_start < byte + text.len() {
            let start = page.page_text[..byte].chars().count();
            return Ok((start, start + text.chars().count()));
        }
    }
    fail(SelectionErrorCode::InvalidReport, "glyph has no grapheme")
}
fn hit_page(page: &PageReport, viewport: &Viewport, point: Point) -> Result<Hit> {
    let p = render_point(viewport, point, page)?;
    let mut groups = BTreeSet::new();
    let mut first = None;
    for (index, c) in page.characters.iter().enumerate() {
        if c.generated
            || c.mapping != "scalar"
            || char::from_u32(c.unicode_value).is_some_and(char::is_whitespace)
        {
            continue;
        }
        if let Some(quad) = c.pixel_quad
            && inside(quad, p)?
        {
            groups.insert(grapheme(page, index)?);
            first.get_or_insert(index);
        }
    }
    if groups.len() > 1 {
        return fail(
            SelectionErrorCode::Ambiguous,
            "pointer intersects multiple graphemes",
        );
    }
    let (start, end) = groups.into_iter().next().ok_or_else(|| SelectionError {
        code: SelectionErrorCode::NoHit,
        message: "pointer does not intersect a visible glyph".into(),
    })?;
    Ok(Hit {
        char_index: first.unwrap(),
        grapheme_start: start,
        grapheme_end: end,
        render_point: p,
    })
}
pub fn hit_test(
    report: &DocumentReport,
    binding: &VersionBinding,
    viewport: &Viewport,
    point: Point,
) -> Result<Hit> {
    hit_page(page(report, binding)?, viewport, point)
}
#[derive(Debug)]
struct Line {
    start: usize,
    end: usize,
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
    typical_width: f64,
}
fn line(page: &PageReport, start: usize, end: usize) -> Result<Option<Line>> {
    let glyphs: Vec<_> = page.characters[start..end]
        .iter()
        .filter(|c| {
            !c.generated && !char::from_u32(c.unicode_value).is_some_and(char::is_whitespace)
        })
        .collect();
    if glyphs.is_empty() {
        return Ok(None);
    }
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut top = f64::INFINITY;
    let mut widths = Vec::new();
    let mut previous: Option<(f64, f64)> = None;
    for glyph in glyphs {
        let rect = glyph.pdf_bounds.as_ref().ok_or_else(|| SelectionError {
            code: SelectionErrorCode::UnsupportedLayout,
            message: "glyph has no PDF geometry".into(),
        })?;
        let x = f64::from(rect.left);
        let r = f64::from(rect.right);
        let b = f64::from(rect.bottom);
        let t = f64::from(rect.top);
        if r <= x || t <= b {
            return fail(
                SelectionErrorCode::UnsupportedLayout,
                "zero-area glyph cannot establish a line",
            );
        }
        if let Some((prev_left, _)) = previous
            && x < prev_left - 0.01
        {
            return fail(
                SelectionErrorCode::UnsupportedLayout,
                "engine line is not left-to-right in PDF user space",
            );
        }
        left = left.min(x);
        right = right.max(r);
        // Punctuation such as a period has no vertical overlap with the body
        // of a capital letter. Establish the band from letters/digits only.
        if char::from_u32(glyph.unicode_value).is_some_and(char::is_alphanumeric) {
            bottom = bottom.max(b);
            top = top.min(t);
        }
        widths.push(r - x);
        previous = Some((x, r));
    }
    if !top.is_finite() || !bottom.is_finite() || top <= bottom {
        return fail(
            SelectionErrorCode::UnsupportedLayout,
            "engine line has no common PDF-space text band",
        );
    }
    widths.sort_by(f64::total_cmp);
    let typical = widths[widths.len() / 2];
    let mut previous_right = None;
    for c in page.characters[start..end].iter().filter(|c| !c.generated) {
        if let Some(rect) = &c.pdf_bounds {
            if let Some(prev) = previous_right
                && f64::from(rect.left) - prev > (typical * 6.0).max((top - bottom) * 3.0)
            {
                return fail(
                    SelectionErrorCode::UnsupportedLayout,
                    "line contains an unexplained column-sized gap",
                );
            }
            previous_right = Some(f64::from(rect.right));
        }
    }
    Ok(Some(Line {
        start,
        end,
        left,
        right,
        bottom,
        top,
        typical_width: typical,
    }))
}
fn lines(page: &PageReport, multiline: bool) -> Result<Vec<Line>> {
    let mut result = Vec::new();
    let mut start = 0;
    for (index, c) in page.page_text.chars().enumerate() {
        if c == '\r' || c == '\n' {
            if let Some(l) = line(page, start, index)? {
                result.push(l)
            }
            start = index + 1
        }
    }
    if let Some(l) = line(page, start, page.characters.len())? {
        result.push(l)
    }
    if multiline {
        result.sort_by(|a, b| b.top.total_cmp(&a.top));
        for pair in result.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            if b.top >= a.bottom {
                return fail(
                    SelectionErrorCode::UnsupportedLayout,
                    "PDF line bands overlap; column/line order is uncertain",
                );
            }
            if (a.left - b.left).abs() > a.typical_width.max(b.typical_width) * 2.0 {
                return fail(
                    SelectionErrorCode::UnsupportedLayout,
                    "line origins do not qualify as one aligned LTR paragraph",
                );
            }
            if a.right <= b.left || b.right <= a.left {
                return fail(
                    SelectionErrorCode::UnsupportedLayout,
                    "paragraph lines have disjoint horizontal extents",
                );
            }
        }
    }
    Ok(result)
}
fn segment(page: &PageReport, start: usize, end: usize) -> Result<Segment> {
    let chars = page
        .characters
        .get(start..end)
        .ok_or_else(|| SelectionError {
            code: SelectionErrorCode::InvalidReport,
            message: "invalid segment range".into(),
        })?;
    let first = chars.first().ok_or_else(|| SelectionError {
        code: SelectionErrorCode::UnsupportedLayout,
        message: "empty segment".into(),
    })?;
    let last = chars.last().unwrap();
    let exact = page
        .page_text
        .get(first.utf8_start..last.utf8_end)
        .ok_or_else(|| SelectionError {
            code: SelectionErrorCode::InvalidReport,
            message: "invalid segment byte range".into(),
        })?;
    Ok(Segment {
        start,
        end,
        exact: exact.into(),
        characters: chars.to_vec(),
    })
}
pub fn drag_selection(
    report: &DocumentReport,
    binding: &VersionBinding,
    viewport: &Viewport,
    anchor: Point,
    focus: Point,
) -> Result<DragSelection> {
    let page = page(report, binding)?;
    let a = hit_page(page, viewport, anchor)?;
    let f = hit_page(page, viewport, focus)?;
    if a.grapheme_start == f.grapheme_start && a.grapheme_end == f.grapheme_end {
        return fail(
            SelectionErrorCode::ClickOnly,
            "same-grapheme click creates no committed selection",
        );
    }
    let engine_lines = lines(page, false)?;
    let a_line = engine_lines
        .iter()
        .position(|l| l.start <= a.grapheme_start && a.grapheme_end <= l.end)
        .ok_or_else(|| SelectionError {
            code: SelectionErrorCode::UnsupportedLayout,
            message: "anchor crosses line boundary".into(),
        })?;
    let f_line = engine_lines
        .iter()
        .position(|l| l.start <= f.grapheme_start && f.grapheme_end <= l.end)
        .ok_or_else(|| SelectionError {
            code: SelectionErrorCode::UnsupportedLayout,
            message: "focus crosses line boundary".into(),
        })?;
    let mut segments = Vec::new();
    let direction;
    if a_line == f_line {
        direction = if a.grapheme_start < f.grapheme_start {
            Direction::Forward
        } else {
            Direction::Backward
        };
        segments.push(segment(
            page,
            a.grapheme_start.min(f.grapheme_start),
            a.grapheme_end.max(f.grapheme_end),
        )?);
    } else {
        let ordered = lines(page, true)?;
        let ai = ordered
            .iter()
            .position(|l| l.start <= a.grapheme_start && a.grapheme_end <= l.end)
            .unwrap();
        let fi = ordered
            .iter()
            .position(|l| l.start <= f.grapheme_start && f.grapheme_end <= l.end)
            .unwrap();
        let (first, last, begin, end) = if ai < fi {
            direction = Direction::Forward;
            (ai, fi, a.grapheme_start, f.grapheme_end)
        } else {
            direction = Direction::Backward;
            (fi, ai, f.grapheme_start, a.grapheme_end)
        };
        for (i, l) in ordered.iter().enumerate().take(last + 1).skip(first) {
            segments.push(segment(
                page,
                if i == first { begin } else { l.start },
                if i == last { end } else { l.end },
            )?);
        }
    }
    let joined_text = segments
        .iter()
        .map(|s| s.exact.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(DragSelection {
        binding: binding.clone(),
        layout_profile: LAYOUT_PROFILE.into(),
        order_basis: "unrotated_pdf_user_space_line_bands".into(),
        committable_single_segment: segments.len() == 1,
        segments,
        separator: "\n".into(),
        joined_text,
        direction,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PointerOutcome {
    Dragging {
        gesture_id: u64,
        preview: Option<DragSelection>,
    },
    Ready {
        selection: DragSelection,
    },
    Click,
    Cancelled,
}
static NEXT_GESTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[derive(Debug)]
pub struct PointerSession {
    binding: VersionBinding,
    viewport: Viewport,
    active: Option<(u64, u64, Point)>,
    preview: Option<DragSelection>,
}
impl PointerSession {
    pub fn new(binding: VersionBinding, viewport: Viewport) -> Result<Self> {
        validate_viewport(&viewport)?;
        Ok(Self {
            binding,
            viewport,
            active: None,
            preview: None,
        })
    }
    fn context(&mut self, binding: &VersionBinding, viewport: &Viewport) -> Result<()> {
        if &self.binding != binding || &self.viewport != viewport {
            self.cancel();
            return fail(
                SelectionErrorCode::Cancelled,
                "document or viewport changed during gesture",
            );
        }
        Ok(())
    }
    pub fn pointer_down(
        &mut self,
        report: &DocumentReport,
        binding: &VersionBinding,
        viewport: &Viewport,
        pointer_id: u64,
        point: Point,
    ) -> Result<PointerOutcome> {
        if self.active.is_some() {
            return fail(
                SelectionErrorCode::WrongPointer,
                "gesture already has a pointer owner",
            );
        }
        self.context(binding, viewport)?;
        self.cancel();
        hit_test(report, binding, viewport, point)?;
        let gesture_id = NEXT_GESTURE
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .map_err(|_| SelectionError {
                code: SelectionErrorCode::Cancelled,
                message: "gesture sequence exhausted".into(),
            })?;
        self.active = Some((pointer_id, gesture_id, point));
        Ok(PointerOutcome::Dragging {
            gesture_id,
            preview: None,
        })
    }
    pub fn pointer_move(
        &mut self,
        report: &DocumentReport,
        binding: &VersionBinding,
        viewport: &Viewport,
        pointer_id: u64,
        gesture_id: u64,
        point: Point,
    ) -> Result<PointerOutcome> {
        let (active, active_gesture, anchor) = self.active.ok_or_else(|| SelectionError {
            code: SelectionErrorCode::NotDragging,
            message: "no active gesture".into(),
        })?;
        if active != pointer_id || active_gesture != gesture_id {
            return fail(
                SelectionErrorCode::WrongPointer,
                "pointer or gesture token does not own this gesture",
            );
        }
        self.context(binding, viewport)?;
        self.preview = None;
        match drag_selection(report, binding, viewport, anchor, point) {
            Ok(selection) => {
                self.preview = Some(selection.clone());
                Ok(PointerOutcome::Dragging {
                    gesture_id,
                    preview: Some(selection),
                })
            }
            Err(e) if e.code == SelectionErrorCode::ClickOnly => Ok(PointerOutcome::Dragging {
                gesture_id,
                preview: None,
            }),
            Err(e) => Err(e),
        }
    }
    pub fn pointer_up(
        &mut self,
        report: &DocumentReport,
        binding: &VersionBinding,
        viewport: &Viewport,
        pointer_id: u64,
        gesture_id: u64,
        point: Point,
    ) -> Result<PointerOutcome> {
        let (active, active_gesture, anchor) = self.active.ok_or_else(|| SelectionError {
            code: SelectionErrorCode::NotDragging,
            message: "no active gesture".into(),
        })?;
        if active != pointer_id || active_gesture != gesture_id {
            return fail(
                SelectionErrorCode::WrongPointer,
                "pointer or gesture token does not own this gesture",
            );
        }
        self.context(binding, viewport)?;
        self.cancel();
        match drag_selection(report, binding, viewport, anchor, point) {
            Ok(selection) => Ok(PointerOutcome::Ready { selection }),
            Err(e) if e.code == SelectionErrorCode::ClickOnly => Ok(PointerOutcome::Click),
            Err(e) => Err(e),
        }
    }
    pub fn cancel(&mut self) -> PointerOutcome {
        self.active = None;
        self.preview = None;
        PointerOutcome::Cancelled
    }
    pub fn preview(&self) -> Option<&DragSelection> {
        self.preview.as_ref()
    }
}
