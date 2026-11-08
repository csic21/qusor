//! Explicit durable authorization profile 2. Temporary leases and approval are excluded.
use super::*;
use extension_api::HOST_API_V0_2;

pub const MAX_PERSISTENT_GRANTS: usize = 32;
pub const MAX_AUTHORIZATION_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersistentGrant {
    pub capability: Capability,
    pub resource: Resource,
    pub resource_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GrantSet {
    pub api_version: ApiVersion,
    pub vault_id: String,
    pub expires_at: u64,
    pub grants: Vec<PersistentGrant>,
}
impl GrantSet {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != HOST_API_V0_2
            || self.grants.is_empty()
            || self.grants.len() > MAX_PERSISTENT_GRANTS
        {
            return Err(StoreError::invalid(
                "unsupported or oversized persistent grant set",
            ));
        }
        Resource::Vault {
            vault_id: self.vault_id.clone(),
        }
        .validate()?;
        for (index, grant) in self.grants.iter().enumerate() {
            grant.resource.validate()?;
            if grant.resource.vault_id() != self.vault_id {
                return Err(StoreError::invalid("persistent grant set crosses vaults"));
            }
            match (grant.capability, &grant.resource, &grant.resource_version) {
                (
                    Capability::ReaderSurface
                    | Capability::SelectionRead
                    | Capability::DocumentMetadataRead,
                    Resource::Document { .. },
                    Some(version),
                ) if valid_version(version) => {}
                (Capability::CapturePropose, Resource::Vault { .. }, None) => {}
                _ => {
                    return Err(StoreError::invalid(
                        "capability or transient resource cannot persist in profile 2",
                    ));
                }
            }
            for previous in &self.grants[..index] {
                if previous.resource == grant.resource {
                    if previous.capability == grant.capability {
                        return Err(StoreError::invalid(
                            "duplicate persistent capability/resource",
                        ));
                    }
                    if matches!(grant.resource, Resource::Document { .. })
                        && previous.resource_version != grant.resource_version
                    {
                        return Err(StoreError::invalid(
                            "one document has inconsistent persistent versions",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
fn valid_version(version: &str) -> bool {
    !version.is_empty() && version.len() <= 128 && !version.chars().any(char::is_control)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SavedGrantSet {
    pub installation_id: i64,
    pub digest: String,
    pub generation: i64,
    pub policy_version: u32,
    pub scope: GrantSet,
}

/// Explicitly versioned; legacy callers must never select the first entry of V2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedAuthorization {
    V1(SavedGrant),
    V2(SavedGrantSet),
}
impl SavedAuthorization {
    pub fn vault_id(&self) -> &str {
        match self {
            Self::V1(record) => &record.scope.library,
            Self::V2(record) => &record.scope.vault_id,
        }
    }
    pub fn durable_grants(&self) -> Vec<extension_host::durable::DurableGrant> {
        use extension_host::{Grant, durable::DurableGrant};
        match self {
            Self::V1(record) => vec![DurableGrant {
                grant: Grant {
                    capability: record.scope.capability,
                    resource: record.scope.resource(),
                },
                expires_at: record.scope.expires_at,
                resource_version: Some(record.scope.version.clone()),
            }],
            Self::V2(record) => record
                .scope
                .grants
                .iter()
                .map(|grant| DurableGrant {
                    grant: Grant {
                        capability: grant.capability,
                        resource: grant.resource.clone(),
                    },
                    expires_at: record.scope.expires_at,
                    resource_version: grant.resource_version.clone(),
                })
                .collect(),
        }
    }
}

impl Store {
    /// Explicit user intent, validated by the actual Host before any durable write.
    pub fn grant_set(
        &mut self,
        operation: &str,
        expected: i64,
        package_id: &str,
        scope: GrantSet,
    ) -> Result<Receipt> {
        validate_command(operation, expected)?;
        scope.validate()?;
        let input = input_hash(&("grant-set", expected, package_id, &scope))?;
        if let Some(receipt) = self.replay(operation, &input)? {
            return Ok(receipt);
        }
        if scope.expires_at <= now()? {
            return Err(StoreError::conflict("grant set already expired"));
        }
        self.check_budget(None)?;
        let old = self.current(package_id)?;
        self.ensure_no_pending(old.installation_id)?;
        self.ensure_worker_access(old.installation_id, None)?;
        if old.status == "uninstalled" || old.revision != expected {
            return Err(StoreError::conflict("installation revision conflict"));
        }
        let bound: Option<String> = self
            .db
            .query_row(
                "SELECT library FROM installation_library WHERE installation_id=?1",
                [old.installation_id],
                |r| r.get(0),
            )
            .optional()?;
        if bound.as_ref().is_some_and(|vault| vault != &scope.vault_id) {
            return Err(StoreError::conflict(
                "installation belongs to another library",
            ));
        }
        let package = self.current_package(&old)?;
        self.check_profile2(&package, &scope)?;
        self.check_private_ready(&old)?;
        self.services
            .grant_validator
            .validate_set(&package, &scope)?;
        let receipt = Receipt {
            operation_id: operation.into(),
            revision: checked_next(old.revision)?,
            generation: checked_next(old.generation)?,
            ..old
        };
        let record = SavedGrantSet {
            installation_id: receipt.installation_id,
            digest: receipt.digest.clone(),
            generation: receipt.generation,
            policy_version: 2,
            scope,
        };
        let encoded = serde_json::to_string(&record)?;
        if encoded.len() > MAX_AUTHORIZATION_BYTES {
            return Err(StoreError::invalid(
                "persistent authorization record budget exceeded",
            ));
        }
        self.services
            .observer
            .checkpoint(Checkpoint::BeforeTransaction);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO installation_library VALUES (?1,?2)",
            params![receipt.installation_id, record.scope.vault_id],
        )?;
        tx.execute("INSERT INTO grants VALUES (?1,?2) ON CONFLICT(installation_id) DO UPDATE SET record=excluded.record", params![receipt.installation_id, encoded])?;
        self.services.observer.checkpoint(Checkpoint::AfterContent);
        save_management(&tx, operation, &input, &receipt)?;
        self.services.observer.checkpoint(Checkpoint::BeforeCommit);
        tx.commit()?;
        self.services.observer.checkpoint(Checkpoint::AfterCommit);
        Ok(receipt)
    }

    fn check_profile2(&self, package: &VerifiedPackage, scope: &GrantSet) -> Result<()> {
        scope.validate()?;
        if !self.supported_apis.contains(&HOST_API_V0_2)
            || !package.manifest().supports_api(HOST_API_V0_2)
        {
            return Err(StoreError::unsupported(
                "persistent profile 2 requires explicit Host API0.2",
            ));
        }
        let manifest = package.manifest();
        if scope.grants.iter().any(|grant| {
            !manifest.required_capabilities.contains(&grant.capability)
                && !manifest.optional_capabilities.contains(&grant.capability)
        }) {
            return Err(StoreError::corrupt(
                "persistent grant is outside the verified manifest",
            ));
        }
        Ok(())
    }

    fn authorization_json(&self, installation_id: i64) -> Result<String> {
        let length: i64 = self.db.query_row(
            "SELECT length(CAST(record AS BLOB)) FROM grants WHERE installation_id=?1",
            [installation_id],
            |r| r.get(0),
        )?;
        if length < 1 || length > MAX_AUTHORIZATION_BYTES as i64 {
            return Err(StoreError::corrupt(
                "persistent authorization record budget exceeded",
            ));
        }
        // Repeat the bound in the value query so no oversized string is allocated
        // even if an out-of-protocol writer changed the row between statements.
        Ok(self.db.query_row("SELECT record FROM grants WHERE installation_id=?1 AND length(CAST(record AS BLOB)) BETWEEN 1 AND ?2", params![installation_id, MAX_AUTHORIZATION_BYTES as i64], |r|r.get(0))?)
    }

    pub fn saved_authorization(&self, item: &Receipt) -> Result<SavedAuthorization> {
        self.read_authorization(item, None)
    }

    /// Reuse only the freshly verified bytes from this same authority read.
    /// No verification cache or caller-provided package crosses the public port.
    pub(super) fn saved_authorization_with_package(
        &self,
        item: &Receipt,
        package: &VerifiedPackage,
    ) -> Result<SavedAuthorization> {
        self.read_authorization(item, Some(package))
    }

    fn read_authorization(
        &self,
        item: &Receipt,
        verified: Option<&VerifiedPackage>,
    ) -> Result<SavedAuthorization> {
        let json = self.authorization_json(item.installation_id)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let version = value
            .get("policy_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| StoreError::corrupt("persistent authorization version is missing"))?;
        let (authorization, installation_id, digest, generation) = match version {
            1 => {
                let record: SavedGrant = serde_json::from_str(&json)?;
                record.scope.validate()?;
                let fields = (
                    record.installation_id,
                    record.digest.clone(),
                    record.generation,
                );
                (SavedAuthorization::V1(record), fields.0, fields.1, fields.2)
            }
            2 => {
                let record: SavedGrantSet = serde_json::from_str(&json)?;
                let loaded;
                let package = if let Some(package) = verified {
                    package
                } else {
                    loaded = self.current_package(item)?;
                    &loaded
                };
                self.check_profile2(package, &record.scope)?;
                let fields = (
                    record.installation_id,
                    record.digest.clone(),
                    record.generation,
                );
                (SavedAuthorization::V2(record), fields.0, fields.1, fields.2)
            }
            _ => {
                return Err(StoreError::unsupported(
                    "unknown persistent authorization profile",
                ));
            }
        };
        let library: String = self.db.query_row(
            "SELECT library FROM installation_library WHERE installation_id=?1",
            [item.installation_id],
            |r| r.get(0),
        )?;
        if library != authorization.vault_id() {
            return Err(StoreError::corrupt("grant library binding requires repair"));
        }
        if item.status == "uninstalled"
            || installation_id != item.installation_id
            || digest != item.digest
            || generation != item.generation
        {
            return Err(StoreError::corrupt("grant binding requires repair"));
        }
        Ok(authorization)
    }
}
