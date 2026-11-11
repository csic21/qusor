//! Host policy for bounded vocabulary study: bind the vault, require the exact
//! `vocab.read` or `vocab.review` grant for the requested operation.
//!
//! This module decides only that one bounded request may proceed. The trusted
//! application adapter still dispatches the permit and runs the Core use case;
//! the policy layer never reads or writes study data or executes guests.
use crate::{Host, Permit, Session};
use extension_api::{
    ApiError, Capability, CapabilityRequest, ErrorCode, HOST_API_V0_5, Resource,
    vocab::{VOCAB_PROTOCOL, VocabOperation, VocabRequest},
};

impl Host {
    /// Authorize one bounded vocabulary request against this host's vault and the
    /// installation's exact grant. Reads require `vocab.read`; a review requires
    /// `vocab.review` so a read-only package can never schedule cards.
    ///
    /// The guest supplies no vault: the Host binds its own. The returned permit
    /// must be dispatched by the trusted adapter and cannot be reused after a
    /// revocation, disable or session close.
    pub fn authorize_vocab(
        &self,
        session: &Session,
        request: &VocabRequest,
    ) -> Result<Permit, ApiError> {
        if request.vocab_protocol != VOCAB_PROTOCOL || request.api_version != HOST_API_V0_5 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "vocab requires protocol1 and Host API0.5",
            ));
        }
        request.validate()?;
        let capability = match &request.operation {
            VocabOperation::Decks {} | VocabOperation::Queue { .. } => Capability::VocabRead,
            VocabOperation::Review { .. } => Capability::VocabReview,
        };
        self.authorize(
            session,
            CapabilityRequest {
                api_version: HOST_API_V0_5,
                request_id: request.request_id.clone(),
                capability,
                resource: Resource::Vault {
                    vault_id: self.vault_id().to_owned(),
                },
            },
        )
    }
}
