"""Build the existing reader chain for AppKit, retaining the fixed NSH baseline.

This is a platform qualification entry, not a replacement product or a mock UI.
It never installs system tools, uploads documents, or changes the upstream source.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

BASE = Path(__file__).resolve().parent
ROOT = BASE.parent.parent
SOURCE = BASE / "target/nsh"
SHA = "913fd76e1f8434ec6cdf248f4476c1ebe08af19f"
LOCK_SHA = "3495ffc4a1fd150bb583e069afcb5b4f2d389dbfb5f8b1c942279eaf9e80414b"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def normalize_helper_paths(helper):
    """Keep generated Cargo manifests reproducible after NSH emits local paths."""
    for manifest in helper.rglob("Cargo.toml"):
        def replace(match):
            path = Path(match.group(1))
            if not path.is_absolute():
                return match.group(0)
            path.relative_to(ROOT)  # Reject generated references outside this checkout.
            return "path = " + json.dumps(Path(os.path.relpath(path, manifest.parent)).as_posix())
        manifest.write_text(re.sub(r'path\s*=\s*"([^"\n]+)"', replace, manifest.read_text()))


def prepare():
    if sys.platform != "darwin":
        raise RuntimeError("AppKit qualification must run on macOS")
    head = subprocess.check_output(["git", "-C", str(SOURCE), "rev-parse", "HEAD"], text=True).strip()
    if head != SHA or digest(SOURCE / "Cargo.lock") != LOCK_SHA:
        raise RuntimeError("NSH source or lock differs from the fixed qualification baseline")
    if subprocess.check_output(["git", "-C", str(SOURCE), "status", "--porcelain", "--untracked-files=no"]):
        raise RuntimeError("NSH tracked source has changes")
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.97.1", CARGO_BUILD_JOBS="1",
               CARGO_TARGET_DIR=str(SOURCE / "target"))
    arch = "aarch64" if platform.machine() in ("arm64", "aarch64") else "x86_64"
    sys.path.insert(0, str(ROOT / "experiments/pdf-provider"))
    import prepare_runtime
    runtime = prepare_runtime.RUNTIMES[f"macos-{arch}"]
    library = ROOT / f"experiments/pdf-provider/target/pdfium-macos-{arch}" / runtime["library"]
    if digest(library) != runtime["library_sha256"]:
        raise RuntimeError("PDFium does not match the fixed macOS qualification artifact")
    sys.path.insert(0, str(BASE / "native-host"))
    import prepare as native_prepare
    target = "aarch64-apple-darwin" if platform.machine() in ("arm64", "aarch64") else "x86_64-apple-darwin"
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--manifest-path",
        str(BASE / "native-host/Cargo.toml"), "--filter-platform", target,
        "--format-version", "1", "--locked", "--offline"], env=env))
    policy = tomllib.loads((ROOT / "config/architecture.toml").read_text())["experiments"]["nsh-native-host"]
    errors = native_prepare.validate_graph(metadata, policy, ROOT)
    active = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    forbidden = [package["name"] for package in metadata["packages"] if package["id"] in active and
        (any(word in package["name"] for word in ("webkit", "javascriptcore", "soup3")) or
         "webview" in active[package["id"]]["features"])]
    if errors or forbidden:
        raise ValueError({"architecture": errors, "web_dependencies_or_features": forbidden})
    subprocess.run(["cargo", "build", "--locked", "-j1", "-p", "nsh-cli"], cwd=SOURCE, env=env, check=True)
    subprocess.run(["cargo", "build", "--manifest-path", str(BASE / "native-host/Cargo.toml"),
                    "--locked", "-j1", "--bin", "qusor-nsh-dynamic-pdf-probe"], env=env, check=True)
    for manifest, binary in (("experiments/pdf-provider/Cargo.toml", "qusor-pdf-provider-probe"),
                             ("experiments/capture-core/Cargo.toml", "qusor-capture-probe")):
        subprocess.run(["cargo", "build", "--manifest-path", str(ROOT / manifest),
                        "--locked", "-j1", "--bin", binary], env=env, check=True)
    import prepare_reader_package
    proof = prepare_reader_package.prepare()
    sys.path.insert(0, str(BASE / "dynamic-pdf"))
    # Import by path to avoid confusing this file with the Linux prepare module.
    import importlib.util
    spec = importlib.util.spec_from_file_location("qusor_dynamic_prepare", BASE / "dynamic-pdf/prepare.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    resources = BASE / "target/dynamic-pdf-resources"
    resources.mkdir(parents=True, exist_ok=True)
    for name in ("reader.package", "publisher-key.txt", "manifest.json", "author-proof.json"):
        shutil.copyfile(BASE / "target/plugin-surface-resources" / name, resources / name)
    editor_proof = module.prepare_editor_package(resources)
    helper = BASE / "dynamic-pdf/.nsh/rust"
    helper.mkdir(parents=True, exist_ok=True)
    if not (helper / "Cargo.lock").exists():
        shutil.copyfile(SOURCE / "Cargo.lock", helper / "Cargo.lock")
    cli = SOURCE / "target/debug/nsh"
    subprocess.run([str(cli), "build", str(BASE / "dynamic-pdf"), "-o",
                    str(BASE / "target/dynamic-pdf-bundle")], env=env, check=True)
    normalize_helper_paths(helper)
    drift = {name: native_prepare.registry_drift(SOURCE / "Cargo.lock", lock,
        ROOT / "experiments/pdf-provider/Cargo.lock", ROOT / "Cargo.lock",
        ROOT / "experiments/extension-package/Cargo.lock")
        for name, lock in (("probe", BASE / "native-host/Cargo.lock"), ("helper", helper / "Cargo.lock"))}
    if any(drift.values()):
        raise ValueError({"registry_lock_drift": drift})
    # Fixed synthetic fixtures only, generated after the asset-free app bundle.
    source = ROOT / "experiments/pdf-provider/fixtures/text-two-pages.pdf"
    with tempfile.TemporaryDirectory(prefix="macos-static-fixture-", dir=BASE / "target") as temporary:
        report = json.loads(subprocess.check_output([str(SOURCE / "target/debug/qusor-pdf-provider-probe"),
            "inspect", str(library), str(source), str(Path(temporary) / "render")], env=env))["report"]
    sys.path.insert(0, str(ROOT / "experiments/capture-core"))
    from pdf_adapter import neutral_snapshot
    test_resources = BASE / "target/plugin-surface-resources"
    (test_resources / "report.json").write_text(json.dumps(report))
    (test_resources / "snapshot.json").write_text(json.dumps(neutral_snapshot(report)))
    shutil.copyfile(source, test_resources / "original.pdf")
    paths = [SOURCE / "target/debug/qusor-nsh-dynamic-pdf-probe",
             BASE / "target/dynamic-pdf-bundle/bundle.nshc", resources / "reader.package",
             resources / "editor-a.package"]
    record = {"platform": "macos", "target": target, "nsh_commit": SHA, "built": True,
              "registry_lock_drift": drift, "architecture_errors": errors,
              "web_dependencies_or_features": forbidden,
              "pdfium_library_sha256": digest(library),
              "started": False, "interaction_verified": False,
              "artifacts": {str(p.relative_to(ROOT)): digest(p) for p in paths},
              "reader_author": proof, "editor_author": editor_proof}
    output = BASE / "target/macos-preparation.json"
    output.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
    print(output)
    return record


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    prepare()
