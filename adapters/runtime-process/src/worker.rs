use crate::{
    protocol::*,
    transport::{read_frame, write_frame},
};
use extension_runtime_api::{MAX_RESPONSE_BYTES, ModuleExecutor};
use std::{
    env,
    io::{self, BufReader},
};

/// Route this before opening application stores or starting a UI. False means
/// ordinary executable mode; true means this process handled the worker protocol.
pub fn worker_main(executor: &dyn ModuleExecutor) -> io::Result<bool> {
    let args: Vec<_> = env::args().collect();
    if args.get(1).map(String::as_str) != Some("--managed-worker") {
        return Ok(false);
    }
    if args.len() != 5 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid managed worker arguments",
        ));
    }
    let parent = args[2]
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid parent PID"))?;
    let start = args[3]
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid parent start"))?;
    let run = args[4].clone();
    crate::guard_worker(parent, start, &run)?;
    let mut output = io::stdout().lock();
    let mut input = BufReader::new(io::stdin().lock());
    write_frame(
        &mut output,
        &Message::GuardReady {
            protocol_version: PROCESS_PROTOCOL,
            run_id: run.clone(),
        },
        MAX_FRAME,
    )?;
    let bytes = read_frame(&mut input, MAX_BOOT)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing process boot"))?;
    let boot: Boot = serde_json::from_slice(&bytes)?;
    if boot.protocol_version != PROCESS_PROTOCOL || boot.run_id != run {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "process boot identity mismatch",
        ));
    }
    let execution = FrozenExecution {
        module: boot.module,
        limits: boot.limits.into(),
        request_limit: boot.request_limit,
    };
    if !execution.valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frozen process execution",
        ));
    }
    write_frame(
        &mut output,
        &Message::Ready {
            run_id: run.clone(),
        },
        MAX_FRAME,
    )?;
    let mut count = 0;
    let result = executor.execute(
        &execution.module,
        &execution.limits,
        execution.request_limit,
        &mut |request: &[u8]| {
            if request.len() > execution.request_limit {
                return Err(channel_error());
            }
            count += 1;
            write_frame(
                &mut output,
                &Message::Call {
                    run_id: run.clone(),
                    call_id: count,
                    bytes: request.to_vec(),
                },
                MAX_FRAME,
            )
            .map_err(|_| channel_error())?;
            let bytes = read_frame(&mut input, MAX_FRAME)
                .map_err(|_| channel_error())?
                .ok_or_else(channel_error)?;
            let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| channel_error())?;
            if reply.run_id != run
                || reply.call_id != count
                || reply.bytes.len() > MAX_RESPONSE_BYTES
            {
                return Err(channel_error());
            }
            Ok(reply.bytes)
        },
    );
    write_frame(
        &mut output,
        &Message::Result {
            run_id: run,
            outcome: WireOutcome::from_execution(result),
        },
        MAX_FRAME,
    )?;
    Ok(true)
}
