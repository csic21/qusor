//! Application-local package enable intent.
//!
//! This is plugin-management state, not Core knowledge. Disabling a package does
//! not delete its data, package bytes or provenance; it only prevents the app
//! composition from mounting that package's Host rows on the next reload.
use crate::package_catalog;
use nsh_core::prelude::System;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

const PROTOCOL_VERSION: u16 = 1;
const RELATIVE_PATH: &str = "extensions/preferences.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    protocol_version: u16,
    disabled_packages: BTreeSet<String>,
}

impl Preferences {
    pub fn load(sys: &System) -> Result<Self, String> {
        let Some(path) = path(sys) else {
            return Ok(Self {
                protocol_version: PROTOCOL_VERSION,
                disabled_packages: BTreeSet::new(),
            });
        };
        if !path.is_file() {
            return Ok(Self {
                protocol_version: PROTOCOL_VERSION,
                disabled_packages: BTreeSet::new(),
            });
        }
        let bytes =
            fs::read(&path).map_err(|error| format!("cannot read extension state: {error}"))?;
        let preferences: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid extension state: {error}"))?;
        if preferences.protocol_version != PROTOCOL_VERSION {
            return Err(format!(
                "unsupported extension state version: {}",
                preferences.protocol_version
            ));
        }
        for package in &preferences.disabled_packages {
            if !package_catalog::is_embedded_package(package) {
                return Err(format!(
                    "unknown embedded package in extension state: {package}"
                ));
            }
        }
        Ok(preferences)
    }

    pub fn save(&self, sys: &System) -> Result<(), String> {
        let path = path(sys).ok_or("the host gave no app data directory")?;
        let directory = path
            .parent()
            .ok_or("extension state path has no parent directory")?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("cannot create extension state directory: {error}"))?;
        let temporary = directory.join("preferences.json.tmp");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("cannot encode extension state: {error}"))?;
        let mut file = File::create(&temporary)
            .map_err(|error| format!("cannot write extension state: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot commit extension state: {error}"))?;
        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|error| format!("cannot replace extension state: {error}"))?;
        }
        fs::rename(&temporary, &path)
            .map_err(|error| format!("cannot replace extension state: {error}"))
    }

    pub fn set_enabled(&mut self, package_id: &str, enabled: bool) -> Result<bool, String> {
        if crate::profile::is_required_package(package_id)? {
            return Err("the Core provider cannot be disabled".into());
        }
        if !package_catalog::is_embedded_package(package_id) {
            return Err(format!("package is not embedded in this app: {package_id}"));
        }
        let was_disabled = self.disabled_packages.contains(package_id);
        if was_disabled == !enabled {
            return Ok(false);
        }
        if enabled {
            self.disabled_packages.remove(package_id);
        } else {
            self.disabled_packages.insert(package_id.to_owned());
        }
        Ok(true)
    }

    pub fn disabled_packages(&self) -> &BTreeSet<String> {
        &self.disabled_packages
    }
}

fn path(sys: &System) -> Option<PathBuf> {
    sys.data_dir.as_deref().map(data_path)
}

fn data_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RELATIVE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preferences() -> Preferences {
        Preferences {
            protocol_version: PROTOCOL_VERSION,
            disabled_packages: BTreeSet::new(),
        }
    }

    #[test]
    fn enable_intent_is_idempotent_and_protects_core() {
        let mut preferences = preferences();
        assert!(!preferences.set_enabled("org.qusor.vocab", true).unwrap());
        assert!(preferences.set_enabled("org.qusor.vocab", false).unwrap());
        assert!(preferences.disabled_packages.contains("org.qusor.vocab"));
        assert!(!preferences.set_enabled("org.qusor.vocab", false).unwrap());
        assert!(preferences.set_enabled("org.qusor.vocab", true).unwrap());
        assert!(preferences.disabled_packages.is_empty());
        assert!(preferences.set_enabled("org.qusor.core", false).is_err());
        assert!(
            preferences
                .set_enabled("org.example.missing", false)
                .is_err()
        );
    }
}
