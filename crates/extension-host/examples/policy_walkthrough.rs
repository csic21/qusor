//! Trusted policy walkthrough using synthetic metadata, not a plugin runtime.
use extension_api::{ApiError, Capability, CapabilityRequest, ErrorCode, Manifest, Resource};
use extension_host::{Grant, Host, Status};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Manifest::from_json(include_str!("../../extension-api/fixtures/manifest.json"))?;
    let request =
        CapabilityRequest::from_json(include_str!("../../extension-api/fixtures/request.json"))?;
    let mut host = Host::new(
        request.resource.vault_id(),
        [
            Capability::SelectionRead,
            Capability::DocumentMetadataRead,
            Capability::CapturePropose,
        ],
    )?;
    let installation = host.register(manifest.clone(), "a".repeat(64))?;
    assert_eq!(host.status(installation)?, Status::Disabled);
    println!("1. Registered metadata: disabled, no implicit grants.");

    // Trusted host action stands in for a future explicit grant UI, never plugin input.
    let grant = Grant {
        capability: request.capability,
        resource: request.resource.clone(),
    };
    host.replace_grants(installation, vec![grant.clone()])?;
    host.enable(installation)?;
    let session = host.open_session(installation)?;
    let permit = host.authorize(&session, request.clone())?;
    host.dispatch(permit, |context| {
        println!(
            "2. Authorized {} for its granted selection in {}.",
            context.package_id, context.vault_id
        );
        Ok(())
    })?;

    let mut cross_vault = request.clone();
    cross_vault.resource = Resource::Selection {
        vault_id: "other-vault".into(),
        selection_id: "selection-1".into(),
    };
    assert_eq!(
        host.authorize(&session, cross_vault).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    println!("3. Cross-vault access denied.");

    let pending = host.authorize(&session, request.clone())?;
    host.replace_grants(installation, vec![])?;
    let result: Result<(), ApiError> = host.dispatch(pending, |_| panic!("revoked callback ran"));
    assert_eq!(result.unwrap_err().code, ErrorCode::StaleContext);
    println!("4. Revoked grants: in-flight callback rejected.");

    let mut updated = manifest.clone();
    updated.package_version.patch += 1;
    host.update(installation, updated, "b".repeat(64))?;
    assert!(host.grants(installation)?.is_empty());
    assert_eq!(host.status(installation)?, Status::Disabled);
    println!("5. Updated metadata: fresh authorization and enable required.");

    host.unregister(installation)?;
    let replacement = host.register(manifest, "a".repeat(64))?;
    host.replace_grants(replacement, vec![grant])?;
    host.enable(replacement)?;
    assert_ne!(installation, replacement);
    assert_eq!(
        host.authorize(&session, request).unwrap_err().code,
        ErrorCode::StaleContext
    );
    println!("6. Re-registered package: old installation/session cannot revive.");
    println!(
        "Policy walkthrough passed; no plugin code, persistence or knowledge writes executed."
    );
    Ok(())
}
