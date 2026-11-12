//! Native diagnostic entry for the resolved application composition.
use crate::{profile, runtime};
use nsh_core::prelude::*;
use serde_json::json;

/// Diagnostic native surface for the resolved desktop composition.
#[module]
pub struct CompositionHost;

#[module]
impl CompositionHost {
    /// Return the validated profile, ordered entries and provider/consumer edges.
    pub fn dump() -> String {
        profile::report_json()
    }

    /// Workspace contributions declared by active, verified ordinary packages.
    pub fn workspaces(sys: System) -> String {
        match runtime::workspace_report(&sys) {
            Ok(value) => format!("{{\"ok\":true,\"value\":{value}}}"),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Embedded packages with their persisted enable state.
    pub fn packages(sys: System) -> String {
        match runtime::package_report(&sys) {
            Ok(value) => format!("{{\"ok\":true,\"value\":{value}}}"),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }

    /// Persist one package enable intent and rebuild the active Host scope.
    pub fn set_package_enabled(sys: System, package_id: String, enabled: bool) -> String {
        match runtime::set_package_enabled(&sys, &package_id, enabled) {
            Ok(value) => format!("{{\"ok\":true,\"value\":{value}}}"),
            Err(error) => json!({"ok": false, "error": error}).to_string(),
        }
    }
}
