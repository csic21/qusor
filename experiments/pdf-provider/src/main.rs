use qusor_pdf_provider_probe::selection::{
    self, Point, PointerOutcome, PointerSession, VersionBinding, Viewport,
};
use qusor_pdf_provider_probe::{ResolveRequest, inspect, resolve};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{env, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionScript {
    binding: VersionBinding,
    viewport: Viewport,
    operation: SelectionOperation,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionOperation {
    Hit { point: Point },
    Drag { anchor: Point, focus: Point },
    Trace { events: Vec<PointerEvent> },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PointerEvent {
    Down {
        pointer_id: u64,
        point: Point,
        binding: Option<VersionBinding>,
        viewport: Option<Viewport>,
    },
    Move {
        pointer_id: u64,
        gesture_id: Option<u64>,
        point: Point,
        binding: Option<VersionBinding>,
        viewport: Option<Viewport>,
    },
    Up {
        pointer_id: u64,
        gesture_id: Option<u64>,
        point: Point,
        binding: Option<VersionBinding>,
        viewport: Option<Viewport>,
    },
    Cancel,
}
fn selection_command(
    library: &str,
    pdf: &str,
    output: &str,
    script: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(script)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 65536 {
        return Err("selection script must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("selection script exceeds budget".into());
    }
    let script: SelectionScript = serde_json::from_slice(&bytes)?;
    if let SelectionOperation::Trace { events } = &script.operation
        && events.len() > 64
    {
        return Err("pointer event budget exceeded".into());
    }
    let report = inspect(Path::new(library), Path::new(pdf), Path::new(output))?;
    match script.operation {
        SelectionOperation::Hit { point } => Ok(
            json!({"ok":true,"hit":selection::hit_test(&report,&script.binding,&script.viewport,point)?}),
        ),
        SelectionOperation::Drag { anchor, focus } => Ok(
            json!({"ok":true,"selection":selection::drag_selection(&report,&script.binding,&script.viewport,anchor,focus)?}),
        ),
        SelectionOperation::Trace { events } => {
            let mut session = PointerSession::new(script.binding.clone(), script.viewport.clone())?;
            let mut gesture = 0;
            let mut trace = Vec::new();
            for event in events {
                let outcome = match event {
                    PointerEvent::Down {
                        pointer_id,
                        point,
                        binding,
                        viewport,
                    } => session.pointer_down(
                        &report,
                        binding.as_ref().unwrap_or(&script.binding),
                        viewport.as_ref().unwrap_or(&script.viewport),
                        pointer_id,
                        point,
                    ),
                    PointerEvent::Move {
                        pointer_id,
                        gesture_id,
                        point,
                        binding,
                        viewport,
                    } => session.pointer_move(
                        &report,
                        binding.as_ref().unwrap_or(&script.binding),
                        viewport.as_ref().unwrap_or(&script.viewport),
                        pointer_id,
                        gesture_id.unwrap_or(gesture),
                        point,
                    ),
                    PointerEvent::Up {
                        pointer_id,
                        gesture_id,
                        point,
                        binding,
                        viewport,
                    } => session.pointer_up(
                        &report,
                        binding.as_ref().unwrap_or(&script.binding),
                        viewport.as_ref().unwrap_or(&script.viewport),
                        pointer_id,
                        gesture_id.unwrap_or(gesture),
                        point,
                    ),
                    PointerEvent::Cancel => Ok(session.cancel()),
                };
                match outcome {
                    Ok(value) => {
                        if let PointerOutcome::Dragging { gesture_id, .. } = &value {
                            gesture = *gesture_id;
                        }
                        trace.push(json!({"ok":true,"outcome":value,"preview":session.preview()}));
                    }
                    Err(error) => {
                        trace.push(json!({"ok":false,"error":error,"preview":session.preview()}))
                    }
                }
            }
            Ok(json!({"ok":true,"trace":trace}))
        }
    }
}
fn run() -> Result<Value, Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("inspect") if args.len()==4 => Ok(json!({"ok":true,"report":inspect(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]))?})),
        Some("select") if args.len()==5 => selection_command(&args[1],&args[2],&args[3],&args[4]),
        Some("resolve") if args.len()==12 => {
            let report=inspect(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[11]))?;
            let selected=resolve(&report,ResolveRequest {page:args[3].parse()?,start:args[4].parse()?,end:args[5].parse()?,original_hash:&args[6],extraction_hash:&args[7],page_hash:&args[8],unit:&args[9],quote:&args[10]})?;
            Ok(json!({"ok":true,"selection":selected}))
        },
        _=>Err("usage: inspect LIBRARY PDF OUTPUTDIR; select LIBRARY PDF OUTPUTDIR SCRIPT_JSON; resolve LIBRARY PDF PAGE START END ORIGINAL_HASH EXTRACTION_HASH PAGE_TEXT_HASH UNIT EXACT_QUOTE OUTPUTDIR".into())
    }
}
fn main() {
    match run() {
        Ok(report) => println!("{report}"),
        Err(error) => {
            println!("{}", json!({"ok":false,"error":error.to_string()}));
            std::process::exit(1)
        }
    }
}
