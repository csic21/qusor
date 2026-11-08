//! Trusted native capture action. PdfProposals and approvals are never guest authority.
use crate::{
    core::{Core, PdfReceipt, Provenance},
    pdf_compound::{PdfCompoundCaptureRequest, PdfCompoundDraft},
    pdf_contract::{PdfCaptureRequest, PdfDraft},
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
pub struct PdfCaptureHost {
    gate: Arc<Mutex<Gate>>,
    wasm: Vec<u8>,
    origin: Provenance,
    vault: String,
    boot: u64,
    document_id: i64,
    selection: PdfDraft,
    compound: Option<PdfCompoundDraft>,
}
/// Opaque native reviewed candidate; deliberately not Deserialize.
pub struct PdfProposal {
    draft: PdfDraft,
    compound: Option<PdfCompoundDraft>,
    hash: String,
    epoch: u64,
    boot: u64,
}
impl PdfProposal {
    /// A compound action must be reviewed in full through `compound()`.
    pub fn draft(&self) -> Result<&PdfDraft> {
        if self.compound.is_some() {
            return Err("compound PDF proposal requires reviewing all segments".into());
        }
        Ok(&self.draft)
    }
    pub fn compound(&self) -> Option<&PdfCompoundDraft> {
        self.compound.as_ref()
    }
    fn whole_hash(&self) -> Result<String> {
        match &self.compound {
            Some(d) => Ok(d.draft_hash()?),
            None => Ok(self.draft.draft_hash()?),
        }
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
}
/// Native user intent; cannot be constructed from a guest request or restored.
pub struct PdfApproval {
    proposal: PdfProposal,
    operation: String,
    note: String,
}
impl PdfCaptureHost {
    pub fn open(
        packages: &Path,
        core_path: &Path,
        vault: &str,
        package_id: &str,
        document_id: i64,
        selection: PdfDraft,
    ) -> Result<Self> {
        let core = Core::open(core_path)?;
        Self::validate_selection(&core, vault, document_id, &selection)?;
        let store = Store::open(packages)?;
        let (receipt, package) = store.frozen_package_for_action(package_id)?;
        let origin = Provenance {
            package_id: package.manifest().package_id.clone(),
            publisher_id: package.manifest().publisher_id.clone(),
            package_digest: package.digest().to_owned(),
            installation_id: receipt.installation_id,
        };
        let mut host = Host::new(
            vault,
            [Capability::SelectionRead, Capability::CapturePropose],
        )?;
        let id = host.register(package.manifest().clone(), package.digest())?;
        Ok(Self {
            gate: Arc::new(Mutex::new(Gate {
                _packages: store,
                host,
                id,
                session: None,
                epoch: 0,
                core,
            })),
            wasm: package.module_bytes().to_vec(),
            origin,
            vault: vault.to_owned(),
            boot: BOOT.fetch_add(1, Ordering::Relaxed),
            document_id,
            selection,
            compound: None,
        })
    }
    pub fn open_compound(
        packages: &Path,
        core_path: &Path,
        vault: &str,
        package_id: &str,
        document_id: i64,
        selection: PdfCompoundDraft,
    ) -> Result<Self> {
        selection.validate()?;
        PdfCompoundCaptureRequest {
            capture_protocol: 2,
            request_id: "pdf-proposal".into(),
            draft: selection.clone(),
        }
        .validate()?;
        let primary = selection
            .segments
            .first()
            .ok_or("compound requires segments")?
            .clone();
        let mut host = Self::open(packages, core_path, vault, package_id, document_id, primary)?;
        {
            let gate = host.gate.lock().map_err(|_| "gate poisoned")?;
            selection.resolve(&gate.core.query_document(document_id)?.snapshot)?;
        }
        host.compound = Some(selection);
        Ok(host)
    }
    fn validate_current(&self, core: &Core) -> Result<()> {
        let document = core.query_document(self.document_id)?;
        if document.vault != self.vault {
            return Err("PDF document belongs to a different vault".into());
        }
        if let Some(draft) = &self.compound {
            draft.resolve(&document.snapshot)?;
        } else {
            document.snapshot.resolve(&self.selection)?;
        }
        Ok(())
    }
    fn selection_bytes(&self) -> Result<Vec<u8>> {
        match &self.compound {
            Some(d) => Ok(d.canonical_bytes()?),
            None => Ok(self.selection.canonical_bytes()?),
        }
    }
    fn validate_selection(
        core: &Core,
        vault: &str,
        document_id: i64,
        draft: &PdfDraft,
    ) -> Result<()> {
        let document = core.query_document(document_id)?;
        if document.vault != vault {
            return Err("PDF document belongs to a different vault".into());
        }
        document.snapshot.resolve(draft)?;
        Ok(())
    }
    /// Historical publisher/package identity from verified installed bytes.
    pub fn provenance(&self) -> &Provenance {
        &self.origin
    }
    pub fn document_title(&self) -> Result<String> {
        Ok(self
            .gate
            .lock()
            .map_err(|_| "PDF gate poisoned")?
            .core
            .query_document(self.document_id)?
            .title)
    }
    /// Explicit trusted action, never inferred from the manifest.
    pub fn grant(&self) -> Result<()> {
        let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
        let id = g.id;
        g.host.replace_grants(
            id,
            vec![
                Grant {
                    capability: Capability::SelectionRead,
                    resource: Resource::Selection {
                        vault_id: self.vault.clone(),
                        selection_id: "active-pdf".to_owned(),
                    },
                },
                Grant {
                    capability: Capability::CapturePropose,
                    resource: Resource::Vault {
                        vault_id: self.vault.clone(),
                    },
                },
            ],
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
    pub fn propose(&self) -> Result<PdfProposal> {
        // Disk integrity and immutable source validation belong to the native
        // preparation phase. Keep them outside the bounded guest execution,
        // while all broker calls still recheck live grants under this gate.
        let (epoch, response) = {
            let gate = self.gate.lock().map_err(|_| "gate poisoned")?;
            self.validate_current(&gate.core)?;
            let response = self.selection_bytes()?;
            if response.len() > 4096 {
                return Err("PDF selection response budget exceeded".into());
            }
            (gate.epoch, response)
        };
        let mut did_read = false;
        let mut drafts = Vec::new();
        let mut detailed_error = None;
        let execution = qusor_wasm_probe::managed_runtime::run_managed_raw(&self.wasm, |bytes| {
            let result = (|| -> Result<Vec<u8>> {
                let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
                if g.epoch != epoch {
                    return Err("grant changed".into());
                }
                let session = g.session.as_ref().ok_or("PDF action not granted")?;
                if let Ok(request) = CapabilityRequest::from_slice(bytes) {
                    if did_read
                        || request.capability != Capability::SelectionRead
                        || request.resource
                            != (Resource::Selection {
                                vault_id: self.vault.clone(),
                                selection_id: "active-pdf".into(),
                            })
                    {
                        return Err("only active PDF selection read is available".into());
                    }
                    let permit = g.host.authorize(session, request)?;
                    g.host.dispatch(permit, |_| Ok(()))?;
                    did_read = true;
                    return Ok(response.clone());
                }
                if !did_read || !drafts.is_empty() {
                    return Err("PDF action requires a read before one proposal".into());
                }
                let (request_id, primary, compound, hash, protocol) = if let Some(trusted) =
                    &self.compound
                {
                    let request = PdfCompoundCaptureRequest::from_slice(bytes)?;
                    if &request.draft != trusted {
                        return Err(
                            "compound PDF proposal must match the whole current selection".into(),
                        );
                    }
                    let hash = request.draft.draft_hash()?;
                    (
                        request.request_id,
                        request.draft.segments[0].clone(),
                        Some(request.draft),
                        hash,
                        2,
                    )
                } else {
                    let request = PdfCaptureRequest::from_slice(bytes)?;
                    if request.draft != self.selection {
                        return Err("PDF proposal must match the current selection".into());
                    }
                    let hash = request.draft.draft_hash()?;
                    (request.request_id, request.draft, None, hash, 1)
                };
                let permit = g.host.authorize(session, self.request(&request_id))?;
                g.host.dispatch(permit, |_| Ok(()))?;
                drafts.push((primary, compound, hash.clone()));
                Ok(serde_json::to_vec(
                    &serde_json::json!({"capture_protocol":protocol,"request_id":request_id,"candidate_id":"pdf-candidate","draft_hash":hash}),
                )?)
            })();
            result.map_err(|error| {
                detailed_error = Some(error.to_string());
                qusor_wasm_probe::ProbeError {
                    code: "pdf_capture_denied",
                }
            })
        });
        if let Some(error) = detailed_error {
            return Err(error.into());
        }
        execution?;
        if !did_read || drafts.len() != 1 {
            return Err("PDF action requires one selection read and one proposal".into());
        }
        let (draft, compound, hash) = drafts.pop().unwrap();
        let g = self.gate.lock().map_err(|_| "gate poisoned")?;
        if g.epoch != epoch || g.session.is_none() {
            return Err("grant changed".into());
        }
        Ok(PdfProposal {
            draft,
            compound,
            hash,
            epoch,
            boot: self.boot,
        })
    }
    pub fn approve(
        &self,
        proposal: PdfProposal,
        expected_hash: &str,
        operation: &str,
        user_note: &str,
    ) -> Result<PdfApproval> {
        if proposal.hash != expected_hash || proposal.boot != self.boot {
            return Err("reviewed draft or host changed".into());
        }
        if operation.is_empty() {
            return Err("operation required".into());
        }
        Ok(PdfApproval {
            proposal,
            operation: operation.to_owned(),
            note: user_note.to_owned(),
        })
    }
    pub fn accept(&self, approval: PdfApproval) -> Result<PdfReceipt> {
        self.accept_with_crash(approval, None)
    }
    pub fn accept_with_crash(
        &self,
        approval: PdfApproval,
        crash: Option<&str>,
    ) -> Result<PdfReceipt> {
        self.accept_inner(approval, crash, || {})
    }
    /// Trusted observer for deterministic gate ordering qualification; no guest hook.
    pub fn accept_observed(
        &self,
        approval: PdfApproval,
        observer: impl FnOnce(),
    ) -> Result<PdfReceipt> {
        self.accept_inner(approval, None, observer)
    }
    fn accept_inner(
        &self,
        approval: PdfApproval,
        crash: Option<&str>,
        observer: impl FnOnce(),
    ) -> Result<PdfReceipt> {
        let mut g = self.gate.lock().map_err(|_| "gate poisoned")?;
        if approval.proposal.boot != self.boot
            || approval.proposal.epoch != g.epoch
            || approval.proposal.hash != approval.proposal.whole_hash()?
        {
            return Err("stale approval".into());
        }
        let session = g.session.as_ref().ok_or("capture not granted")?;
        let permit = g.host.authorize(session, self.request("native-accept"))?;
        // This gate serializes final authoritative dispatch, native revoke, and Core commit.
        g.host.dispatch(permit, |_| Ok(()))?;
        observer();
        if let Some(compound) = &approval.proposal.compound {
            g.core
                .capture_pdf_compound(
                    &approval.operation,
                    self.document_id,
                    compound,
                    &self.origin,
                    &approval.note,
                    crash,
                )
                .map_err(Into::into)
        } else {
            g.core
                .capture_pdf(
                    &approval.operation,
                    self.document_id,
                    &approval.proposal.draft,
                    &self.origin,
                    &approval.note,
                    crash,
                )
                .map_err(Into::into)
        }
    }
    pub fn receipt(&self, op: &str) -> Result<Option<PdfReceipt>> {
        self.gate
            .lock()
            .map_err(|_| "gate poisoned")?
            .core
            .pdf_receipt(op)
            .map_err(Into::into)
    }
}
