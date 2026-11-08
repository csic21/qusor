//! Wasmi adapter with no Host, grants, filesystem, WASI or synthetic service data.
//! The outer supervisor remains responsible for native callback/compiler watchdogs.
pub use extension_runtime_api::{
    CapabilityBroker, ExecutionLimits, ExecutionResult, MAX_PRIVATE_BYTES, MAX_RAW_REQUEST_BYTES,
    MAX_RESPONSE_BYTES, MAX_WASM_BYTES, ModuleExecutor, RuntimeError, RuntimeErrorKind,
};
use wasmi::{CompilationMode, Config, EnforcedLimits, StoreLimits, StoreLimitsBuilder};
mod managed;
pub mod private_runtime;

/// Concrete adapter builder shared by the legacy qualification harness.
/// Wasmi types remain confined to this infrastructure crate, never the runtime port.
pub fn bounded_runtime_config() -> Config {
    let mut config = Config::default();
    config
        .consume_fuel(true)
        .allow_start_fn(false)
        .ignore_custom_sections(true)
        .wasm_multi_memory(false)
        .compilation_mode(CompilationMode::Eager)
        .enforced_limits(EnforcedLimits::strict())
        .set_min_stack_height(4096)
        .set_max_stack_height(256 * 1024)
        .set_max_recursion_depth(128)
        .set_max_cached_stacks(0);
    config
}
/// Concrete instance limits reused by the legacy qualification harness.
pub fn bounded_runtime_limits() -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(128 * 1024)
        .memories(1)
        .table_elements(16)
        .tables(1)
        .instances(1)
        .trap_on_grow_failure(true)
        .build()
}
/// Stateless engine adapter. Every invocation owns its VM and limits.
pub struct WasmiRuntime;
impl ModuleExecutor for WasmiRuntime {
    fn execute(
        &self,
        wasm: &[u8],
        limits: &ExecutionLimits,
        request_limit: usize,
        broker: &mut dyn CapabilityBroker,
    ) -> extension_runtime_api::Result<ExecutionResult> {
        managed::run_bounded(wasm, limits, request_limit, |bytes| broker.call(bytes))
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
