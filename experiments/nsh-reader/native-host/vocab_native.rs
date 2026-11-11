//! Workbench vocabulary host: an ordinary signed package, the public
//! vocab-study-v1 wire and a real Core database. Startup input is environment
//! only; TSX never sees a database path, a package key or Host authority.
use extension_api::{vocab::*, *};
use extension_host::{Grant, Host, Session};
use extension_package::package::verify_for_host;
use learning_application::study::StudyApplication;
use nsh_core::prelude::*;
use qusor_capture_probe::vocab_bridge::VocabBridge;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;

struct Runtime {
    host: Host,
    session: Session,
    bridge: VocabBridge<SqliteStore>,
}
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

/// Startup-only wiring. A missing environment leaves the module unavailable so
/// the workbench can keep its fixture for headless tests.
pub fn initialize_from_env() -> Result<(), String> {
    let Ok(data) = std::env::var("QUSOR_VOCAB_DATA") else {
        return Ok(());
    };
    let package = std::env::var("QUSOR_VOCAB_PACKAGE").map_err(text)?;
    let key = std::env::var("QUSOR_VOCAB_KEY").map_err(text)?;
    initialize(Path::new(&data), Path::new(&package), Path::new(&key))
}

/// Trusted startup input: a seeded Core directory, the ordinary signed vocab
/// package and its trusted public key file.
pub fn initialize(data: &Path, package: &Path, key: &Path) -> Result<(), String> {
    let bytes = fs::read(package).map_err(text)?;
    let trusted = fs::read_to_string(key).map_err(text)?;
    let verified = verify_for_host(&bytes, trusted.trim(), &SUPPORTED_HOST_APIS).map_err(text)?;
    let mut host = Host::new(
        "workbench-vault",
        [Capability::VocabRead, Capability::VocabReview],
    )
    .map_err(text)?;
    let installation = host
        .register(verified.manifest().clone(), verified.digest())
        .map_err(text)?;
    host.replace_grants(
        installation,
        vec![
            Grant {
                capability: Capability::VocabRead,
                resource: extension_api::Resource::Vault {
                    vault_id: "workbench-vault".into(),
                },
            },
            Grant {
                capability: Capability::VocabReview,
                resource: extension_api::Resource::Vault {
                    vault_id: "workbench-vault".into(),
                },
            },
        ],
    )
    .map_err(text)?;
    host.enable(installation).map_err(text)?;
    let session = host
        .open_session_for_api(installation, HOST_API_V0_5)
        .map_err(text)?;
    let app = StudyApplication::new(SqliteStore::open(data).map_err(text)?);
    let now = now_ms();
    let bridge = VocabBridge::new(app, "workbench-vault", now, day_start_ms(now));
    *RUNTIME.lock().map_err(text)? = Some(Runtime {
        host,
        session,
        bridge,
    });
    Ok(())
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(1)
}

/// Local midnight is supplied by the trusted launcher; the UTC fallback keeps a
/// manually started probe deterministic enough for review.
fn day_start_ms(now: i64) -> i64 {
    std::env::var("QUSOR_VOCAB_DAY_START_MS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= now)
        .unwrap_or(now - now.rem_euclid(86_400_000))
}

/// The normal host initializes at startup; a test host may only call the
/// module, so initialization is also lazy when the environment is present.
fn ensure_runtime() -> Result<(), String> {
    if RUNTIME.lock().map_err(text)?.is_some() {
        return Ok(());
    }
    initialize_from_env()
}

fn answer(f: impl FnOnce(&mut Runtime) -> Result<Value, String>) -> String {
    if let Err(error) = ensure_runtime() {
        return json!({"ok": false, "error": error}).to_string();
    }
    let value = match RUNTIME.lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(runtime) => {
                let now = now_ms();
                runtime.bridge.set_clock(now, day_start_ms(now));
                match f(runtime) {
                    Ok(value) => json!({"ok": true, "value": value}),
                    Err(error) => json!({"ok": false, "error": error}),
                }
            }
            None => json!({"ok": false, "error": "vocab runtime unavailable"}),
        },
        Err(_) => json!({"ok": false, "error": "vocab runtime unavailable"}),
    };
    value.to_string()
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
}

fn parse_rating(value: &str) -> Result<VocabRating, String> {
    match value {
        "again" => Ok(VocabRating::Again),
        "hard" => Ok(VocabRating::Hard),
        "good" => Ok(VocabRating::Good),
        "easy" => Ok(VocabRating::Easy),
        _ => Err("unknown vocab rating".into()),
    }
}

/// One in-process render capture of the app window. This is the same kind of
/// evidence the NSH test harness takes: it needs no screen-recording grant and
/// does not depend on which Space or window is frontmost.
#[cfg(target_os = "macos")]
fn capture_window(path: &Path) -> Result<(), String> {
    use objc2::MainThreadMarker;
    if MainThreadMarker::new().is_none() {
        let path = path.to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        dispatch2::DispatchQueue::main().exec_async(move || {
            let _ = sender.send(capture_on_main(&path));
        });
        return receiver
            .recv()
            .map_err(|_| "main thread capture did not answer".to_string())?;
    }
    capture_on_main(path)
}

#[cfg(target_os = "macos")]
fn capture_on_main(path: &Path) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSApplication, NSBitmapImageFileType};
    use objc2_foundation::{NSDictionary, NSString};
    let marker = MainThreadMarker::new().ok_or("capture must run on the main thread")?;
    let application = NSApplication::sharedApplication(marker);
    let window = application
        .keyWindow()
        .or_else(|| application.windows().iter().next())
        .ok_or("no app window is open")?;
    let view = window.contentView().ok_or("no content view")?;
    let bounds = view.bounds();
    let bitmap = view
        .bitmapImageRepForCachingDisplayInRect(bounds)
        .ok_or("cannot allocate a window bitmap")?;
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
    let properties: objc2::rc::Retained<NSDictionary<_, AnyObject>> = NSDictionary::new();
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }
    .ok_or("cannot encode the window bitmap")?;
    let target = NSString::from_str(path.to_str().ok_or("capture path must be UTF-8")?);
    if !data.writeToFile_atomically(&target, true) {
        return Err("cannot write the window capture".into());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn capture_window(_path: &Path) -> Result<(), String> {
    Err("window capture is only wired for macOS".into())
}

#[module]
pub struct VocabHost;

#[module]
impl VocabHost {
    /// Wordbooks and their total/due/new counters from the package-authorized Core.
    pub fn decks() -> String {
        answer(|runtime| {
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: "workbench-decks".into(),
                operation: VocabOperation::Decks {},
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
            let permit = host.authorize_vocab(session, &request).map_err(text)?;
            let response = host
                .dispatch(permit, |_context| Ok(bridge.respond(&request)))
                .map_err(text)?;
            match response.result {
                VocabResult::Decks { decks } => Ok(json!({ "decks": decks })),
                VocabResult::Queue { .. } => Err("unexpected queue response".into()),
                VocabResult::Review(_) => Err("unexpected review response".into()),
                VocabResult::Error { error } => Err(wire_error(error)),
            }
        })
    }

    /// One bounded study queue page: due reviews first, then new cards.
    pub fn queue(wordbook_id: i64, limit: u16) -> String {
        answer(|runtime| {
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: "workbench-queue".into(),
                operation: VocabOperation::Queue { wordbook_id, limit },
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
            let permit = host.authorize_vocab(session, &request).map_err(text)?;
            let response = host
                .dispatch(permit, |_context| Ok(bridge.respond(&request)))
                .map_err(text)?;
            match response.result {
                VocabResult::Queue {
                    cards,
                    remaining_due,
                    remaining_new,
                } => Ok(json!({
                    "cards": cards,
                    "remaining_due": remaining_due,
                    "remaining_new": remaining_new,
                })),
                VocabResult::Decks { .. } => Err("unexpected decks response".into()),
                VocabResult::Review(_) => Err("unexpected review response".into()),
                VocabResult::Error { error } => Err(wire_error(error)),
            }
        })
    }

    /// Commit one graded review through the public wire. `operation_id` is the
    /// caller's user-action key: repeating it must not schedule the card twice.
    pub fn review(card_id: i64, rating: String, operation_id: String) -> String {
        answer(|runtime| {
            let rating = parse_rating(&rating)?;
            let request = VocabRequest {
                vocab_protocol: VOCAB_PROTOCOL,
                api_version: HOST_API_V0_5,
                request_id: operation_id,
                operation: VocabOperation::Review { card_id, rating },
            };
            let Runtime {
                host,
                session,
                bridge,
            } = runtime;
            let permit = host.authorize_vocab(session, &request).map_err(text)?;
            let response = host
                .dispatch(permit, |_context| Ok(bridge.respond(&request)))
                .map_err(text)?;
            match response.result {
                VocabResult::Review(outcome) => Ok(json!({
                    "card_id": outcome.card_id,
                    "stage": outcome.stage,
                    "due_at_ms": outcome.due_at_ms,
                    "scheduled_minutes": outcome.scheduled_minutes,
                    "remaining_due": outcome.remaining_due,
                    "remaining_new": outcome.remaining_new,
                })),
                VocabResult::Decks { .. } => Err("unexpected decks response".into()),
                VocabResult::Queue { .. } => Err("unexpected queue response".into()),
                VocabResult::Error { error } => Err(wire_error(error)),
            }
        })
    }

    /// Evidence-only: report whether the host was started for capture and which
    /// workspace the launcher asked for.
    pub fn evidence() -> String {
        let active = std::env::var_os("QUSOR_WORKBENCH_EVIDENCE").is_some();
        let dir = std::env::var("QUSOR_WORKBENCH_CAPTURE_DIR").unwrap_or_default();
        let target = std::env::var("QUSOR_WORKBENCH_EVIDENCE_TARGET")
            .unwrap_or_else(|_| "notes".to_string());
        json!({"ok": true, "value": {"active": active, "dir": dir, "target": target}}).to_string()
    }

    /// Evidence-only: capture the current app window into a PNG.
    pub fn capture(path: String) -> String {
        match capture_window(Path::new(&path)) {
            Ok(()) => json!({"ok": true, "value": {"path": path}}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }
}
