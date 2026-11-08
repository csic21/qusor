//! Trusted native capture action. Proposals and approvals are never guest authority.
use crate::{
    contract::{CaptureDraft, CaptureRequest, CaptureResponse},
    core::{Core, Provenance, Receipt},
};
use extension_api::{Capability, CapabilityRequest, HOST_API, Resource};
use extension_host::{Grant, Host, InstallationId, Session};
use qusor_package_probe::store::Store;
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static BOOT: AtomicU64 = AtomicU64::new(1);
struct Gate {
    _packages: Store,
    host: Host,
    id: InstallationId,
    session: Option<Session>,
    epoch: u64,
    core: Core,
}
#[derive(Clone)]
pub struct CaptureHost {
    gate: Arc<Mutex<Gate>>,
    wasm: Vec<u8>,
    origin: Provenance,
    vault: String,
    boot: u64,
}
/// Opaque native reviewed candidate; deliberately not Deserialize.
pub struct Proposal {
    draft: CaptureDraft,
    hash: String,
    epoch: u64,
    boot: u64,
}
impl Proposal {
    pub fn draft(&self) -> &CaptureDraft {
        &self.draft
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
}
/// Native user intent; cannot be constructed from a guest request or restored.
pub struct Approval {
    proposal: Proposal,
    operation: String,
    note: String,
}
impl CaptureHost {
    pub fn open(packages: &Path, core: &Path, vault: &str, package_id: &str) -> Result<Self> {
        let store = Store::open(packages)?;
        let (receipt, package) = store.frozen_package_for_action(package_id)?;
        let origin = Provenance {
            package_id: package.manifest().package_id.clone(),
            publisher_id: package.manifest().publisher_id.clone(),
            package_digest: package.digest().to_owned(),
            installation_id: receipt.installation_id,
        };
        let mut host = Host::new(vault, [Capability::CapturePropose])?;
        let id = host.register(package.manifest().clone(), package.digest())?;
        Ok(Self {
            gate: Arc::new(Mutex::new(Gate {
                _packages: store,
                host,
                id,
                session: None,
                epoch: 0,
                core: Core::open(core)?,
            })),
            wasm: package.module_bytes().to_vec(),
            origin,
            vault: vault.to_owned(),
            boot: BOOT.fetch_add(1, Ordering::Relaxed),
        })
    }
    /// Historical publisher/package identity from verified installed bytes.
    pub fn provenance(&self) -> &Provenance {
        &self.origin
    }
    /// Explicit trusted action, never inferred from the manifest.
    pub fn grant(&self) -> Result<()> {
        let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
        let id = g.id;
        g.host.replace_grants(
            id,
            vec![Grant {
                capability: Capability::CapturePropose,
                resource: Resource::Vault {
                    vault_id: self.vault.clone(),
                },
            }],
        )?;
        g.host.enable(id)?;
        g.session = Some(g.host.open_session(id)?);
        g.epoch += 1;
        Ok(())
    }
    pub fn revoke(&self) -> Result<()> {
        let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
        let id = g.id;
        g.host.replace_grants(id, vec![])?;
        g.session = None;
        g.epoch += 1;
        Ok(())
    }
    fn request(&self, id: &str) -> CapabilityRequest {
        CapabilityRequest {
            api_version: HOST_API,
            request_id: id.to_owned(),
            capability: Capability::CapturePropose,
            resource: Resource::Vault {
                vault_id: self.vault.clone(),
            },
        }
    }
    pub fn propose(&self) -> Result<Proposal> {
        let epoch = self.gate.lock().map_err(|_| "gate poisoned")?.epoch;
        let mut drafts = Vec::new();
        let mut contract_error = None;
        let execution = qusor_wasm_probe::managed_runtime::run_managed_raw(&self.wasm, |bytes| {
            let request = CaptureRequest::from_slice(bytes).map_err(|error| {
                contract_error = Some(error);
                qusor_wasm_probe::ProbeError {
                    code: "invalid_capture_request",
                }
            })?;
            let mut g = self.gate.lock().map_err(|_| qusor_wasm_probe::ProbeError {
                code: "gate_poisoned",
            })?;
            let authorized = (|| -> Result<()> {
                if g.epoch != epoch {
                    return Err("grant changed".into());
                }
                let s = g.session.as_ref().ok_or("capture not granted")?;
                let permit = g.host.authorize(s, self.request(&request.request_id))?;
                g.host.dispatch(permit, |_| Ok(()))?;
                Ok(())
            })();
            authorized.map_err(|_| qusor_wasm_probe::ProbeError {
                code: "capture_denied",
            })?;
            let hash = request
                .draft
                .draft_hash()
                .map_err(|_| qusor_wasm_probe::ProbeError {
                    code: "invalid_capture_request",
                })?;
            let response = serde_json::to_vec(&CaptureResponse {
                capture_protocol: 1,
                request_id: request.request_id,
                candidate_id: format!("candidate-{}", drafts.len() + 1),
                draft_hash: hash,
            })
            .map_err(|_| qusor_wasm_probe::ProbeError {
                code: "response_error",
            })?;
            drafts.push(request.draft);
            Ok(response)
        });
        if let Some(error) = contract_error {
            return Err(error.into());
        }
        execution?;
        if drafts.len() != 1 {
            return Err("exactly one capture proposal required".into());
        }
        let draft = drafts.pop().unwrap();
        let hash = draft.draft_hash()?;
        let g = self.gate.lock().map_err(|_| "gate poisoned")?;
        if g.epoch != epoch || g.session.is_none() {
            return Err("grant changed".into());
        }
        Ok(Proposal {
            draft,
            hash,
            epoch,
            boot: self.boot,
        })
    }
    pub fn approve(
        &self,
        proposal: Proposal,
        expected_hash: &str,
        operation: &str,
        user_note: &str,
    ) -> Result<Approval> {
        if proposal.hash != expected_hash || proposal.boot != self.boot {
            return Err("reviewed draft or host changed".into());
        }
        if operation.is_empty() {
            return Err("operation required".into());
        }
        Ok(Approval {
            proposal,
            operation: operation.to_owned(),
            note: user_note.to_owned(),
        })
    }
    pub fn accept(&self, approval: Approval) -> Result<Receipt> {
        self.accept_with_crash(approval, None)
    }
    pub fn accept_with_crash(&self, approval: Approval, crash: Option<&str>) -> Result<Receipt> {
        self.accept_inner(approval, crash, || {})
    }
    /// Trusted observer for deterministic gate ordering qualification; no guest hook.
    pub fn accept_observed(&self, approval: Approval, observer: impl FnOnce()) -> Result<Receipt> {
        self.accept_inner(approval, None, observer)
    }
    fn accept_inner(
        &self,
        approval: Approval,
        crash: Option<&str>,
        observer: impl FnOnce(),
    ) -> Result<Receipt> {
        let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
        if approval.proposal.boot != self.boot
            || approval.proposal.epoch != g.epoch
            || approval.proposal.hash != approval.proposal.draft.draft_hash()?
        {
            return Err("stale approval".into());
        }
        let session = g.session.as_ref().ok_or("capture not granted")?;
        let permit = g.host.authorize(session, self.request("native-accept"))?;
        // This gate serializes final authoritative dispatch, native revoke, and Core commit.
        g.host.dispatch(permit, |_| Ok(()))?;
        observer();
        g.core
            .commit(
                &approval.operation,
                &self.vault,
                &approval.proposal.draft,
                &self.origin,
                &approval.note,
                crash,
            )
            .map_err(Into::into)
    }
    pub fn receipt(&self, op: &str) -> Result<Option<Receipt>> {
        self.gate
            .lock()
            .map_err(|_| "gate poisoned")?
            .core
            .receipt(op)
            .map_err(Into::into)
    }
}
