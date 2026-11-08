//! Verified component installation facts. Vault affinity is not an execution grant.
use super::*;
use extension_api::{HOST_API_V0_3, composition::CompositionDescriptor};
use extension_host::durable::{AuthorityRead, ComponentAuthoritySource, ComponentInstallation};
const COMPOSITION_ASSET: &str = "assets/ui-composition.json";

fn composition(package: &VerifiedPackage) -> Result<(CompositionDescriptor, String)> {
    if !package.manifest().supports_api(HOST_API_V0_3) {
        return Err(StoreError::unsupported(
            "component requires explicit Host API0.3",
        ));
    }
    let (mime, bytes) = package
        .file(COMPOSITION_ASSET)
        .ok_or_else(|| StoreError::invalid("signed composition asset missing"))?;
    if mime != "application/json" {
        return Err(StoreError::invalid("composition asset must be JSON"));
    }
    let descriptor = CompositionDescriptor::from_slice(bytes)
        .map_err(|_| StoreError::invalid("invalid bounded composition asset"))?;
    descriptor
        .validate_for_manifest(package.manifest())
        .map_err(|_| StoreError::invalid("composition contribution mismatch"))?;
    Ok((descriptor, format!("{:x}", Sha256::digest(bytes))))
}

impl Store {
    /// Bind an installed component to a vault, without enabling it or saving any
    /// draft access. Each mount still requires a new explicit live user intent.
    pub fn bind_component_vault(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        vault_id: &str,
    ) -> Result<Receipt> {
        validate_command(operation, expected)?;
        Resource::Vault {
            vault_id: vault_id.into(),
        }
        .validate()?;
        let input = input_hash(&("bind-component-vault", expected, package_id, vault_id))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        self.check_budget(None)?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err(StoreError::conflict(
                "component installation revision conflict",
            ));
        }
        let package = self.current_package(&old)?;
        composition(&package)?;
        self.check_private_pair_with_package(&old, &package, true)?;
        let bound: Option<String> = self
            .db
            .query_row(
                "SELECT library FROM installation_library WHERE installation_id=?1",
                [old.installation_id],
                |row| row.get(0),
            )
            .optional()?;
        if bound.as_ref().is_some_and(|bound| bound != vault_id) {
            return Err(StoreError::conflict("component belongs to another vault"));
        }
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO installation_library VALUES (?1,?2)",
            params![receipt.installation_id, vault_id],
        )?;
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        save_management(&tx, operation, &input, &receipt)?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }
}
impl ComponentAuthoritySource for Store {
    fn component_installation(
        &self,
        package_id: &str,
        read: AuthorityRead<'_>,
    ) -> std::result::Result<ComponentInstallation, extension_api::ApiError> {
        let read_facts = || -> Result<ComponentInstallation> {
            let item = self.current(package_id)?;
            if item.status == "uninstalled" {
                return Err(StoreError::conflict("component uninstalled"));
            }
            self.ensure_no_pending(item.installation_id)?;
            self.ensure_worker_access(
                item.installation_id,
                match read {
                    AuthorityRead::Activation => None,
                    AuthorityRead::Current { run_id } => run_id,
                },
            )?;
            // Read fresh bytes/key on every call; memoize only exact pure verification.
            let package = self.current_package_shared(&item)?;
            self.check_private_pair_with_package(
                &item,
                &package,
                matches!(read, AuthorityRead::Activation),
            )?;
            let (composition, composition_sha256) = composition(&package)?;
            let vault_id: String = self.db.query_row(
                "SELECT library FROM installation_library WHERE installation_id=?1",
                [item.installation_id],
                |row| row.get(0),
            )?;
            Resource::Vault {
                vault_id: vault_id.clone(),
            }
            .validate()?;
            Ok(ComponentInstallation {
                installation_id: item.installation_id,
                revision: item.revision,
                generation: item.generation,
                manifest: package.manifest().clone(),
                digest: item.digest,
                vault_id,
                composition,
                composition_sha256,
            })
        };
        read_facts().map_err(|_| {
            extension_api::ApiError::new(
                extension_api::ErrorCode::PermissionDenied,
                "component installation facts unavailable or invalid",
            )
        })
    }
}
