"""Reproduce the local experiment. Run from WSL with locked Cargo caches prepared."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from check_architecture import validate_graph  # noqa: E402

CHECKS: list[dict] = []


def command(args: list[str | Path], cwd: Path = HERE, *, env=None) -> subprocess.CompletedProcess:
    argv = [str(arg) for arg in args]
    print("+", shlex.join(argv), flush=True)
    result = subprocess.run(argv, cwd=cwd, env=env, text=True, encoding="utf-8",
                            capture_output=True, timeout=300, check=False)
    CHECKS.append({"command": argv, "cwd": str(cwd), "exit": result.returncode,
                   "stdout": result.stdout, "stderr": result.stderr})
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metadata(manifest: Path, *, env=None) -> dict:
    return json.loads(command(["cargo", "metadata", "--manifest-path", manifest,
                              "--format-version", "1", "--locked", "--offline", "--all-features"],
                             env=env).stdout)


def require(condition: bool, message: str):
    if not condition:
        raise RuntimeError(message)


def production_runtime_fingerprints():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "config/architecture.toml"]
    for crate in (ROOT / "crates/extension-runtime-api", ROOT / "adapters/runtime-wasm",
                  ROOT / "crates/extension-api", ROOT / "crates/extension-host"):
        paths += [crate / "Cargo.toml", *sorted((crate / "src").rglob("*.rs")),
                  *sorted((crate / "tests").rglob("*.rs"))]
    return {str(path.relative_to(ROOT)): digest(path) for path in paths}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="write a detailed JSON evidence file")
    options = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("this validation profile is Linux/WSL only")
    production_sources = production_runtime_fingerprints()
    policy = tomllib.loads((ROOT / "config/architecture.toml").read_text())
    manifests = [HERE / "Cargo.toml", HERE / "author/Cargo.toml"]
    metas = [metadata(manifest) for manifest in manifests]
    for name, meta in zip(["extension-wasm", "extension-wasm-author"], metas):
        errors = validate_graph(meta, policy["experiments"][name], ROOT)
        require(not errors, "\n".join(errors))
    root_meta = metadata(ROOT / "Cargo.toml")
    require(not validate_graph(root_meta, policy, ROOT), "production dependency policy failed")
    # Qualified runtime infrastructure is now a root adapter. Its presence must
    # not introduce execution or platform dependencies into Core/protocol/policy.
    nodes = {node["id"]: node for node in root_meta["resolve"]["nodes"]}
    packages_by_id = {package["id"]: package for package in root_meta["packages"]}
    pure = {"learning-domain", "core-api", "learning-ports", "learning-application",
            "extension-api", "extension-host", "extension-package", "extension-runtime-api"}
    forbidden = {"wasmi", "wat", "runtime-wasm", "qusor-wasm-probe"}
    for package in root_meta["packages"]:
        if package["name"] not in pure:
            continue
        pending, closure = [package["id"]], set()
        while pending:
            package_id = pending.pop()
            if package_id in closure:
                continue
            closure.add(package_id)
            pending.extend(dep["pkg"] for dep in nodes[package_id]["deps"])
        require(not any(packages_by_id[package_id]["name"] in forbidden or
                        packages_by_id[package_id]["name"].startswith("nsh-")
                        for package_id in closure),
                f"runtime/platform dependency leaked into {package['name']}")
    command(["cargo", "fmt", "--all", "--", "--check"], cwd=ROOT)
    command(["cargo", "test", "-p", "runtime-wasm", "--locked", "--offline", "--", "--test-threads=1"], cwd=ROOT)
    command(["cargo", "clippy", "-p", "extension-runtime-api", "-p", "runtime-wasm",
             "--all-targets", "--locked", "--offline", "--", "-D", "warnings"], cwd=ROOT)
    for manifest in manifests:
        command(["cargo", "fmt", "--manifest-path", manifest, "--all", "--", "--check"])
        # Include dev targets before freezing the executable: test-only WAT shares
        # wasmparser with Wasmi, so Cargo's dev feature unification can otherwise
        # rebuild the CLI when the later integration tests run.
        command(["cargo", "build", "--manifest-path", manifest, "--all-targets", "--locked", "--offline"])
    runner = Path(metas[0]["target_directory"]) / "debug/qusor-wasm-probe"
    host_before = digest(runner)
    packages = HERE / "packages"
    packages.mkdir(exist_ok=True)
    # The author compiles fixtures in a fresh tree outside QUSOR, after the host is built.
    # Its metadata must not resolve any local dependency into the QUSOR checkout.
    with tempfile.TemporaryDirectory(prefix="qusor-independent-author-") as scratch:
        outside = Path(scratch)
        shutil.copytree(HERE / "author", outside / "author",
                        ignore=shutil.ignore_patterns("target", ".git"))
        shutil.copytree(HERE / "fixtures", outside / "fixtures")
        author_env = dict(os.environ, CARGO_TARGET_DIR=str(outside / "target"))
        external = metadata(outside / "author/Cargo.toml", env=author_env)
        for package in external["packages"]:
            require(not Path(package["manifest_path"]).is_relative_to(ROOT),
                    "independent author resolved a QUSOR path")
        command(["cargo", "build", "--manifest-path", outside / "author/Cargo.toml",
                 "--locked", "--offline"], env=author_env)
        builder = outside / "target/debug/build-plugin"
        for fixture in sorted((outside / "fixtures").glob("*.wat")):
            manifest = outside / "fixtures" / (
                "manifest-v2.json" if fixture.stem == "valid-v2" else "manifest.json")
            command([builder, fixture, manifest, packages / fixture.stem])
    for manifest in manifests:
        command(["cargo", "test", "--manifest-path", manifest, "--locked", "--offline", "--", "--test-threads=1"])
        command(["cargo", "clippy", "--manifest-path", manifest, "--all-targets",
                 "--locked", "--offline", "--", "-D", "warnings"])
    scenarios = []
    expected = {
        "valid": "ok", "valid-v2": "ok", "multiple-calls": "ok",
        "max-escaped-output": "ok", "cross-vault": "permission_denied",
        "cross-selection": "permission_denied", "unauthorized-capture": "permission_denied",
        "infinite-loop": "fuel_exhausted", "memory-grow": "vm_trap",
        "initial-memory": "instantiate_failed", "table-grow": "vm_trap",
        "deep-recursion": "vm_trap", "ambient-import": "unknown_import",
        "start-function": "invalid_module", "invalid-pointer": "invalid_pointer",
        "invalid-output-pointer": "invalid_pointer", "oversized-request": "host_buffer_limit",
        "oversized-output": "output_size_limit",
    }

    def scenario(name: str, expected_code: str, block: bool = False):
        args = [str(runner), str(packages / name)]
        if block:
            args.append("--block-host-call")
        started = time.monotonic()
        completed = subprocess.run(args, capture_output=True, text=True, encoding="utf-8",
                                   timeout=5, check=False)
        elapsed = time.monotonic() - started
        report = json.loads(completed.stdout)
        require(report["code"] == expected_code, f"{name}: {report}")
        require(report["ok"] == (expected_code == "ok"), f"{name}: inconsistent report")
        require(completed.returncode == (0 if report["ok"] else 1), f"{name}: bad exit")
        if report["ok"]:
            expected_output = ("\0" * 4096 if name == "max-escaped-output" else
                               ("[excerpt v2] " if name == "valid-v2" else "[excerpt] ") +
                               "本地选区: Rust & Wasm.")
            require(report["result"]["output"] == expected_output, f"{name}: wrong output")
        record = {"fixture": name, "blocked_host": block, "seconds": elapsed,
                  "exit": completed.returncode, "report": report}
        scenarios.append(record)
        print(f"{name}{' (blocked host)' if block else ''}: {report['code']}", flush=True)

    for name, code in expected.items():
        scenario(name, code)
    scenario("valid", "watchdog_timeout", block=True)
    scenario("valid", "ok")  # A fresh supervised invocation recovers after worker termination.
    host_after = digest(runner)
    require(host_before == host_after, "host executable changed while building/running plugins")
    require(digest(packages / "valid/module.wasm") != digest(packages / "valid-v2/module.wasm"),
            "author versions produced identical modules")
    hashed = [HERE / "Cargo.lock", HERE / "author/Cargo.lock",
              HERE / "Cargo.toml", HERE / "runner/Cargo.toml", HERE / "author/Cargo.toml",
              HERE / "validate.py", *sorted((HERE / "runner/src").glob("*.rs")),
              *sorted((HERE / "runner/tests").glob("*.rs")),
              *sorted((HERE / "author/src").glob("*.rs")),
              *sorted((HERE / "fixtures").glob("*")),
              *sorted((ROOT / "experiments/extension-package/fixtures").glob("private-*.wat"))]
    hashed += [p for p in packages.glob("*/*") if p.is_file()]
    require(production_runtime_fingerprints() == production_sources, "production runtime source changed during qualification")
    report = {
        "status": "passed", "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "environment": {"platform": platform.platform(), "machine": platform.machine(),
                        "rustc": command(["rustc", "-Vv"]).stdout,
                        "os_release": Path("/etc/os-release").read_text(),
                        "mode": "debug/unoptimized", "test_threads": 1},
        "production_runtime_sources": production_sources,
        "independent_author": True, "host_sha256_before": host_before,
        "host_sha256_after": host_after,
        "hashes": {str(p.relative_to(ROOT)): digest(p) for p in hashed},
        "checks": CHECKS, "scenarios": scenarios,
    }
    if options.output:
        options.output.parent.mkdir(parents=True, exist_ok=True)
        options.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                                  encoding="utf-8")
    print(f"PASS: {len(scenarios)} supervised scenarios; independent author; fixed host {host_after}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
