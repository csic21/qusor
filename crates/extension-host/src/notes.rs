//! Host policy for bounded note reads: bind the vault, require `notes.read`.
//!
//! This module decides only that one bounded request may proceed. The trusted
//! application adapter still dispatches the permit and runs the Core query;
//! the policy layer never reads knowledge, resolves anchors or executes guests.
use crate::{Host, Permit, Session};
use extension_api::{
    ApiError, Capability, CapabilityRequest, ErrorCode, HOST_API_V0_4, Resource,
    notes::{NOTES_PROTOCOL, NotesRequest},
};

impl Host {
    /// Authorize one bounded note read against this host's vault and the
    /// installation's exact `notes.read` grant.
    ///
    /// The guest supplies no vault: the Host binds its own. The returned permit
    /// must be dispatched by the trusted adapter and cannot be reused after a
    /// revocation, disable or session close.
    pub fn authorize_notes(
        &self,
        session: &Session,
        request: &NotesRequest,
    ) -> Result<Permit, ApiError> {
        if request.notes_protocol != NOTES_PROTOCOL || request.api_version != HOST_API_V0_4 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "notes require protocol1 and Host API0.4",
            ));
        }
        request.validate()?;
        self.authorize(
            session,
            CapabilityRequest {
                api_version: HOST_API_V0_4,
                request_id: request.request_id.clone(),
                capability: Capability::NotesRead,
                resource: Resource::Vault {
                    vault_id: self.vault_id().to_owned(),
                },
            },
        )
    }
}
