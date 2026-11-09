//! Native PDF Surface experiment: frozen report and authoritative gesture lifecycle.
use nsh_core::prelude::*;
use qusor_pdf_provider_probe::{
    DocumentReport,
    selection::{Point, PointerOutcome, PointerSession, VersionBinding, Viewport},
};
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::Mutex};

struct Surface {
    report: DocumentReport,
    report_digest: String,
    page: usize,
    zoom: f64,
    generation: u64,
    session: PointerSession,
    active_gesture: Option<u64>,
    active_pointer: Option<u64>,
    binding: VersionBinding,
    viewport: Viewport,
    closed: bool,
}
static SURFACE: Mutex<Option<Surface>> = Mutex::new(None);

/// Startup-only input, not exposed to the TSX module or plugin code.
pub fn initialize(path: &Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("report must be regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("report budget exceeded".into());
    }
    let report_digest = qusor_pdf_provider_probe::sha256(&bytes);
    let report: DocumentReport = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let binding = VersionBinding::from_report(&report, 0).map_err(|e| e.to_string())?;
    let viewport = Viewport {
        generation: 1,
        ..Viewport::default()
    };
    if let Err(error) = qusor_pdf_provider_probe::selection::hit_test(
        &report,
        &binding,
        &viewport,
        Point { x: 0.0, y: 0.0 },
    ) && matches!(
        error.code,
        qusor_pdf_provider_probe::selection::SelectionErrorCode::InvalidReport
            | qusor_pdf_provider_probe::selection::SelectionErrorCode::VersionMismatch
    ) {
        return Err(error.to_string());
    }
    let session =
        PointerSession::new(binding.clone(), viewport.clone()).map_err(|e| e.to_string())?;
    *SURFACE.lock().map_err(|_| "surface lock poisoned")? = Some(Surface {
        report,
        report_digest,
        page: 0,
        zoom: 1.0,
        generation: 1,
        session,
        active_gesture: None,
        active_pointer: None,
        binding,
        viewport,
        closed: false,
    });
    Ok(())
}

/// Frozen native PDF Surface operations for the experimental desktop host.
#[module]
pub struct PdfSurface;

fn with_surface(f: impl FnOnce(&mut Surface) -> Result<Value, String>) -> String {
    let result = (|| {
        let mut state = SURFACE.lock().map_err(|_| "surface lock poisoned")?;
        let surface = state.as_mut().ok_or("surface unavailable")?;
        f(surface)
    })();
    let value = match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    let text = value.to_string();
    eprintln!("PDF_SURFACE {text}");
    text
}
fn context(surface: &mut Surface, generation: u64) -> Result<(), String> {
    if surface.closed || generation != surface.generation {
        return Err("stale or closed surface".into());
    }
    Ok(())
}
fn configuration(surface: &Surface) -> Value {
    let p = &surface.report.pages[surface.page];
    json!({"generation":surface.generation,"page":surface.page,"pages":surface.report.pages.len(),"zoom":surface.zoom,
        "width":p.pixel_width as f64 * surface.zoom,"height":p.pixel_height as f64 * surface.zoom,
        "binding":surface.binding})
}
fn outcome(surface: &Surface, result: PointerOutcome) -> Value {
    let (selection, gesture) = match &result {
        PointerOutcome::Ready { selection } => (Some(selection), None),
        PointerOutcome::Dragging {
            preview,
            gesture_id,
        } => (preview.as_ref(), Some(*gesture_id)),
        _ => (None, None),
    };
    let boxes: Vec<Value> = selection
        .into_iter()
        .flat_map(|s| &s.segments)
        .flat_map(|s| &s.characters)
        .filter_map(|c| c.pixel_quad)
        .map(|q| {
            let left = q.iter().map(|p| p[0]).min().unwrap() as f64 * surface.zoom;
            let top = q.iter().map(|p| p[1]).min().unwrap() as f64 * surface.zoom;
            let right = q.iter().map(|p| p[0]).max().unwrap() as f64 * surface.zoom;
            let bottom = q.iter().map(|p| p[1]).max().unwrap() as f64 * surface.zoom;
            json!([left, top, right - left, bottom - top])
        })
        .collect();
    json!({"generation":surface.generation,"gesture":gesture,"outcome":result,
        "text":selection.map(|s|s.joined_text.as_str()),"boxes":boxes})
}
#[module]
impl PdfSurface {
    /// Reconfigure atomically, cancelling any prior pointer and selection.
    pub fn configure(page: u32, zoom: f64, expected_report_digest: String) -> String {
        with_surface(|s| {
            if expected_report_digest != s.report_digest {
                return Err("display/report binding mismatch".into());
            }
            if s.closed
                || page as usize >= s.report.pages.len()
                || !zoom.is_finite()
                || !(0.25..=1.25).contains(&zoom)
            {
                return Err("invalid surface configuration".into());
            }
            s.session.cancel();
            s.active_gesture = None;
            s.active_pointer = None;
            s.generation = s.generation.checked_add(1).ok_or("generation exhausted")?;
            s.page = page as usize;
            s.zoom = zoom;
            s.binding =
                VersionBinding::from_report(&s.report, s.page).map_err(|e| e.to_string())?;
            s.viewport = Viewport {
                zoom,
                generation: s.generation,
                ..Viewport::default()
            };
            s.session = PointerSession::new(s.binding.clone(), s.viewport.clone())
                .map_err(|e| e.to_string())?;
            Ok(configuration(s))
        })
    }
    /// Begin a single desktop mouse gesture on a real glyph.
    pub fn down(generation: u64, pointer: u64, x: f64, y: f64) -> String {
        with_surface(|s| {
            context(s, generation)?;
            let result = s
                .session
                .pointer_down(&s.report, &s.binding, &s.viewport, pointer, Point { x, y })
                .map_err(|e| e.to_string())?;
            if let PointerOutcome::Dragging { gesture_id, .. } = &result {
                s.active_gesture = Some(*gesture_id);
                s.active_pointer = Some(pointer);
            }
            Ok(outcome(s, result))
        })
    }
    /// Move only the current desktop mouse gesture; stale tokens do not cancel its successor.
    pub fn motion(generation: u64, pointer: u64, gesture: u64, x: f64, y: f64) -> String {
        with_surface(|s| {
            context(s, generation)?;
            if s.active_pointer != Some(pointer) {
                return Err("pointer does not own gesture".into());
            }
            if s.active_gesture != Some(gesture) {
                return Err("stale gesture token".into());
            }
            let result = s.session.pointer_move(
                &s.report,
                &s.binding,
                &s.viewport,
                pointer,
                gesture,
                Point { x, y },
            );
            match result {
                Ok(r) => Ok(outcome(s, r)),
                Err(e) => {
                    s.session.cancel();
                    s.active_gesture = None;
                    s.active_pointer = None;
                    Err(e.to_string())
                }
            }
        })
    }
    /// Complete only the current gesture; invalid endpoints clear its preview.
    pub fn up(generation: u64, pointer: u64, gesture: u64, x: f64, y: f64) -> String {
        with_surface(|s| {
            context(s, generation)?;
            if s.active_pointer != Some(pointer) {
                return Err("pointer does not own gesture".into());
            }
            if s.active_gesture != Some(gesture) {
                return Err("stale gesture token".into());
            }
            s.active_gesture = None;
            s.active_pointer = None;
            let result = s.session.pointer_up(
                &s.report,
                &s.binding,
                &s.viewport,
                pointer,
                gesture,
                Point { x, y },
            );
            match result {
                Ok(r) => Ok(outcome(s, r)),
                Err(e) => {
                    s.session.cancel();
                    s.active_gesture = None;
                    s.active_pointer = None;
                    Err(e.to_string())
                }
            }
        })
    }
    /// Cancel and clear the current gesture.
    pub fn cancel(generation: u64) -> String {
        with_surface(|s| {
            context(s, generation)?;
            let result = s.session.cancel();
            s.active_gesture = None;
            s.active_pointer = None;
            Ok(outcome(s, result))
        })
    }
    /// Permanently close this surface generation.
    pub fn close(generation: u64) -> String {
        with_surface(|s| {
            context(s, generation)?;
            s.session.cancel();
            s.closed = true;
            s.active_gesture = None;
            s.active_pointer = None;
            s.generation = s.generation.checked_add(1).ok_or("generation exhausted")?;
            Ok(json!({"closed":true,"generation":s.generation}))
        })
    }
}
#[native_module("qusor/pdf-surface")]
fn native(registry: &mut Registry) {
    registry.module::<PdfSurface>();
    registry.module::<plugin_surface::ReaderSurface>();
    registry.module::<dynamic_module::DynamicReader>();
    registry.module::<plugin_surface::native_editor::NativeEditor>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, process::Command};
    static TEST_GATE: Mutex<()> = Mutex::new(());
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-surface-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn fixture() -> (Scratch, String, DocumentReport) {
        let tmp = Scratch::new();
        let provider = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../pdf-provider");
        let adjacent = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap()
            .join("qusor-pdf-provider-probe");
        let binary = std::env::var_os("QUSOR_PDF_PROVIDER_PROBE").map(PathBuf::from)
            .unwrap_or_else(|| if adjacent.is_file() { adjacent } else { provider.join("target/debug/qusor-pdf-provider-probe") });
        let library = std::env::var_os("QUSOR_PDF_LIBRARY").map(PathBuf::from).unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
                provider.join(format!("target/pdfium-macos-{arch}/lib/libpdfium.dylib"))
            } else { provider.join("target/pdfium/lib/libpdfium.so") }
        });
        let output = Command::new(binary)
            .arg("inspect")
            .arg(library)
            .arg(provider.join("fixtures/text-two-pages.pdf"))
            .arg(tmp.0.join("render"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        let bytes = serde_json::to_vec(&value["report"]).unwrap();
        let report = serde_json::from_slice(&bytes).unwrap();
        let digest = qusor_pdf_provider_probe::sha256(&bytes);
        let path = tmp.0.join("report.json");
        std::fs::write(&path, bytes).unwrap();
        initialize(&path).unwrap();
        (tmp, digest, report)
    }
    fn value(raw: String) -> Value {
        serde_json::from_str(&raw).unwrap()
    }
    fn center(report: &DocumentReport, page: usize, index: usize, zoom: f64) -> Point {
        let q = report.pages[page].characters[index].pixel_quad.unwrap();
        Point {
            x: q.iter().map(|p| p[0] as f64).sum::<f64>() / 4.0 * zoom,
            y: q.iter().map(|p| p[1] as f64).sum::<f64>() / 4.0 * zoom,
        }
    }
    fn configure(page: u32, zoom: f64, digest: &str) -> u64 {
        let r = value(PdfSurface::configure(page, zoom, digest.into()));
        assert_eq!(r["ok"], true);
        r["value"]["generation"].as_u64().unwrap()
    }
    fn down(g: u64, p: Point) -> u64 {
        let r = value(PdfSurface::down(g, 1, p.x, p.y));
        assert_eq!(r["ok"], true);
        r["value"]["gesture"].as_u64().unwrap()
    }
    #[test]
    fn startup_budget_and_display_digest_are_enforced() {
        let _guard = TEST_GATE.lock().unwrap();
        let tmp = Scratch::new();
        let file = tmp.0.join("oversized.json");
        std::fs::write(&file, vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
        assert_eq!(initialize(&file).unwrap_err(), "report budget exceeded");
        let (_tmp, digest, _report) = fixture();
        let result = value(PdfSurface::configure(0, 0.5, "0".repeat(64)));
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"], "display/report binding mismatch");
        configure(0, 0.5, &digest);
    }
    #[test]
    fn page_and_zoom_changes_reject_old_events_without_cancelling_successor() {
        let _guard = TEST_GATE.lock().unwrap();
        let (_tmp, digest, report) = fixture();
        let old = configure(0, 0.5, &digest);
        let p = center(&report, 0, 0, 0.5);
        let gesture = down(old, p);
        let current = configure(1, 0.5, &digest);
        let p = center(&report, 1, 0, 0.5);
        let next = down(current, p);
        assert_eq!(
            value(PdfSurface::up(old, 1, gesture, p.x, p.y))["ok"],
            false
        );
        assert_eq!(
            value(PdfSurface::up(current, 1, gesture, p.x, p.y))["ok"],
            false
        );
        assert_eq!(
            value(PdfSurface::up(current, 2, next, p.x, p.y))["ok"],
            false
        );
        assert_eq!(
            value(PdfSurface::up(current, 1, next, p.x, p.y))["ok"],
            true
        );
        let old = current;
        let gesture = down(old, p);
        let current = configure(1, 0.6, &digest);
        assert_eq!(
            value(PdfSurface::motion(old, 1, gesture, p.x, p.y))["ok"],
            false
        );
        let p = center(&report, 1, 0, 0.6);
        let next = down(current, p);
        assert_eq!(
            value(PdfSurface::up(current, 1, next, p.x, p.y))["ok"],
            true
        );
    }
    #[test]
    fn invalid_endpoint_clears_gesture_and_closed_surface_rejects_all_events() {
        let _guard = TEST_GATE.lock().unwrap();
        let (_tmp, digest, report) = fixture();
        let g = configure(0, 0.5, &digest);
        let p = center(&report, 0, 0, 0.5);
        let token = down(g, p);
        assert_eq!(
            value(PdfSurface::up(g, 1, token, -10.0, -10.0))["ok"],
            false
        );
        assert_eq!(value(PdfSurface::up(g, 1, token, p.x, p.y))["ok"], false);
        let token = down(g, p);
        assert_eq!(value(PdfSurface::close(g))["ok"], true);
        assert_eq!(value(PdfSurface::up(g, 1, token, p.x, p.y))["ok"], false);
        assert_eq!(value(PdfSurface::down(g, 1, p.x, p.y))["ok"], false);
        assert_eq!(value(PdfSurface::configure(0, 0.5, digest))["ok"], false);
    }
}

pub mod plugin_surface;
/// Ordinary signed-package/Host-registry integration probe.
mod reader_authority;

/// Runtime-selected native file/Blob qualification, sharing the existing reader backend.
pub mod dynamic_module;
pub mod dynamic_worker;
