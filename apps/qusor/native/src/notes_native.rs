//! Trusted notes Host mounted by the application composition runtime.
use crate::{package_catalog, runtime, vault::VaultStore};
use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_app_bridges::notes_bridge::NotesReadBridge;
use serde_json::{Value, json};

pub(crate) struct NotesRuntime {
    host: Host,
    session: Session,
    bridge: NotesReadBridge<VaultStore>,
    document: i64,
}

impl NotesRuntime {
    pub(crate) fn mount(store: VaultStore) -> Result<Self, String> {
        crate::profile::require_active(crate::profile::NOTES_HOST_ENTRY)?;
        let verified = package_catalog::verify("org.qusor.notes")?;
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
        Ok(Self {
            host,
            session,
            bridge: NotesReadBridge::new(KnowledgeApplication::new(store), "qusor-vault"),
            document: 0,
        })
    }

    fn page(&mut self) -> Result<Value, String> {
        let request = NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version: HOST_API_V0_4,
            request_id: request_id("page"),
            operation: NotesOperation::Page {
                cursor: None,
                limit: MAX_NOTE_PAGE,
            },
        };
        let Self {
            host,
            session,
            bridge,
            document,
        } = self;
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
    }

    fn body(
        &mut self,
        capture_id: i64,
        field: String,
        offset: u32,
        max_bytes: u16,
    ) -> Result<Value, String> {
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
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
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
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn request_id(suffix: &str) -> String {
    format!("workbench-{suffix}")
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
}

fn answer(value: Result<Value, String>) -> String {
    match value {
        Ok(value) => json!({"ok": true, "value": value}).to_string(),
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

/// Native Host surface for bounded, package-authorized note reads.
#[module]
pub struct NotesHost;

#[module]
impl NotesHost {
    /// One bounded newest-first page from the package-authorized Core reader.
    pub fn page(sys: System) -> String {
        answer(runtime::with_notes(&sys, NotesRuntime::page))
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
        answer(runtime::with_notes(&sys, |notes| {
            notes.body(capture_id, field, offset, max_bytes)
        }))
    }
}
