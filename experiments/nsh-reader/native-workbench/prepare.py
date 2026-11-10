#!/usr/bin/env python3
"""Prepare an isolated NSH .app for native UI review without launching it."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import subprocess
import sys


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", default="1280x820")
    parser.add_argument("--build-only", action="store_true")
    args = parser.parse_args()
    try:
        width, height = map(int, args.size.split("x"))
        if width < 900 or height < 680:
            raise ValueError()
    except ValueError:
        raise SystemExit("--size 最小为 900x680") from None
    here = Path(__file__).resolve().parent
    root = here.parents[2]
    cache = here.parent / "target/native-workbench"
    host_binary = here.parent / "target/nsh/target/debug/qusor-nsh-workbench-host"
    cli = here.parent / "target/nsh/target/debug/nsh"
    if not cli.is_file():
        raise SystemExit("先按 ../README.md 准备固定 NSH CLI 与 native-host")
    cache.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.97.1", CARGO_NET_OFFLINE="true",
               CARGO_TARGET_DIR=str(here.parent / "target/nsh/target"))
    subprocess.run(["cargo", "build", "--manifest-path", str(here.parent / "native-host/Cargo.toml"),
                    "--offline", "--bin", "qusor-nsh-workbench-host"], env=env, check=True)
    bundle_dir = cache / "bundle"
    subprocess.run([str(cli), "build", "App.tsx", "-o", str(bundle_dir)], cwd=here, env=env, check=True)
    meta_path = bundle_dir / "app.json"
    meta = json.loads(meta_path.read_text())
    meta["window"]["defaultSize"] = {"width": width, "height": height}
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, separators=(",", ":")) + "\n")
    if args.build_only:
        return
    # Ordinary signed notes package, built outside the repository from the SDK.
    subprocess.run([sys.executable, str(root / "plugins/notes/build.py")], check=True)
    proof = json.loads((root / "plugins/notes/target/author-proof.json").read_text())
    shutil.copyfile(root / "plugins/notes/target/notes.qpk", cache / "notes.package")
    (cache / "notes-key.txt").write_text(proof["publisher_key"] + "\n")
    # Seed a real Core database with Chinese saved notes for the window.
    seed = root / "experiments/capture-core/target/debug/seed_workbench_notes"
    if not seed.is_file():
        subprocess.run(["cargo", "build", "--manifest-path",
                        str(root / "experiments/capture-core/Cargo.toml"),
                        "--offline", "--bin", "seed_workbench_notes"], check=True)
    core = cache / "core"
    shutil.rmtree(core, ignore_errors=True)
    subprocess.run([str(seed), str(core), str(cache / "seed.json")], check=True)
    seed_receipt = json.loads((cache / "seed.json").read_text())
    app = cache / "QUSOR 工作台研究.app"
    macos = app / "Contents/MacOS"
    macos.mkdir(parents=True, exist_ok=True)
    linked_host = macos / "qusor-native-host"
    host_digest = hashlib.sha256(host_binary.read_bytes()).hexdigest()
    if linked_host.exists():
        linked_host.unlink()
    shutil.copy2(host_binary, linked_host)
    launcher = macos / "qusor-workbench-study"
    if launcher.exists():
        launcher.unlink()
    command = [str(linked_host), "--bundle", str(bundle_dir / "bundle.nshc"),
               "--app-id", "org.qusor.workbenchstudy", "--data-dir", str(cache / "data"),
               "--name", "QUSOR 工作台研究", "--window-size", args.size]
    log = cache / "native.log"
    exports = {
        "QUSOR_NOTES_DATA": str(core),
        "QUSOR_NOTES_PACKAGE": str(cache / "notes.package"),
        "QUSOR_NOTES_KEY": str(cache / "notes-key.txt"),
        "QUSOR_NOTES_DOCUMENT": str(seed_receipt["document_id"]),
    }
    launcher.write_text("#!/bin/sh\n" + "\n".join(f"export {key}={shlex.quote(value)}" for key, value in exports.items())
                        + "\nexec " + shlex.join(command) + ' "$@" >>' + shlex.quote(str(log)) + " 2>&1\n")
    launcher.chmod(0o755)
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": launcher.name, "CFBundleIdentifier": "org.qusor.workbenchstudy",
        "CFBundleName": "QUSOR 工作台研究", "CFBundleDisplayName": "QUSOR 工作台研究",
        "CFBundlePackageType": "APPL", "CFBundleVersion": "1", "NSHighResolutionCapable": True,
    }))
    digest = hashlib.sha256()
    for path in sorted(bundle_dir.rglob("*")):
        if path.is_file():
            digest.update(path.relative_to(bundle_dir).as_posix().encode() + b"\0")
            digest.update(path.read_bytes())
    receipt = {"app": str(app), "command": command, "size": [width, height],
               "bundle_tree_sha256": digest.hexdigest(),
               "host_sha256": host_digest,
               "notes_package_sha256": hashlib.sha256((cache / "notes.package").read_bytes()).hexdigest(),
               "notes_publisher_key": proof["publisher_key"],
               "seed": seed_receipt,
               "notes_core": str(core),
               "fixture": "Real Core notes in a sealed local data directory; ordinary signed notes package; no network."}
    (cache / "artifact.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(receipt, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
