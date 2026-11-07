//! Process watchdog for this experiment. This is fault isolation, not an OS sandbox.
use qusor_wasm_probe::{Options, Package, Probe, Report};
use std::{
    env,
    io::{self, Read},
    path::PathBuf,
    process::{self, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const WATCHDOG: Duration = Duration::from_millis(1000);
const MAX_REPORT: u64 = 6 * qusor_wasm_probe::MAX_RESPONSE_BYTES as u64 + 1024;

fn emit(report: &Report) {
    println!(
        "{}",
        serde_json::to_string(report).expect("fixed report must serialize")
    );
}
fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let (worker, args) = if args.first().is_some_and(|arg| arg == "--worker") {
        (true, &args[1..])
    } else {
        (false, &args[..])
    };
    let valid_args = args.len() == 1 || (args.len() == 2 && args[1] == "--block-host-call");
    let report = if !valid_args {
        Report::failure("usage: qusor-wasm-probe [--worker] <package-dir> [--block-host-call]")
    } else if worker {
        let directory = PathBuf::from(&args[0]);
        Report::from_result(Package::load(&directory).and_then(|package| {
            Probe::new(
                package,
                Options {
                    block_host_call: args.len() == 2,
                    ..Options::default()
                },
            )?
            .run()
        }))
    } else {
        supervise(args)
    };
    emit(&report);
    process::exit(if report.ok { 0 } else { 1 });
}
fn supervise(args: &[std::ffi::OsString]) -> Report {
    let Ok(executable) = env::current_exe() else {
        return Report::failure("worker_launch_failed");
    };
    let started = Instant::now();
    let mut child = match Command::new(executable)
        .arg("--worker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Report::failure("worker_launch_failed"),
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Report::failure("worker_output_failed");
    };
    // Drain a bounded amount concurrently so a full pipe cannot stall the watchdog.
    let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        stdout.take(MAX_REPORT + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < WATCHDOG => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("watchdog_timeout");
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("worker_wait_failed");
            }
        }
    };
    let bytes = reader.join();
    let status = match status {
        Ok(status) => status,
        Err(code) => return Report::failure(code),
    };
    let bytes = match bytes {
        Ok(Ok(bytes)) if bytes.len() <= MAX_REPORT as usize => bytes,
        _ => return Report::failure("worker_output_failed"),
    };
    match serde_json::from_slice::<Report>(&bytes) {
        Ok(report) if status.success() == report.ok => report,
        _ => Report::failure("worker_failed"),
    }
}
