"""Build the fixed upstream Rust+TSX example with a native-only Linux host.

This is a P02 bridge probe, not the QUSOR reading application or plugin SDK.
It writes only ignored build/cache state and checks the pinned source/lock.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

BASE = Path(__file__).resolve().parent
READER = BASE.parent
ROOT = READER.parent.parent
SOURCE = READER / "target/nsh"
SHA = "913fd76e1f8434ec6cdf248f4476c1ebe08af19f"
LOCK_SHA = "3495ffc4a1fd150bb583e069afcb5b4f2d389dbfb5f8b1c942279eaf9e80414b"
sys.path.insert(0, str(READER))
from native_prefix import runtime_environment
sys.path.insert(0, str(ROOT / "scripts"))
from check_architecture import validate_graph


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, env, log, cwd=None):
    before = time.monotonic()
    with log.open("w") as output:
        result = subprocess.run(command, env=env, stdout=output, stderr=subprocess.STDOUT, cwd=cwd)
    record = {"command": command, "exit_code": result.returncode,
              "seconds": round(time.monotonic() - before, 3), "log": str(log.relative_to(READER)),
              "log_sha256": sha(log)}
    if result.returncode:
        raise RuntimeError(json.dumps(record))
    return record


def registry_drift(reference, candidate, supplemental=None, additional=None, package_baseline=None):
    def records(path):
        return {(p["name"], p["version"], p.get("source")): p.get("checksum")
                for p in tomllib.loads(path.read_text())["package"] if p.get("source", "").startswith("registry+")}
    original = records(reference)
    if supplemental is not None:
        original.update(records(supplemental))
    if additional is not None:
        original.update(records(additional))
    if package_baseline is not None:
        original.update(records(package_baseline))
    return [key for key, checksum in records(candidate).items() if original.get(key) != checksum]


def prepare(*, native_binary=None):
    if subprocess.check_output(["git", "-C", str(SOURCE), "rev-parse", "HEAD"], text=True).strip() != SHA:
        raise ValueError("NSH source revision mismatch")
    if subprocess.check_output(["git", "-C", str(SOURCE), "status", "--porcelain", "--untracked-files=no"], text=True).strip():
        raise ValueError("NSH tracked source has changes")
    if sha(SOURCE / "Cargo.lock") != LOCK_SHA:
        raise ValueError("NSH lockfile mismatch")
    env = runtime_environment(READER / "target/native-prefix")
    env.update(RUSTUP_TOOLCHAIN="1.97.1", CARGO_NET_OFFLINE="true",
               CARGO_TARGET_DIR=str(SOURCE / "target"))
    # Native links are large; preserve an explicit caller override, otherwise
    # avoid simultaneous linker working sets exhausting this validation host.
    env.setdefault("CARGO_BUILD_JOBS", "1")
    helper = SOURCE / "examples/rust-exports/.nsh/rust"
    helper.mkdir(parents=True, exist_ok=True)
    if not (helper / "Cargo.lock").exists():
        shutil.copyfile(SOURCE / "Cargo.lock", helper / "Cargo.lock")
    logs = READER / "target/native-prefix-downloads"
    logs.mkdir(parents=True, exist_ok=True)
    metadata = subprocess.check_output(["cargo", "metadata", "--manifest-path", str(BASE / "Cargo.toml"),
        "--filter-platform", "x86_64-unknown-linux-gnu", "--format-version", "1", "--locked", "--offline"], env=env)
    metadata = json.loads(metadata)
    policy = tomllib.loads((ROOT / "config/architecture.toml").read_text())["experiments"]["nsh-native-host"]
    errors = validate_graph(metadata, policy, ROOT)
    active = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    forbidden = [p["name"] for p in metadata["packages"] if p["id"] in active and
                 any(word in p["name"] for word in ["webkit", "javascriptcore", "soup3"])]
    webview = [p["name"] for p in metadata["packages"] if p["id"] in active and
               "webview" in active[p["id"]]["features"]]
    if errors or forbidden or webview:
        raise ValueError({"architecture": errors, "web_dependencies": forbidden, "webview_features": webview})
    build = ["cargo", "build", "--manifest-path", str(BASE / "Cargo.toml"), "--locked", "--offline"]
    if native_binary is not None:
        package = next(p for p in metadata["packages"] if Path(p["manifest_path"]) == BASE / "Cargo.toml")
        binaries = {t["name"] for t in package["targets"] if "bin" in t["kind"]}
        if native_binary not in binaries:
            raise ValueError("requested native binary is not declared in the checked manifest")
        build.extend(["--bin", native_binary])
    commands = [run(["cargo", "build", "-p", "nsh-cli", "--locked", "--offline"],
                    env, logs / "native-probe-cli-build.log", cwd=SOURCE),
                run(build,
                    env, logs / "native-probe-build.log"),
                run([str(SOURCE / "target/debug/nsh"), "build", str(SOURCE / "examples/rust-exports"),
                     "-o", str(READER / "target/native-bundle"), "--verbose"],
                    env, logs / "native-probe-bundle.log")]
    drift = {"probe": registry_drift(SOURCE / "Cargo.lock", BASE / "Cargo.lock", ROOT / "experiments/pdf-provider/Cargo.lock", ROOT / "Cargo.lock", ROOT / "experiments/extension-package/Cargo.lock"),
             "helper": registry_drift(SOURCE / "Cargo.lock", helper / "Cargo.lock", ROOT / "experiments/pdf-provider/Cargo.lock", ROOT / "Cargo.lock", ROOT / "experiments/extension-package/Cargo.lock")}
    if any(drift.values()):
        raise ValueError({"registry_lock_drift": drift})
    return {"registry_lock_drift": drift, "supplemental_provider_lock_sha256":sha(ROOT / "experiments/pdf-provider/Cargo.lock"), "supplemental_root_lock_sha256":sha(ROOT / "Cargo.lock"), "supplemental_package_lock_sha256":sha(ROOT / "experiments/extension-package/Cargo.lock"), "source_sha": SHA, "source_lock_sha256": LOCK_SHA,
            "probe_lock_sha256": sha(BASE / "Cargo.lock"), "commands": commands,
            "web_dependencies": forbidden, "webview_features": webview,
            "native_binary": native_binary or "all declared binaries",
            "build_jobs": env["CARGO_BUILD_JOBS"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = prepare()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
