"""Build the ordinary Rust Wasm vocabulary package outside the repository from exported SDK.

This follows the isolated third-party author path:
1. Export `api`, `package`, and `qusor-sdk` into a temporary directory outside the repo.
2. Copy `plugins/vocab` into that temp directory with path dependencies pointing only at the exported SDK.
3. Verify both plugin and signer `Cargo.lock` files contain zero private Host/Core/NSH/SQLite crates.
4. Compile `plugins/vocab` for `wasm32-unknown-unknown --release --offline --locked`.
5. Sign `vocab.wasm` + `manifest.json` with `PackageBudgets::desktop_standard()`.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "experiments/capture-core"))
from export_sdk import export_sdk  # noqa: E402

SEED = "33" * 32

AUTHOR_MAIN = r'''//! SDK-only ordinary vocabulary package builder; no Host, Core, NSH or runtime dependency.
use extension_api::{Manifest, SUPPORTED_HOST_APIS};
use qusor_package_probe::package::{
    PackageBudgets, PackageFile, build_signed_with_budgets, publisher_key,
};
use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: build-vocab-plugin MANIFEST WASM_FILE SEED_HEX OUTPUT".into());
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
    let wasm = fs::read(&args[1])?;
    let budgets = PackageBudgets::desktop_standard();
    let bytes = build_signed_with_budgets(
        manifest,
        "vocab.wasm",
        vec![PackageFile {
            path: "vocab.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &seed,
        &SUPPORTED_HOST_APIS,
        &budgets,
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
name = "build-vocab-plugin"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT"
publish = false

[dependencies]
extension-api = { path = "../api" }
qusor-package-probe = { package = "extension-package", path = "../package" }
serde_json = "=1.0.149"

[lints.rust]
unsafe_code = "forbid"
"""

PLUGIN_EXPORTED_CARGO = """[workspace]
resolver = "3"

[package]
name = "qusor-vocab"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
extension-api = { path = "../api" }
qusor-sdk = { path = "../qusor-sdk" }
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.149"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = "symbols"

[lints.rust]
unsafe_code = "deny"
"""

FORBIDDEN_TOKENS = (
    "nsh",
    "learning",
    "sqlite",
    "extension-host",
    "extension-store",
    "extension-ports",
    "extension-runtime-api",
    "runtime-wasm",
    "runtime-process",
    "core-api",
    "blob-store",
    "capability-composition",
    "qusor-app",
    "qusor-android",
    "qusor-pdf",
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def toolchain_env() -> dict[str, str]:
    env = os.environ.copy()
    if "RUSTUP_TOOLCHAIN" not in env:
        toolchain_file = ROOT / "rust-toolchain.toml"
        if toolchain_file.is_file():
            channel = tomllib.loads(toolchain_file.read_text(encoding="utf-8")).get("toolchain", {}).get("channel")
            if isinstance(channel, str) and channel:
                env["RUSTUP_TOOLCHAIN"] = channel
    return env


def run(command: list[str], cwd: Path) -> str:
    result = subprocess.run(command, cwd=cwd, env=toolchain_env(), check=False, text=True, capture_output=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed: {' '.join(command)}")
    return (result.stdout + result.stderr).strip()


def verify_lockfile_isolation(lock_path: Path, label: str) -> list[str]:
    lock = lock_path.read_text(encoding="utf-8")
    names = [
        line.split('name = "', 1)[1].split('"', 1)[0]
        for line in lock.splitlines()
        if line.startswith('name = "')
    ]
    leaked = sorted(name for name in names if any(token in name for token in FORBIDDEN_TOKENS))
    if leaked:
        raise SystemExit(f"{label} Cargo.lock reached private crates: {leaked}")
    return sorted(names)


def main() -> int:
    target = HERE / "target"
    target.mkdir(exist_ok=True)
    outside = Path(tempfile.mkdtemp(prefix="qusor-vocab-sdk-"))
    try:
        sdk = outside / "sdk"
        fingerprints = export_sdk(sdk)

        plugin_dir = sdk / "vocab-plugin"
        shutil.copytree(HERE / "src", plugin_dir / "src")
        (plugin_dir / "Cargo.toml").write_text(PLUGIN_EXPORTED_CARGO, encoding="utf-8")
        run(["cargo", "generate-lockfile", "--offline"], plugin_dir)
        plugin_lock_packages = verify_lockfile_isolation(plugin_dir / "Cargo.lock", "vocab-plugin")
        plugin_lock_sha = digest((plugin_dir / "Cargo.lock").read_bytes())
        plugin_build_log = run(
            [
                "cargo",
                "build",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
                "--offline",
                "--locked",
                "--quiet",
            ],
            plugin_dir,
        )
        wasm_path = plugin_dir / "target" / "wasm32-unknown-unknown" / "release" / "qusor_vocab.wasm"
        if not wasm_path.is_file():
            raise SystemExit(f"compiled wasm artifact missing: {wasm_path}")
        wasm_bytes = wasm_path.read_bytes()

        author = sdk / "vocab-author"
        (author / "src").mkdir(parents=True)
        (author / "Cargo.toml").write_text(AUTHOR_CARGO, encoding="utf-8")
        (author / "src" / "main.rs").write_text(AUTHOR_MAIN, encoding="utf-8")
        run(["cargo", "generate-lockfile", "--offline"], author)
        author_lock_packages = verify_lockfile_isolation(author / "Cargo.lock", "vocab-author")
        author_lock_sha = digest((author / "Cargo.lock").read_bytes())
        author_build_log = run(["cargo", "build", "--offline", "--locked", "--quiet"], author)

        package_path = target / "vocab.qpk"
        signer = json.loads(
            run(
                [
                    str(author / "target" / "debug" / "build-vocab-plugin"),
                    str(HERE / "manifest.json"),
                    str(wasm_path),
                    SEED,
                    str(package_path),
                ],
                author,
            )
        )
        package = package_path.read_bytes()
        manifest = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
        proof = {
            "package": {
                "path": "plugins/vocab/target/vocab.qpk",
                "sha256": digest(package),
                "bytes": len(package),
                "manifest_sha256": digest((HERE / "manifest.json").read_bytes()),
                "wasm_sha256": digest(wasm_bytes),
                "wasm_bytes": len(wasm_bytes),
                "package_id": manifest["package_id"],
                "host_api": manifest["host_api"],
                "required_capabilities": manifest["required_capabilities"],
                "contributions": manifest["contributions"],
            },
            "publisher_key": signer["publisher_key"],
            "sdk_fingerprints": fingerprints,
            "plugin_lock_sha256": plugin_lock_sha,
            "plugin_lock_packages": plugin_lock_packages,
            "author_lock_sha256": author_lock_sha,
            "author_lock_packages": author_lock_packages,
            "plugin_manifest_dependencies": {
                "extension-api": "../api",
                "qusor-sdk": "../qusor-sdk",
                "serde": "=1.0.228",
                "serde_json": "=1.0.149",
            },
            "author_manifest_dependencies": {
                "extension-api": "../api",
                "qusor-package-probe": "../package (extension-package)",
                "serde_json": "=1.0.149",
            },
            "build": {
                "outside": str(outside),
                "cargo": run(["cargo", "--version"], author),
                "target": "wasm32-unknown-unknown",
                "profile": "release",
                "generate_lock": "--offline",
                "plugin_build": "--target wasm32-unknown-unknown --release --offline --locked --quiet",
                "author_build": "--offline --locked --quiet",
                "plugin_log": plugin_build_log,
                "author_log": author_build_log,
                "seed": "fixed public test seed (not a production key)",
            },
            "not_verified": [
                "production publisher key management",
            ],
        }
        (target / "author-proof.json").write_text(
            json.dumps(proof, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        print(
            json.dumps(
                {
                    "package": proof["package"]["path"],
                    "sha256": proof["package"]["sha256"],
                    "wasm_bytes": len(wasm_bytes),
                    "package_bytes": len(package),
                    "publisher_key": proof["publisher_key"],
                    "proof": "plugins/vocab/target/author-proof.json",
                },
                indent=2,
            )
        )
        return 0
    finally:
        shutil.rmtree(outside, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
