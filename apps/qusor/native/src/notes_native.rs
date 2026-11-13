//! Trusted notes Host mounted by the application composition runtime.
use crate::{package_catalog, runtime, vault::VaultStore};
use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use learning_application::{KnowledgeApplication, pdf_notes::NoteBodyField};
use nsh_core::prelude::*;
use qusor_app_bridges::notes_bridge::NotesReadBridge;
use serde_json::{Value, json};

pub(crate) struct NotesRuntime {
    host: Host,
    session: Session,
    store: VaultStore,
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
            store: store.clone(),
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
            ..
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

    pub(crate) fn epub_notes_page(&self, cursor: String, limit: u16) -> Result<Value, String> {
        let app = KnowledgeApplication::new(self.store.clone());
        let after = (!cursor.is_empty()).then_some(cursor.as_str());
        let page_limit = usize::from(limit).clamp(1, MAX_NOTE_PAGE.into());
        let page = app
            .epub_notes_page("qusor-vault", None, after, page_limit)
            .map_err(text)?;
        let notes = page
            .notes
            .into_iter()
            .map(|item| {
                json!({
                    "capture_id": item.capture_id,
                    "commit_rev": item.commit_rev,
                    "operation_id": item.operation_id,
                    "document_id": item.document_id,
                    "document_title": item.document_title,
                    "chapter_index": item.chapter_index,
                    "quote_preview": item.quote_preview,
                    "quote_bytes": item.quote_bytes,
                    "quote_truncated": item.quote_truncated,
                    "note_preview": item.note_preview,
                    "note_bytes": item.note_bytes,
                    "note_truncated": item.note_truncated,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "notes": notes,
            "next_cursor": page.next_cursor,
        }))
    }

    pub(crate) fn epub_body(
        &self,
        capture_id: i64,
        field: String,
        offset: u32,
        max_bytes: u16,
    ) -> Result<Value, String> {
        let body_field = match field.as_str() {
            "quote" => NoteBodyField::Quote,
            "note" => NoteBodyField::Note,
            _ => return Err("unknown note field".into()),
        };
        let app = KnowledgeApplication::new(self.store.clone());
        let chunk = app
            .epub_note_body(
                "qusor-vault",
                capture_id,
                body_field,
                offset as usize,
                usize::from(max_bytes),
            )
            .map_err(text)?;
        Ok(json!({
            "capture_id": chunk.capture_id,
            "commit_rev": chunk.commit_rev,
            "document_id": chunk.document_id,
            "field": field,
            "offset": chunk.offset,
            "total_bytes": chunk.total_bytes,
            "text": chunk.text,
            "next_offset": chunk.next_offset,
        }))
    }

    pub(crate) fn update_note(&mut self, capture_id: i64, format: String, note: String) -> Result<Value, String> {
        let mut app = KnowledgeApplication::new(self.store.clone());
        match format.as_str() {
            "pdf" => app
                .update_pdf_capture_note("qusor-vault", capture_id, &note)
                .map_err(text)?,
            "epub" => app
                .update_epub_capture_note("qusor-vault", capture_id, &note)
                .map_err(text)?,
            _ => return Err("unknown note format".into()),
        }
        Ok(json!({
            "capture_id": capture_id,
            "format": format,
            "updated": true,
        }))
    }

    pub(crate) fn delete_note(&mut self, capture_id: i64, format: String) -> Result<Value, String> {
        let mut app = KnowledgeApplication::new(self.store.clone());
        match format.as_str() {
            "pdf" => app
                .delete_pdf_capture("qusor-vault", capture_id)
                .map_err(text)?,
            "epub" => app
                .delete_epub_capture("qusor-vault", capture_id)
                .map_err(text)?,
            _ => return Err("unknown note format".into()),
        }
        Ok(json!({
            "capture_id": capture_id,
            "format": format,
            "deleted": true,
        }))
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

    /// One bounded newest-first page of EPUB note previews.
    pub fn epub_notes_page(sys: System, cursor: String, limit: u16) -> String {
        answer(runtime::with_notes(&sys, |notes| {
            notes.epub_notes_page(cursor, limit)
        }))
    }

    /// One bounded UTF-8 chunk of a saved EPUB quote or note.
    pub fn epub_body(
        sys: System,
        capture_id: i64,
        field: String,
        offset: u32,
        max_bytes: u16,
    ) -> String {
        answer(runtime::with_notes(&sys, |notes| {
            notes.epub_body(capture_id, field, offset, max_bytes)
        }))
    }

    /// Update the user note attached to a saved PDF or EPUB capture.
    pub fn update_note(sys: System, capture_id: i64, format: String, note: String) -> String {
        answer(runtime::with_notes(&sys, |notes| {
            notes.update_note(capture_id, format, note)
        }))
    }

    /// Delete a saved PDF or EPUB capture/note.
    pub fn delete_note(sys: System, capture_id: i64, format: String) -> String {
        answer(runtime::with_notes(&sys, |notes| {
            notes.delete_note(capture_id, format)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_api::{
        contract::sha256,
        epub_contract::{EpubChapterSnapshot, EpubSnapshot},
        knowledge::Provenance,
    };
    use learning_application::epub_library::EpubSelectionRange;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qusor-notes-native-test-{}-{}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn epub_notes_page_body_update_and_delete() {
        let scratch = Scratch::new();
        let store = VaultStore::open_app(&scratch.0.join("data")).unwrap();

        let chapter_text = "Alpha Beta Gamma Delta";
        let snapshot = EpubSnapshot {
            vault: "qusor-vault".into(),
            title: "Notes Test EPUB".into(),
            author: "Tester".into(),
            original_sha256: sha256(b"fake-epub-bytes"),
            original_bytes: 15,
            extraction_revision: "qusor-epub-rs-v1".into(),
            chapters: vec![EpubChapterSnapshot {
                chapter_index: 0,
                title: "Chapter 1".into(),
                href: "ch1.xhtml".into(),
                text: chapter_text.into(),
            }],
            toc: vec![],
        };
        let mut app = KnowledgeApplication::new(store.clone());
        let doc = app
            .import_epub("notes-epub-import", "qusor-vault", &snapshot)
            .unwrap();
        let provenance = Provenance {
            package_id: "org.qusor.library".into(),
            publisher_id: "org.qusor".into(),
            package_digest: "a".repeat(64),
            installation_id: 1,
        };
        let cap = app
            .capture_epub_selection(
                "notes-epub-cap-1",
                "qusor-vault",
                EpubSelectionRange {
                    document_id: doc.document_id,
                    chapter_index: 0,
                    start: 6,
                    end: 16,
                },
                &provenance,
                "Original epub note",
            )
            .unwrap();

        let mut notes = NotesRuntime::mount(store).unwrap();
        let page = notes.epub_notes_page(String::new(), 10).unwrap();
        assert_eq!(page["notes"].as_array().unwrap().len(), 1);
        assert_eq!(page["notes"][0]["capture_id"], cap.capture_id);
        assert_eq!(page["notes"][0]["quote_preview"], "Beta Gamma");
        assert_eq!(page["notes"][0]["note_preview"], "Original epub note");

        let quote_body = notes
            .epub_body(cap.capture_id, "quote".into(), 0, 256)
            .unwrap();
        assert_eq!(quote_body["text"], "Beta Gamma");

        notes
            .update_note(cap.capture_id, "epub".into(), "Updated epub note".into())
            .unwrap();
        let note_body = notes
            .epub_body(cap.capture_id, "note".into(), 0, 256)
            .unwrap();
        assert_eq!(note_body["text"], "Updated epub note");

        assert!(
            notes
                .update_note(cap.capture_id, "unknown".into(), "x".into())
                .is_err()
        );
        assert!(notes.delete_note(cap.capture_id, "unknown".into()).is_err());

        notes.delete_note(cap.capture_id, "epub".into()).unwrap();
        let empty = notes.epub_notes_page(String::new(), 10).unwrap();
        assert!(empty["notes"].as_array().unwrap().is_empty());
    }
}
