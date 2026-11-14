//! Generic declarative UI slot host (`ui_slot`) backed by `WasmiSession`,
//! `Host` capability authorization, authorized Core wire bridges, and
//! `PluginUiHost` validation / generation fencing.
//!
//! This module contains zero package-specific or feature-specific hardcoding:
//! any verified embedded package declaring a `ContributionKind::UiSlot`
//! contribution can be mounted and driven through the same contract.

use crate::{core_facade, package_catalog, runtime, vault::VaultStore};
use extension_api::{
    Capability, Resource, SUPPORTED_HOST_APIS,
    library::LibraryRequest,
    notes::NotesRequest,
    ui::{
        SlotDescriptor, SlotRegion, UI_SLOT_PROTOCOL, UiBudgets, UiEventEnvelope, UiPatch,
        UiSlotRegistration, UiTree,
    },
    vocab::{VocabRequest, VocabResponse, VocabResult},
};
use extension_host::{Grant, Host, Session, ui::PluginUiHost};
use extension_runtime_api::{
    CapabilityBroker, ModuleSession, RuntimeBudgets, RuntimeError, SessionExecutor,
};
use nsh_core::prelude::*;
use runtime_wasm::WasmiRuntime;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        mpsc::{self, Sender, SyncSender},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

static SLOT_ACTOR: Mutex<Option<Sender<SlotCommand>>> = Mutex::new(None);

const HOST_SUPPORTED_CAPABILITIES: [Capability; 5] = [
    Capability::VocabRead,
    Capability::VocabReview,
    Capability::NotesRead,
    Capability::LibraryRead,
    Capability::UiSlot,
];

enum SlotCommand {
    Mount {
        contribution_id: String,
        store: VaultStore,
        reply: SyncSender<Result<Value, String>>,
    },
    DispatchEvent {
        mount_id: String,
        envelope_json: String,
        reply: SyncSender<Result<Value, String>>,
    },
    Unmount {
        mount_id: String,
        reply: SyncSender<Result<Value, String>>,
    },
    UnmountPackage {
        package_id: String,
        reply: SyncSender<()>,
    },
}

struct SlotEntry {
    package_id: String,
    host: Host,
    session: Session,
    wasm: Box<dyn ModuleSession>,
    store: VaultStore,
    request_seq: u64,
}

struct SlotManager {
    ui_host: PluginUiHost,
    slots: BTreeMap<String, SlotEntry>,
    next_mount_seq: u64,
}

impl SlotManager {
    fn new() -> Self {
        Self {
            ui_host: PluginUiHost::new(UiBudgets::default()),
            slots: BTreeMap::new(),
            next_mount_seq: 0,
        }
    }

    fn mount(&mut self, contribution_id: &str, store: VaultStore) -> Result<Value, String> {
        let (verified, contribution) = package_catalog::find_ui_slot(contribution_id)?;
        let manifest = verified.manifest().clone();
        let wasm_bytes = verified.module_bytes();

        let vault_id = store.vault_id().to_owned();
        let mut host = Host::new(vault_id.clone(), HOST_SUPPORTED_CAPABILITIES).map_err(text)?;
        let installation = host
            .register(manifest.clone(), verified.digest())
            .map_err(text)?;

        let grants: Vec<Grant> = manifest
            .required_capabilities
            .iter()
            .chain(
                manifest
                    .optional_capabilities
                    .iter()
                    .filter(|cap| HOST_SUPPORTED_CAPABILITIES.contains(cap)),
            )
            .map(|&capability| Grant {
                capability,
                resource: Resource::Vault {
                    vault_id: vault_id.clone(),
                },
            })
            .collect();
        host.replace_grants(installation, grants).map_err(text)?;
        host.enable(installation).map_err(text)?;
        let session = host
            .open_negotiated_session(installation, &SUPPORTED_HOST_APIS)
            .map_err(text)?;
        let api_version = host.session_api(&session).map_err(text)?;

        let required_caps = if contribution.required_capabilities.is_empty() {
            manifest.required_capabilities.clone()
        } else {
            contribution.required_capabilities.clone()
        };
        let registration = UiSlotRegistration {
            slot_protocol: UI_SLOT_PROTOCOL,
            api_version,
            descriptor: SlotDescriptor {
                contribution_id: contribution.id.clone(),
                region: SlotRegion::WorkspaceContent,
                role: "qusor.ui.slot".into(),
                min_width_pt: 320,
                supports_compact: true,
            },
            required_capabilities: required_caps,
        };

        let mut wasm = WasmiRuntime
            .instantiate(wasm_bytes, &runtime_budgets())
            .map_err(text)?;

        self.next_mount_seq += 1;
        let seq = self.next_mount_seq;
        let mount_id = format!("slot-{seq}");
        let permit = host
            .authorize_ui_slot(&session, api_version, format!("slot-mount-{seq}"))
            .map_err(text)?;
        host.dispatch(permit, |_| Ok(())).map_err(text)?;

        let generation = self
            .ui_host
            .mount_registered_slot(&mount_id, &manifest.package_id, &registration)
            .map_err(text)?
            .generation();

        let render_input = serde_json::to_vec(&json!({
            "contribution_id": contribution.id,
            "mount_id": mount_id,
            "generation": generation,
            "api_version": api_version,
            "compact": false,
        }))
        .map_err(text)?;

        let result = {
            let mut broker = SlotBroker {
                host: &mut host,
                session: &session,
                store: &store,
            };
            wasm.invoke("render_slot", &render_input, None, &mut broker)
                .map_err(text)?
        };

        if let Err(err) = self
            .ui_host
            .apply_tree_json(&mount_id, result.output.as_bytes())
        {
            let _ = self.ui_host.unmount_slot(&mount_id);
            return Err(text(err));
        }

        let tree = self
            .ui_host
            .slot(&mount_id)
            .and_then(|slot| slot.active_tree())
            .cloned()
            .ok_or_else(|| "mounted slot produced no UiTree".to_string())?;

        self.slots.insert(
            mount_id.clone(),
            SlotEntry {
                package_id: manifest.package_id,
                host,
                session,
                wasm,
                store,
                request_seq: seq,
            },
        );

        Ok(json!({
            "mount_id": mount_id,
            "generation": generation,
            "contribution_id": contribution.id,
            "title": contribution.title,
            "tree": tree,
        }))
    }

    fn dispatch_event(&mut self, mount_id: &str, envelope_json: &str) -> Result<Value, String> {
        let envelope = UiEventEnvelope::from_json(envelope_json).map_err(text)?;
        if envelope.mount_id != mount_id {
            return Err(format!(
                "envelope mount_id `{}` does not match `{mount_id}`",
                envelope.mount_id
            ));
        }
        self.ui_host.verify_inbound_event(&envelope).map_err(text)?;

        let result = {
            let entry = self
                .slots
                .get_mut(mount_id)
                .ok_or_else(|| format!("slot `{mount_id}` is not mounted"))?;
            entry.request_seq += 1;
            let api_version = entry.host.session_api(&entry.session).map_err(text)?;
            let permit = entry
                .host
                .authorize_ui_slot(
                    &entry.session,
                    api_version,
                    format!("slot-evt-{}", entry.request_seq),
                )
                .map_err(text)?;
            entry.host.dispatch(permit, |_| Ok(())).map_err(text)?;

            let mut broker = SlotBroker {
                host: &mut entry.host,
                session: &entry.session,
                store: &entry.store,
            };
            entry
                .wasm
                .invoke(
                    "handle_slot_event",
                    envelope_json.as_bytes(),
                    None,
                    &mut broker,
                )
                .map_err(text)?
        };

        let budgets = self.ui_host.budgets();
        if UiTree::from_slice_with_budgets(result.output.as_bytes(), budgets).is_ok() {
            self.ui_host
                .apply_tree_json(mount_id, result.output.as_bytes())
                .map_err(text)?;
        } else if UiPatch::from_slice_with_budgets(result.output.as_bytes(), budgets).is_ok() {
            self.ui_host
                .apply_patch_json(result.output.as_bytes())
                .map_err(text)?;
        } else {
            return Err("plugin event handler returned neither valid UiTree nor UiPatch".into());
        }

        let slot = self
            .ui_host
            .slot(mount_id)
            .ok_or_else(|| format!("slot `{mount_id}` disappeared after update"))?;
        let tree = slot
            .active_tree()
            .cloned()
            .ok_or_else(|| "slot has no active UiTree".to_string())?;
        Ok(json!({
            "mount_id": mount_id,
            "generation": slot.generation(),
            "tree": tree,
        }))
    }

    fn unmount(&mut self, mount_id: &str) -> Result<Value, String> {
        self.slots.remove(mount_id);
        let _ = self.ui_host.unmount_slot(mount_id);
        Ok(json!({ "unmounted": mount_id }))
    }

    fn unmount_package(&mut self, package_id: &str) {
        let doomed: Vec<String> = self
            .slots
            .iter()
            .filter(|(_, entry)| entry.package_id == package_id)
            .map(|(mount_id, _)| mount_id.clone())
            .collect();
        for mount_id in doomed {
            self.slots.remove(&mount_id);
            let _ = self.ui_host.unmount_slot(&mount_id);
        }
    }
}

struct SlotBroker<'a> {
    host: &'a mut Host,
    session: &'a Session,
    store: &'a VaultStore,
}

impl CapabilityBroker for SlotBroker<'_> {
    fn call(&mut self, request_bytes: &[u8]) -> Result<Vec<u8>, RuntimeError> {
        if let Ok(req) = VocabRequest::from_slice(request_bytes) {
            let permit = match self.host.authorize_vocab(self.session, &req) {
                Ok(permit) => permit,
                Err(error) => {
                    let reply = VocabResponse {
                        request_id: req.request_id,
                        result: VocabResult::Error { error },
                    };
                    return serde_json::to_vec(&reply)
                        .map_err(|_| RuntimeError::new("invalid_output"));
                }
            };
            let now = now_ms();
            let mut bridge = core_facade::new_vocab_bridge(self.store, now, day_start_ms(now));
            let response = self
                .host
                .dispatch(permit, |_ctx| Ok(bridge.respond(&req)))
                .map_err(|_| RuntimeError::new("broker_rejected"))?;
            return serde_json::to_vec(&response).map_err(|_| RuntimeError::new("invalid_output"));
        }

        if let Ok(req) = NotesRequest::from_slice(request_bytes) {
            let permit = self
                .host
                .authorize_notes(self.session, &req)
                .map_err(|_| RuntimeError::new("permission_denied"))?;
            let bridge = core_facade::new_notes_bridge(self.store);
            let response = self
                .host
                .dispatch(permit, |_ctx| Ok(bridge.respond(&req)))
                .map_err(|_| RuntimeError::new("broker_rejected"))?;
            return serde_json::to_vec(&response).map_err(|_| RuntimeError::new("invalid_output"));
        }

        if let Ok(req) = LibraryRequest::from_slice(request_bytes) {
            let permit = self
                .host
                .authorize_library(self.session, &req)
                .map_err(|_| RuntimeError::new("permission_denied"))?;
            let mut bridge = core_facade::new_library_bridge(self.store);
            let response = self
                .host
                .dispatch(permit, |ctx| Ok(bridge.respond(&req, ctx)))
                .map_err(|_| RuntimeError::new("broker_rejected"))?;
            return serde_json::to_vec(&response).map_err(|_| RuntimeError::new("invalid_output"));
        }

        Err(RuntimeError::new("unsupported_protocol"))
    }
}

fn spawn_actor() -> Result<Sender<SlotCommand>, String> {
    let (tx, rx) = mpsc::channel::<SlotCommand>();
    thread::Builder::new()
        .name("qusor-plugin-slot".to_owned())
        .spawn(move || {
            let mut manager = SlotManager::new();
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    SlotCommand::Mount {
                        contribution_id,
                        store,
                        reply,
                    } => {
                        let _ = reply.send(manager.mount(&contribution_id, store));
                    }
                    SlotCommand::DispatchEvent {
                        mount_id,
                        envelope_json,
                        reply,
                    } => {
                        let _ = reply.send(manager.dispatch_event(&mount_id, &envelope_json));
                    }
                    SlotCommand::Unmount { mount_id, reply } => {
                        let _ = reply.send(manager.unmount(&mount_id));
                    }
                    SlotCommand::UnmountPackage { package_id, reply } => {
                        manager.unmount_package(&package_id);
                        let _ = reply.send(());
                    }
                }
            }
        })
        .map_err(text)?;
    Ok(tx)
}

fn send_command(build: impl FnOnce() -> SlotCommand) -> Result<(), String> {
    let mut guard = SLOT_ACTOR
        .lock()
        .map_err(|_| "plugin slot actor lock poisoned".to_string())?;
    let sender = match guard.as_ref() {
        Some(existing) => existing.clone(),
        None => {
            let fresh = spawn_actor()?;
            *guard = Some(fresh.clone());
            fresh
        }
    };
    let cmd = build();
    if sender.send(cmd).is_err() {
        let fresh = spawn_actor()?;
        *guard = Some(fresh);
        return Err("plugin slot actor restarted; please retry".into());
    }
    Ok(())
}

pub(crate) fn unmount_for_package(package_id: &str) {
    let Ok(guard) = SLOT_ACTOR.lock() else {
        return;
    };
    let Some(sender) = guard.as_ref() else {
        return;
    };
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    if sender
        .send(SlotCommand::UnmountPackage {
            package_id: package_id.to_owned(),
            reply: reply_tx,
        })
        .is_ok()
    {
        let _ = reply_rx.recv();
    }
}

fn runtime_budgets() -> RuntimeBudgets {
    #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
    {
        RuntimeBudgets::mobile_standard()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    {
        RuntimeBudgets::desktop_standard()
    }
}

fn mount_slot_inner(sys: &System, contribution_id: &str) -> Result<Value, String> {
    runtime::ensure_contribution_active(sys, contribution_id)?;
    let store = runtime::core_handle(sys)?;
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    send_command(|| SlotCommand::Mount {
        contribution_id: contribution_id.to_owned(),
        store,
        reply: reply_tx,
    })?;
    reply_rx
        .recv()
        .map_err(|_| "plugin slot actor closed unexpectedly".to_string())?
}

fn dispatch_event_inner(mount_id: &str, envelope_json: &str) -> Result<Value, String> {
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    send_command(|| SlotCommand::DispatchEvent {
        mount_id: mount_id.to_owned(),
        envelope_json: envelope_json.to_owned(),
        reply: reply_tx,
    })?;
    reply_rx
        .recv()
        .map_err(|_| "plugin slot actor closed unexpectedly".to_string())?
}

fn unmount_slot_inner(mount_id: &str) -> Result<Value, String> {
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    send_command(|| SlotCommand::Unmount {
        mount_id: mount_id.to_owned(),
        reply: reply_tx,
    })?;
    reply_rx
        .recv()
        .map_err(|_| "plugin slot actor closed unexpectedly".to_string())?
}

/// Generic host module exposing `ui_slot` mounting and event dispatch to TSX.
#[module]
pub struct PluginSlotHost;

#[module]
impl PluginSlotHost {
    /// Mount any active verified package contribution of kind `ui_slot`.
    pub fn mount_slot(sys: System, contribution_id: String) -> String {
        answer(mount_slot_inner(&sys, &contribution_id))
    }

    /// Forward a generation-fenced `UiEventEnvelope` JSON payload to a mounted slot.
    pub fn dispatch_event(_sys: System, mount_id: String, envelope_json: String) -> String {
        answer(dispatch_event_inner(&mount_id, &envelope_json))
    }

    /// Unmount a previously mounted UI slot and release its Wasm session.
    pub fn unmount_slot(_sys: System, mount_id: String) -> String {
        answer(unmount_slot_inner(&mount_id))
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn day_start_ms(now_ms: i64) -> i64 {
    const DAY: i64 = 86_400_000;
    now_ms.saturating_sub(now_ms.rem_euclid(DAY))
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn answer(result: Result<Value, String>) -> String {
    match result {
        Ok(value) => json!({ "ok": true, "value": value }).to_string(),
        Err(error) => json!({ "ok": false, "error": error }).to_string(),
    }
}
