//! Authority-free execution boundary. Callers own identity, grants and result acceptance.
//! Fuel and cooperative deadlines cannot preempt a native broker or compiler; hosts
//! still need a separately supervised process watchdog for those blocking operations.
pub mod process;
pub use process::{CleanupOutcome, ProcessIdentity};

use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

/// Legacy 0.1–0.7 probe defaults kept for backward compatibility with existing
/// `ModuleExecutor::execute` callers and process-worker protocol tests.
pub const MAX_WASM_BYTES: usize = 256 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 4096;
pub const MAX_RAW_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_PRIVATE_BYTES: usize = 64 * 1024;

/// Absolute hard ceilings enforced even when the Host supplies a custom profile.
/// Any configuration exceeding these bounds fails closed before instantiation.
pub const HARD_MAX_WASM_BYTES: usize = 16 * 1024 * 1024;
pub const HARD_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
pub const HARD_MAX_MEMORIES: usize = 1;
pub const HARD_MAX_TABLES: usize = 4;
pub const HARD_MAX_TABLE_ELEMENTS: usize = 16_384;
pub const HARD_MAX_INSTANCES: usize = 1;
pub const HARD_MAX_STACK_HEIGHT_BYTES: usize = 2 * 1024 * 1024;
pub const HARD_MAX_RECURSION_DEPTH: usize = 512;
pub const HARD_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const HARD_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const HARD_MAX_PRIVATE_BYTES: usize = 4 * 1024 * 1024;
pub const HARD_MAX_STREAM_CHUNK_BYTES: usize = 1024 * 1024;
pub const HARD_MAX_STREAM_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const HARD_MAX_STREAM_CHUNKS: usize = 1024;
pub const HARD_MAX_TOTAL_FUEL: u64 = 200_000_000;
pub const HARD_MAX_DEADLINE: Duration = Duration::from_secs(10);
pub const HARD_MAX_HOST_CALLS: usize = 256;

/// Host-selected execution profile used to inject bounded runtime budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBudgetProfile {
    /// Legacy qualification baseline (256 KiB module, 128 KiB memory, 4 KiB output, 1M fuel).
    LegacyProbe,
    /// Constrained mobile / Web interpreter profile sized for compiled Rust Wasm plugins.
    MobileStandard,
    /// Desktop standard profile sized for compiled Rust Wasm plugins.
    DesktopStandard,
}

/// Host-injected, strictly bounded runtime resource profile.
///
/// Defaults use [`RuntimeBudgetProfile::DesktopStandard`] so real Rust `wasm32`
/// plugins (allocator, `serde_json`, function table slots, UI/data payloads) fit
/// within MiB-class bounds while keeping hard ceilings enforced by [`Self::validate`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBudgets {
    pub max_wasm_bytes: usize,
    pub max_memory_bytes: usize,
    pub max_memories: usize,
    pub max_tables: usize,
    pub max_table_elements: usize,
    pub max_instances: usize,
    pub max_stack_height_bytes: usize,
    pub max_recursion_depth: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_private_bytes: usize,
    pub max_stream_chunk_bytes: usize,
    pub max_stream_total_bytes: usize,
    pub max_stream_chunks: usize,
    pub execution: ExecutionLimits,
}

impl Default for RuntimeBudgets {
    fn default() -> Self {
        Self::desktop_standard()
    }
}

impl RuntimeBudgets {
    /// Returns the Host budget preset for `profile`.
    pub fn for_profile(profile: RuntimeBudgetProfile) -> Self {
        match profile {
            RuntimeBudgetProfile::LegacyProbe => Self::legacy_probe(),
            RuntimeBudgetProfile::MobileStandard => Self::mobile_standard(),
            RuntimeBudgetProfile::DesktopStandard => Self::desktop_standard(),
        }
    }

    /// Legacy 0.1–0.7 qualification baseline.
    pub fn legacy_probe() -> Self {
        Self {
            max_wasm_bytes: MAX_WASM_BYTES,
            max_memory_bytes: 128 * 1024,
            max_memories: 1,
            max_tables: 1,
            max_table_elements: 16,
            max_instances: 1,
            max_stack_height_bytes: 256 * 1024,
            max_recursion_depth: 128,
            max_request_bytes: MAX_RAW_REQUEST_BYTES,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_private_bytes: MAX_PRIVATE_BYTES,
            max_stream_chunk_bytes: MAX_RESPONSE_BYTES,
            max_stream_total_bytes: MAX_RAW_REQUEST_BYTES,
            max_stream_chunks: 16,
            execution: ExecutionLimits::default(),
        }
    }

    /// Constrained mobile / Web interpreter profile (4 MiB module, 16 MiB memory, 20M fuel).
    pub fn mobile_standard() -> Self {
        Self {
            max_wasm_bytes: 4 * 1024 * 1024,
            max_memory_bytes: 16 * 1024 * 1024,
            max_memories: 1,
            max_tables: 2,
            max_table_elements: 4096,
            max_instances: 1,
            max_stack_height_bytes: 512 * 1024,
            max_recursion_depth: 256,
            max_request_bytes: 512 * 1024,
            max_response_bytes: 1024 * 1024,
            max_private_bytes: 512 * 1024,
            max_stream_chunk_bytes: 256 * 1024,
            max_stream_total_bytes: 4 * 1024 * 1024,
            max_stream_chunks: 64,
            execution: ExecutionLimits {
                total_fuel: 20_000_000,
                slice_fuel: 100_000,
                deadline: Duration::from_millis(1000),
                host_calls: 64,
            },
        }
    }

    /// Desktop standard profile (8 MiB module, 32 MiB memory, 50M fuel).
    pub fn desktop_standard() -> Self {
        Self {
            max_wasm_bytes: 8 * 1024 * 1024,
            max_memory_bytes: 32 * 1024 * 1024,
            max_memories: 1,
            max_tables: 2,
            max_table_elements: 8192,
            max_instances: 1,
            max_stack_height_bytes: 1024 * 1024,
            max_recursion_depth: 256,
            max_request_bytes: 1024 * 1024,
            max_response_bytes: 2 * 1024 * 1024,
            max_private_bytes: 1024 * 1024,
            max_stream_chunk_bytes: 256 * 1024,
            max_stream_total_bytes: 8 * 1024 * 1024,
            max_stream_chunks: 128,
            execution: ExecutionLimits {
                total_fuel: 50_000_000,
                slice_fuel: 250_000,
                deadline: Duration::from_millis(2000),
                host_calls: 128,
            },
        }
    }

    /// Fails closed if any budget is zero or exceeds the hard security ceiling.
    pub fn validate(&self) -> Result<()> {
        self.execution.validate()?;
        let invalid = self.max_wasm_bytes == 0
            || self.max_wasm_bytes > HARD_MAX_WASM_BYTES
            || self.max_memory_bytes < 64 * 1024
            || self.max_memory_bytes > HARD_MAX_MEMORY_BYTES
            || self.max_memories == 0
            || self.max_memories > HARD_MAX_MEMORIES
            || self.max_tables == 0
            || self.max_tables > HARD_MAX_TABLES
            || self.max_table_elements == 0
            || self.max_table_elements > HARD_MAX_TABLE_ELEMENTS
            || self.max_instances == 0
            || self.max_instances > HARD_MAX_INSTANCES
            || self.max_stack_height_bytes < 4096
            || self.max_stack_height_bytes > HARD_MAX_STACK_HEIGHT_BYTES
            || self.max_recursion_depth == 0
            || self.max_recursion_depth > HARD_MAX_RECURSION_DEPTH
            || self.max_request_bytes == 0
            || self.max_request_bytes > HARD_MAX_REQUEST_BYTES
            || self.max_response_bytes == 0
            || self.max_response_bytes > HARD_MAX_RESPONSE_BYTES
            || self.max_private_bytes == 0
            || self.max_private_bytes > HARD_MAX_PRIVATE_BYTES
            || self.max_stream_chunk_bytes == 0
            || self.max_stream_chunk_bytes > HARD_MAX_STREAM_CHUNK_BYTES
            || self.max_stream_total_bytes == 0
            || self.max_stream_total_bytes > HARD_MAX_STREAM_TOTAL_BYTES
            || self.max_stream_chunk_bytes > self.max_stream_total_bytes
            || self.max_stream_chunks == 0
            || self.max_stream_chunks > HARD_MAX_STREAM_CHUNKS
            || self.execution.total_fuel > HARD_MAX_TOTAL_FUEL
            || self.execution.slice_fuel > HARD_MAX_TOTAL_FUEL
            || self.execution.deadline > HARD_MAX_DEADLINE
            || self.execution.host_calls > HARD_MAX_HOST_CALLS;
        if invalid {
            return Err(RuntimeError::new("runtime_configuration"));
        }
        Ok(())
    }
}

/// Trusted host configuration, never accepted from a guest package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
            | "output_capacity"
            | "stream_not_active" => K::InvalidBuffer,
            "invalid_request" | "invalid_schema" => K::InvalidRequest,
            "package_size_limit"
            | "module_size_limit"
            | "output_size_limit"
            | "host_buffer_limit"
            | "host_response_limit"
            | "host_call_limit"
            | "state_size_limit"
            | "stream_chunk_limit"
            | "stream_total_limit"
            | "memory_limit" => K::BudgetExceeded,
            "fuel_exhausted" => K::FuelExhausted,
            "deadline_exceeded" => K::DeadlineExceeded,
            "state_rejected" => K::PrivateStateRejected,
            "vm_trap" | "session_poisoned" => K::Trap,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub output: String,
    pub calls: usize,
    pub fuel_consumed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamExecutionResult {
    pub output: String,
    pub chunks_emitted: usize,
    pub stream_bytes: usize,
    pub calls: usize,
    pub fuel_consumed: u64,
}

/// Bounded streaming sink fed by `qusor.emit_chunk` during an invocation.
pub trait ResponseSink {
    fn push_chunk(&mut self, chunk: &[u8]) -> Result<()>;
}

impl<F: FnMut(&[u8]) -> Result<()>> ResponseSink for F {
    fn push_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        self(chunk)
    }
}

/// Default in-memory bounded chunk collector for Host callers.
#[derive(Debug, Clone)]
pub struct BoundedStreamCollector {
    max_chunk_bytes: usize,
    max_total_bytes: usize,
    max_chunks: usize,
    total_bytes: usize,
    chunks: Vec<Vec<u8>>,
}

impl BoundedStreamCollector {
    pub fn from_budgets(budgets: &RuntimeBudgets) -> Result<Self> {
        budgets.validate()?;
        Ok(Self {
            max_chunk_bytes: budgets.max_stream_chunk_bytes,
            max_total_bytes: budgets.max_stream_total_bytes,
            max_chunks: budgets.max_stream_chunks,
            total_bytes: 0,
            chunks: Vec::new(),
        })
    }

    pub fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn into_concatenated(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.total_bytes);
        for chunk in self.chunks {
            out.extend_from_slice(&chunk);
        }
        out
    }
}

impl ResponseSink for BoundedStreamCollector {
    fn push_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        if chunk.len() > self.max_chunk_bytes {
            return Err(RuntimeError::new("stream_chunk_limit"));
        }
        if self.chunks.len() >= self.max_chunks {
            return Err(RuntimeError::new("stream_total_limit"));
        }
        let next_total = self
            .total_bytes
            .checked_add(chunk.len())
            .ok_or_else(|| RuntimeError::new("stream_total_limit"))?;
        if next_total > self.max_total_bytes {
            return Err(RuntimeError::new("stream_total_limit"));
        }
        self.total_bytes = next_total;
        self.chunks.push(chunk.to_vec());
        Ok(())
    }
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

/// Long-lived stateful Wasm module session preserving instance memory across invocations.
///
/// Each turn still enforces fresh per-invocation fuel, deadline, and host-call budgets;
/// any trap or budget fault poisons the session so corrupted guest memory cannot be reused.
pub trait ModuleSession {
    fn budgets(&self) -> &RuntimeBudgets;

    fn invoke(
        &mut self,
        entrypoint: &str,
        input: &[u8],
        limits: Option<&ExecutionLimits>,
        broker: &mut dyn CapabilityBroker,
    ) -> Result<ExecutionResult>;

    fn invoke_streaming(
        &mut self,
        entrypoint: &str,
        input: &[u8],
        limits: Option<&ExecutionLimits>,
        broker: &mut dyn CapabilityBroker,
        sink: &mut dyn ResponseSink,
    ) -> Result<StreamExecutionResult>;

    fn total_calls(&self) -> usize;
    fn total_fuel_consumed(&self) -> u64;
    fn is_poisoned(&self) -> bool;
}

/// Instantiates long-lived stateful plugin sessions under Host-injected budgets.
pub trait SessionExecutor {
    fn instantiate(&self, wasm: &[u8], budgets: &RuntimeBudgets) -> Result<Box<dyn ModuleSession>>;
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
    fn execute_with_budgets(
        &self,
        wasm: &[u8],
        budgets: &RuntimeBudgets,
        broker: &mut dyn CapabilityBroker,
    ) -> Result<ExecutionResult> {
        budgets.validate()?;
        self.execute(wasm, &budgets.execution, budgets.max_request_bytes, broker)
    }
    fn execute_streaming(
        &self,
        _wasm: &[u8],
        budgets: &RuntimeBudgets,
        _broker: &mut dyn CapabilityBroker,
        _sink: &mut dyn ResponseSink,
    ) -> Result<StreamExecutionResult> {
        budgets.validate()?;
        Err(RuntimeError::new("runtime_configuration"))
    }
    fn migrate(
        &self,
        wasm: &[u8],
        from_schema: u32,
        to_schema: u32,
        input: &[u8],
    ) -> Result<Vec<u8>>;
    fn preflight(&self, wasm: &[u8], schema: u32, input: &[u8]) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_profiles_validate_and_fail_closed_above_hard_ceilings() {
        for profile in [
            RuntimeBudgetProfile::LegacyProbe,
            RuntimeBudgetProfile::MobileStandard,
            RuntimeBudgetProfile::DesktopStandard,
        ] {
            let budgets = RuntimeBudgets::for_profile(profile);
            budgets.validate().unwrap();
        }
        assert_eq!(
            RuntimeBudgets::default(),
            RuntimeBudgets::desktop_standard()
        );
        assert!(RuntimeBudgets::desktop_standard().max_wasm_bytes >= 4 * 1024 * 1024);
        assert!(RuntimeBudgets::desktop_standard().max_memory_bytes >= 16 * 1024 * 1024);

        let mut over_wasm = RuntimeBudgets::desktop_standard();
        over_wasm.max_wasm_bytes = HARD_MAX_WASM_BYTES + 1;
        assert_eq!(
            over_wasm.validate().unwrap_err().kind(),
            RuntimeErrorKind::InvalidConfiguration
        );

        let mut over_memory = RuntimeBudgets::desktop_standard();
        over_memory.max_memory_bytes = HARD_MAX_MEMORY_BYTES + 1;
        assert_eq!(
            over_memory.validate().unwrap_err().kind(),
            RuntimeErrorKind::InvalidConfiguration
        );

        let mut zero_response = RuntimeBudgets::desktop_standard();
        zero_response.max_response_bytes = 0;
        assert_eq!(
            zero_response.validate().unwrap_err().kind(),
            RuntimeErrorKind::InvalidConfiguration
        );

        let mut bad_stream = RuntimeBudgets::desktop_standard();
        bad_stream.max_stream_chunk_bytes = bad_stream.max_stream_total_bytes + 1;
        assert_eq!(
            bad_stream.validate().unwrap_err().kind(),
            RuntimeErrorKind::InvalidConfiguration
        );
    }

    #[test]
    fn bounded_stream_collector_enforces_chunk_and_total_caps() {
        let mut budgets = RuntimeBudgets::legacy_probe();
        budgets.max_stream_chunk_bytes = 4;
        budgets.max_stream_total_bytes = 6;
        budgets.max_stream_chunks = 3;
        let mut collector = BoundedStreamCollector::from_budgets(&budgets).unwrap();
        collector.push_chunk(b"abcd").unwrap();
        assert_eq!(
            collector.push_chunk(b"abcde").unwrap_err().code,
            "stream_chunk_limit"
        );
        assert_eq!(
            collector.push_chunk(b"efg").unwrap_err().code,
            "stream_total_limit"
        );
        collector.push_chunk(b"ef").unwrap();
        assert_eq!(collector.total_bytes(), 6);
        assert_eq!(collector.into_concatenated(), b"abcdef");
    }
}
