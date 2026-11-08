//! Independent author: public capture contract + public package signing only.
use extension_api::{Capability, CapabilityRequest, HOST_API, Manifest, Resource};
use qusor_capture_probe::contract::CaptureRequest;
use qusor_capture_probe::pdf_compound::PDF_COMPOUND_PROTOCOL_VERSION;
use qusor_capture_probe::pdf_contract::PDF_PROTOCOL_VERSION;
use qusor_package_probe::package::{PackageFile, build_signed, publisher_key};
use std::{env, error::Error, ffi::OsStr, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("build-capture-plugin: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if (args.len() == 5 || args.len() == 7)
        && (args[0] == "--pdf-relay" || args[0] == "--pdf-compound-relay")
    {
        let protocol = if args[0] == "--pdf-compound-relay" {
            PDF_COMPOUND_PROTOCOL_VERSION
        } else {
            PDF_PROTOCOL_VERSION
        };
        let mut assets = Vec::new();
        let private_abi = if args.len() == 7 {
            let descriptor = fs::read(&args[5])?;
            let metadata: serde_json::Value = serde_json::from_slice(&descriptor)?;
            let schema = u32::try_from(
                metadata["private_schema"]
                    .as_u64()
                    .ok_or("private schema required")?,
            )?;
            assets.push(PackageFile {
                path: "assets/lifecycle.json".into(),
                mime: "application/json".into(),
                bytes: descriptor,
            });
            if args[6] != "-" {
                assets.push(PackageFile {
                    path: "migration.wasm".into(),
                    mime: "application/wasm".into(),
                    bytes: wat::parse_file(Path::new(&args[6]))?,
                });
            }
            format!(
                "(func(export \"alloc\")(param i32)(result i32)i32.const 32768)(func(export \"qusor_abi_version\")(result i32)i32.const 1)(func(export \"qusor_state_check\")(param i32 i32 i32)(result i32)local.get 0 i32.const {schema} i32.ne)"
            )
        } else {
            String::new()
        };
        let vault = args[1].to_str().ok_or("vault must be UTF-8")?;
        let request = CapabilityRequest {
            api_version: HOST_API,
            request_id: "read-pdf-selection".into(),
            capability: Capability::SelectionRead,
            resource: Resource::Selection {
                vault_id: vault.into(),
                selection_id: "active-pdf".into(),
            },
        };
        request.validate()?;
        let raw = serde_json::to_vec(&request)?;
        let prefix =
            format!("{{\"capture_protocol\":{protocol},\"request_id\":\"pdf-proposal\",\"draft\":");
        let escaped = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|byte| format!("\\{byte:02x}"))
                .collect::<String>()
        };
        let module = wat::parse_str(format!(
            r#"(module (import "qusor" "call" (func $call(param i32 i32 i32 i32)(result i32)))
            (memory(export "memory") 2 2)
            {private_abi}
            (data(i32.const 0)"{}") (data(i32.const 16384)"{}")
            (func(export "run")(result i32 i32)(local $n i32)
              i32.const 0 i32.const {} i32.const 4096 i32.const 4096 call $call local.set $n
              i32.const {} i32.const 4096 local.get $n memory.copy
              i32.const {} local.get $n i32.add i32.const 125 i32.store8
              i32.const 16384 local.get $n i32.const {} i32.add
              i32.const 65536 i32.const 4096 call $call local.set $n
              i32.const 65536 local.get $n))"#,
            escaped(&raw),
            escaped(prefix.as_bytes()),
            raw.len(),
            16384 + prefix.len(),
            16384 + prefix.len(),
            prefix.len() + 1
        ))?;
        return write_signed(module, &args[2], &args[3], &args[4], assets);
    }
    if args.len() != 4 && !(args.len() == 5 && args[4] == "--unchecked-test-request") {
        return Err("usage: build-capture-plugin REQUEST MANIFEST SEED_HEX OUTPUT [--unchecked-test-request]".into());
    }
    let raw = fs::read(&args[0])?;
    let request = if args.len() == 5 {
        // Host validation is mandatory even for adversarial authors; this mode
        // constructs signed negative fixtures, not a trusted-input exception.
        raw
    } else {
        CaptureRequest::from_slice(&raw)?.canonical_bytes()?
    };
    if request.len() > 64 * 1024 {
        return Err("capture request exceeds guest profile".into());
    }
    let literal: String = request.iter().map(|byte| format!("\\{byte:02x}")).collect();
    let wat = format!(
        "(module (import \"qusor\" \"call\" (func $call (param i32 i32 i32 i32) (result i32)))\n\
         (memory (export \"memory\") 2 2)\n\
         (data (i32.const 0) \"{literal}\")\n\
         (func (export \"run\") (result i32 i32) (local $n i32)\n\
         i32.const 0 i32.const {} i32.const 65536 i32.const 4096 call $call local.set $n\n\
         i32.const 65536 local.get $n))",
        request.len()
    );
    let module = wat::parse_str(&wat)?;
    write_signed(module, &args[1], &args[2], &args[3], vec![])
}
fn write_signed(
    module: Vec<u8>,
    manifest_path: &OsStr,
    seed_path: &OsStr,
    output: &OsStr,
    extra: Vec<PackageFile>,
) -> Result<(), Box<dyn Error>> {
    let manifest = Manifest::from_slice(&fs::read(manifest_path)?)?;
    let text = fs::read_to_string(seed_path)?;
    let text = text.trim();
    if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("seed must be 32 hexadecimal bytes".into());
    }
    let mut seed = [0u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    let mut files = vec![
        PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: module,
        },
        PackageFile {
            path: "assets/source-license.txt".into(),
            mime: "text/plain".into(),
            bytes: b"Synthetic capture fixtures, QUSOR MIT license.\n".to_vec(),
        },
    ];
    files.extend(extra);
    let package = build_signed(manifest, "module.wasm", files, &seed)?;
    fs::write(Path::new(output), package)?;
    println!("{}", publisher_key(&seed));
    Ok(())
}
