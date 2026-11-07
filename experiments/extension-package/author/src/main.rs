//! Author tool consuming the same package contract that the verifier uses.
//!
//! validate.py exports the package library and its authoritative wire crate, then
//! builds this tool outside the checkout. No installer or host sources are sent
//! to that independent author.

use std::{env, error::Error, fs, path::Path, process::ExitCode};

use extension_api::Manifest;
use qusor_package_probe::package::{PackageFile, build_signed, publisher_key};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("build-signed-plugin: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: build-signed-plugin <source.wat> <manifest.json> <asset> <seed.hex> <output.qpkg>"
                .into(),
        );
    }
    let module = wat::parse_file(Path::new(&args[0]))?;
    let manifest = Manifest::from_slice(&fs::read(&args[1])?)?;
    let asset = fs::read(&args[2])?;
    let seed_text = fs::read_to_string(&args[3])?;
    let seed_text = seed_text.trim();
    if seed_text.len() != 64 || !seed_text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("seed must be exactly 32 hexadecimal bytes".into());
    }
    let mut seed = [0u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&seed_text[index * 2..index * 2 + 2], 16)?;
    }
    let package = build_signed(
        manifest,
        "module.wasm",
        vec![
            PackageFile {
                path: "module.wasm".into(),
                mime: "application/wasm".into(),
                bytes: module,
            },
            PackageFile {
                path: "assets/sample.txt".into(),
                mime: "text/plain".into(),
                bytes: asset,
            },
        ],
        &seed,
    )?;
    fs::write(&args[4], &package)?;
    // Public key only. A seed/private key must never become package metadata.
    println!("{}", publisher_key(&seed));
    Ok(())
}
