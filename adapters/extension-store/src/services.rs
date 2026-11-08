//! Trusted application ports. None is a guest capability or authorization proof.
use crate::{GrantScope, GrantSet, Result, StoreError};
use extension_package::package::VerifiedPackage;
use std::sync::Arc;

/// Validate requested persistent intent against the running Host's actual policy.
/// The application serializes this call and the durable commit with policy mutation.
pub trait GrantValidator: Send + Sync {
    fn validate(&self, package: &VerifiedPackage, scope: &GrantScope) -> Result<()>;
    fn validate_set(&self, _: &VerifiedPackage, _: &GrantSet) -> Result<()> {
        Err(StoreError::external(
            "trusted Host grant-set validator is unavailable",
        ))
    }
}
/// Authority-free private-data validation/migration, always outside SQL transactions.
/// The implementation must enforce execution/input/output budgets independently.
pub trait PrivateStateExecutor: Send + Sync {
    fn preflight(&self, module: &[u8], schema: u32, state: &[u8]) -> Result<()>;
    fn migrate(&self, module: &[u8], from: u32, to: u32, state: &[u8]) -> Result<Vec<u8>>;
}
/// Observe transaction boundaries. This is trusted instrumentation, not a guest hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkpoint {
    BeforeTransaction,
    AfterContent,
    BeforeCommit,
    AfterCommit,
    BeforePrepare,
    AfterPrepare,
    AfterMigration,
    AfterSnapshot,
}
pub trait CheckpointObserver: Send + Sync {
    fn checkpoint(&self, point: Checkpoint);
}
#[derive(Clone)]
pub struct StoreServices {
    pub grant_validator: Arc<dyn GrantValidator>,
    pub private_executor: Arc<dyn PrivateStateExecutor>,
    pub observer: Arc<dyn CheckpointObserver>,
}
struct Unavailable;
impl GrantValidator for Unavailable {
    fn validate(&self, _: &VerifiedPackage, _: &GrantScope) -> Result<()> {
        Err(StoreError::external(
            "trusted Host grant validator is unavailable",
        ))
    }
}
impl PrivateStateExecutor for Unavailable {
    fn preflight(&self, _: &[u8], _: u32, _: &[u8]) -> Result<()> {
        Err(StoreError::external(
            "private state executor is unavailable",
        ))
    }
    fn migrate(&self, _: &[u8], _: u32, _: u32, _: &[u8]) -> Result<Vec<u8>> {
        Err(StoreError::external(
            "private state executor is unavailable",
        ))
    }
}
struct NoObserver;
impl CheckpointObserver for NoObserver {
    fn checkpoint(&self, _: Checkpoint) {}
}
impl Default for StoreServices {
    fn default() -> Self {
        Self {
            grant_validator: Arc::new(Unavailable),
            private_executor: Arc::new(Unavailable),
            observer: Arc::new(NoObserver),
        }
    }
}
