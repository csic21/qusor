//! Parent-only bridge for the managed-process experiment. Never serialized to a worker.
use super::*;

impl Store {
    pub(crate) fn managed_module(
        &mut self,
        token: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<(Receipt, Vec<u8>)> {
        self.check_session(token, library, document, version)?;
        let id = self
            .sessions
            .get(token)
            .ok_or("unknown or stale session")?
            .installation_id;
        let item = self
            .status()?
            .into_iter()
            .find(|item| item.installation_id == id)
            .ok_or("installation removed")?;
        let package = self.current_package(&item)?;
        if package.module_bytes().len() > qusor_wasm_probe::MAX_WASM_BYTES {
            return Err("package_size_limit".into());
        }
        Ok((item, package.module_bytes().to_vec()))
    }

    /// Recheck pure policy only. Initial activation already validated the immutable
    /// private pair; any legal mutation advances generation and invalidates tokens.
    /// No guest Wasm or provider I/O executes in the parent's per-message gate.
    pub(crate) fn managed_validate(
        &mut self,
        token: &str,
        run_id: &str,
        library: &str,
        document: &str,
        version: &str,
    ) -> Result<()> {
        let (_, current) =
            self.checked_session_grant_for_worker(token, library, document, version, Some(run_id))?;
        self.dispatch_session_check(token, &current)
    }

    pub(crate) fn managed_call(
        &mut self,
        token: &str,
        run_id: &str,
        library: &str,
        document: &str,
        version: &str,
        request: CapabilityRequest,
    ) -> Result<Vec<u8>> {
        self.managed_validate(token, run_id, library, document, version)?;
        let live = self
            .sessions
            .get_mut(token)
            .ok_or("unknown or stale session")?;
        let permit = live.host.authorize(&live.session, request)?;
        live.host.dispatch(permit, |context| {
            if context.request.capability != Capability::DocumentMetadataRead
                || context.request.resource != live.grant.scope.resource()
            {
                return Err(extension_api::ApiError::new(
                    extension_api::ErrorCode::PermissionDenied,
                    "managed service scope mismatch",
                ));
            }
            Ok(())
        })?;
        Ok(serde_json::to_vec(&serde_json::json!({
            "synthetic": true, "library": library, "document": document,
            "version": version, "title": "Synthetic metadata fixture",
        }))?)
    }

    pub(crate) fn managed_invalidate(&mut self, token: &str) {
        self.sessions.remove(token);
    }
}
