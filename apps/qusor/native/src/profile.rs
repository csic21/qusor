//! Declarative desktop composition for the packaged application.
//!
//! The profile is data, not executable configuration. It is parsed once, resolved
//! through `capability-composition`, and used to gate Host activation. Plugin
//! permissions and package verification remain separate.
use capability_composition::{
    CompositionLayer, CompositionPlan, CompositionProfile, Entry, EntryId, ServiceId,
    ServiceProvision, ServiceRequirement, ServiceVersion, VersionRequirement,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::OnceLock;

const PROFILE_JSON: &str = include_str!("../../desktop-profile.json");
pub const NOTES_HOST_ENTRY: &str = "app.qusor.notes.host";
pub const VOCAB_HOST_ENTRY: &str = "app.qusor.vocab.host";
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
            .map(parse_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let layer =
            CompositionLayer::new(layer.name, entries).map_err(|error| error.to_string())?;
        profile
            .push_layer(layer)
            .map_err(|error| error.to_string())?;
    }
    profile.resolve().map_err(|error| error.to_string())
}

fn parse_entry(source: EntrySource) -> Result<Entry, String> {
    let mut entry = Entry::new(source.id, source.package).map_err(|error| error.to_string())?;
    entry = entry.enabled(source.enabled).required(source.required);
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
        assert_eq!(plan.entries().len(), 4);
        assert!(
            plan.entry(&EntryId::parse("app.qusor.notes.workspace").unwrap())
                .is_some()
        );
        assert!(
            plan.entry(&EntryId::parse("app.qusor.vocab.workspace").unwrap())
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
    fn diagnostic_receipt_is_bounded_and_contains_no_runtime_authority() {
        let receipt: serde_json::Value = serde_json::from_str(&report_json()).unwrap();
        assert_eq!(receipt["ok"], true);
        assert_eq!(receipt["value"]["profile"], "desktop");
        assert_eq!(receipt["value"]["entries"].as_array().unwrap().len(), 4);
        assert!(receipt["value"]["dump"].as_str().unwrap().contains("base"));
        assert!(receipt.get("grants").is_none());
        assert!(receipt.get("paths").is_none());
    }
}
