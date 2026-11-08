//! Compatibility bridge through the authority-free production runtime port.
pub use extension_runtime_api::MAX_PRIVATE_BYTES;
use extension_runtime_api::{ModuleExecutor, Result};
pub fn run_migration(
    wasm: &[u8],
    from_schema: u32,
    to_schema: u32,
    input: &[u8],
) -> Result<Vec<u8>> {
    runtime_wasm::WasmiRuntime.migrate(wasm, from_schema, to_schema, input)
}
pub fn preflight(wasm: &[u8], schema: u32, input: &[u8]) -> Result<()> {
    runtime_wasm::WasmiRuntime.preflight(wasm, schema, input)
}
