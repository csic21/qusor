//! Bounded private-state migration and activation preflight. No grants or Core access.
//! Fuel bounds guest instructions; the deadline cannot preempt native compilation.
use crate::{MAX_WASM_BYTES, RuntimeError, bounded_runtime_config, bounded_runtime_limits};
use std::time::{Duration, Instant};
use wasmi::{
    Engine, Instance, Linker, Memory, Module, Store, StoreLimits, TypedFunc, TypedResumableCall,
    WasmParams, WasmResults,
};

pub use extension_runtime_api::MAX_PRIVATE_BYTES;
const TOTAL_FUEL: u64 = 1_000_000;
const SLICE_FUEL: u64 = 10_000;
const DEADLINE: Duration = Duration::from_millis(250);
type Result<T> = std::result::Result<T, RuntimeError>;

struct State {
    limits: StoreLimits,
    denied_call: bool,
}
struct Runtime {
    store: Store<State>,
    instance: Instance,
    memory: Memory,
    started: Instant,
    spent: u64,
}
impl Runtime {
    fn new(wasm: &[u8], allow_denied_broker: bool) -> Result<Self> {
        let started = Instant::now();
        if wasm.len() > MAX_WASM_BYTES {
            return Err(RuntimeError::new("private_module_size_limit"));
        }
        let engine = Engine::new(&bounded_runtime_config());
        let module =
            Module::new(&engine, wasm).map_err(|_| RuntimeError::new("private_invalid_module"))?;
        if module.imports().any(|import| {
            !allow_denied_broker || import.module() != "qusor" || import.name() != "call"
        }) {
            return Err(RuntimeError::new("private_import_denied"));
        }
        let limits = bounded_runtime_limits();
        let mut store = Store::new(
            &engine,
            State {
                limits,
                denied_call: false,
            },
        );
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(SLICE_FUEL)
            .map_err(|_| RuntimeError::new("private_runtime_configuration"))?;
        let mut linker = Linker::<State>::new(&engine);
        if allow_denied_broker {
            linker
                .func_wrap(
                    "qusor",
                    "call",
                    |mut caller: wasmi::Caller<'_, State>,
                     _: i32,
                     _: i32,
                     _: i32,
                     _: i32|
                     -> std::result::Result<i32, wasmi::Error> {
                        caller.data_mut().denied_call = true;
                        Err(wasmi::Error::new(
                            "private preflight denies business capabilities",
                        ))
                    },
                )
                .map_err(|_| RuntimeError::new("private_runtime_configuration"))?;
        }
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|_| RuntimeError::new("private_instantiate_failed"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| RuntimeError::new("private_missing_memory"))?;
        let runtime = Self {
            store,
            instance,
            memory,
            started,
            spent: 0,
        };
        runtime.check_deadline()?;
        Ok(runtime)
    }
    fn check_deadline(&self) -> Result<()> {
        if self.started.elapsed() >= DEADLINE {
            return Err(RuntimeError::new("private_deadline_exceeded"));
        }
        Ok(())
    }
    fn function<P: WasmParams, R: WasmResults>(&self, name: &str) -> Result<TypedFunc<P, R>> {
        self.instance
            .get_typed_func(&self.store, name)
            .map_err(|_| RuntimeError::new("private_invalid_abi"))
    }
    fn vm_error(&self) -> RuntimeError {
        RuntimeError::new(if self.store.data().denied_call {
            "private_capability_denied"
        } else {
            "private_vm_trap"
        })
    }
    fn invoke<P: WasmParams, R: WasmResults>(
        &mut self,
        function: TypedFunc<P, R>,
        args: P,
    ) -> Result<R> {
        self.check_deadline()?;
        let mut allocated = self
            .store
            .get_fuel()
            .map_err(|_| RuntimeError::new("private_runtime_configuration"))?;
        let mut outcome = function
            .call_resumable(&mut self.store, args)
            .map_err(|_| self.vm_error())?;
        loop {
            let remaining = self
                .store
                .get_fuel()
                .map_err(|_| RuntimeError::new("private_runtime_configuration"))?;
            self.spent += allocated - remaining;
            self.check_deadline()?;
            match outcome {
                TypedResumableCall::Finished(result) => return Ok(result),
                TypedResumableCall::HostTrap(_) => return Err(self.vm_error()),
                TypedResumableCall::OutOfFuel(suspended) => {
                    let available = TOTAL_FUEL - self.spent;
                    let required = suspended.required_fuel();
                    if available == 0 || required > available {
                        return Err(RuntimeError::new("private_fuel_exhausted"));
                    }
                    allocated = SLICE_FUEL.max(required).min(available);
                    self.store
                        .set_fuel(allocated)
                        .map_err(|_| RuntimeError::new("private_runtime_configuration"))?;
                    outcome = suspended
                        .resume(&mut self.store)
                        .map_err(|_| self.vm_error())?;
                }
            }
        }
    }
    fn copy_input(&mut self, input: &[u8]) -> Result<i32> {
        let alloc = self.function::<i32, i32>("alloc")?;
        let pointer = self.invoke(alloc, input.len() as i32)?;
        let offset =
            usize::try_from(pointer).map_err(|_| RuntimeError::new("private_invalid_pointer"))?;
        self.memory
            .write(&mut self.store, offset, input)
            .map_err(|_| RuntimeError::new("private_invalid_pointer"))?;
        Ok(pointer)
    }
}
fn validate_input(input: &[u8], schemas: &[u32]) -> Result<()> {
    if input.len() > MAX_PRIVATE_BYTES {
        return Err(RuntimeError::new("private_state_size_limit"));
    }
    if schemas
        .iter()
        .any(|schema| *schema == 0 || *schema > i32::MAX as u32)
    {
        return Err(RuntimeError::new("private_invalid_schema"));
    }
    Ok(())
}

/// Execute an import-free author migration against a copy of old private bytes.
/// Allocation, migration and output validation share one instruction/deadline budget.
pub fn run_migration(
    wasm: &[u8],
    from_schema: u32,
    to_schema: u32,
    input: &[u8],
) -> Result<Vec<u8>> {
    validate_input(input, &[from_schema, to_schema])?;
    let mut runtime = Runtime::new(wasm, false)?;
    let migrate = runtime.function::<(i32, i32, i32, i32), i64>("migrate")?;
    let pointer = runtime.copy_input(input)?;
    let packed = runtime.invoke(
        migrate,
        (
            from_schema as i32,
            to_schema as i32,
            pointer,
            input.len() as i32,
        ),
    )? as u64;
    let offset = (packed >> 32) as usize;
    let length = (packed & u32::MAX as u64) as usize;
    if length > MAX_PRIVATE_BYTES {
        return Err(RuntimeError::new("private_state_size_limit"));
    }
    let mut output = vec![0; length];
    runtime
        .memory
        .read(&runtime.store, offset, &mut output)
        .map_err(|_| RuntimeError::new("private_invalid_pointer"))?;
    runtime.check_deadline()?;
    Ok(output)
}

/// Verify the real activation ABI and ask that module to validate matching state.
/// The broker is linked only to reject every business call; `run` is not activated.
pub fn preflight(wasm: &[u8], schema: u32, input: &[u8]) -> Result<()> {
    validate_input(input, &[schema])?;
    let mut runtime = Runtime::new(wasm, true)?;
    runtime.function::<(), (i32, i32)>("run")?;
    let abi = runtime.function::<(), i32>("qusor_abi_version")?;
    let check = runtime.function::<(i32, i32, i32), i32>("qusor_state_check")?;
    if runtime.invoke(abi, ())? != 1 {
        return Err(RuntimeError::new("private_invalid_abi"));
    }
    let pointer = runtime.copy_input(input)?;
    if runtime.invoke(check, (schema as i32, pointer, input.len() as i32))? != 0 {
        return Err(RuntimeError::new("private_state_rejected"));
    }
    runtime.check_deadline()
}
