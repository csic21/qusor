//! Trusted Library Host mounted by the application composition runtime.
use crate::{package_catalog, profile, runtime, vault::VaultStore};
use extension_api::{ApiError, Capability, HOST_API_V0_6, Resource, library::*};
use extension_host::{Grant, Host, Session};
use learning_application::KnowledgeApplication;
use nsh_core::prelude::*;
use qusor_app_bridges::library_bridge::LibraryReadBridge;
use serde_json::{Value, json};

pub(crate) struct LibraryRuntime {
    host: Host,
    session: Session,
    bridge: LibraryReadBridge<VaultStore>,
}

impl LibraryRuntime {
    pub(crate) fn mount(store: VaultStore) -> Result<Self, String> {
        profile::require_active(profile::LIBRARY_HOST_ENTRY)?;
        let verified = package_catalog::verify("org.qusor.library")?;
        let mut host = Host::new("qusor-vault", [Capability::LibraryRead]).map_err(text)?;
        let installation = host
            .register(verified.manifest().clone(), verified.digest())
            .map_err(text)?;
        host.replace_grants(
            installation,
            vec![Grant {
                capability: Capability::LibraryRead,
                resource: Resource::Vault {
                    vault_id: "qusor-vault".into(),
                },
            }],
        )
        .map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_session_for_api(installation, HOST_API_V0_6)
            .map_err(text)?;
        Ok(Self {
            host,
            session,
            bridge: LibraryReadBridge::new(KnowledgeApplication::new(store), "qusor-vault"),
        })
    }

    fn page(&mut self, cursor: Option<String>, limit: u16) -> Result<Value, String> {
        let request = LibraryRequest {
            library_protocol: LIBRARY_PROTOCOL,
            api_version: HOST_API_V0_6,
            request_id: "workbench-library-page".into(),
            operation: LibraryOperation::Page { cursor, limit },
        };
        let permit = self
            .host
            .authorize_library(&self.session, &request)
            .map_err(text)?;
        let response = self
            .host
            .dispatch(permit, |_context| Ok(self.bridge.respond(&request)))
            .map_err(text)?;
        match response.result {
            LibraryResult::Page {
                documents,
                next_cursor,
            } => Ok(json!({
                "documents": documents,
                "next_cursor": next_cursor,
            })),
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
}
