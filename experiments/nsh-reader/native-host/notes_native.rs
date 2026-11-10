//! Workbench notes host: an ordinary signed package, the public notes.read wire
//! and a real Core database. Startup input is environment only; TSX never sees
//! a database path, a package key or Host authority.
use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use extension_package::package::verify_for_host;
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_capture_probe::notes_bridge::NotesReadBridge;
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Mutex};
use storage_sqlite::SqliteStore;

struct Runtime {
    host: Host,
    session: Session,
    bridge: NotesReadBridge<SqliteStore>,
    document: i64,
}
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

/// Startup-only wiring. A missing environment leaves the module unavailable so
/// the workbench can keep its fixture for headless tests.
pub fn initialize_from_env() -> Result<(), String> {
    let Ok(data) = std::env::var("QUSOR_NOTES_DATA") else {
        return Ok(());
    };
    let package = std::env::var("QUSOR_NOTES_PACKAGE").map_err(text)?;
    let key = std::env::var("QUSOR_NOTES_KEY").map_err(text)?;
    let document = std::env::var("QUSOR_NOTES_DOCUMENT")
        .map_err(text)?
        .parse::<i64>()
        .map_err(text)?;
    initialize(
        Path::new(&data),
        Path::new(&package),
        Path::new(&key),
        document,
    )
}

/// Trusted startup input: a seeded Core directory, the ordinary signed notes
/// package, its trusted public key file and the current document id.
pub fn initialize(data: &Path, package: &Path, key: &Path, document: i64) -> Result<(), String> {
    let bytes = fs::read(package).map_err(text)?;
    let trusted = fs::read_to_string(key).map_err(text)?;
    let verified = verify_for_host(&bytes, trusted.trim(), &SUPPORTED_HOST_APIS).map_err(text)?;
    let mut host = Host::new("workbench-vault", [Capability::NotesRead]).map_err(text)?;
    let installation = host
        .register(verified.manifest().clone(), verified.digest())
        .map_err(text)?;
    host.replace_grants(
        installation,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: extension_api::Resource::Vault {
                vault_id: "workbench-vault".into(),
            },
        }],
    )
    .map_err(text)?;
    host.enable(installation).map_err(text)?;
    let session = host
        .open_session_for_api(installation, HOST_API_V0_4)
        .map_err(text)?;
    let app = KnowledgeApplication::new(SqliteStore::open(data).map_err(text)?);
    *RUNTIME.lock().map_err(text)? = Some(Runtime {
        host,
        session,
        bridge: NotesReadBridge::new(app, "workbench-vault"),
        document,
    });
    Ok(())
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
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
            Some(runtime) => match f(runtime) {
                Ok(value) => json!({"ok": true, "value": value}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": "notes runtime unavailable"}),
        },
        Err(_) => json!({"ok": false, "error": "notes runtime unavailable"}),
    };
    value.to_string()
}

fn request_id(suffix: &str) -> String {
    format!("workbench-{suffix}")
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
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
pub struct NotesHost;

#[module]
impl NotesHost {
    /// One bounded newest-first page from the package-authorized Core reader.
    pub fn page() -> String {
        answer(|runtime| {
            let request = NotesRequest {
                notes_protocol: NOTES_PROTOCOL,
                api_version: HOST_API_V0_4,
                request_id: request_id("page"),
                operation: NotesOperation::Page {
                    cursor: None,
                    limit: MAX_NOTE_PAGE,
                },
            };
            let Runtime {
                host,
                session,
                bridge,
                document,
            } = runtime;
            let permit = host.authorize_notes(session, &request).map_err(text)?;
            let response = host
                .dispatch(permit, |_context| Ok(bridge.respond(&request)))
                .map_err(text)?;
            match response.result {
                NotesResult::Page { notes, next_cursor } => Ok(json!({
                    "current_document_id": *document,
                    "notes": notes,
                    "next_cursor": next_cursor,
                })),
                NotesResult::Body(_) => Err("unexpected body response".into()),
                NotesResult::Error { error } => Err(wire_error(error)),
            }
        })
    }

    /// Evidence-only: report whether the host was started for capture.
    pub fn evidence() -> String {
        let active = std::env::var_os("QUSOR_WORKBENCH_EVIDENCE").is_some();
        let dir = std::env::var("QUSOR_WORKBENCH_CAPTURE_DIR").unwrap_or_default();
        json!({"ok": true, "value": {"active": active, "dir": dir}}).to_string()
    }

    /// Evidence-only: capture the current app window into a PNG.
    pub fn capture(path: String) -> String {
        match capture_window(Path::new(&path)) {
            Ok(()) => json!({"ok": true, "value": {"path": path}}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// One bounded UTF-8 chunk of the saved quote or note.
    pub fn body(capture_id: i64, field: String, offset: u32, max_bytes: u16) -> String {
        answer(|runtime| {
            let field = match field.as_str() {
                "quote" => NoteField::Quote,
                "note" => NoteField::Note,
                _ => return Err("unknown note field".into()),
            };
            let request = NotesRequest {
                notes_protocol: NOTES_PROTOCOL,
                api_version: HOST_API_V0_4,
                request_id: request_id("body"),
                operation: NotesOperation::Body {
                    capture_id,
                    field,
                    offset,
                    max_bytes,
                },
            };
            let Runtime {
                host,
                session,
                bridge,
                ..
            } = runtime;
            let permit = host.authorize_notes(session, &request).map_err(text)?;
            let response = host
                .dispatch(permit, |_context| Ok(bridge.respond(&request)))
                .map_err(text)?;
            match response.result {
                NotesResult::Body(body) => Ok(json!({
                    "capture_id": body.capture_id,
                    "commit_rev": body.commit_rev,
                    "document_id": body.document_id,
                    "field": body.field,
                    "offset": body.offset,
                    "total_bytes": body.total_bytes,
                    "text": body.text,
                    "next_offset": body.next_offset,
                })),
                NotesResult::Page { .. } => Err("unexpected page response".into()),
                NotesResult::Error { error } => Err(wire_error(error)),
            }
        })
    }
}
