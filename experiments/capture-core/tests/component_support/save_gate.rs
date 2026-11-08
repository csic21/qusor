//! Explicit native save fixture; this does not claim a reader gesture or Surface lease.
use extension_api::*;
use extension_host::{Grant, durable::*};
use extension_package::package::VerifiedPackage;
use extension_store::{GrantScope, GrantSet, GrantValidator, StoreError};
use learning_application::{
    KnowledgeApplication, annotation_draft::AnnotationDrafts, draft_text::DraftId,
};
use std::sync::{Arc, Mutex};
use storage_sqlite::SqliteStore;

pub struct Policy(pub Arc<Mutex<DurableHost>>);
impl GrantValidator for Policy {
    fn validate(&self, _: &VerifiedPackage, _: &GrantScope) -> extension_store::Result<()> {
        Err(StoreError::unsupported(
            "fixture uses explicit profile2 only",
        ))
    }
    fn validate_set(&self, p: &VerifiedPackage, s: &GrantSet) -> extension_store::Result<()> {
        s.validate()?;
        self.0
            .lock()
            .unwrap()
            .validate_activation_intent(
                p.manifest(),
                p.digest(),
                &s.grants
                    .iter()
                    .map(|g| Grant {
                        capability: g.capability,
                        resource: g.resource.clone(),
                    })
                    .collect::<Vec<_>>(),
                s.api_version,
            )
            .map_err(StoreError::from)
    }
}
#[allow(clippy::too_many_arguments)] // Keep original capture identity, app registry and actual Core explicit.
pub fn save(
    host: &mut DurableHost,
    store: &extension_store::Store,
    session: &DurableSession,
    drafts: &mut AnnotationDrafts,
    id: &DraftId,
    core: &mut KnowledgeApplication<SqliteStore>,
    origin: &core_api::knowledge::Provenance,
    generation: i64,
    operation: &str,
    expected_text: &str,
) -> core_api::knowledge::PdfReceipt {
    let request = VersionedRequest {
        resource_version: None,
        request: CapabilityRequest {
            api_version: HOST_API_V0_2,
            request_id: "explicit-native-save".into(),
            capability: Capability::CapturePropose,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        },
    };
    host.dispatch(store, session, super::now(), None, request, |ctx| {
        assert_eq!(ctx.package_id, origin.package_id);
        assert_eq!(ctx.package_digest, origin.package_digest);
        let (source, bound_generation) = drafts.source(id).map_err(super::api)?;
        assert_eq!(source.provenance(), origin);
        assert_eq!(bound_generation, generation);
        let revision = drafts
            .read_chunk(id, None, 0, 512)
            .map_err(super::api)?
            .revision;
        let frozen = drafts
            .freeze_save(id, revision, operation)
            .map_err(super::api)?;
        let receipt = drafts.commit(core, &frozen).map_err(super::api)?;
        // Same immutable payload reconciliation never creates another operation.
        assert_eq!(
            drafts.reconcile(core, &frozen).map_err(super::api)?,
            Some(receipt.clone())
        );
        let record = core
            .query_pdf_complete(receipt.capture_id)
            .map_err(super::api)?;
        assert_eq!(record.user_note.as_bytes(), expected_text.as_bytes());
        assert_eq!(&record.provenance, origin);
        assert_eq!(
            &record.draft,
            drafts
                .source(id)
                .map_err(super::api)?
                .0
                .single()
                .ok_or_else(|| super::api("single-only capture fixture received compound source"))?
                .selection()
        );
        Ok(receipt)
    })
    .unwrap()
}
