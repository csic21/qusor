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
    cache = here.parent / "target/native-workbench"
    native = here.parent / "target/nsh/target/debug/qusor-nsh-native-host-probe"
    cli = native.with_name("nsh")
    if not native.is_file() or not cli.is_file():
        raise SystemExit("先按 ../README.md 准备固定 NSH CLI 与 native-host")
    cache.mkdir(parents=True, exist_ok=True)
    bundle_dir = cache / "bundle"
    subprocess.run([str(cli), "build", "App.tsx", "-o", str(bundle_dir)], cwd=here, check=True)
    meta_path = bundle_dir / "app.json"
    meta = json.loads(meta_path.read_text())
    meta["window"]["defaultSize"] = {"width": width, "height": height}
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, separators=(",", ":")) + "\n")
    if args.build_only:
        return
    app = cache / "QUSOR 工作台研究.app"
    macos = app / "Contents/MacOS"
    macos.mkdir(parents=True, exist_ok=True)
    linked_host = macos / "qusor-native-host"
    # A distinct executable inode keeps this review app separate from the
    # older preview in macOS app/window discovery. Bytes stay pinned.
    host_digest = hashlib.sha256(native.read_bytes()).hexdigest()
    if linked_host.exists() and os.path.samefile(linked_host, native):
        linked_host.unlink()
    if not linked_host.exists() or hashlib.sha256(linked_host.read_bytes()).hexdigest() != host_digest:
        shutil.copy2(native, linked_host)
    launcher = macos / "qusor-workbench-study"
    if launcher.exists():
        launcher.unlink()
    command = [str(linked_host), "--bundle", str(bundle_dir / "bundle.nshc"),
               "--app-id", "org.qusor.workbenchstudy", "--data-dir", str(cache / "data"),
               "--name", "QUSOR 工作台研究", "--window-size", args.size]
    log = cache / "native.log"
    launcher.write_text("#!/bin/sh\nexec " + shlex.join(command) + ' "$@" >>' + shlex.quote(str(log)) + " 2>&1\n")
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
               "fixture": "Self-authored, window-local. No Core, external agents, network or real plugin packages."}
    (cache / "artifact.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(receipt, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
