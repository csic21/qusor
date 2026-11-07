"""Validate the Linux/WSL signed-package and durable-installation experiment.

This exports an experimental SDK from the authoritative package/wire sources.
It does not release a production SDK or validate UI, real knowledge, or mobile
distribution. The fixed signing seeds below are public test fixtures only.
"""
from __future__ import annotations

import argparse
import copy
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
SCENARIOS: list[dict] = []
PUBLIC_TEST_SEEDS = ("11" * 32, "22" * 32)
SDK_DEPENDENCIES = {"extension-api", "ed25519-dalek", "serde", "serde_json", "sha2"}


def require(condition: bool, message: str):
    if not condition:
        raise RuntimeError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args, cwd: Path = HERE, *, env=None, exits=(0,), timeout=300):
    argv = [str(arg) for arg in args]
    print("+", shlex.join(argv), flush=True)
    result = subprocess.run(argv, cwd=cwd, env=env, text=True, encoding="utf-8",
                            capture_output=True, timeout=timeout, check=False)
    CHECKS.append({"command": argv, "cwd": str(cwd), "exit": result.returncode,
                   "stdout": result.stdout, "stderr": result.stderr})
    require(result.returncode in exits, result.stdout + result.stderr)
    return result


def metadata(manifest: Path, *, env=None) -> dict:
    return json.loads(command(["cargo", "metadata", "--manifest-path", manifest,
                               "--format-version", "1", "--locked", "--offline",
                               "--all-features"], env=env).stdout)


def toml_value(value):
    """Encode only values present in these pinned, deliberately small manifests."""
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{key} = {toml_value(item)}"
                                for key, item in value.items()) + " }"
    raise TypeError(value)


def manifest_text(package: dict, dependencies: dict, *, workspace=False):
    lines = ['[workspace]', 'resolver = "3"', ""] if workspace else []
    lines += ["[package]"]
    lines += [f"{key} = {toml_value(value)}" for key, value in package.items()]
    lines += ["", "[dependencies]"]
    lines += [f"{key} = {toml_value(value)}" for key, value in dependencies.items()]
    lines += ["", "[lints.rust]", 'unsafe_code = "forbid"', ""]
    return "\n".join(lines)


def export_sdk(outside: Path):
    """Export public contract source; never copy host/store/runner implementation."""
    parent = tomllib.loads((HERE / "Cargo.toml").read_text(encoding="utf-8"))
    sdk_dependencies = {name: copy.deepcopy(value)
                        for name, value in parent["dependencies"].items()
                        if name in SDK_DEPENDENCIES}
    require(set(sdk_dependencies) == SDK_DEPENDENCIES, "SDK dependency export is incomplete")
    sdk_dependencies["extension-api"] = {"path": "api"}
    (outside / "src").mkdir()
    shutil.copy2(HERE / "src/package.rs", outside / "src/package.rs")
    (outside / "src/lib.rs").write_text("pub mod package;\n", encoding="utf-8")
    (outside / "Cargo.toml").write_text(
        manifest_text(parent["package"], sdk_dependencies, workspace=True), encoding="utf-8")

    # Expand workspace inheritance from the authoritative API manifest; there is
    # no second handwritten wire schema and no reference to the original tree.
    api_source = ROOT / "crates/extension-api"
    api = tomllib.loads((api_source / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    api_package = {
        name: workspace["package"][name] if isinstance(value, dict) and value.get("workspace")
        else value for name, value in api["package"].items()
    }
    shutil.copytree(api_source / "src", outside / "api/src")
    (outside / "api/Cargo.toml").write_text(
        manifest_text(api_package, api["dependencies"]), encoding="utf-8")

    shutil.copytree(HERE / "author", outside / "author",
                    ignore=shutil.ignore_patterns("target", "Cargo.lock", "__pycache__"))
    author = tomllib.loads((HERE / "author/Cargo.toml").read_text(encoding="utf-8"))
    author["dependencies"]["extension-api"] = {"path": "../api"}
    (outside / "author/Cargo.toml").write_text(
        manifest_text(author["package"], author["dependencies"], workspace=True), encoding="utf-8")
    require(not (outside / "src/store.rs").exists(), "installer source leaked into author SDK")
    require(not (outside / "src/main.rs").exists(), "host source leaked into author SDK")
    for path in (outside / "src/package.rs", outside / "api/src/lib.rs"):
        source = HERE / "src/package.rs" if path.name == "package.rs" else api_source / "src/lib.rs"
        require(digest(path) == digest(source), "exported contract differs from authority")
    # The author receives these data/source fixtures with the SDK, not repository
    # access. Its signed packages remain ordinary identities throughout.
    fixtures = outside / "fixtures"
    fixtures.mkdir()
    for name in ("valid.wat", "valid-v2.wat", "manifest.json", "manifest-v2.json"):
        shutil.copy2(ROOT / "experiments/extension-wasm/fixtures" / name, fixtures / name)
    (fixtures / "asset.txt").write_text("Synthetic packaged asset, no user data.\n", encoding="utf-8")
    (fixtures / "test-seed.hex").write_text(PUBLIC_TEST_SEEDS[0], encoding="ascii")
    (fixtures / "stranger-seed.hex").write_text(PUBLIC_TEST_SEEDS[1], encoding="ascii")
    (outside / "README.md").write_text(
        "# Experimental package SDK\n\n"
        "This is a Linux/WSL validation artifact, not a production SDK release.\n"
        "The package contract is exported verbatim from src/package.rs; api/src\n"
        "is the authoritative extension-api wire source. Only these public\n"
        "contracts are provided. The author tool accepts a WAT module, manifest,\n"
        "asset, 32-byte hexadecimal seed file, and output package path.\n"
        "The included test seeds are PUBLIC fixtures; never use them for real signing.\n"
        "Build offline with cargo build --manifest-path author/Cargo.toml --locked --offline.\n",
        encoding="utf-8")
    return fixtures


def host_action(runner, name, args, *, ok=True, crash=False):
    started = time.monotonic()
    completed = command([runner, *args], exits=(86,) if crash else ((0,) if ok else (1,)), timeout=30)
    if crash:
        require(not completed.stdout.strip(), f"{name}: crash returned a completion receipt")
        report = {"crash_exit": 86}
    else:
        report = json.loads(completed.stdout)
        require(report["ok"] == ok, f"{name}: {report}")
    SCENARIOS.append({"name": name, "arguments": [str(arg) for arg in args],
                      "seconds": time.monotonic() - started, "exit": completed.returncode,
                      "report": report})
    print(f"{name}: " + ("injected crash" if crash else ("passed" if ok else "rejected")), flush=True)
    return report


def installation(status, package_id):
    matches = [item for item in status["installations"]
               if item["package_id"] == package_id and item["status"] != "uninstalled"]
    require(len(matches) <= 1, "more than one current installation of the same package")
    return matches[0] if matches else None


def validate_lifecycle(runner, packages, key, scratch, expected_digests, package_id, stranger_key):
    def action(name, *args, **kwargs):
        return host_action(runner, name, args, **kwargs)

    def assert_current(name, store, expected_digest, revision=None):
        state = installation(action(name, "status", store), package_id)
        if expected_digest is None:
            require(state is None, f"{name}: uncommitted/uninstalled package became current")
        else:
            require(state is not None and state["digest"] == expected_digest, f"{name}: wrong package")
            require(state["status"] == "disabled", f"{name}: installation acquired execution authority")
            if revision is not None:
                require(state["revision"] == revision, f"{name}: wrong installation revision")
        return state

    scratch.mkdir(mode=0o700)
    normal = scratch / "normal"
    first = action("install v1", "install", normal, "install-v1", 0, packages["v1"], key)["receipt"]
    require(first["status"] == "disabled" and first["revision"] == 1, "initial installation state")
    assert_current("restart after install", normal, expected_digests["v1"], 1)
    replay = action("install receipt replay", "install", normal, "install-v1", 0,
                    packages["v1"], key)["receipt"]
    require(first == replay, "same operation did not return the committed receipt")
    action("operation id conflict", "install", normal, "install-v1", 0, packages["v2"], key, ok=False)
    action("stale revision", "install", normal, "stale-update", 0, packages["v2"], key, ok=False)
    action("unknown publisher cannot replace installation", "install", normal, "stranger-update", 1,
           packages["stranger"], stranger_key, ok=False)
    assert_current("rejected changes preserve v1", normal, expected_digests["v1"], 1)
    second = action("update v2", "install", normal, "update-v2", 1, packages["v2"], key)["receipt"]
    require(second["installation_id"] == first["installation_id"], "update replaced installation identity")
    require(second["generation"] > first["generation"], "update did not invalidate old authorization generation")
    assert_current("restart after update", normal, expected_digests["v2"], 2)
    action("ordinary update cannot roll back", "install", normal, "rollback-attempt", 2,
           packages["v1"], key, ok=False)
    uninstalled = action("uninstall", "uninstall", normal, "remove", 2, package_id)["receipt"]
    require(uninstalled["status"] == "uninstalled", "uninstall did not persist tombstone")
    assert_current("restart after uninstall", normal, None)
    repeated = action("uninstall receipt replay", "uninstall", normal, "remove", 2, package_id)["receipt"]
    require(uninstalled == repeated, "uninstall receipt is not idempotent")

    action("uninstalled identity remains bound", "install", normal, "takeover-after-remove", 0,
           packages["stranger"], stranger_key, ok=False)
    reinstalled = action("reinstall creates new identity", "install", normal, "reinstall", 0,
                         packages["v1"], key)["receipt"]
    require(reinstalled["installation_id"] != first["installation_id"],
            "reinstall revived the old installation identity")
    require(reinstalled["generation"] > uninstalled["generation"],
            "reinstall reused a revoked authorization generation")
    assert_current("restart after reinstall", normal, expected_digests["v1"], reinstalled["revision"])

    points = ("before_transaction", "after_content", "before_commit", "after_commit")
    for point in points:
        store = scratch / f"fresh-{point}"
        args = ("install", store, "fresh", 0, packages["v1"], key)
        action(f"install crash {point}", *args, "--crash", point, crash=True)
        committed = point == "after_commit"
        assert_current(f"recover fresh {point}", store,
                       expected_digests["v1"] if committed else None, 1 if committed else None)
        receipt = action(f"retry fresh {point}", *args)["receipt"]
        require(receipt["revision"] == 1, f"{point}: retry duplicated installation")
        assert_current(f"restart after retry {point}", store, expected_digests["v1"], 1)

    for point in points:
        store = scratch / f"update-{point}"
        old = action(f"prepare update {point}", "install", store, "base", 0, packages["v1"], key)["receipt"]
        args = ("install", store, "next", 1, packages["v2"], key)
        action(f"update crash {point}", *args, "--crash", point, crash=True)
        committed = point == "after_commit"
        state = assert_current(f"recover update {point}", store,
                               expected_digests["v2" if committed else "v1"], 2 if committed else 1)
        require(state["installation_id"] == old["installation_id"], "crash changed install identity")
        receipt = action(f"retry update {point}", *args)["receipt"]
        require(receipt["revision"] == 2, f"{point}: retry duplicated update")
        assert_current(f"restart after update retry {point}", store, expected_digests["v2"], 2)

    for point in points:
        store = scratch / f"uninstall-{point}"
        action(f"prepare uninstall {point}", "install", store, "base", 0, packages["v1"], key)
        args = ("uninstall", store, "remove", 1, package_id)
        action(f"uninstall crash {point}", *args, "--crash", point, crash=True)
        assert_current(f"recover uninstall {point}", store,
                       None if point == "after_commit" else expected_digests["v1"])
        receipt = action(f"retry uninstall {point}", *args)["receipt"]
        require(receipt["status"] == "uninstalled" and receipt["revision"] == 2,
                f"{point}: retry did not recover uninstall tombstone")
        assert_current(f"restart after uninstall retry {point}", store, None)


def validate(report):
    policy = tomllib.loads((ROOT / "config/architecture.toml").read_text(encoding="utf-8"))
    manifests = [HERE / "Cargo.toml", HERE / "author/Cargo.toml"]
    metas = [metadata(manifest) for manifest in manifests]
    for name, meta in zip(("extension-package", "extension-package-author"), metas):
        errors = validate_graph(meta, policy["experiments"][name], ROOT)
        require(not errors, "\n".join(errors))
    root_meta = metadata(ROOT / "Cargo.toml")
    require(not validate_graph(root_meta, policy, ROOT), "production dependency policy failed")
    require(not ({"ed25519-dalek", "rusqlite", "fs2", "wasmi"} &
                 {package["name"] for package in root_meta["packages"]}),
            "experiment dependency leaked into production")
    for manifest in manifests:
        command(["cargo", "fmt", "--manifest-path", manifest, "--all", "--", "--check"])
        command(["cargo", "test", "--manifest-path", manifest, "--locked", "--offline"])
        command(["cargo", "clippy", "--manifest-path", manifest, "--all-targets", "--locked",
                 "--offline", "--", "-D", "warnings"])
    command(["cargo", "build", "--manifest-path", manifests[0], "--locked", "--offline"])
    runner = Path(metas[0]["target_directory"]) / "debug/qusor-package-probe"
    report["host_sha256_before"] = digest(runner)

    with tempfile.TemporaryDirectory(prefix="qusor-package-author-") as directory:
        outside = Path(directory)
        require(not outside.is_relative_to(ROOT), "author scratch is inside QUSOR")
        fixtures = export_sdk(outside)
        author_env = dict(os.environ, CARGO_TARGET_DIR=str(outside / "target"))
        external_manifest = outside / "author/Cargo.toml"
        command(["cargo", "generate-lockfile", "--manifest-path", external_manifest, "--offline"],
                cwd=outside, env=author_env)
        external_meta = metadata(external_manifest, env=author_env)
        for package in external_meta["packages"]:
            path = Path(package["manifest_path"]).resolve()
            require(not path.is_relative_to(ROOT), "independent author resolved QUSOR source")
            if package.get("source") is None:
                require(path.is_relative_to(outside), "author resolved unexported local source")
            require(package["name"] not in {"extension-host", "qusor-wasm-probe", "rusqlite", "fs2"},
                    "private host/runtime dependency leaked into SDK")
        command(["cargo", "build", "--manifest-path", external_manifest, "--locked", "--offline"],
                cwd=outside, env=author_env)
        report["independent_author"] = {
            "passed": True,
            "local_packages": [{"name": p["name"],
                                "manifest": str(Path(p["manifest_path"]).relative_to(outside))}
                               for p in external_meta["packages"] if p.get("source") is None],
            "public_test_keys_only": True,
            "exported_hashes": {str(p.relative_to(outside)): digest(p)
                                for p in sorted(outside.rglob("*"))
                                if p.is_file() and "target" not in p.relative_to(outside).parts
                                and p.suffix != ".hex"},
        }
        builder = outside / "target/debug/build-signed-plugin"
        packages = {name: outside / f"{name}.qpkg" for name in ("v1", "v2", "stranger")}
        keys = {}
        for name in packages:
            version2 = name != "v1"
            result = command([
                builder, fixtures / ("valid-v2.wat" if version2 else "valid.wat"),
                fixtures / ("manifest-v2.json" if version2 else "manifest.json"),
                fixtures / "asset.txt",
                fixtures / ("stranger-seed.hex" if name == "stranger" else "test-seed.hex"),
                packages[name],
            ], cwd=outside, env=author_env)
            keys[name] = result.stdout.strip()
            require(len(keys[name]) == 64, "author must report one public key")
        require(keys["v1"] == keys["v2"] != keys["stranger"], "fixture signing identities")
        key = keys["v1"]
        report["package_sha256"] = {name: digest(path) for name, path in packages.items()}
        require(report["package_sha256"]["v1"] != report["package_sha256"]["v2"],
                "independent versions produced identical package bytes")
        reports = {}
        for name in ("v1", "v2"):
            reports[name] = host_action(runner, f"verify {name}", ("verify", packages[name], key))
            executed = host_action(runner, f"execute {name}", ("probe", packages[name], key))
            expected_output = ("[excerpt v2] " if name == "v2" else "[excerpt] ") + "本地选区: Rust & Wasm."
            require(executed["code"] == "ok" and executed["result"]["output"] == expected_output,
                    f"{name}: independently authored module did not process host selection")
        original = json.loads(packages["v1"].read_text(encoding="utf-8"))
        for field in ("manifest", "module", "asset", "descriptor", "path", "signature"):
            changed = copy.deepcopy(original)
            if field == "manifest":
                changed["payload"]["manifest"]["optional_capabilities"].append("document.metadata.read")
            elif field in ("module", "asset"):
                wanted = "module.wasm" if field == "module" else "assets/sample.txt"
                record = next(f for f in changed["payload"]["files"] if f["path"] == wanted)
                raw = bytes.fromhex(record["data"])
                altered = raw[:-1] + bytes([raw[-1] ^ 1])
                record["data"] = altered.hex()
                # Also recompute the plain hash: an unsigned digest alone must
                # not be enough to bless altered module or asset bytes.
                record["sha256"] = hashlib.sha256(altered).hexdigest()
            elif field == "descriptor":
                changed["payload"]["files"][0]["mime"] = "application/octet-stream"
            elif field == "path":
                changed["payload"]["files"][0]["path"] = "../outside"
            else:
                signature = changed["signature"]["signature"]
                changed["signature"]["signature"] = ("0" if signature[0] != "0" else "1") + signature[1:]
            target = outside / f"tampered-{field}.qpkg"
            target.write_text(json.dumps(changed, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
            host_action(runner, f"reject changed {field}", ("probe", target, key), ok=False)
        host_action(runner, "reject unknown signing key", ("probe", packages["stranger"], key), ok=False)
        # The same stranger key is cryptographically valid under its own explicit
        # trust choice, proving the preceding failure is an identity check.
        host_action(runner, "verify separate explicit trust", ("verify", packages["stranger"], keys["stranger"]))
        expected_digests = {name: reports[name]["digest"] for name in ("v1", "v2")}
        validate_lifecycle(runner, packages, key, outside / "stores", expected_digests,
                           reports["v1"]["package_id"], keys["stranger"])
        report["host_sha256_after"] = digest(runner)
        require(report["host_sha256_before"] == report["host_sha256_after"],
                "host binary changed during independent builds or lifecycle scenarios")
    report["source_hashes"] = {
        str(p.relative_to(ROOT)): digest(p)
        for p in [*sorted((HERE / "src").glob("*.rs")), HERE / "Cargo.toml", HERE / "Cargo.lock",
                  HERE / "author/Cargo.toml", HERE / "author/Cargo.lock",
                  HERE / "author/src/main.rs", HERE / "validate.py"]
    }
    report["environment"] = {
        "platform": platform.platform(), "machine": platform.machine(),
        "rustc": command(["rustc", "-Vv"]).stdout,
        "os_release": Path("/etc/os-release").read_text(encoding="utf-8"),
        "mode": "debug/unoptimized; Linux/WSL local filesystem; process termination, not power-cut testing",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="write JSON evidence, including any failed check")
    options = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("this validation profile is Linux/WSL only")
    if shutil.which("cargo") is None:
        os.environ["PATH"] = str(Path.home() / ".cargo/bin") + os.pathsep + os.environ.get("PATH", "")
    report = {
        "status": "running",
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "scope": "experimental signed offline package + disabled-only SQLite installation",
        "not_validated": [
            "production SDK or installer", "real knowledge or reading",
            "grant/session restoration", "private-state migration or rollback",
            "long-lived multi-plugin supervisor", "NSH UI",
            "native Windows/macOS/mobile or store distribution", "power loss durability",
        ],
        "checks": CHECKS, "scenarios": SCENARIOS,
    }
    try:
        validate(report)
        report["status"] = "passed"
        print(f"PASS: {len(SCENARIOS)} scenarios; independent author; fixed host {report['host_sha256_after']}")
        return 0
    except Exception as error:
        report["status"] = "failed"
        report["error"] = f"{type(error).__name__}: {error}"
        print(report["error"], file=sys.stderr)
        return 1
    finally:
        if options.output:
            options.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                                      encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
