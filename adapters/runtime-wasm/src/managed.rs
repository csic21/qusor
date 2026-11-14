//! Bounded raw execution, streaming chunks, and stateful sessions.
//! External broker is the sole authority/data source.
use crate::{
    CapabilityBroker, ExecutionLimits, ExecutionResult, ModuleSession, ResponseSink,
    RuntimeBudgets, RuntimeError, StreamExecutionResult, bounded_runtime_config_for,
    bounded_runtime_limits_for,
};
use std::{fmt, time::Instant};
use wasmi::{
    Caller, Engine, Instance, Linker, Memory, Module, Store, StoreLimits, TypedFunc,
    TypedResumableCall, Val, WasmParams, WasmResults,
};

type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug)]
enum HostYield {
    BrokerCall {
        request: Vec<u8>,
        out_ptr: usize,
        out_cap: usize,
    },
    StreamChunk {
        chunk: Vec<u8>,
    },
}

impl fmt::Display for HostYield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("qusor host yield")
    }
}

impl std::error::Error for HostYield {}
impl wasmi::errors::HostError for HostYield {}

struct SessionState {
    limits: StoreLimits,
    budgets: RuntimeBudgets,
    turn_limits: ExecutionLimits,
    turn_started: Instant,
    turn_calls: usize,
    turn_chunks: usize,
    turn_stream_bytes: usize,
    streaming_active: bool,
    error: Option<RuntimeError>,
}

impl SessionState {
    fn check_deadline(&self) -> Result<()> {
        if self.turn_started.elapsed() >= self.turn_limits.deadline {
            return Err(RuntimeError::new("deadline_exceeded"));
        }
        Ok(())
    }

    fn vm_error(&self) -> RuntimeError {
        self.error
            .clone()
            .unwrap_or_else(|| RuntimeError::new("vm_trap"))
    }

    fn begin_turn(&mut self, limits: &ExecutionLimits, streaming_active: bool) {
        self.turn_limits = limits.clone();
        self.turn_started = Instant::now();
        self.turn_calls = 0;
        self.turn_chunks = 0;
        self.turn_stream_bytes = 0;
        self.streaming_active = streaming_active;
        self.error = None;
    }
}

pub(crate) struct WasmiSession {
    budgets: RuntimeBudgets,
    store: Store<SessionState>,
    instance: Instance,
    memory: Memory,
    total_calls: usize,
    total_fuel_consumed: u64,
    poisoned: bool,
}

impl WasmiSession {
    pub(crate) fn instantiate(wasm: &[u8], budgets: &RuntimeBudgets) -> Result<Self> {
        instantiate_session(wasm, budgets, true)
    }

    fn run_turn(
        &mut self,
        entrypoint: &str,
        input: &[u8],
        limits: Option<&ExecutionLimits>,
        broker: &mut dyn CapabilityBroker,
        mut sink: Option<&mut dyn ResponseSink>,
    ) -> Result<(String, usize, usize, usize, u64)> {
        if self.poisoned {
            return Err(RuntimeError::new("session_poisoned"));
        }
        let turn_limits = match limits {
            Some(custom) => {
                let mut check = self.budgets.clone();
                check.execution = custom.clone();
                check.validate()?;
                custom.clone()
            }
            None => self.budgets.execution.clone(),
        };
        if input.len() > self.budgets.max_request_bytes {
            return Err(RuntimeError::new("host_buffer_limit"));
        }

        let streaming_active = sink.is_some();
        self.store
            .data_mut()
            .begin_turn(&turn_limits, streaming_active);

        let result = (|| {
            let (mut slice, _) = prepare_turn_fuel(&mut self.store, &turn_limits)?;
            let mut spent = 0u64;

            let (pointer, length) = if let Ok(func) = self
                .instance
                .get_typed_func::<(i32, i32), (i32, i32)>(&self.store, entrypoint)
            {
                let input_len_i32 = i32::try_from(input.len())
                    .map_err(|_| RuntimeError::new("host_buffer_limit"))?;
                let input_ptr = if input.is_empty() {
                    0
                } else {
                    let alloc = self
                        .instance
                        .get_typed_func::<i32, i32>(&self.store, "qusor_alloc")
                        .or_else(|_| {
                            self.instance
                                .get_typed_func::<i32, i32>(&self.store, "alloc")
                        })
                        .map_err(|_| RuntimeError::new("invalid_entrypoint"))?;
                    let (ptr, alloc_spent, next_slice) = drive_resumable(
                        &mut self.store,
                        &self.memory,
                        &self.budgets,
                        &turn_limits,
                        alloc,
                        input_len_i32,
                        slice,
                        0,
                        broker,
                        &mut sink,
                    )?;
                    spent = alloc_spent;
                    slice = next_slice;
                    let offset =
                        usize::try_from(ptr).map_err(|_| RuntimeError::new("invalid_pointer"))?;
                    self.memory
                        .write(&mut self.store, offset, input)
                        .map_err(|_| RuntimeError::new("invalid_pointer"))?;
                    ptr
                };
                let (output_pair, total_spent, _) = drive_resumable(
                    &mut self.store,
                    &self.memory,
                    &self.budgets,
                    &turn_limits,
                    func,
                    (input_ptr, input_len_i32),
                    slice,
                    spent,
                    broker,
                    &mut sink,
                )?;
                spent = total_spent;
                output_pair
            } else if input.is_empty() {
                let func = self
                    .instance
                    .get_typed_func::<(), (i32, i32)>(&self.store, entrypoint)
                    .map_err(|_| RuntimeError::new("invalid_entrypoint"))?;
                let (output_pair, total_spent, _) = drive_resumable(
                    &mut self.store,
                    &self.memory,
                    &self.budgets,
                    &turn_limits,
                    func,
                    (),
                    slice,
                    0,
                    broker,
                    &mut sink,
                )?;
                spent = total_spent;
                output_pair
            } else {
                return Err(RuntimeError::new("invalid_entrypoint"));
            };

            let output =
                read_guest_output(&self.store, &self.memory, &self.budgets, pointer, length)?;
            self.store.data().check_deadline()?;
            let calls = self.store.data().turn_calls;
            let chunks = self.store.data().turn_chunks;
            let stream_bytes = self.store.data().turn_stream_bytes;
            Ok((output, calls, chunks, stream_bytes, spent))
        })();

        match result {
            Ok((output, calls, chunks, stream_bytes, spent)) => {
                self.total_calls += calls;
                self.total_fuel_consumed += spent;
                Ok((output, calls, chunks, stream_bytes, spent))
            }
            Err(err) => {
                // Any VM/budget/broker failure poisons the stateful instance so
                // partially mutated guest memory cannot be reused.
                self.poisoned = true;
                Err(err)
            }
        }
    }
}

impl ModuleSession for WasmiSession {
    fn budgets(&self) -> &RuntimeBudgets {
        &self.budgets
    }

    fn invoke(
        &mut self,
        entrypoint: &str,
        input: &[u8],
        limits: Option<&ExecutionLimits>,
        broker: &mut dyn CapabilityBroker,
    ) -> Result<ExecutionResult> {
        let (output, calls, _, _, fuel_consumed) =
            self.run_turn(entrypoint, input, limits, broker, None)?;
        Ok(ExecutionResult {
            output,
            calls,
            fuel_consumed,
        })
    }

    fn invoke_streaming(
        &mut self,
        entrypoint: &str,
        input: &[u8],
        limits: Option<&ExecutionLimits>,
        broker: &mut dyn CapabilityBroker,
        sink: &mut dyn ResponseSink,
    ) -> Result<StreamExecutionResult> {
        let (output, calls, chunks_emitted, stream_bytes, fuel_consumed) =
            self.run_turn(entrypoint, input, limits, broker, Some(sink))?;
        Ok(StreamExecutionResult {
            output,
            chunks_emitted,
            stream_bytes,
            calls,
            fuel_consumed,
        })
    }

    fn total_calls(&self) -> usize {
        self.total_calls
    }

    fn total_fuel_consumed(&self) -> u64 {
        self.total_fuel_consumed
    }

    fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

pub(crate) fn run_bounded_with_budgets(
    wasm: &[u8],
    budgets: &RuntimeBudgets,
    broker: &mut dyn CapabilityBroker,
) -> Result<ExecutionResult> {
    let mut session = instantiate_session(wasm, budgets, false)?;
    session.invoke("run", &[], None, broker)
}

pub(crate) fn run_streaming_with_budgets(
    wasm: &[u8],
    budgets: &RuntimeBudgets,
    broker: &mut dyn CapabilityBroker,
    sink: &mut dyn ResponseSink,
) -> Result<StreamExecutionResult> {
    let mut session = instantiate_session(wasm, budgets, true)?;
    session.invoke_streaming("run", &[], None, broker, sink)
}

fn instantiate_session(
    wasm: &[u8],
    budgets: &RuntimeBudgets,
    allow_stream_import: bool,
) -> Result<WasmiSession> {
    budgets.validate()?;
    let started = Instant::now();
    if wasm.len() > budgets.max_wasm_bytes {
        return Err(RuntimeError::new("package_size_limit"));
    }
    let config = bounded_runtime_config_for(budgets)?;
    let store_limits = bounded_runtime_limits_for(budgets)?;
    let engine = Engine::new(&config);
    let module = Module::new(&engine, wasm).map_err(|_| RuntimeError::new("invalid_module"))?;
    for import in module.imports() {
        let allowed = match (import.module(), import.name()) {
            ("qusor", "call") => true,
            ("qusor", "emit_chunk") => allow_stream_import,
            _ => false,
        };
        if !allowed {
            return Err(RuntimeError::new("unknown_import"));
        }
    }

    let mut store = Store::new(
        &engine,
        SessionState {
            limits: store_limits,
            budgets: budgets.clone(),
            turn_limits: budgets.execution.clone(),
            turn_started: started,
            turn_calls: 0,
            turn_chunks: 0,
            turn_stream_bytes: 0,
            streaming_active: false,
            error: None,
        },
    );
    store.limiter(|state| &mut state.limits);
    store.data().check_deadline()?;

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "qusor",
            "call",
            |mut caller: Caller<'_, SessionState>,
             ptr: i32,
             len: i32,
             out: i32,
             cap: i32|
             -> std::result::Result<i32, wasmi::Error> {
                match prepare_broker_call(&mut caller, ptr, len, out, cap) {
                    Ok(yield_op) => Err(wasmi::Error::host(yield_op)),
                    Err(error) => {
                        caller.data_mut().error = Some(error);
                        Err(wasmi::Error::new(
                            "external capability broker rejected call",
                        ))
                    }
                }
            },
        )
        .map_err(|_| RuntimeError::new("runtime_configuration"))?;

    if allow_stream_import {
        linker
            .func_wrap(
                "qusor",
                "emit_chunk",
                |mut caller: Caller<'_, SessionState>,
                 ptr: i32,
                 len: i32|
                 -> std::result::Result<i32, wasmi::Error> {
                    match prepare_stream_chunk(&mut caller, ptr, len) {
                        Ok(yield_op) => Err(wasmi::Error::host(yield_op)),
                        Err(error) => {
                            caller.data_mut().error = Some(error);
                            Err(wasmi::Error::new("streaming sink rejected chunk"))
                        }
                    }
                },
            )
            .map_err(|_| RuntimeError::new("runtime_configuration"))?;
    }

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| RuntimeError::new("instantiate_failed"))?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| RuntimeError::new("missing_memory"))?;
    store.data().check_deadline()?;

    Ok(WasmiSession {
        budgets: budgets.clone(),
        store,
        instance,
        memory,
        total_calls: 0,
        total_fuel_consumed: 0,
        poisoned: false,
    })
}

fn prepare_turn_fuel(
    store: &mut Store<SessionState>,
    limits: &ExecutionLimits,
) -> Result<(u64, ())> {
    store.data().check_deadline()?;
    let slice = limits.slice_fuel.min(limits.total_fuel);
    store
        .set_fuel(slice)
        .map_err(|_| RuntimeError::new("runtime_configuration"))?;
    Ok((slice, ()))
}

#[allow(clippy::too_many_arguments)]
fn drive_resumable<P: WasmParams, R: WasmResults>(
    store: &mut Store<SessionState>,
    memory: &Memory,
    budgets: &RuntimeBudgets,
    limits: &ExecutionLimits,
    func: TypedFunc<P, R>,
    args: P,
    mut slice: u64,
    mut spent: u64,
    broker: &mut dyn CapabilityBroker,
    sink: &mut Option<&mut dyn ResponseSink>,
) -> Result<(R, u64, u64)> {
    store.data().check_deadline()?;
    let mut outcome = func
        .call_resumable(&mut *store, args)
        .map_err(|_| store.data().vm_error())?;

    loop {
        let fuel_left = store
            .get_fuel()
            .map_err(|_| RuntimeError::new("runtime_configuration"))?;
        spent += slice.saturating_sub(fuel_left);
        store.data().check_deadline()?;

        match outcome {
            TypedResumableCall::Finished(result) => return Ok((result, spent, fuel_left)),
            TypedResumableCall::OutOfFuel(suspended) => {
                let remaining = limits.total_fuel.saturating_sub(spent);
                let required = suspended.required_fuel();
                if remaining == 0 || required > remaining {
                    return Err(RuntimeError::new("fuel_exhausted"));
                }
                slice = limits.slice_fuel.max(required).min(remaining);
                store
                    .set_fuel(slice)
                    .map_err(|_| RuntimeError::new("runtime_configuration"))?;
                outcome = suspended
                    .resume(&mut *store)
                    .map_err(|_| store.data().vm_error())?;
            }
            TypedResumableCall::HostTrap(trap) => {
                if let Some(HostYield::BrokerCall {
                    request,
                    out_ptr,
                    out_cap,
                }) = trap.host_error().downcast_ref::<HostYield>()
                {
                    let request = request.clone();
                    let out_ptr = *out_ptr;
                    let out_cap = *out_cap;
                    let response = broker.call(&request)?;
                    store.data().check_deadline()?;
                    if response.len() > budgets.max_response_bytes {
                        return Err(RuntimeError::new("host_response_limit"));
                    }
                    if response.len() > out_cap {
                        return Err(RuntimeError::new("output_capacity"));
                    }
                    memory
                        .write(&mut *store, out_ptr, &response)
                        .map_err(|_| RuntimeError::new("invalid_pointer"))?;
                    let reply_len = i32::try_from(response.len())
                        .map_err(|_| RuntimeError::new("host_response_limit"))?;
                    slice = fuel_left;
                    outcome = trap
                        .resume(&mut *store, &[Val::I32(reply_len)])
                        .map_err(|_| store.data().vm_error())?;
                } else if let Some(HostYield::StreamChunk { chunk }) =
                    trap.host_error().downcast_ref::<HostYield>()
                {
                    let chunk = chunk.clone();
                    let target = sink
                        .as_deref_mut()
                        .ok_or_else(|| RuntimeError::new("stream_not_active"))?;
                    target.push_chunk(&chunk)?;
                    store.data().check_deadline()?;
                    slice = fuel_left;
                    outcome = trap
                        .resume(&mut *store, &[Val::I32(0)])
                        .map_err(|_| store.data().vm_error())?;
                } else {
                    return Err(store.data().vm_error());
                }
            }
        }
    }
}

fn read_guest_output(
    store: &Store<SessionState>,
    memory: &Memory,
    budgets: &RuntimeBudgets,
    pointer: i32,
    length: i32,
) -> Result<String> {
    let pointer = usize::try_from(pointer).map_err(|_| RuntimeError::new("invalid_output"))?;
    let length = usize::try_from(length).map_err(|_| RuntimeError::new("invalid_output"))?;
    if length > budgets.max_response_bytes {
        return Err(RuntimeError::new("output_size_limit"));
    }
    let mut output = vec![0; length];
    memory
        .read(store, pointer, &mut output)
        .map_err(|_| RuntimeError::new("invalid_output"))?;
    String::from_utf8(output).map_err(|_| RuntimeError::new("invalid_output"))
}

fn prepare_broker_call(
    caller: &mut Caller<'_, SessionState>,
    ptr: i32,
    len: i32,
    out: i32,
    cap: i32,
) -> Result<HostYield> {
    caller.data().check_deadline()?;
    if caller.data().turn_calls >= caller.data().turn_limits.host_calls {
        return Err(RuntimeError::new("host_call_limit"));
    }
    let pointer = usize::try_from(ptr).map_err(|_| RuntimeError::new("invalid_pointer"))?;
    let length = usize::try_from(len).map_err(|_| RuntimeError::new("invalid_request_size"))?;
    let output = usize::try_from(out).map_err(|_| RuntimeError::new("invalid_pointer"))?;
    let capacity =
        usize::try_from(cap).map_err(|_| RuntimeError::new("invalid_output_capacity"))?;
    if length > caller.data().budgets.max_request_bytes
        || capacity > caller.data().budgets.max_response_bytes
    {
        return Err(RuntimeError::new("host_buffer_limit"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| RuntimeError::new("missing_memory"))?;
    // Validate both guest ranges before the parent broker performs any work.
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
    caller.data_mut().turn_calls += 1;
    Ok(HostYield::BrokerCall {
        request: bytes,
        out_ptr: output,
        out_cap: capacity,
    })
}

fn prepare_stream_chunk(
    caller: &mut Caller<'_, SessionState>,
    ptr: i32,
    len: i32,
) -> Result<HostYield> {
    caller.data().check_deadline()?;
    if !caller.data().streaming_active {
        return Err(RuntimeError::new("stream_not_active"));
    }
    let pointer = usize::try_from(ptr).map_err(|_| RuntimeError::new("invalid_pointer"))?;
    let length = usize::try_from(len).map_err(|_| RuntimeError::new("invalid_request_size"))?;
    if length > caller.data().budgets.max_stream_chunk_bytes {
        return Err(RuntimeError::new("stream_chunk_limit"));
    }
    if caller.data().turn_chunks >= caller.data().budgets.max_stream_chunks {
        return Err(RuntimeError::new("stream_total_limit"));
    }
    let next_total = caller
        .data()
        .turn_stream_bytes
        .checked_add(length)
        .ok_or_else(|| RuntimeError::new("stream_total_limit"))?;
    if next_total > caller.data().budgets.max_stream_total_bytes {
        return Err(RuntimeError::new("stream_total_limit"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| RuntimeError::new("missing_memory"))?;
    let memory_size = memory.data(&*caller).len();
    if pointer
        .checked_add(length)
        .is_none_or(|end| end > memory_size)
    {
        return Err(RuntimeError::new("invalid_pointer"));
    }
    let mut chunk = vec![0; length];
    memory
        .read(&*caller, pointer, &mut chunk)
        .map_err(|_| RuntimeError::new("invalid_pointer"))?;
    caller.data_mut().turn_chunks += 1;
    caller.data_mut().turn_stream_bytes = next_total;
    Ok(HostYield::StreamChunk { chunk })
}
