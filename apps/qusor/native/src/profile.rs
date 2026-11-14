//! Declarative desktop composition for the packaged application.
//!
//! The profile is data, not executable configuration. Base service providers
//! come from `desktop-profile.json`, while workspace feature entries are
//! generated from the verified package catalog (`package_catalog`). Plugin
//! permissions and package verification remain separate.
use crate::package_catalog;
use capability_composition::{
    CompositionLayer, CompositionPlan, CompositionProfile, Entry, EntryId, ServiceId,
    ServiceProvision, ServiceRequirement, ServiceVersion, VersionRequirement,
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeSet, sync::OnceLock};

const PROFILE_JSON: &str = include_str!("../../desktop-profile.json");
pub const VAULT_READ_SERVICE: &str = "vault.read";
pub const NOTES_READ_SERVICE: &str = "notes.read";
pub const VOCAB_READ_SERVICE: &str = "vocab.read";
pub const LIBRARY_READ_SERVICE: &str = "library.read";
static PROFILE: OnceLock<Result<CompositionPlan, String>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileSource {
    protocol_version: u16,
    profile: String,
    layers: Vec<LayerSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerSource {
    name: String,
    entries: Vec<EntrySource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntrySource {
    id: String,
    package: String,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    provides: Vec<ProvisionSource>,
    #[serde(default)]
    requires: Vec<RequirementSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvisionSource {
    service: String,
    major: u16,
    minor: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementSource {
    service: String,
    major: u16,
    min_minor: u16,
}

fn enabled_by_default() -> bool {
    true
}

/// Resolve the embedded desktop profile once. A malformed or unsatisfied profile
/// is a product startup error, not a reason to silently skip a plugin.
pub fn plan() -> Result<&'static CompositionPlan, String> {
    match PROFILE.get_or_init(load) {
        Ok(plan) => Ok(plan),
        Err(error) => Err(error.clone()),
    }
}

/// Resolve the embedded profile with process-local package enable intent applied.
/// Disabling a provider also removes its workspace consumer rows, so the plan
/// never claims that a missing service is still active.
pub fn plan_with_disabled(disabled_packages: &BTreeSet<String>) -> Result<CompositionPlan, String> {
    if disabled_packages.is_empty() {
        return plan().cloned();
    }
    load_with_disabled(disabled_packages)
}

/// Gate a Host initializer on an enabled row. This keeps a disabled plugin from
/// starting merely because a native method happened to be called.
pub fn require_active(entry: &str) -> Result<(), String> {
    let entry = EntryId::parse(entry).map_err(|error| error.to_string())?;
    let plan = plan()?;
    if plan.entry(&entry).is_some() {
        Ok(())
    } else {
        Err(format!("composition entry is missing or disabled: {entry}"))
    }
}

/// Gate a Host initializer on an active provider of `service`.
pub fn require_service_active(service: &str) -> Result<(), String> {
    let service = ServiceId::parse(service).map_err(|error| error.to_string())?;
    let plan = plan()?;
    if plan.provider(&service).is_some() {
        Ok(())
    } else {
        Err(format!(
            "composition provider for service is missing or disabled: {service}"
        ))
    }
}

/// Whether `package_id` is declared as `required` in the resolved base profile.
pub fn is_required_package(package_id: &str) -> Result<bool, String> {
    let plan = plan()?;
    Ok(plan.entries().iter().any(|planned| {
        planned.entry().is_required() && planned.entry().package().as_str() == package_id
    }))
}

/// Diagnostic receipt consumed by the native host and evidence tests. It exposes
/// only the resolved composition; no package bytes, paths or grants are included.
pub fn report_json() -> String {
    match plan() {
        Ok(plan) => {
            let entries = plan
                .entries()
                .iter()
                .map(|planned| {
                    let entry = planned.entry();
                    json!({
                        "id": entry.id().as_str(),
                        "package": entry.package().as_str(),
                        "layer": planned.layer().as_str(),
                        "required": entry.is_required(),
                        "provides": entry.provides().iter().map(|provision| json!({
                            "service": provision.service().as_str(),
                            "version": provision.version().to_string(),
                        })).collect::<Vec<_>>(),
                        "requires": entry.requires().iter().map(|requirement| json!({
                            "service": requirement.service().as_str(),
                            "version": requirement.version().to_string(),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "ok": true,
                "value": {
                    "profile": plan.profile().as_str(),
                    "entries": entries,
                    "dump": plan.dump(),
                }
            })
            .to_string()
        }
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

fn load() -> Result<CompositionPlan, String> {
    load_with_disabled(&BTreeSet::new())
}

fn load_with_disabled(disabled_packages: &BTreeSet<String>) -> Result<CompositionPlan, String> {
    let source: ProfileSource =
        serde_json::from_str(PROFILE_JSON).map_err(|error| format!("invalid profile: {error}"))?;
    if source.protocol_version != 1 {
        return Err(format!(
            "unsupported composition profile version: {}",
            source.protocol_version
        ));
    }
    let mut profile =
        CompositionProfile::new(&source.profile).map_err(|error| error.to_string())?;
    for layer in source.layers {
        let entries = layer
            .entries
            .into_iter()
            .map(|entry| parse_entry(entry, disabled_packages))
            .collect::<Result<Vec<_>, _>>()?;
        let layer =
            CompositionLayer::new(layer.name, entries).map_err(|error| error.to_string())?;
        profile
            .push_layer(layer)
            .map_err(|error| error.to_string())?;
    }
    let feature_entries = catalog_feature_entries(disabled_packages)?;
    if !feature_entries.is_empty() {
        let features = CompositionLayer::new("features", feature_entries)
            .map_err(|error| error.to_string())?;
        profile
            .push_layer(features)
            .map_err(|error| error.to_string())?;
    }
    profile.resolve().map_err(|error| error.to_string())
}

fn catalog_feature_entries(disabled_packages: &BTreeSet<String>) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for verified in package_catalog::verify_all()? {
        let manifest = verified.manifest();
        let enabled = !disabled_packages.contains(&manifest.package_id);
        for contribution in &manifest.contributions {
            if package_catalog::workspace_kind(contribution.kind).is_none() {
                continue;
            }
            let mut entry = Entry::new(contribution.id.clone(), manifest.package_id.clone())
                .map_err(|error| error.to_string())?
                .enabled(enabled)
                .required(false);
            let capabilities = if contribution.required_capabilities.is_empty() {
                &manifest.required_capabilities
            } else {
                &contribution.required_capabilities
            };
            for capability in capabilities {
                entry = entry.requiring(ServiceRequirement::new(
                    ServiceId::parse(capability_service_name(*capability))
                        .map_err(|error| error.to_string())?,
                    VersionRequirement::new(manifest.host_api.major, manifest.host_api.min_minor),
                ));
            }
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn capability_service_name(capability: extension_api::Capability) -> &'static str {
    use extension_api::Capability;
    match capability {
        Capability::SelectionRead => "selection.read",
        Capability::DocumentMetadataRead => "document.metadata.read",
        Capability::CapturePropose => "capture.propose",
        Capability::ReaderSurface => "reader.surface",
        Capability::NoteDraftEdit => "note.draft.edit",
        Capability::NotesRead => "notes.read",
        Capability::VocabRead => "vocab.read",
        Capability::VocabReview => "vocab.review",
        Capability::LibraryRead => "library.read",
        Capability::UiSlot => "ui.slot",
    }
}

fn parse_entry(source: EntrySource, disabled_packages: &BTreeSet<String>) -> Result<Entry, String> {
    let mut entry = Entry::new(source.id, source.package).map_err(|error| error.to_string())?;
    let enabled = source.enabled
        && !disabled_packages
            .iter()
            .any(|package| package == entry.package().as_str());
    entry = entry.enabled(enabled).required(source.required);
    for provision in source.provides {
        entry = entry.providing(ServiceProvision::new(
            ServiceId::parse(provision.service).map_err(|error| error.to_string())?,
            ServiceVersion::new(provision.major, provision.minor),
        ));
    }
    for requirement in source.requires {
        entry = entry.requiring(ServiceRequirement::new(
            ServiceId::parse(requirement.service).map_err(|error| error.to_string())?,
            VersionRequirement::new(requirement.major, requirement.min_minor),
        ));
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_desktop_profile_resolves_its_real_consumers() {
        let plan = plan().unwrap();
        assert_eq!(plan.profile().as_str(), "desktop");
        assert_eq!(plan.entries().len(), 9);
        assert!(
            plan.provider(&capability_composition::ServiceId::parse("vault.read").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("org.qusor.notes.workspace").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("org.qusor.vocab.workspace").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("org.qusor.reader.preview.workspace").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("org.qusor.library.workspace").unwrap())
                .is_some()
        );
        assert!(
            plan.provider(&capability_composition::ServiceId::parse("reader.surface").unwrap())
                .is_some()
        );
        assert!(plan.dump().contains("provides vocab.review 0.5"));
    }

    #[test]
    fn active_gate_rejects_unknown_rows() {
        let error = require_active("app.qusor.reader.host").unwrap_err();
        assert!(error.contains("missing or disabled"));
    }

    #[test]
    fn disabled_package_removes_its_host_and_workspace_rows() {
        let disabled = BTreeSet::from(["org.qusor.vocab".into()]);
        let plan = plan_with_disabled(&disabled).unwrap();
        assert!(
            plan.entry(&EntryId::parse("app.qusor.core").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("app.qusor.notes.host").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("app.qusor.vocab.host").unwrap())
                .is_none()
        );
        assert!(
            plan.entry(&EntryId::parse("org.qusor.vocab.workspace").unwrap())
                .is_none()
        );
    }

    #[test]
    fn diagnostic_receipt_is_bounded_and_contains_no_runtime_authority() {
        let receipt: serde_json::Value = serde_json::from_str(&report_json()).unwrap();
        assert_eq!(receipt["ok"], true);
        assert_eq!(receipt["value"]["profile"], "desktop");
        assert_eq!(receipt["value"]["entries"].as_array().unwrap().len(), 9);
        assert!(receipt["value"]["dump"].as_str().unwrap().contains("base"));
        assert!(receipt.get("grants").is_none());
        assert!(receipt.get("paths").is_none());
    }
}
