//! Trusted Library Host mounted by the application composition runtime.
use crate::{package_catalog, profile, runtime, vault::VaultStore};
use extension_api::{ApiError, Capability, HOST_API_V0_7, Resource, library::*};
use extension_host::{Grant, Host, Session};
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_app_bridges::library_bridge::LibraryReadBridge;
use serde_json::{Value, json};

pub(crate) struct LibraryRuntime {
    host: Host,
    session: Session,
    store: VaultStore,
    bridge: LibraryReadBridge<VaultStore>,
}

impl LibraryRuntime {
    pub(crate) fn mount(store: VaultStore) -> Result<Self, String> {
        profile::require_active(profile::LIBRARY_HOST_ENTRY)?;
        let verified = package_catalog::verify("org.qusor.library")?;
        let mut host = Host::new(
            "qusor-vault",
            [Capability::LibraryRead, Capability::CapturePropose],
        )
        .map_err(text)?;
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        host.replace_grants(
            installation,
            vec![
                Grant {
                    capability: Capability::LibraryRead,
                    resource: Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
                Grant {
                    capability: Capability::CapturePropose,
                    resource: Resource::Vault {
                        vault_id: "qusor-vault".into(),
                    },
                },
            ],
        )
        .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_session_for_api(installation, HOST_API_V0_7)
            .map_err(text)?;
        Ok(Self {
            host,
            session,
            store: store.clone(),
            bridge: LibraryReadBridge::new(KnowledgeApplication::new(store), "qusor-vault"),
        })
    }

    #[cfg(unix)]
    pub(crate) fn import_pdf(
        &self,
        sys: &System,
        title: String,
        bytes: Vec<u8>,
    ) -> Result<Value, String> {
        let imported = crate::pdf_reader::import(sys, self.store.clone(), title, bytes)?;
        Ok(crate::pdf_reader::import_value(imported))
    }

    #[cfg(unix)]
    pub(crate) fn render_pdf_page(
        &self,
        sys: &System,
        document_id: i64,
        page_index: u32,
    ) -> Result<crate::pdf_page::PdfPageData, String> {
        crate::pdf_reader::render_page(sys, self.store.clone(), document_id, page_index)
    }

    fn page(&mut self, cursor: Option<String>, limit: u16) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-page".into(),
            operation: LibraryOperation::Page { cursor, limit },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Page {
                documents,
                next_cursor,
            } => Ok(json!({
                "documents": documents,
                "next_cursor": next_cursor,
            })),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn page_text(
        &mut self,
        document_id: i64,
        page_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-page-text".into(),
            operation: LibraryOperation::PageText {
                document_id,
                page_index,
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
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::PageText {
                document_id,
                page_index,
                offset,
                total_bytes,
                text,
                next_offset,
            } => Ok(json!({
                "document_id": document_id,
                "page_index": page_index,
                "offset": offset,
                "total_bytes": total_bytes,
                "text": text,
                "next_offset": next_offset,
            })),
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn capture_selection(
        &mut self,
        document_id: i64,
        page_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: operation_id,
            operation: LibraryOperation::CaptureSelection {
                document_id,
                page_index,
                start,
                end,
                note,
            },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Capture {
                capture_id,
                commit_rev,
            } => Ok(json!({
                "capture_id": capture_id,
                "commit_rev": commit_rev,
            })),
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::SavedSelections { .. } => {
                Err("unexpected Library saved-selection result".into())
            }
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }

    fn saved_selections(&mut self, document_id: i64, page_index: u32) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_7,
            request_id: "workbench-library-saved-selections".into(),
            operation: LibraryOperation::SavedSelections {
                document_id,
                page_index,
            },
        };
        let Self {
            host,
            session,
            bridge,
            ..
        } = self;
        let permit = host.authorize_library(session, &request).map_err(text)?;
        let response = host
            .dispatch(permit, |context| Ok(bridge.respond(&request, context)))
            .map_err(text)?;
        match response.result {
            LibraryResult::SavedSelections {
                document_id,
                page_index,
                ranges,
            } => Ok(json!({
                "document_id": document_id,
                "page_index": page_index,
                "ranges": ranges,
            })),
            LibraryResult::Page { .. } => Err("unexpected Library page result".into()),
            LibraryResult::PageText { .. } => Err("unexpected Library text result".into()),
            LibraryResult::Capture { .. } => Err("unexpected Library capture result".into()),
            LibraryResult::Error { error } => Err(wire_error(error)),
        }
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn wire_error(error: ApiError) -> String {
    format!("{:?}: {}", error.code, error.message)
}

/// Native Library surface for bounded metadata reads.
#[module]
pub struct LibraryHost;

#[module]
impl LibraryHost {
    /// Import one user-selected PDF into the local Core.
    pub fn import_pdf(sys: System, title: String, bytes: Vec<u8>) -> Later<String> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::import(sys.data_dir, title, bytes))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, title, bytes);
            Later::new(async {
                Ok(
                    json!({"ok": false, "error": "PDF import is not available on this platform yet"})
                        .to_string(),
                )
            })
        }
    }

    /// Render one page from the retained original as a native image.
    pub fn page_image(
        sys: System,
        document_id: i64,
        page_index: u32,
    ) -> Later<crate::pdf_page::PdfPageFile> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::render(
                sys.data_dir,
                document_id,
                page_index,
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, document_id, page_index);
            Later::new(async {
                Ok(crate::pdf_page::failed_page(
                    "PDF page rendering is not available on this platform yet".into(),
                ))
            })
        }
    }

    /// Render a page into the bounded native cache without crossing an image.
    pub fn prefetch_page(sys: System, document_id: i64, page_index: u32) -> Later<String> {
        #[cfg(unix)]
        {
            Later::new(crate::pdf_service::prefetch(
                sys.data_dir,
                document_id,
                page_index,
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = (sys, document_id, page_index);
            Later::new(async {
                Ok(
                    json!({"ok": false, "error": "PDF prefetch is not available on this platform"})
                        .to_string(),
                )
            })
        }
    }

    /// One newest-first page of PDF metadata in the current vault.
    pub fn page(sys: System, cursor: String, limit: u16) -> String {
        let cursor = if cursor.is_empty() {
            None
        } else {
            Some(cursor)
        };
        match runtime::with_library(&sys, |library| library.page(cursor, limit)) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// One bounded text-layer chunk for a Library document page.
    pub fn page_text(
        sys: System,
        document_id: i64,
        page_index: u32,
        offset: u32,
        max_bytes: u16,
    ) -> String {
        match runtime::with_library(&sys, |library| {
            library.page_text(document_id, page_index, offset, max_bytes)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Saved scalar ranges on one PDF page for page-image highlights.
    pub fn saved_selections(sys: System, document_id: i64, page_index: u32) -> String {
        match runtime::with_library(&sys, |library| {
            library.saved_selections(document_id, page_index)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Capture the current scalar range through the trusted Core adapter.
    pub fn capture_selection(
        sys: System,
        document_id: i64,
        page_index: u32,
        start: u32,
        end: u32,
        note: String,
        operation_id: String,
    ) -> String {
        match runtime::with_library(&sys, |library| {
            library.capture_selection(document_id, page_index, start, end, note, operation_id)
        }) {
            Ok(value) => json!({"ok": true, "value": value}).to_string(),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }
}
