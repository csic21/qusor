//! Embedded ordinary packages and the workspace contributions they declare.
//!
//! The catalog is trusted composition data: it verifies the shipped package
//! before exposing contribution metadata, and only returns rows already enabled
//! by the resolved profile. It never mounts a package or grants a capability.
use crate::profile;
use capability_composition::{CompositionPlan, EntryId};
use extension_api::{ContributionKind, Manifest, SUPPORTED_HOST_APIS};
use extension_package::package::{VerifiedPackage, verify_for_host};
use serde_json::json;

struct EmbeddedAsset {
    bytes: &'static [u8],
    key: &'static str,
}

const EMBEDDED_ASSETS: [EmbeddedAsset; 4] = [
    EmbeddedAsset {
        bytes: include_bytes!("../assets/notes.qpk"),
        key: include_str!("../assets/notes.pub"),
    },
    EmbeddedAsset {
        bytes: include_bytes!("../assets/vocab.qpk"),
        key: include_str!("../assets/vocab.pub"),
    },
    EmbeddedAsset {
        bytes: include_bytes!("../assets/reader.qpk"),
        key: include_str!("../assets/reader.pub"),
    },
    EmbeddedAsset {
        bytes: include_bytes!("../assets/library.qpk"),
        key: include_str!("../assets/library.pub"),
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub package: String,
    pub title: String,
    pub kind: String,
}

pub fn verify_all() -> Result<Vec<VerifiedPackage>, String> {
    let mut verified = Vec::with_capacity(EMBEDDED_ASSETS.len());
    for asset in &EMBEDDED_ASSETS {
        verified.push(
            verify_for_host(asset.bytes, asset.key.trim(), &SUPPORTED_HOST_APIS)
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(verified)
}

pub fn verify(package_id: &str) -> Result<VerifiedPackage, String> {
    verify_all()?
        .into_iter()
        .find(|package| package.manifest().package_id == package_id)
        .ok_or_else(|| format!("no embedded package named {package_id}"))
}

#[cfg(test)]
pub fn verify_for_contribution(kind: ContributionKind) -> Result<VerifiedPackage, String> {
    verify_all()?
        .into_iter()
        .find(|package| {
            package
                .manifest()
                .contributions
                .iter()
                .any(|contribution| contribution.kind == kind)
        })
        .ok_or_else(|| format!("no embedded package declares contribution {:?}", kind))
}

pub fn is_embedded_package(package_id: &str) -> bool {
    verify_all().is_ok_and(|packages| {
        packages
            .iter()
            .any(|package| package.manifest().package_id == package_id)
    })
}

pub fn workspace_contributions() -> Result<Vec<WorkspaceDescriptor>, String> {
    let plan = profile::plan()?;
    workspace_contributions_for(plan)
}

pub fn workspace_contributions_for(
    plan: &CompositionPlan,
) -> Result<Vec<WorkspaceDescriptor>, String> {
    let mut workspaces = Vec::new();
    for verified in verify_all()? {
        let manifest: &Manifest = verified.manifest();
        for contribution in &manifest.contributions {
            let Some(kind) = workspace_kind(contribution.kind) else {
                continue;
            };
            if !is_active(plan, contribution.id.as_str())? {
                continue;
            }
            workspaces.push(WorkspaceDescriptor {
                id: contribution.id.clone(),
                package: manifest.package_id.clone(),
                title: contribution.title.clone(),
                kind: kind.into(),
            });
        }
    }
    Ok(workspaces)
}

fn is_active(plan: &CompositionPlan, id: &str) -> Result<bool, String> {
    let id = EntryId::parse(id).map_err(|error| error.to_string())?;
    Ok(plan.entry(&id).is_some())
}

pub fn workspace_kind(kind: ContributionKind) -> Option<&'static str> {
    match kind {
        ContributionKind::NotesWorkspace => Some("notes_workspace"),
        ContributionKind::VocabWorkspace => Some("vocab_workspace"),
        ContributionKind::ReaderWorkspace => Some("reader_workspace"),
        ContributionKind::LibraryWorkspace => Some("library_workspace"),
        ContributionKind::Command
        | ContributionKind::SelectionAction
        | ContributionKind::ComponentProvider
        | ContributionKind::UiSlot => None,
    }
}

pub fn report_json() -> Result<String, String> {
    let workspaces = workspace_contributions()?;
    Ok(report_with(workspaces))
}

pub fn report_json_for(plan: &CompositionPlan) -> Result<String, String> {
    let workspaces = workspace_contributions_for(plan)?;
    Ok(report_with(workspaces))
}

pub fn package_report_for(plan: &CompositionPlan) -> Result<String, String> {
    let mut packages = Vec::new();
    for verified in verify_all()? {
        let manifest = verified.manifest();
        let title = manifest
            .contributions
            .iter()
            .find(|contribution| workspace_kind(contribution.kind).is_some())
            .map(|contribution| contribution.title.clone())
            .unwrap_or_else(|| manifest.package_id.clone());
        let version = manifest.package_version;
        let enabled = plan
            .entries()
            .iter()
            .any(|planned| planned.entry().package().as_str() == manifest.package_id);
        packages.push(json!({
            "id": manifest.package_id,
            "title": title,
            "version": format!("{}.{}.{}", version.major, version.minor, version.patch),
            "enabled": enabled,
        }));
    }
    Ok(json!({ "packages": packages }).to_string())
}

fn report_with(workspaces: Vec<WorkspaceDescriptor>) -> String {
    json!({
        "workspaces": workspaces
            .into_iter()
            .map(|workspace| json!({
                "id": workspace.id,
                "package": workspace.package,
                "title": workspace.title,
                "kind": workspace.kind,
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_contributions_are_manifest_backed_and_profile_gated() {
        let workspaces = workspace_contributions().unwrap();
        assert_eq!(workspaces.len(), 4);
        assert!(workspaces.iter().any(|workspace| {
            workspace.id == "org.qusor.notes.workspace"
                && workspace.kind == "notes_workspace"
                && workspace.title == "想法"
        }));
        assert!(workspaces.iter().any(|workspace| {
            workspace.id == "org.qusor.vocab.workspace"
                && workspace.kind == "vocab_workspace"
                && workspace.title == "背单词"
        }));
        assert!(workspaces.iter().any(|workspace| {
            workspace.id == "org.qusor.reader.preview.workspace"
                && workspace.kind == "reader_workspace"
                && workspace.package == "org.qusor.reader.preview"
        }));
        assert!(workspaces.iter().any(|workspace| {
            workspace.id == "org.qusor.library.workspace"
                && workspace.kind == "library_workspace"
                && workspace.package == "org.qusor.library"
        }));
    }

    #[test]
    fn disabled_package_disappears_from_the_workspace_catalog() {
        let disabled = std::collections::BTreeSet::from(["org.qusor.vocab".into()]);
        let plan = profile::plan_with_disabled(&disabled).unwrap();
        let workspaces = workspace_contributions_for(&plan).unwrap();
        assert_eq!(workspaces.len(), 3);
        assert!(
            workspaces
                .iter()
                .any(|workspace| workspace.package == "org.qusor.notes")
        );
        assert!(
            workspaces
                .iter()
                .any(|workspace| workspace.package == "org.qusor.reader.preview")
        );
        assert!(
            workspaces
                .iter()
                .any(|workspace| workspace.package == "org.qusor.library")
        );
    }
}
