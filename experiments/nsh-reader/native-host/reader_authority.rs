//! Qualification composition of the production journal and one actual policy Host.
//! These explicit test grants are not a product trust/permission UI.
use core_api::knowledge::PdfDocumentRecord;
use extension_api::surface::*;
use extension_api::{Capability, CapabilityRequest, HOST_API_V0_2, Resource, SUPPORTED_HOST_APIS};
use extension_host::{
    Grant,
    durable::{DurableHost, DurableSession, DurableSurface, VersionedRequest},
    surface::*,
};
use extension_package::package::VerifiedPackage;
use extension_store::{
    GrantScope, GrantSet, GrantValidator, PersistentGrant, PrivateStateExecutor, Receipt,
    SavedAuthorization, Store, StoreError, StoreServices,
};
use runtime_wasm::{ModuleExecutor, WasmiRuntime};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, String>;
fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(text)
}
struct Services(Arc<Mutex<DurableHost>>);
impl GrantValidator for Services {
    fn validate(
        &self,
        package: &VerifiedPackage,
        scope: &GrantScope,
    ) -> extension_store::Result<()> {
        let api = package
            .manifest()
            .negotiate_api(&SUPPORTED_HOST_APIS)
            .ok_or_else(|| StoreError::invalid("no compatible API"))?;
        self.0
            .lock()
            .map_err(|_| StoreError::external("policy lock unavailable"))?
            .validate_activation_intent(
                package.manifest(),
                package.digest(),
                &[Grant {
                    capability: scope.capability,
                    resource: scope.resource(),
                }],
                api,
            )
            .map_err(StoreError::from)
    }
    fn validate_set(
        &self,
        package: &VerifiedPackage,
        scope: &GrantSet,
    ) -> extension_store::Result<()> {
        self.0
            .lock()
            .map_err(|_| StoreError::external("policy lock unavailable"))?
            .validate_activation_intent(
                package.manifest(),
                package.digest(),
                &scope
                    .grants
                    .iter()
                    .map(|g| Grant {
                        capability: g.capability,
                        resource: g.resource.clone(),
                    })
                    .collect::<Vec<_>>(),
                scope.api_version,
            )
            .map_err(StoreError::from)
    }
}
impl PrivateStateExecutor for Services {
    fn preflight(&self, module: &[u8], schema: u32, state: &[u8]) -> extension_store::Result<()> {
        WasmiRuntime
            .preflight(module, schema, state)
            .map_err(|e| StoreError::external(e.to_string()))
    }
    fn migrate(
        &self,
        module: &[u8],
        from: u32,
        to: u32,
        state: &[u8],
    ) -> extension_store::Result<Vec<u8>> {
        WasmiRuntime
            .migrate(module, from, to, state)
            .map_err(|e| StoreError::external(e.to_string()))
    }
}
pub(crate) struct ReaderAuthority {
    journal: Store,
    host: Arc<Mutex<DurableHost>>,
    package_id: String,
    active: Option<DurableSession>,
}
impl ReaderAuthority {
    pub fn install(
        path: &Path,
        package: &VerifiedPackage,
        nonce: [u8; 16],
    ) -> Result<(Self, Receipt, VerifiedPackage)> {
        let mut host = DurableHost::new(
            "surface-vault",
            [
                Capability::NoteDraftEdit,
                Capability::ReaderSurface,
                Capability::SelectionRead,
                Capability::CapturePropose,
            ],
        )
        .map_err(text)?;
        host.configure_surface_identity(nonce).map_err(text)?;
        host.configure_component_identity(nonce).map_err(text)?;
        let host = Arc::new(Mutex::new(host));
        let services = Arc::new(Services(host.clone()));
        let mut journal = Store::open_with_services(
            path,
            &SUPPORTED_HOST_APIS,
            StoreServices {
                grant_validator: services.clone(),
                private_executor: services,
                ..StoreServices::default()
            },
        )
        .map_err(text)?;
        journal
            .install("fixture-reader-install", 0, package)
            .map_err(text)?;
        let (receipt, frozen) = journal
            .frozen_package_for_action(&package.manifest().package_id)
            .map_err(text)?;
        Ok((
            Self {
                journal,
                host,
                package_id: package.manifest().package_id.clone(),
                active: None,
            },
            receipt,
            frozen,
        ))
    }
    /// Trusted qualification/user-open action, not manifest-derived automatic grants.
    /// On restart restore the exact saved set; never renew expired or disabled intent.
    pub fn authorize_document(
        &mut self,
        document: &PdfDocumentRecord,
        operation: &str,
        allow_scope_change: bool,
    ) -> Result<DurableSession> {
        let current = self.journal.current(&self.package_id).map_err(text)?;
        let time = now()?;
        let resource = Resource::Document {
            vault_id: document.vault.clone(),
            document_id: document.receipt.document_id.to_string(),
        };
        let version = document.receipt.extraction_revision.clone();
        let saved = if current.revision == 1 {
            None
        } else {
            Some(self.journal.saved_authorization(&current).map_err(text)?)
        };
        let (expires, capabilities, already_bound) = match saved {
            None => (
                time.checked_add(24 * 60 * 60)
                    .ok_or("authorization clock overflow")?,
                vec![
                    Capability::ReaderSurface,
                    Capability::SelectionRead,
                    Capability::CapturePropose,
                ],
                false,
            ),
            Some(SavedAuthorization::V1(_)) => {
                return Err("legacy metadata permission does not authorize reading".into());
            }
            Some(SavedAuthorization::V2(record)) => {
                if record.scope.expires_at <= time {
                    return Err("reading authorization expired; explicit renewal required".into());
                }
                let bound = record.scope.grants.iter().any(|g| {
                    g.capability == Capability::ReaderSurface
                        && g.resource == resource
                        && g.resource_version.as_deref() == Some(version.as_str())
                });
                let capabilities = record
                    .scope
                    .grants
                    .iter()
                    .map(|g| g.capability)
                    .collect::<Vec<_>>();
                (record.scope.expires_at, capabilities, bound)
            }
        };
        if !capabilities.contains(&Capability::ReaderSurface) {
            return Err("reading capability is not authorized".into());
        }
        if current.revision != 1 && !already_bound && !allow_scope_change {
            return Err("saved reading permission belongs to another document or version".into());
        }
        if !already_bound {
            let mut grants = vec![PersistentGrant {
                capability: Capability::ReaderSurface,
                resource: resource.clone(),
                resource_version: Some(version.clone()),
            }];
            if capabilities.contains(&Capability::SelectionRead) {
                grants.push(PersistentGrant {
                    capability: Capability::SelectionRead,
                    resource,
                    resource_version: Some(version),
                });
            }
            if capabilities.contains(&Capability::CapturePropose) {
                grants.push(PersistentGrant {
                    capability: Capability::CapturePropose,
                    resource: Resource::Vault {
                        vault_id: document.vault.clone(),
                    },
                    resource_version: None,
                });
            }
            self.journal
                .grant_set(
                    operation,
                    current.revision,
                    &self.package_id,
                    GrantSet {
                        api_version: HOST_API_V0_2,
                        vault_id: document.vault.clone(),
                        expires_at: expires,
                        grants,
                    },
                )
                .map_err(text)?;
        }
        let session = self
            .host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .activate(&self.journal, &self.package_id, HOST_API_V0_2, time)
            .map_err(text)?;
        self.active = Some(session.clone());
        Ok(session)
    }
    #[cfg(test)]
    pub fn saved_authorization_for_test(&self) -> Result<SavedAuthorization> {
        let current = self.journal.current(&self.package_id).map_err(text)?;
        self.journal.saved_authorization(&current).map_err(text)
    }
    fn scoped<T>(
        &self,
        session: &DurableSession,
        action: impl FnOnce(&mut DurableSurface<'_>) -> std::result::Result<T, extension_api::ApiError>,
    ) -> Result<T> {
        self.host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .with_surface(&self.journal, session, now()?, None, action)
            .map_err(text)
    }
    fn native<T>(
        &self,
        action: impl FnOnce(&mut DurableSurface<'_>) -> std::result::Result<T, extension_api::ApiError>,
    ) -> Result<T> {
        self.scoped(
            self.active.as_ref().ok_or("reading session closed")?,
            action,
        )
    }
    /// Trusted native experiment composition only; never exported to guest/JS.
    /// Gate -> policy lock. Callers must not wait for workers inside this closure.
    pub(crate) fn with_component_authority<T>(
        &mut self,
        run: impl FnOnce(&mut Store, &mut DurableHost) -> Result<T>,
    ) -> Result<T> {
        let mut host = self.host.lock().map_err(|_| "policy lock unavailable")?;
        run(&mut self.journal, &mut host)
    }
    pub(crate) fn reader_receipt(&self) -> Result<Receipt> {
        self.journal.current(&self.package_id).map_err(text)
    }
    pub fn policy(
        &self,
        session: &DurableSession,
        request: CapabilityRequest,
        version: Option<String>,
    ) -> Result<()> {
        self.host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .dispatch(
                &self.journal,
                session,
                now()?,
                None,
                VersionedRequest {
                    request,
                    resource_version: version,
                },
                |_| Ok(()),
            )
            .map_err(text)
    }
    pub fn close_session(&mut self, session: &DurableSession) -> Result<()> {
        if self.active.is_none() {
            return Ok(());
        }
        self.host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .close(session)
            .map_err(text)?;
        self.active = None;
        Ok(())
    }
    pub fn disable(&mut self, operation: &str) -> Result<()> {
        let receipt = self.journal.current(&self.package_id).map_err(text)?;
        self.journal
            .stop(operation, receipt.revision, &self.package_id, "disable")
            .map_err(text)?;
        if let Some(session) = self.active.take() {
            let _ = self
                .host
                .lock()
                .map_err(|_| "policy lock unavailable")?
                .close(&session);
        }
        Ok(())
    }
    pub fn preflight_surface_request(&self, s: &DurableSession, r: &SurfaceRequest) -> Result<()> {
        self.scoped(s, |h| h.preflight_surface_request(r))
    }
    pub fn open_surface(
        &self,
        s: &DurableSession,
        r: &SurfaceRequest,
        d: TrustedSurfaceDocument,
    ) -> Result<SurfaceTransition> {
        self.scoped(s, |h| h.open_surface(r, d))
    }
    pub fn configure_surface(
        &self,
        s: &DurableSession,
        r: &SurfaceRequest,
    ) -> Result<SurfaceTransition> {
        self.scoped(s, |h| h.configure_surface(r))
    }
    pub fn close_surface(
        &self,
        s: &DurableSession,
        r: &SurfaceRequest,
    ) -> Result<SurfaceDescriptor> {
        self.scoped(s, |h| h.close_surface(r))
    }
    pub fn read_surface_selection(
        &self,
        s: &DurableSession,
        r: &SurfaceRequest,
    ) -> Result<SurfaceResponse> {
        self.scoped(s, |h| h.read_surface_selection(r))
    }
    pub fn surface_state(&self, s: &DurableSession, handle: &str) -> Result<SurfaceDescriptor> {
        self.scoped(s, |h| h.surface_state(handle))
    }
    pub fn surface_result_state(
        &self,
        s: &DurableSession,
        handle: &str,
    ) -> Result<SurfaceDescriptor> {
        self.scoped(s, |h| h.surface_result_state(handle))
    }
    pub fn native_frame_ready(
        &self,
        t: &NativeFrameTicket,
        f: FrameMetadata,
    ) -> Result<SurfaceDescriptor> {
        self.native(|h| h.native_frame_ready(t, f))
    }
    pub fn native_frame_failed(&self, t: &NativeFrameTicket) -> Result<SurfaceDescriptor> {
        self.native(|h| h.native_frame_failed(t))
    }
    pub fn native_input_ticket(&self, t: &NativeFrameTicket) -> Result<NativeInputTicket> {
        self.native(|h| h.native_input_ticket(t))
    }
    pub fn surface_begin_gesture(&self, t: &NativeInputTicket) -> Result<NativeGestureTicket> {
        self.native(|h| h.surface_begin_gesture(t))
    }
    pub fn publish_surface_selection(
        &self,
        t: &NativeGestureTicket,
        s: TrustedSurfaceSelection,
    ) -> Result<String> {
        self.native(|h| h.publish_surface_selection(t, s))
    }
    pub fn cancel_surface_gesture(&self, t: &NativeGestureTicket) -> Result<()> {
        self.native(|h| h.cancel_surface_gesture(t))
    }
    pub fn clear_surface_selection(&self, t: &NativeInputTicket) -> Result<()> {
        self.native(|h| h.clear_surface_selection(t))
    }
    pub fn pending_surface_cleanup(&self) -> Result<Vec<NativeCleanupTicket>> {
        Ok(self
            .host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .pending_surface_cleanup())
    }
    pub fn confirm_surface_closed(&self, t: &NativeCleanupTicket) -> Result<()> {
        self.host
            .lock()
            .map_err(|_| "policy lock unavailable")?
            .confirm_surface_closed(t)
            .map_err(text)
    }
}
