use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use extension_package::package::verify_for_host;
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_app_bridges::notes_bridge::NotesReadBridge;
use serde_json::{Value, json};
use std::sync::Mutex;
use storage_sqlite::SqliteStore;

const NOTES_PACKAGE: &[u8] = include_bytes!("../assets/notes.qpk");
const NOTES_PACKAGE_KEY: &str = include_str!("../assets/notes.pub");

struct Runtime {
    host: Host,
    session: Session,
    bridge: NotesReadBridge<SqliteStore>,
    document: i64,
}
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

/// Trusted startup wiring: the host's own app data directory plus the ordinary
/// signed package shipped inside this app. TSX never sees a path, a key or Host
/// authority.
pub fn initialize(sys: &System) -> Result<(), String> {
    crate::profile::require_active(crate::profile::NOTES_HOST_ENTRY)?;
    let data_dir = sys
        .data_dir
        .clone()
        .ok_or("the host gave no app data directory")?;
    std::fs::create_dir_all(&data_dir).map_err(text)?;
    let verified = verify_for_host(
        NOTES_PACKAGE,
        NOTES_PACKAGE_KEY.trim(),
        &SUPPORTED_HOST_APIS,
    )
    .map_err(text)?;
    let mut host = Host::new("qusor-vault", [Capability::NotesRead]).map_err(text)?;
    let installation = host
        .register(verified.manifest().clone(), verified.digest())
        .map_err(text)?;
    host.replace_grants(
        installation,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: extension_api::Resource::Vault {
                vault_id: "qusor-vault".into(),
            },
        }],
    )
    .map_err(text)?;
    host.enable(installation).map_err(text)?;
    let session = host
        .open_session_for_api(installation, HOST_API_V0_4)
        .map_err(text)?;
    let app = KnowledgeApplication::new(SqliteStore::open(data_dir.join("notes")).map_err(text)?);
    *RUNTIME.lock().map_err(text)? = Some(Runtime {
        host,
        session,
        bridge: NotesReadBridge::new(app, "qusor-vault"),
        document: 0,
    });
    Ok(())
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// The normal host initializes at startup; a test host may only call the
/// module, so initialization is also lazy when the environment is present.
fn ensure_runtime(sys: &System) -> Result<(), String> {
    if RUNTIME.lock().map_err(text)?.is_some() {
        return Ok(());
    }
    initialize(sys)
}

fn answer(sys: &System, f: impl FnOnce(&mut Runtime) -> Result<Value, String>) -> String {
    if let Err(error) = ensure_runtime(sys) {
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

#[module]
pub struct NotesHost;

#[module]
impl NotesHost {
    /// One bounded newest-first page from the package-authorized Core reader.
    pub fn page(sys: System) -> String {
        answer(&sys, |runtime| {
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

    /// Evidence-only: a packaged build has no in-process capture path.
    pub fn capture(path: String) -> String {
        json!({"ok": false, "error": "window capture is not available in this build", "path": path})
            .to_string()
    }

    /// One bounded UTF-8 chunk of the saved quote or note.
    pub fn body(
        sys: System,
        capture_id: i64,
        field: String,
        offset: u32,
        max_bytes: u16,
    ) -> String {
        answer(&sys, |runtime| {
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
