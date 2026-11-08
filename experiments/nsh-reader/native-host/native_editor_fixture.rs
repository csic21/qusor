//! Restricted synthetic-source setup; no native rendering/ack qualification.
use super::*;
pub(super) struct Fixture {
    data: PathBuf,
    _serial: std::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    pub(super) fn new(label: &str) -> (Self, String, String) {
        let serial = super::super::SERVICE_TEST.lock().unwrap();
        assert!(!BUSY.load(Ordering::SeqCst));
        CANCEL.store(false, Ordering::SeqCst);
        assert!(EDITOR.lock().unwrap().is_none());
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reader_resources = root.join("../target/plugin-surface-resources");
        initialize(&root.join("../target/dynamic-pdf-resources")).unwrap();
        let data = std::env::temp_dir().join(format!(
            "native-editor-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&data).unwrap();
        super::super::initialize(&reader_resources, &data).unwrap();
        let opened: Value =
            serde_json::from_str(&ReaderSurface::open(current_report_digest().unwrap())).unwrap();
        assert_eq!(opened["ok"], true);
        let cfg = &opened["value"];
        let handle = cfg["handle"].as_str().unwrap().to_owned();
        let generation = cfg["generation"].as_u64().unwrap();
        let ready: Value = serde_json::from_str(&ReaderSurface::ready(
            handle.clone(),
            generation,
            cfg["frame_id"].as_str().unwrap().into(),
            current_report_digest().unwrap(),
        ))
        .unwrap();
        assert_eq!(ready["ok"], true);
        let point = |index: usize| {
            let svc = service().unwrap();
            let g = svc.gate.lock().unwrap();
            let q = g.report.pages[0].characters[index].pixel_quad.unwrap();
            Point {
                x: q.iter().map(|p| p[0] as f64).sum::<f64>() / 4.,
                y: q.iter().map(|p| p[1] as f64).sum::<f64>() / 4.,
            }
        };
        let a = point(0);
        let b = point(4);
        let down: Value = serde_json::from_str(&ReaderSurface::down(
            handle.clone(),
            generation,
            71,
            a.x,
            a.y,
        ))
        .unwrap();
        assert_eq!(down["ok"], true);
        let up: Value = serde_json::from_str(&ReaderSurface::up(
            handle,
            generation,
            71,
            down["value"]["gesture"].as_u64().unwrap(),
            b.x,
            b.y,
        ))
        .unwrap();
        assert_eq!(up["ok"], true);
        let candidate = up["value"]["candidate"]["candidate_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let value = begin(candidate.clone(), "a".into()).unwrap();
        let binding = value["binding"].as_str().unwrap().into();
        (
            Self {
                data,
                _serial: serial,
            },
            candidate,
            binding,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
        CANCEL.store(false, Ordering::SeqCst);
        EDITOR.lock().unwrap().take();
        *STATUS.lock().unwrap() = Value::Null;
        super::super::shutdown().unwrap();
        std::fs::remove_dir_all(&self.data).unwrap();
    }
}
