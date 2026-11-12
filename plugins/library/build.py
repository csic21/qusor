"""Build the ordinary Library package outside the repository from the exported SDK.

This is the author path: the SDK is exported to a temporary directory, the
signing tool is compiled there with SDK crates only, and the signed package is
copied back as a build artifact. No Host, Core, storage, NSH or runtime crate is
reachable from the author workspace.
"""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "experiments/capture-core"))
from export_sdk import export_sdk  # noqa: E402

SEED = "33" * 32
AUTHOR_MAIN = r'''//! SDK-only ordinary Library package builder; no Host, Core, NSH or runtime dependency.
use extension_api::{Manifest, SUPPORTED_HOST_APIS};
use qusor_package_probe::package::{PackageFile, build_signed_for_host, publisher_key};
use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: build-library-plugin MANIFEST GUEST_WAT SEED_HEX OUTPUT".into());
    }
    let manifest = Manifest::from_slice(&fs::read(&args[0])?)?;
    let seed_hex = &args[2];
    if seed_hex.len() != 64 {
        return Err("seed must be 32 bytes".into());
    }
    let mut seed = [0u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&seed_hex[index * 2..index * 2 + 2], 16)?;
    }
    let wasm = wat::parse_file(&args[1])?;
    let bytes = build_signed_for_host(
        manifest,
        "library.wasm",
        vec![PackageFile {
            path: "library.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &seed,
        &SUPPORTED_HOST_APIS,
    )?;
    if let Some(parent) = Path::new(&args[3]).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args[3], &bytes)?;
    println!(
        "{}",
        serde_json::json!({"publisher_key": publisher_key(&seed), "bytes": bytes.len()})
    );
    Ok(())
}
'''

AUTHOR_CARGO = """[workspace]
resolver = "3"

[package]
name = "build-library-plugin"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT"
publish = false

[dependencies]
extension-api = { path = "../api" }
qusor-package-probe = { package = "extension-package", path = "../package" }
serde_json = "=1.0.149"
wat = "=1.228.0"

[lints.rust]
unsafe_code = "forbid"
"""

FORBIDDEN = ("nsh", "learning", "sqlite", "extension-host", "runtime-wasm",
             "runtime-process", "qusor", "core-api")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(command: list[str], cwd: Path) -> str:
    result = subprocess.run(command, cwd=cwd, check=False, text=True,
                            capture_output=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed: {' '.join(command)}")
    return (result.stdout + result.stderr).strip()


def main() -> int:
    target = HERE / "target"
    target.mkdir(exist_ok=True)
    outside = Path(tempfile.mkdtemp(prefix="qusor-library-sdk-"))
    try:
        sdk = outside / "sdk"
        fingerprints = export_sdk(sdk)
        author = sdk / "library-author"
        (author / "src").mkdir(parents=True)
        (author / "Cargo.toml").write_text(AUTHOR_CARGO)
        (author / "src" / "main.rs").write_text(AUTHOR_MAIN)
        run(["cargo", "generate-lockfile", "--offline"], author)
        lock = (author / "Cargo.lock").read_text()
        names = [line.split('name = "', 1)[1].split('"', 1)[0]
                 for line in lock.splitlines() if line.startswith('name = "')]
        leaked = sorted(name for name in names
                        if any(token in name for token in FORBIDDEN))
        if leaked:
            raise SystemExit(f"author lock reached private crates: {leaked}")
        build_log = run(["cargo", "build", "--offline", "--locked", "--quiet"], author)
        package_path = target / "library.qpk"
        signer = json.loads(run(
            [str(author / "target" / "debug" / "build-library-plugin"),
             str(HERE / "manifest.json"), str(HERE / "guest.wat"),
             SEED, str(package_path)], author))
        package = package_path.read_bytes()
        manifest = json.loads((HERE / "manifest.json").read_text())
        proof = {
            "package": {
                "path": "plugins/library/target/library.qpk",
                "sha256": digest(package),
                "bytes": len(package),
                "manifest_sha256": digest((HERE / "manifest.json").read_bytes()),
                "guest_wat_sha256": digest((HERE / "guest.wat").read_bytes()),
                "package_id": manifest["package_id"],
                "host_api": manifest["host_api"],
                "required_capabilities": manifest["required_capabilities"],
                "contributions": manifest["contributions"],
            },
            "publisher_key": signer["publisher_key"],
            "sdk_fingerprints": fingerprints,
            "author_lock_sha256": digest(lock.encode()),
            "author_lock_packages": sorted(names),
            "author_manifest_dependencies": {
                "extension-api": "../api",
                "qusor-package-probe": "../package (extension-package)",
                "serde_json": "=1.0.149",
                "wat": "=1.228.0",
            },
            "build": {
                "outside": str(outside),
                "cargo": run(["cargo", "--version"], author),
                "generate_lock": "--offline",
                "build": "--offline --locked --quiet",
                "log": build_log,
                "seed": "fixed public test seed (not a production key)",
            },
            "not_verified": [
                "package store installation and host-side persistent authority",
                "workspace UI and real window",
                "production publisher key management",
            ],
        }
        (target / "author-proof.json").write_text(
            json.dumps(proof, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({
            "package": proof["package"]["path"],
            "sha256": proof["package"]["sha256"],
            "publisher_key": proof["publisher_key"],
            "proof": "plugins/library/target/author-proof.json",
        }, indent=2))
        return 0
    finally:
        shutil.rmtree(outside, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
