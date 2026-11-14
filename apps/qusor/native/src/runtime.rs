//! Application composition root and scoped Host runtime.
//!
//! The composition plan decides which entries exist and their order. This module
//! mounts the host-side entries in that order, owns their values, and exposes
//! typed access to the native module adapters. UI contribution entries are not
//! mounted here yet; their rows remain declared for the shell stage.
use crate::{
    extension_preferences::Preferences,
    library_native::LibraryRuntime,
    notes_native::NotesRuntime,
    package_catalog, profile,
    vault::{self, VaultStore},
    vocab_native::VocabRuntime,
};
use capability_composition::{EntryId, RuntimeScope, ServiceId};
use nsh_core::prelude::System;
use std::{collections::BTreeSet, sync::Mutex};

static RUNTIME: Mutex<Option<AppRuntime>> = Mutex::new(None);

enum AppService {
    // The Core value is held for lifecycle/order; consumers receive clones from
    // the notes and vocabulary service constructors.
    #[allow(dead_code)]
    Core(VaultStore),
    Notes(NotesRuntime),
    Vocab(VocabRuntime),
    Library(LibraryRuntime),
}

struct AppRuntime {
    scope: RuntimeScope<AppService>,
}

fn provider_entry(
    scope: &RuntimeScope<AppService>,
    service: &str,
) -> Result<Option<EntryId>, String> {
    let service = ServiceId::parse(service).map_err(|error| error.to_string())?;
    Ok(scope
        .plan()
        .provider(&service)
        .map(|binding| binding.entry().clone()))
}

impl AppRuntime {
    fn boot(
        sys: &System,
        disabled_packages: BTreeSet<String>,
        existing_core: Option<VaultStore>,
    ) -> Result<Self, String> {
        let plan = profile::plan_with_disabled(&disabled_packages)?;
        let entries = plan
            .entries()
            .iter()
            .map(|planned| {
                let entry = planned.entry();
                let services = entry
                    .provides()
                    .iter()
                    .map(|provision| provision.service().as_str().to_string())
                    .collect::<Vec<_>>();
                (
                    entry.id().clone(),
                    entry.package().as_str().to_string(),
                    services,
                )
            })
            .collect::<Vec<_>>();
        let mut scope = RuntimeScope::new(plan);
        let mut core = existing_core;

        for (entry, package_id, services) in entries {
            let provides = |target: &str| services.iter().any(|service| service == target);
            if provides(profile::VAULT_READ_SERVICE) {
                let store = match core.clone() {
                    Some(store) => store,
                    None => vault::open(sys)?,
                };
                // Bundled study content is optional: a seed failure must not
                // block notes or reading from opening the same Core.
                if core.is_none() {
                    let _ = store.ensure_starter_wordbook();
                }
                core = Some(store.clone());
                scope
                    .mount(&entry, AppService::Core(store))
                    .map_err(|error| error.to_string())?;
            } else if provides(profile::NOTES_READ_SERVICE) {
                let store = core
                    .clone()
                    .ok_or("the notes Host was not preceded by the Core provider")?;
                let runtime = NotesRuntime::mount_for_package(store, &package_id)?;
                scope
                    .mount(&entry, AppService::Notes(runtime))
                    .map_err(|error| error.to_string())?;
            } else if provides(profile::VOCAB_READ_SERVICE) {
                let store = core
                    .clone()
                    .ok_or("the vocabulary Host was not preceded by the Core provider")?;
                let runtime = VocabRuntime::mount_for_package(store, &package_id)?;
                scope
                    .mount(&entry, AppService::Vocab(runtime))
                    .map_err(|error| error.to_string())?;
            } else if provides(profile::LIBRARY_READ_SERVICE) {
                let store = core
                    .clone()
                    .ok_or("the Library Host was not preceded by the Core provider")?;
                let runtime = LibraryRuntime::mount_for_package(store, &package_id)?;
                scope
                    .mount(&entry, AppService::Library(runtime))
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(Self { scope })
    }

    fn core(&self) -> Result<VaultStore, String> {
        let entry = provider_entry(&self.scope, profile::VAULT_READ_SERVICE)?
            .ok_or_else(|| "Core provider is not mounted".to_string())?;
        match self.scope.get(&entry) {
            Some(AppService::Core(core)) => Ok(core.clone()),
            Some(_) => Err("composition entry does not mount the Core provider".into()),
            None => Err("Core provider is not mounted".into()),
        }
    }
}

impl Drop for AppRuntime {
    fn drop(&mut self) {
        // Reverse activation order releases the Host sessions before the Core
        // clones they hold.
        drop(self.scope.unmount_all());
    }
}

fn with_runtime<T>(
    sys: &System,
    operation: impl FnOnce(&mut AppRuntime) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = RUNTIME
        .lock()
        .map_err(|_| "application runtime unavailable")?;
    if guard.is_none() {
        let preferences = Preferences::load(sys)?;
        *guard = Some(AppRuntime::boot(
            sys,
            preferences.disabled_packages().clone(),
            None,
        )?);
    }
    operation(guard.as_mut().ok_or("application runtime unavailable")?)
}

/// Apply one process-level package enable intent and rebuild the Host scope.
/// The Core handle is reused, so disabled Hosts can be dropped without closing,
/// migrating or deleting the knowledge store. Preferences are committed before
/// the old Host scope is replaced.
pub(crate) fn set_package_enabled(
    sys: &System,
    package_id: &str,
    enabled: bool,
) -> Result<String, String> {
    let mut preferences = Preferences::load(sys)?;
    let changed = preferences.set_enabled(package_id, enabled)?;
    let disabled = preferences.disabled_packages().clone();

    {
        let mut guard = RUNTIME
            .lock()
            .map_err(|_| "application runtime unavailable")?;
        if changed {
            let replacement = match guard.as_ref() {
                Some(runtime) => Some(AppRuntime::boot(
                    sys,
                    disabled.clone(),
                    Some(runtime.core()?),
                )?),
                None => None,
            };
            preferences.save(sys)?;
            if let Some(replacement) = replacement {
                *guard = Some(replacement);
            }
        }
    }
    workspace_report(sys)
}

pub(crate) fn workspace_report(sys: &System) -> Result<String, String> {
    let preferences = Preferences::load(sys)?;
    let plan = profile::plan_with_disabled(preferences.disabled_packages())?;
    package_catalog::report_json_for(&plan)
}

pub(crate) fn package_report(sys: &System) -> Result<String, String> {
    let preferences = Preferences::load(sys)?;
    let plan = profile::plan_with_disabled(preferences.disabled_packages())?;
    package_catalog::package_report_for(&plan)
}

pub(crate) fn core_handle(sys: &System) -> Result<VaultStore, String> {
    with_runtime(sys, |runtime| runtime.core())
}

pub(crate) fn with_notes<T>(
    sys: &System,
    operation: impl FnOnce(&mut NotesRuntime) -> Result<T, String>,
) -> Result<T, String> {
    with_runtime(sys, |runtime| {
        let entry = provider_entry(&runtime.scope, profile::NOTES_READ_SERVICE)?
            .ok_or_else(|| "notes runtime is not mounted".to_string())?;
        match runtime.scope.get_mut(&entry) {
            Some(AppService::Notes(notes)) => operation(notes),
            Some(_) => Err("composition entry does not mount a notes runtime".into()),
            None => Err("notes runtime is not mounted".into()),
        }
    })
}

pub(crate) fn with_vocab<T>(
    sys: &System,
    operation: impl FnOnce(&mut VocabRuntime) -> Result<T, String>,
) -> Result<T, String> {
    with_runtime(sys, |runtime| {
        let entry = provider_entry(&runtime.scope, profile::VOCAB_READ_SERVICE)?
            .ok_or_else(|| "vocabulary runtime is not mounted".to_string())?;
        match runtime.scope.get_mut(&entry) {
            Some(AppService::Vocab(vocab)) => operation(vocab),
            Some(_) => Err("composition entry does not mount a vocabulary runtime".into()),
            None => Err("vocabulary runtime is not mounted".into()),
        }
    })
}

pub(crate) fn with_library<T>(
    sys: &System,
    operation: impl FnOnce(&mut LibraryRuntime) -> Result<T, String>,
) -> Result<T, String> {
    with_runtime(sys, |runtime| {
        let entry = provider_entry(&runtime.scope, profile::LIBRARY_READ_SERVICE)?
            .ok_or_else(|| "Library runtime is not mounted".to_string())?;
        match runtime.scope.get_mut(&entry) {
            Some(AppService::Library(library)) => operation(library),
            Some(_) => Err("composition entry does not mount a Library runtime".into()),
            None => Err("Library runtime is not mounted".into()),
        }
    })
}
