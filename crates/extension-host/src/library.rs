//! Host policy for bounded PDF Library metadata reads: bind the vault and
//! require the installation's exact `library.read` grant.
//!
//! This module authorizes one request only. The trusted application adapter
//! still dispatches the permit and runs the Core query; policy never reads
//! knowledge, exposes original bytes or executes guests.
use crate::{Host, Permit, Session};
use extension_api::{
    ApiError, Capability, CapabilityRequest, ErrorCode, HOST_API_V0_6, Resource,
    library::{LIBRARY_PROTOCOL, LibraryRequest},
};

impl Host {
    /// Authorize one bounded Library metadata request against this host's vault.
    ///
    /// The guest supplies no vault: the Host binds its own. The returned permit
    /// cannot be reused after revocation, disable or session close.
    pub fn authorize_library(
        &self,
        session: &Session,
        request: &LibraryRequest,
    ) -> Result<Permit, ApiError> {
        if request.library_protocol != LIBRARY_PROTOCOL
            || !extension_api::is_supported_api(request.api_version)
            || request.api_version < HOST_API_V0_6
        {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "library requires protocol1 and minimum Host API0.6",
            ));
        }
        request.validate()?;
        let capability = match &request.operation {
            extension_api::library::LibraryOperation::CaptureSelection { .. } => {
                Capability::CapturePropose
            }
            _ => Capability::LibraryRead,
        };
        self.authorize(
            session,
            CapabilityRequest {
                api_version: request.api_version,
                request_id: request.request_id.clone(),
                capability,
                resource: Resource::Vault {
                    vault_id: self.vault_id().to_owned(),
                },
            },
        )
    }
}
