//! Bounded raw execution. External broker is the sole authority/data source.
use crate::{
    ExecutionLimits, ExecutionResult, MAX_RAW_REQUEST_BYTES, MAX_RESPONSE_BYTES, MAX_WASM_BYTES,
    RuntimeError, bounded_runtime_config, bounded_runtime_limits,
};
use std::time::Instant;
use wasmi::{Caller, Engine, Linker, Module, Store, StoreLimits, TypedResumableCall};
type Result<T> = std::result::Result<T, RuntimeError>;
struct State<B> {
    broker: B,
    limits: StoreLimits,
    options: ExecutionLimits,
    started: Instant,
    calls: usize,
    request_limit: usize,
    error: Option<RuntimeError>,
}

impl<B> State<B> {
    fn check_deadline(&self) -> Result<()> {
        if self.started.elapsed() >= self.options.deadline {
            return Err(RuntimeError::new("deadline_exceeded"));
        }
        Ok(())
    }
    fn vm_error(&self) -> RuntimeError {
        self.error
            .clone()
            .unwrap_or_else(|| RuntimeError::new("vm_trap"))
    }
}

pub(crate) fn run_bounded(
    wasm: &[u8],
    limits: &ExecutionLimits,
    request_limit: usize,
    broker: impl FnMut(&[u8]) -> Result<Vec<u8>>,
) -> Result<ExecutionResult> {
    limits.validate()?;
    if request_limit == 0 || request_limit > MAX_RAW_REQUEST_BYTES {
        return Err(RuntimeError::new("runtime_configuration"));
    }
    let options = limits.clone();
    let started = Instant::now();
    if wasm.len() > MAX_WASM_BYTES {
        return Err(RuntimeError::new("package_size_limit"));
    }
    let engine = Engine::new(&bounded_runtime_config());
    let module = Module::new(&engine, wasm).map_err(|_| RuntimeError::new("invalid_module"))?;
    if module
        .imports()
        .any(|import| import.module() != "qusor" || import.name() != "call")
    {
        return Err(RuntimeError::new("unknown_import"));
    }
    let mut store = Store::new(
        &engine,
        State {
            broker,
            limits: bounded_runtime_limits(),
            options: options.clone(),
            started,
            calls: 0,
            request_limit,
            error: None,
        },
    );
    store.limiter(|state| &mut state.limits);
    store.data().check_deadline()?;
    let mut slice = options.slice_fuel.min(options.total_fuel);
    store
        .set_fuel(slice)
        .map_err(|_| RuntimeError::new("runtime_configuration"))?;
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "qusor",
            "call",
            |mut caller: Caller<'_, State<_>>, ptr: i32, len: i32, out: i32, cap: i32| {
                match forward(&mut caller, ptr, len, out, cap) {
                    Ok(length) => Ok(length),
                    Err(error) => {
                        caller.data_mut().error = Some(error);
                        Err(wasmi::Error::new("external capability broker rejected call"))
                    }
                }
            },
        )
        .map_err(|_| RuntimeError::new("runtime_configuration"))?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| RuntimeError::new("instantiate_failed"))?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| RuntimeError::new("missing_memory"))?;
    let run = instance
        .get_typed_func::<(), (i32, i32)>(&store, "run")
        .map_err(|_| RuntimeError::new("invalid_entrypoint"))?;
    store.data().check_deadline()?;
    let mut outcome = run
        .call_resumable(&mut store, ())
        .map_err(|_| store.data().vm_error())?;
    let mut spent = 0;
    let (pointer, length) = loop {
        spent += slice
            - store
                .get_fuel()
                .map_err(|_| RuntimeError::new("runtime_configuration"))?;
        store.data().check_deadline()?;
        match outcome {
            TypedResumableCall::Finished(result) => break result,
            TypedResumableCall::HostTrap(_) => return Err(store.data().vm_error()),
            TypedResumableCall::OutOfFuel(suspended) => {
                let remaining = options.total_fuel - spent;
                let required = suspended.required_fuel();
                if remaining == 0 || required > remaining {
                    return Err(RuntimeError::new("fuel_exhausted"));
                }
                slice = options.slice_fuel.max(required).min(remaining);
                store
                    .set_fuel(slice)
                    .map_err(|_| RuntimeError::new("runtime_configuration"))?;
                outcome = suspended
                    .resume(&mut store)
                    .map_err(|_| store.data().vm_error())?;
            }
        }
    };
    let pointer = usize::try_from(pointer).map_err(|_| RuntimeError::new("invalid_output"))?;
    let length = usize::try_from(length).map_err(|_| RuntimeError::new("invalid_output"))?;
    if length > MAX_RESPONSE_BYTES {
        return Err(RuntimeError::new("output_size_limit"));
    }
    let mut output = vec![0; length];
    memory
        .read(&store, pointer, &mut output)
        .map_err(|_| RuntimeError::new("invalid_output"))?;
    let output = String::from_utf8(output).map_err(|_| RuntimeError::new("invalid_output"))?;
    store.data().check_deadline()?;
    Ok(ExecutionResult {
        output,
        calls: store.data().calls,
        fuel_consumed: spent,
    })
}

fn forward<B: FnMut(&[u8]) -> Result<Vec<u8>>>(
    caller: &mut Caller<'_, State<B>>,
    ptr: i32,
    len: i32,
    out: i32,
    cap: i32,
) -> Result<i32> {
    caller.data().check_deadline()?;
    if caller.data().calls >= caller.data().options.host_calls {
        return Err(RuntimeError::new("host_call_limit"));
    }
    let pointer = usize::try_from(ptr).map_err(|_| RuntimeError::new("invalid_pointer"))?;
    let length = usize::try_from(len).map_err(|_| RuntimeError::new("invalid_request_size"))?;
    let output = usize::try_from(out).map_err(|_| RuntimeError::new("invalid_pointer"))?;
    let capacity =
        usize::try_from(cap).map_err(|_| RuntimeError::new("invalid_output_capacity"))?;
    if length > caller.data().request_limit || capacity > MAX_RESPONSE_BYTES {
        return Err(RuntimeError::new("host_buffer_limit"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| RuntimeError::new("missing_memory"))?;
    // Validate both guest ranges before a parent broker can perform any work.
    let memory_size = memory.data(&*caller).len();
    if pointer
        .checked_add(length)
        .is_none_or(|end| end > memory_size)
        || output
            .checked_add(capacity)
            .is_none_or(|end| end > memory_size)
    {
        return Err(RuntimeError::new("invalid_pointer"));
    }
    let mut bytes = vec![0; length];
    memory
        .read(&*caller, pointer, &mut bytes)
        .map_err(|_| RuntimeError::new("invalid_pointer"))?;
    caller.data_mut().calls += 1;
    let response = (caller.data_mut().broker)(&bytes)?;
    caller.data().check_deadline()?;
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(RuntimeError::new("host_response_limit"));
    }
    if response.len() > capacity {
        return Err(RuntimeError::new("output_capacity"));
    }
    memory
        .write(&mut *caller, output, &response)
        .map_err(|_| RuntimeError::new("invalid_pointer"))?;
    Ok(response.len() as i32)
}
