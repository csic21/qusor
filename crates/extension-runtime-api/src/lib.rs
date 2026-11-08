//! Authority-free execution boundary. Callers own identity, grants and result acceptance.
//! Fuel and cooperative deadlines cannot preempt a native broker or compiler; hosts
//! still need a separately supervised process watchdog for those blocking operations.
pub mod process;
pub use process::{CleanupOutcome, ProcessIdentity};

use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

pub const MAX_WASM_BYTES: usize = 256 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 4096;
pub const MAX_RAW_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_PRIVATE_BYTES: usize = 64 * 1024;

/// Trusted host configuration, never accepted from a guest package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub total_fuel: u64,
    pub slice_fuel: u64,
    pub deadline: Duration,
    pub host_calls: usize,
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            total_fuel: 1_000_000,
            slice_fuel: 10_000,
            deadline: Duration::from_millis(250),
            host_calls: 8,
        }
    }
}
impl ExecutionLimits {
    pub fn validate(&self) -> Result<()> {
        if self.total_fuel == 0
            || self.slice_fuel == 0
            || self.deadline.is_zero()
            || self.host_calls == 0
        {
            return Err(RuntimeError::new("runtime_configuration"));
        }
        Ok(())
    }
}
/// Stable categories for host handling, independent from interpreter diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    InvalidConfiguration,
    InvalidModule,
    InvalidBuffer,
    InvalidRequest,
    BudgetExceeded,
    FuelExhausted,
    DeadlineExceeded,
    PrivateStateRejected,
    BrokerRejected,
    Trap,
}
/// Stable machine-readable error code; broker codes are preserved without guest text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: &'static str,
}
impl RuntimeError {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }
}
impl RuntimeError {
    pub fn kind(&self) -> RuntimeErrorKind {
        use RuntimeErrorKind as K;
        let code = self.code.strip_prefix("private_").unwrap_or(self.code);
        match code {
            "runtime_configuration" => K::InvalidConfiguration,
            "invalid_module" | "unknown_import" | "import_denied" | "instantiate_failed"
            | "missing_memory" | "invalid_entrypoint" | "invalid_abi" => K::InvalidModule,
            "invalid_pointer"
            | "invalid_output"
            | "invalid_request_size"
            | "invalid_output_capacity"
            | "output_capacity" => K::InvalidBuffer,
            "invalid_request" | "invalid_schema" => K::InvalidRequest,
            "package_size_limit"
            | "module_size_limit"
            | "output_size_limit"
            | "host_buffer_limit"
            | "host_response_limit"
            | "host_call_limit"
            | "state_size_limit" => K::BudgetExceeded,
            "fuel_exhausted" => K::FuelExhausted,
            "deadline_exceeded" => K::DeadlineExceeded,
            "state_rejected" => K::PrivateStateRejected,
            "vm_trap" => K::Trap,
            _ => K::BrokerRejected,
        }
    }
}
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for RuntimeError {}
pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub output: String,
    pub calls: usize,
    pub fuel_consumed: u64,
}

/// The outer Host checks every call; the runtime never manufactures permissions/data.
pub trait CapabilityBroker {
    fn call(&mut self, request: &[u8]) -> Result<Vec<u8>>;
}
impl<F: FnMut(&[u8]) -> Result<Vec<u8>>> CapabilityBroker for F {
    fn call(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        self(request)
    }
}
/// Consume already-frozen verifier bytes, not filesystem/package paths.
pub trait ModuleExecutor {
    fn execute(
        &self,
        wasm: &[u8],
        limits: &ExecutionLimits,
        request_limit: usize,
        broker: &mut dyn CapabilityBroker,
    ) -> Result<ExecutionResult>;
    fn migrate(
        &self,
        wasm: &[u8],
        from_schema: u32,
        to_schema: u32,
        input: &[u8],
    ) -> Result<Vec<u8>>;
    fn preflight(&self, wasm: &[u8], schema: u32, input: &[u8]) -> Result<()>;
}
