//! Dedicated bounded native parser and image encoding jobs. Never runs in a Wasm broker.
use crate::plugin_surface::{self, PreparedDocument};
use qusor_pdf_provider_probe::{DocumentReport, sha256};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
pub type Result<T> = std::result::Result<T, String>;
pub const MAX_PDF: usize = 16 * 1024 * 1024;
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_BITMAP: usize = 16 * 1024 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LIBRARY_SHA: &str = "1c251efce460d67437a4c76d4be9033ae2f17bf5603f5bff1af10209b94ca723";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const LIBRARY_SHA: &str = "a45c44645523968525c710462691dd974f4efb1013ce1c249919a0939cfb373a";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const LIBRARY_SHA: &str = "a9f428a16f46bab89646c8724b348e01c3fe707e3126d9356026cd876a74aecb";
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
const LIBRARY_SHA: &str = "unqualified-pdfium-target";
static SERIAL_WORKER: Mutex<()> = Mutex::new(());
fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(text)?;
    let metadata = file.metadata().map_err(text)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("native input budget exceeded or nonregular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(text)?;
    if bytes.len() > limit {
        return Err("native input budget exceeded".into());
    }
    Ok(bytes)
}
pub struct Artifact {
    pub directory: PathBuf,
    pub report: Arc<DocumentReport>,
    pub report_digest: String,
    cache: Mutex<Vec<(usize, Arc<[u8]>)>>,
}
impl Drop for Artifact {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
struct PendingDir {
    path: Option<PathBuf>,
    retain: Arc<AtomicBool>,
}
impl Drop for PendingDir {
    fn drop(&mut self) {
        if !self.retain.load(Ordering::SeqCst)
            && let Some(path) = &self.path
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}
fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().ok_or("native marker parent missing")?)
        .and_then(|file| file.sync_all())
        .map_err(text)
}
fn marker_write(path: &Path, value: &Value, create: bool) -> Result<()> {
    let target = if create {
        path.to_path_buf()
    } else {
        path.with_extension("update")
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(text)?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(text)?;
    drop(file);
    if !create {
        fs::rename(&target, path).map_err(text)?;
    }
    sync_parent(path)
}
struct ChildLease {
    child: std::process::Child,
    quarantine: PathBuf,
    retain: Arc<AtomicBool>,
    marker: Option<PathBuf>,
    identity: Option<runtime_process::ProcessIdentity>,
    reaped: bool,
}
impl ChildLease {
    fn quarantine(&self, message: &str) {
        self.retain.store(true, Ordering::SeqCst);
        let _ = fs::write(&self.quarantine, message);
        let _ = sync_parent(&self.quarantine);
    }
    fn confirmed(&mut self) -> Result<()> {
        self.reaped = true;
        if self.marker.is_some() && self.identity.is_none() {
            self.quarantine("spawn occurred without verified identity; no automatic replacement\n");
            return Err("native worker identity was not confirmed; parser quarantined".into());
        }
        if let Some(marker) = self.marker.take()
            && let Err(error) = fs::remove_file(&marker)
                .map_err(text)
                .and_then(|_| sync_parent(&marker))
        {
            self.quarantine("native worker reaped but durable marker cleanup failed\n");
            return Err(error);
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        if self.child.try_wait().map_err(text)?.is_some() {
            return self.confirmed();
        }
        if let Some(identity) = &self.identity {
            #[cfg(target_os = "macos")]
            let _ = runtime_process::cleanup_owned(&mut self.child, identity).map_err(text)?;
            #[cfg(not(target_os = "macos"))]
            let _ = runtime_process::cleanup(identity).map_err(text)?;
        } else {
            let _ = self.child.kill();
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            if self.child.try_wait().map_err(text)?.is_some() {
                return self.confirmed();
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.quarantine("native worker exit unconfirmed; no automatic replacement\n");
        Err("native worker exit unconfirmed; parser quarantined".into())
    }
}
impl Drop for ChildLease {
    fn drop(&mut self) {
        if !self.reaped && self.stop().is_err() {
            self.quarantine("native worker cleanup unconfirmed; no automatic replacement\n");
        }
    }
}
pub struct WorkerConfig {
    pub worker: PathBuf,
    pub library: PathBuf,
    pub jobs: PathBuf,
}
fn current(generation: &AtomicU64, job: u64) -> Result<()> {
    if generation.load(Ordering::SeqCst) != job {
        Err("import cancelled or superseded".into())
    } else {
        Ok(())
    }
}
pub fn inspect(
    config: &WorkerConfig,
    path: &Path,
    job: u64,
    generation: &AtomicU64,
    nonce: &str,
) -> Result<(PreparedDocument, Arc<Artifact>)> {
    let _serial = SERIAL_WORKER
        .lock()
        .map_err(|_| "native worker lock poisoned")?;
    current(generation, job)?;
    if config.jobs.join("QUARANTINED").exists() || config.jobs.join("worker-pending.json").exists()
    {
        return Err("native parser is quarantined; exit was not confirmed".into());
    }
    let original = bounded(path, MAX_PDF)?;
    if !original.starts_with(b"%PDF-") {
        return Err("not a PDF header".into());
    }
    if sha256(&bounded(&config.library, 64 * 1024 * 1024)?) != LIBRARY_SHA {
        return Err("native provider library mismatch".into());
    }
    fs::create_dir_all(&config.jobs).map_err(text)?;
    let directory = config.jobs.join(format!("{nonce}-{job}"));
    fs::create_dir(&directory).map_err(text)?;
    let retain = Arc::new(AtomicBool::new(false));
    let mut cleanup = PendingDir {
        path: Some(directory.clone()),
        retain: retain.clone(),
    };
    let input = directory.join("original.pdf");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&input)
        .map_err(text)?;
    file.write_all(&original).map_err(text)?;
    drop(file);
    let stderr = directory.join("stderr.txt");
    let parent = runtime_process::current().map_err(text)?;
    let run = format!("pdf-{nonce}-{job}");
    let marker = config.jobs.join("worker-pending.json");
    marker_write(
        &marker,
        &serde_json::json!({"native_pdf_protocol":1,"phase":"spawn_attempted","run_id":run,"owner":parent}),
        true,
    )?;
    let mut command = Command::new(&config.worker);
    command
        .arg("--managed-worker")
        .arg(parent.pid.to_string())
        .arg(parent.start_time_ticks.to_string())
        .arg(&run);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            fs::remove_file(&marker).map_err(text)?;
            sync_parent(&marker)?;
            return Err(text(error));
        }
    };
    let mut lease = ChildLease {
        child,
        quarantine: config.jobs.join("QUARANTINED"),
        retain,
        marker: Some(marker.clone()),
        identity: None,
        reaped: false,
    };
    let stdout = lease
        .child
        .stdout
        .take()
        .ok_or("native worker stdout unavailable")?;
    let stderr_pipe = lease
        .child
        .stderr
        .take()
        .ok_or("native stderr unavailable")?;
    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let report_overflow = stdout_overflow.clone();
    let reader = thread::spawn(move || -> Result<Vec<u8>> {
        let mut input = BufReader::new(stdout);
        let mut control = Vec::new();
        let control_result = Read::by_ref(&mut input)
            .take(4097)
            .read_until(b'\n', &mut control)
            .map_err(text)
            .and_then(|_| {
                if control.len() > 4096 || control.last() != Some(&b'\n') {
                    Err("native GuardReady budget or framing invalid".into())
                } else {
                    Ok(control)
                }
            });
        if ready_tx.send(control_result).is_err() {
            return Err("native parent closed before GuardReady".into());
        }
        let mut bytes = Vec::new();
        input
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(text)?;
        if bytes.len() > MAX_OUTPUT {
            report_overflow.store(true, Ordering::SeqCst);
        }
        Ok(bytes)
    });
    let diagnostics_overflow = stderr_overflow.clone();
    let errors = thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr_pipe
            .take(65537)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        if result.as_ref().is_ok_and(|b| b.len() > 65536) {
            stderr_overflow.store(true, Ordering::SeqCst);
        }
        result
    });
    let started = Instant::now();
    let mut booted = false;
    let status = loop {
        if !booted {
            match ready_rx.try_recv() {
                Ok(control) => {
                    let value: Value = serde_json::from_slice(&control?).map_err(text)?;
                    if value["native_pdf_protocol"] != 1
                        || value["kind"] != "guard_ready"
                        || value["run_id"] != run
                        || value.as_object().is_none_or(|o| o.len() != 4)
                    {
                        return Err("native GuardReady binding mismatch".into());
                    }
                    let reported: runtime_process::ProcessIdentity =
                        serde_json::from_value(value["identity"].clone()).map_err(text)?;
                    let actual = runtime_process::inspect(lease.child.id(), &run).map_err(text)?;
                    if reported != actual {
                        return Err("native worker OS identity mismatch".into());
                    }
                    lease.identity = Some(actual.clone());
                    marker_write(
                        &marker,
                        &serde_json::json!({"native_pdf_protocol":1,"phase":"identity_bound","run_id":run,"owner":parent,"worker":actual}),
                        false,
                    )?;
                    current(generation, job)?;
                    let boot=serde_json::json!({"native_pdf_protocol":1,"library":config.library,"input":input,"output":directory}).to_string();
                    if boot.len() > 4096 {
                        return Err("native Boot path budget exceeded".into());
                    }
                    let mut sink = lease
                        .child
                        .stdin
                        .take()
                        .ok_or("native Boot channel missing")?;
                    sink.write_all(boot.as_bytes()).map_err(text)?;
                    drop(sink);
                    booted = true;
                    eprintln!(
                        "DYNAMIC_WORKER_BOOT {}",
                        serde_json::json!({"run_id":run,"worker":actual,"guard_ready":true,"identity_bound":true})
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => return Err("native GuardReady channel closed".into()),
            }
        }
        if let Some(status) = lease.child.try_wait().map_err(text)? {
            if !booted {
                return Err("native worker exited before authorized Boot".into());
            }
            lease.confirmed()?;
            break status;
        }
        let stopped = current(generation, job).is_err();
        let over =
            stdout_overflow.load(Ordering::SeqCst) || diagnostics_overflow.load(Ordering::SeqCst);
        if stopped || over || started.elapsed() > Duration::from_secs(if booted { 15 } else { 2 }) {
            lease.stop()?;
            let _ = reader.join();
            let _ = errors.join();
            return Err(if stopped {
                "import cancelled or superseded"
            } else {
                "native worker deadline or output budget exceeded"
            }
            .into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = reader
        .join()
        .map_err(|_| "native worker reader failed")?
        .map_err(text)?;
    let diagnostics = errors
        .join()
        .map_err(|_| "native stderr reader failed")?
        .map_err(text)?;
    if diagnostics.len() > 65536 {
        return Err("native diagnostics byte budget exceeded".into());
    }
    fs::write(&stderr, &diagnostics).map_err(text)?;
    if output.len() > MAX_OUTPUT {
        return Err("native worker report budget exceeded".into());
    }
    let envelope: Value = serde_json::from_slice(&output).map_err(text)?;
    if !status.success() || envelope["ok"] != true {
        return Err(envelope["error"]
            .as_str()
            .unwrap_or("native PDF parsing failed")
            .into());
    }
    let report: DocumentReport =
        serde_json::from_value(envelope["report"].clone()).map_err(text)?;
    let report_bytes = serde_json::to_vec(&report).map_err(text)?;
    if report.extractor.library_sha256 != LIBRARY_SHA {
        return Err("native report library version mismatch".into());
    }
    if report.original_sha256 != sha256(&original) || report.original_bytes != original.len() {
        return Err("native report input hash mismatch".into());
    }
    fs::write(directory.join("report.json"), &report_bytes).map_err(text)?;
    let snapshot = plugin_surface::snapshot_from_report(&report)?;
    snapshot.validate_original(&original).map_err(text)?;
    current(generation, job)?;
    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Local PDF")
        .chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect::<String>();
    let report_digest = sha256(&report_bytes);
    let artifact = Arc::new(Artifact {
        directory: cleanup.path.take().ok_or("native job directory missing")?,
        report: Arc::new(report.clone()),
        report_digest: report_digest.clone(),
        cache: Mutex::new(Vec::new()),
    });
    let prepared = PreparedDocument {
        report,
        report_digest,
        snapshot,
        original,
        title,
        operation: format!("dynamic-import-{nonce}-{job}"),
    };
    Ok((prepared, artifact))
}
pub fn png(artifact: &Artifact, page: usize) -> Result<Arc<[u8]>> {
    {
        let mut cache = artifact
            .cache
            .lock()
            .map_err(|_| "native PNG cache unavailable")?;
        if let Some(index) = cache.iter().position(|(key, _)| *key == page) {
            let entry = cache.remove(index);
            let result = entry.1.clone();
            cache.push(entry);
            return Ok(result);
        }
    }
    let _serial = SERIAL_WORKER
        .lock()
        .map_err(|_| "native worker lock poisoned")?;
    let page = artifact
        .report
        .pages
        .get(page)
        .ok_or("native page out of range")?;
    if page.rendered_file != format!("page-{}.ppm", page.page_index) {
        return Err("native page path mismatch".into());
    }
    let bytes = bounded(&artifact.directory.join(&page.rendered_file), MAX_BITMAP)?;
    if sha256(&bytes) != page.rendered_sha256 {
        return Err("native bitmap hash mismatch".into());
    }
    let mut parts = bytes.splitn(4, |b| *b == b'\n');
    if parts.next() != Some(&b"P6"[..]) {
        return Err("unsupported native bitmap".into());
    }
    let size = std::str::from_utf8(parts.next().ok_or("bitmap size missing")?).map_err(text)?;
    let dimensions = size
        .split_whitespace()
        .map(str::parse::<i32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(text)?;
    if dimensions != [page.pixel_width, page.pixel_height] || parts.next() != Some(&b"255"[..]) {
        return Err("native bitmap geometry mismatch".into());
    }
    let raw = parts.next().ok_or("bitmap data missing")?;
    let width = u32::try_from(page.pixel_width).map_err(text)?;
    let height = u32::try_from(page.pixel_height).map_err(text)?;
    if raw.len() != width as usize * height as usize * 3 {
        return Err("native bitmap data mismatch".into());
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(text)?;
        writer.write_image_data(raw).map_err(text)?;
    }
    if output.len() > MAX_BITMAP {
        return Err("native PNG budget exceeded".into());
    }
    let output: Arc<[u8]> = output.into();
    let mut cache = artifact
        .cache
        .lock()
        .map_err(|_| "native PNG cache unavailable")?;
    while !cache.is_empty()
        && (cache.len() >= 3
            || cache.iter().map(|(_, value)| value.len()).sum::<usize>() + output.len()
                > 32 * 1024 * 1024)
    {
        cache.remove(0);
    }
    cache.push((page.page_index, output.clone()));
    Ok(output)
}

/// Same-executable native worker profile: guard before reading Boot or any PDF.
pub fn managed_main(args: &[String]) -> std::process::ExitCode {
    let run = (|| -> Result<Value> {
        if args.len() != 3 {
            return Err("invalid managed native worker arguments".into());
        }
        let parent = args[0].parse().map_err(text)?;
        let start = args[1].parse().map_err(text)?;
        let run = &args[2];
        runtime_process::guard_worker(parent, start, run).map_err(text)?;
        let identity = runtime_process::inspect(std::process::id(), run).map_err(text)?;
        println!(
            "{}",
            serde_json::json!({"native_pdf_protocol":1,"kind":"guard_ready","run_id":run,"identity":identity})
        );
        std::io::stdout().flush().map_err(text)?;
        let mut request = Vec::new();
        std::io::stdin()
            .take(4097)
            .read_to_end(&mut request)
            .map_err(text)?;
        if request.len() > 4096 {
            return Err("native Boot byte budget exceeded".into());
        }
        let value: Value = serde_json::from_slice(&request).map_err(text)?;
        if value["native_pdf_protocol"] != 1 || value.as_object().is_none_or(|o| o.len() != 4) {
            return Err("invalid native Boot envelope".into());
        }
        let path = |name: &str| -> Result<PathBuf> {
            value[name]
                .as_str()
                .map(PathBuf::from)
                .ok_or("native Boot path missing".into())
        };
        let report =
            qusor_pdf_provider_probe::inspect(&path("library")?, &path("input")?, &path("output")?)
                .map_err(text)?;
        Ok(serde_json::json!({"ok":true,"report":report}))
    })();
    match run {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", serde_json::json!({"ok":false,"error":error}));
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(label: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "qusor-dynamic-{label}-{}-{}",
                std::process::id(),
                Instant::now().elapsed().as_nanos()
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn selected_fifo_or_symlink_is_rejected_without_blocking_the_worker() {
        let scratch = Scratch::new("fifo");
        let fifo = scratch.0.join("selected.pdf");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(bounded(&fifo, MAX_PDF));
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(2))
                .expect("regular-file check must not wait for a FIFO writer")
                .is_err()
        );
        let target = scratch.0.join("plain.pdf");
        fs::write(&target, b"%PDF-1.7\n").unwrap();
        let link = scratch.0.join("link.pdf");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(bounded(&link, MAX_PDF).is_err());
    }
    #[test]
    fn exited_child_without_bound_identity_keeps_durable_quarantine() {
        let scratch = Scratch::new("unbound");
        let marker = scratch.0.join("worker-pending.json");
        marker_write(
            &marker,
            &serde_json::json!({"phase":"spawn_attempted"}),
            true,
        )
        .unwrap();
        let retain = Arc::new(AtomicBool::new(false));
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        child.wait().unwrap();
        let mut lease = ChildLease {
            child,
            quarantine: scratch.0.join("QUARANTINED"),
            retain: retain.clone(),
            marker: Some(marker.clone()),
            identity: None,
            reaped: false,
        };
        assert!(lease.stop().is_err());
        assert!(lease.reaped);
        assert!(marker.exists());
        assert!(lease.quarantine.exists());
        assert!(retain.load(Ordering::SeqCst));
    }
    #[test]
    fn owned_parser_termination_is_confirmed_before_its_slot_is_reused() {
        let scratch = Scratch::new("stop");
        let retain = Arc::new(AtomicBool::new(false));
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let mut lease = ChildLease {
            child,
            quarantine: scratch.0.join("QUARANTINED"),
            retain: retain.clone(),
            marker: None,
            identity: None,
            reaped: false,
        };
        lease.stop().unwrap();
        assert!(lease.child.try_wait().unwrap().is_some());
        assert!(!retain.load(Ordering::SeqCst));
        assert!(!lease.quarantine.exists());
    }
}
