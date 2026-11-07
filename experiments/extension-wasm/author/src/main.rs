//! Independent experiment author tool: no QUSOR crate or parent workspace required.
use std::{env, error::Error, fs, path::Path, process::ExitCode};

use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("build-plugin: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("usage: build-plugin <source.wat> <manifest.json> <output-dir>".into());
    }
    build(
        Path::new(&arguments[0]),
        Path::new(&arguments[1]),
        Path::new(&arguments[2]),
    )
}

fn build(source: &Path, manifest_path: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    // Finish parsing both inputs before creating any package files. This compiler
    // produces bytes; the runner remains responsible for validating the Wasm ABI,
    // capability manifest, package limits and digest before instantiation.
    let module = wat::parse_file(source)?;
    let manifest = fs::read(manifest_path)?;
    let _: serde_json::Value = serde_json::from_slice(&manifest)?;
    let digest = format!("{:x}", Sha256::digest(&module));
    let descriptor = serde_json::json!({
        "format_version": 1,
        "entry": "module.wasm",
        "sha256": digest,
    });
    let descriptor = serde_json::to_vec_pretty(&descriptor)?;

    fs::create_dir_all(output)?;
    fs::write(output.join("module.wasm"), &module)?;
    // Preserve the submitted manifest bytes. In particular, do not normalize
    // duplicate JSON fields into an apparently valid manifest before host checks.
    fs::write(output.join("manifest.json"), &manifest)?;
    fs::write(output.join("package.json"), descriptor)?;
    println!(
        "Built {} ({} Wasm bytes, sha256 {digest})",
        output.display(),
        module.len()
    );
    Ok(())
}
