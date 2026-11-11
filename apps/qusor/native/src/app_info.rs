//! App identity and the receipts a self-update evidence run leaves behind.
//!
//! `info` is the production surface: the id, version and build `nsh package`
//! wrote into the app's own `nsh-updater.json` asset, so the shell can name the
//! running version and the update dialog can say what an update replaces.
//!
//! Everything else is evidence plumbing for the local update self-test, and it
//! only works after an evidence run armed it: `QUSOR_UPDATE_EVIDENCE` names the
//! directory in the first run, and the marker that run writes into the app's
//! data directory tells the restarted build where to report. A normal launch has
//! neither, so it cannot write receipts, arm a restart or quit through this
//! module. Core knowledge and package authority never pass through here.

use nsh_core::prelude::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Names the directory receipts go to; set only by the self-test harness.
const EVIDENCE_ENV: &str = "QUSOR_UPDATE_EVIDENCE";
/// The marker an evidence run leaves for the build that replaces it.
const MARKER: &str = "update-evidence.json";
/// A receipt is one short step name and one bounded JSON payload.
const MAX_STEP_BYTES: usize = 48;
const MAX_PAYLOAD_BYTES: usize = 8 * 1024;

/// The packaged app's own description, as `nsh package` wrote it.
struct Config {
    id: String,
    version: String,
    build: Option<u64>,
}

fn config() -> Option<Config> {
    let executable = std::env::current_exe().ok()?;
    let contents = executable.parent()?.parent()?;
    let asset = contents
        .join("Resources")
        .join("nsh")
        .join("assets")
        .join("nsh-updater.json");
    let value: Value = serde_json::from_slice(&fs::read(asset).ok()?).ok()?;
    Some(Config {
        id: value.get("id")?.as_str()?.to_owned(),
        version: value.get("version")?.as_str()?.to_owned(),
        build: value.get("build").and_then(Value::as_u64),
    })
}

fn data_dir(sys: &System) -> Result<PathBuf, String> {
    sys.data_dir
        .clone()
        .ok_or_else(|| "the host gave no app data directory".into())
}

fn marker_path(sys: &System) -> Result<PathBuf, String> {
    Ok(data_dir(sys)?.join(MARKER))
}

/// Where this run reports evidence: `None` when the run is not an evidence run.
fn evidence(sys: &System) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(EVIDENCE_ENV).filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let path = marker_path(sys).ok()?;
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let dir = value.get("dir")?.as_str()?;
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// Whether a previous evidence build armed this one: it reports the restarted
/// state once and removes the marker through `consume`.
fn restarted(sys: &System) -> bool {
    marker_path(sys).is_ok_and(|path| path.is_file())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn valid_step(step: &str) -> bool {
    !step.is_empty()
        && step.len() <= MAX_STEP_BYTES
        && step
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn answer(value: Result<Value, String>) -> String {
    match value {
        Ok(value) => json!({"ok": true, "value": value}).to_string(),
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

/// The running app's identity plus the evidence receipts of a self-update test
/// (TSX: `qusor/pdf-surface`'s `AppInfo`). Only `info` is meant for normal use.
#[module]
pub struct AppInfo;

#[module]
impl AppInfo {
    /// The running app: id, version, build, data directory, and whether this run
    /// is an evidence run (and whether it is the build that replaced one).
    pub fn info(sys: System) -> String {
        answer((|| {
            let shipped = config();
            Ok(json!({
                "id": shipped.as_ref().map(|config| config.id.clone()),
                "version": shipped.as_ref().map(|config| config.version.clone()).unwrap_or_else(|| "dev".into()),
                "build": shipped.as_ref().and_then(|config| config.build),
                "data_dir": data_dir(&sys)?,
                "evidence": match evidence(&sys) {
                    Some(dir) => json!({
                        "active": true,
                        "dir": dir,
                        "restarted": restarted(&sys),
                    }),
                    None => json!({ "active": false }),
                },
            }))
        })())
    }

    /// Writes one evidence receipt: `<dir>/<step>.json`. Bounded and only while
    /// this run is an evidence run.
    pub fn receipt(sys: System, step: String, payload: String) -> String {
        answer((|| {
            let dir = evidence(&sys).ok_or("not an evidence run")?;
            if !valid_step(&step) {
                return Err("receipt step is not a short name".into());
            }
            if payload.len() > MAX_PAYLOAD_BYTES {
                return Err("receipt payload exceeds its budget".into());
            }
            let payload: Value =
                serde_json::from_str(&payload).map_err(|_| "receipt payload is not JSON")?;
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let receipt = json!({"at_ms": now_ms(), "step": step, "payload": payload});
            let path = dir.join(format!("{step}.json"));
            fs::write(&path, serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            Ok(json!({"path": path}))
        })())
    }

    /// Arms the build that replaces this one: it reports into the same directory
    /// once it runs, then removes the marker with `consume`.
    pub fn arm(sys: System) -> String {
        answer((|| {
            let dir = evidence(&sys).ok_or("not an evidence run")?;
            let shipped = config();
            let marker = json!({
                "dir": dir,
                "from_version": shipped.as_ref().map(|config| config.version.clone()),
                "at_ms": now_ms(),
            });
            let path = marker_path(&sys)?;
            fs::create_dir_all(data_dir(&sys)?).map_err(|error| error.to_string())?;
            fs::write(&path, serde_json::to_vec(&marker).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            Ok(json!({"path": path}))
        })())
    }

    /// Removes the marker once the restarted build reported its state.
    pub fn consume(sys: System) -> String {
        answer((|| {
            let path = marker_path(&sys)?;
            match fs::remove_file(&path) {
                Ok(()) => Ok(json!({"removed": path})),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(json!({"removed": null}))
                }
                Err(error) => Err(error.to_string()),
            }
        })())
    }

    /// Ends an evidence run the way quitting the app would. Only an evidence run
    /// may ask for it.
    pub fn finish(sys: System) -> String {
        answer((|| {
            evidence(&sys).ok_or("not an evidence run")?;
            nsh_core::element::desktop::quit();
            Ok(json!({"quitting": true}))
        })())
    }
}
