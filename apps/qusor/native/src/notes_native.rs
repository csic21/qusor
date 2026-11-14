//! Trusted notes Host mounted by the application composition runtime.
use crate::{core_facade, package_catalog, profile, runtime, vault::VaultStore};
use extension_api::{
    notes::*,
    orthogonal::{CapabilityId, STANDARD_ORTHOGONAL_CAPABILITIES},
    *,
};
use extension_host::{Grant, Host, OrthogonalGrant, Session};
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
    pub(crate) fn mount_for_package(store: VaultStore, package_id: &str) -> Result<Self, String> {
        let verified = package_catalog::verify(package_id)?;
        Self::mount_verified(store, verified)
    }

    fn mount_verified(
        store: VaultStore,
        verified: extension_package::package::VerifiedPackage,
    ) -> Result<Self, String> {
        profile::require_service_active(profile::NOTES_READ_SERVICE)?;
        let vault_id = store.vault_id();
        let mut host =
            Host::new(vault_id, [Capability::NotesRead, Capability::UiSlot]).map_err(text)?;
        for cap in STANDARD_ORTHOGONAL_CAPABILITIES {
            let cap_id =
                CapabilityId::parse_with_code(cap, ErrorCode::InvalidRequest).map_err(text)?;
            host.configure_orthogonal_support(cap_id, 1, 0)
                .map_err(text)?;
        }
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        host.replace_grants(
            installation,
            vec![
                Grant {
                    capability: Capability::NotesRead,
                    resource: extension_api::Resource::Vault {
                        vault_id: vault_id.to_owned(),
                    },
                },
                Grant {
                    capability: Capability::UiSlot,
                    resource: extension_api::Resource::Vault {
                        vault_id: vault_id.to_owned(),
                    },
                },
            ],
        )
        .map_err(text)?;
        let orthogonal_grants: Vec<OrthogonalGrant> = verified
            .manifest()
            .effective_orthogonal_requirements()
            .into_iter()
            .map(|req| OrthogonalGrant {
                capability: req.id,
                min_minor: req.min_minor,
                resource: extension_api::Resource::Vault {
                    vault_id: vault_id.to_owned(),
                },
            })
            .collect();
        host.replace_orthogonal_grants(installation, orthogonal_grants)
            .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_negotiated_session(installation, &SUPPORTED_HOST_APIS)
            .map_err(text)?;
        let bridge = core_facade::new_notes_bridge(&store);
        Ok(Self {
            host,
            session,
            bridge,
            document: 0,
        })
    }

    fn page(&mut self) -> Result<Value, String> {
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version,
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
        let api_version = self.host.session_api(&self.session).map_err(text)?;
        let request = NotesRequest {
            notes_protocol: NOTES_PROTOCOL,
            api_version,
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

#[cfg(test)]
mod tests {
    use super::*;
    use core_api::{
        contract::sha256,
        epub_contract::{EpubChapterSnapshot, EpubSnapshot},
        knowledge::Provenance,
    };
    use extension_api::orthogonal::{
        CORE_KNOWLEDGE_PROTOCOL, CoreKnowledgeOperation, CoreKnowledgeRequest, CoreKnowledgeResult,
        DocumentFormat,
    };
    use learning_application::{KnowledgeApplication, epub_library::EpubSelectionRange};
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

        let mut notes = NotesRuntime::mount_for_package(store.clone(), "org.qusor.notes").unwrap();
        let mut ortho = core_facade::new_orthogonal_bridge(&store);
        let vault_resource = extension_api::Resource::Vault {
            vault_id: store.vault_id().to_owned(),
        };
        let mut call =
            |notes: &mut NotesRuntime, req_id: &str, operation: CoreKnowledgeOperation| {
                let req = CoreKnowledgeRequest {
                    protocol_version: CORE_KNOWLEDGE_PROTOCOL,
                    api_version: HOST_API_V0_8,
                    request_id: req_id.into(),
                    operation,
                };
                let permit = notes
                    .host
                    .authorize_core_knowledge(&notes.session, &req, vault_resource.clone())
                    .unwrap();
                notes
                    .host
                    .dispatch_orthogonal(permit, |_ctx| Ok(ortho.respond(&req, &provenance)))
                    .unwrap()
                    .result
            };

        let CoreKnowledgeResult::NotesPage { notes: items, .. } = call(
            &mut notes,
            "n1",
            CoreKnowledgeOperation::NotesPage {
                format: Some(DocumentFormat::Epub),
                document_id: None,
                cursor: None,
                limit: 10,
            },
        ) else {
            panic!("expected NotesPage");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].capture_id, cap.capture_id);
        assert_eq!(items[0].quote_preview, "Beta Gamma");
        assert_eq!(items[0].note_preview, "Original epub note");

        let CoreKnowledgeResult::NoteUpdated { .. } = call(
            &mut notes,
            "n2",
            CoreKnowledgeOperation::UpdateCaptureNote {
                format: DocumentFormat::Epub,
                capture_id: cap.capture_id,
                note: "Updated epub note".into(),
            },
        ) else {
            panic!("expected NoteUpdated");
        };

        let CoreKnowledgeResult::NoteBody(body) = call(
            &mut notes,
            "n3",
            CoreKnowledgeOperation::NoteBody {
                format: DocumentFormat::Epub,
                capture_id: cap.capture_id,
                field: NoteField::Note,
                offset: 0,
                max_bytes: 256,
            },
        ) else {
            panic!("expected NoteBody");
        };
        assert_eq!(body.text, "Updated epub note");

        let CoreKnowledgeResult::CaptureDeleted { .. } = call(
            &mut notes,
            "n4",
            CoreKnowledgeOperation::DeleteCapture {
                format: DocumentFormat::Epub,
                capture_id: cap.capture_id,
            },
        ) else {
            panic!("expected CaptureDeleted");
        };
    }
}
