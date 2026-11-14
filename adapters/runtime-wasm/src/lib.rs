//! Wasmi adapter with no Host, grants, filesystem, WASI or synthetic service data.
//! The outer supervisor remains responsible for native callback/compiler watchdogs.
pub use extension_runtime_api::{
    BoundedStreamCollector, CapabilityBroker, ExecutionLimits, ExecutionResult, MAX_PRIVATE_BYTES,
    MAX_RAW_REQUEST_BYTES, MAX_RESPONSE_BYTES, MAX_WASM_BYTES, ModuleExecutor, ModuleSession,
    ResponseSink, RuntimeBudgetProfile, RuntimeBudgets, RuntimeError, RuntimeErrorKind,
    SessionExecutor, StreamExecutionResult,
};
use wasmi::{CompilationMode, Config, EnforcedLimits, StoreLimits, StoreLimitsBuilder};
mod managed;
pub mod private_runtime;

/// Concrete adapter builder for the given Host-validated budget profile.
/// Wasmi types remain confined to this infrastructure crate, never the runtime port.
pub fn bounded_runtime_config_for(
    budgets: &RuntimeBudgets,
) -> extension_runtime_api::Result<Config> {
    budgets.validate()?;
    let mut config = Config::default();
    config
        .consume_fuel(true)
        .allow_start_fn(false)
        .ignore_custom_sections(true)
        .wasm_multi_memory(false)
        .compilation_mode(CompilationMode::Eager)
        .enforced_limits(EnforcedLimits::strict())
        .set_min_stack_height(4096)
        .set_max_stack_height(budgets.max_stack_height_bytes)
        .set_max_recursion_depth(budgets.max_recursion_depth)
        .set_max_cached_stacks(0);
    Ok(config)
}

/// Concrete adapter builder shared by the legacy qualification harness.
pub fn bounded_runtime_config() -> Config {
    bounded_runtime_config_for(&RuntimeBudgets::legacy_probe())
        .expect("legacy probe runtime budgets must validate")
}

/// Concrete instance limits for the given Host-validated budget profile.
pub fn bounded_runtime_limits_for(
    budgets: &RuntimeBudgets,
) -> extension_runtime_api::Result<StoreLimits> {
    budgets.validate()?;
    Ok(StoreLimitsBuilder::new()
        .memory_size(budgets.max_memory_bytes)
        .memories(budgets.max_memories)
        .table_elements(budgets.max_table_elements)
        .tables(budgets.max_tables)
        .instances(budgets.max_instances)
        .trap_on_grow_failure(true)
        .build())
}

/// Concrete instance limits reused by the legacy qualification harness.
pub fn bounded_runtime_limits() -> StoreLimits {
    bounded_runtime_limits_for(&RuntimeBudgets::legacy_probe())
        .expect("legacy probe store limits must validate")
}

/// Wasmi runtime adapter supporting both bounded one-shot calls and long-lived sessions.
pub struct WasmiRuntime;

impl ModuleExecutor for WasmiRuntime {
    fn execute(
        &self,
        wasm: &[u8],
        limits: &ExecutionLimits,
        request_limit: usize,
        broker: &mut dyn CapabilityBroker,
    ) -> extension_runtime_api::Result<ExecutionResult> {
        limits.validate()?;
        if request_limit == 0 || request_limit > MAX_RAW_REQUEST_BYTES {
            return Err(RuntimeError::new("runtime_configuration"));
        }
        let mut budgets = RuntimeBudgets::legacy_probe();
        budgets.execution = limits.clone();
        budgets.max_request_bytes = request_limit;
        managed::run_bounded_with_budgets(wasm, &budgets, broker)
    }

    fn execute_with_budgets(
        &self,
        wasm: &[u8],
        budgets: &RuntimeBudgets,
        broker: &mut dyn CapabilityBroker,
    ) -> extension_runtime_api::Result<ExecutionResult> {
        managed::run_bounded_with_budgets(wasm, budgets, broker)
    }

    fn execute_streaming(
        &self,
        wasm: &[u8],
        budgets: &RuntimeBudgets,
        broker: &mut dyn CapabilityBroker,
        sink: &mut dyn ResponseSink,
    ) -> extension_runtime_api::Result<StreamExecutionResult> {
        managed::run_streaming_with_budgets(wasm, budgets, broker, sink)
    }

    fn migrate(
        &self,
        wasm: &[u8],
        from_schema: u32,
        to_schema: u32,
        input: &[u8],
    ) -> extension_runtime_api::Result<Vec<u8>> {
        private_runtime::run_migration(wasm, from_schema, to_schema, input)
    }

    fn preflight(
        &self,
        wasm: &[u8],
        schema: u32,
        input: &[u8],
    ) -> extension_runtime_api::Result<()> {
        private_runtime::preflight(wasm, schema, input)
    }
}

impl SessionExecutor for WasmiRuntime {
    fn instantiate(
        &self,
        wasm: &[u8],
        budgets: &RuntimeBudgets,
    ) -> extension_runtime_api::Result<Box<dyn ModuleSession>> {
        let session = managed::WasmiSession::instantiate(wasm, budgets)?;
        Ok(Box::new(session))
    }
}

/// Parse legacy policy wire only; actual authorization remains in the caller's broker.
pub fn run_managed(
    wasm: &[u8],
    mut broker: impl FnMut(extension_api::CapabilityRequest) -> extension_runtime_api::Result<Vec<u8>>,
) -> extension_runtime_api::Result<ExecutionResult> {
    let mut raw = |bytes: &[u8]| {
        let request = extension_api::CapabilityRequest::from_slice(bytes)
            .map_err(|_| RuntimeError::new("invalid_request"))?;
        broker(request)
    };
    WasmiRuntime.execute(
        wasm,
        &ExecutionLimits::default(),
        extension_api::MAX_REQUEST_BYTES,
        &mut raw,
    )
}

/// A protocol-specific outer broker validates the bounded raw request.
pub fn run_managed_raw(
    wasm: &[u8],
    mut broker: impl FnMut(&[u8]) -> extension_runtime_api::Result<Vec<u8>>,
) -> extension_runtime_api::Result<ExecutionResult> {
    WasmiRuntime.execute(
        wasm,
        &ExecutionLimits::default(),
        MAX_RAW_REQUEST_BYTES,
        &mut broker,
    )
}

/// Execute under a Host-injected budget profile with a raw protocol broker.
pub fn run_managed_with_budgets(
    wasm: &[u8],
    budgets: &RuntimeBudgets,
    mut broker: impl FnMut(&[u8]) -> extension_runtime_api::Result<Vec<u8>>,
) -> extension_runtime_api::Result<ExecutionResult> {
    WasmiRuntime.execute_with_budgets(wasm, budgets, &mut broker)
}
