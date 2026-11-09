#!/usr/bin/env python3
"""Build a UI-only study using the existing, isolated NSH native host."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shlex
import subprocess


def bundle_hash(path: Path) -> str:
    digest = hashlib.sha256()
    for item in sorted(path.rglob("*")):
        if item.is_file():
            digest.update(item.relative_to(path).as_posix().encode() + b"\0")
            digest.update(item.read_bytes())
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--size", default="1240x820")
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    experiment = here.parent
    cache = experiment / "target/native-design"
    cache.mkdir(parents=True, exist_ok=True)
    native = experiment / "target/nsh/target/debug/qusor-nsh-native-host-probe"
    cli = native.with_name("nsh")
    if not native.is_file() or not cli.is_file():
        raise SystemExit("先按 ../README.md 准备固定 NSH CLI 与默认 native-host；本脚本不安装或构建 Rust。")
    bundle_dir = cache / "bundle"
    subprocess.run([str(cli), "build", "App.tsx", "-o", str(bundle_dir)], cwd=here, check=True)
    bundle = bundle_dir / "bundle.nshc"
    try:
        width, height = (int(part) for part in args.size.split("x"))
        if width < 360 or height < 600:
            raise ValueError()
    except ValueError:
        raise SystemExit("--size 使用宽x高，最小 360x600。") from None
    # The bundle's launch metadata overrides the host's size default. Keep the
    # requested review profile in the actual companion metadata as well.
    metadata_path = bundle_dir / "app.json"
    metadata = json.loads(metadata_path.read_text())
    metadata["window"]["defaultSize"] = {"width": width, "height": height}
    metadata_path.write_text(json.dumps(metadata, ensure_ascii=False, separators=(",", ":")) + "\n")
    if args.build_only:
        return
    app = cache / "QUSOR 原生设计.app"
    macos = app / "Contents/MacOS"
    macos.mkdir(parents=True, exist_ok=True)
    native_executable = macos / "qusor-native-host"
    if native_executable.exists() and not os.path.samefile(native_executable, native):
        native_executable.unlink()
    if not native_executable.exists():
        os.link(native, native_executable)
    executable = macos / "qusor-native-design"
    plist = {"CFBundleExecutable": executable.name, "CFBundleIdentifier": "org.qusor.designpreview",
             "CFBundleName": "QUSOR 原生设计", "CFBundleDisplayName": "QUSOR 原生设计",
             "CFBundlePackageType": "APPL", "CFBundleVersion": "1", "NSHighResolutionCapable": True}
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(plist))
    command = [str(native_executable), "--bundle", str(bundle), "--app-id", plist["CFBundleIdentifier"],
               "--data-dir", str(cache / "data"), "--name", plist["CFBundleName"],
               "--window-size", args.size]
    # Launch Services and a double click must supply the same frozen bundle.
    # Unlink an earlier hard link before writing; never truncate the host inode.
    if executable.exists():
        executable.unlink()
    executable.write_text("#!/bin/sh\nexec " + shlex.join(command) + ' "$@"\n')
    executable.chmod(0o755)
    receipt = {"command": command, "launcher": str(executable),
               "bundle_tree_sha256": bundle_hash(bundle_dir),
               "host_sha256": hashlib.sha256(native.read_bytes()).hexdigest(),
               "fixture": "Synthetic content only; UI state is not persisted."}
    with (cache / "native.log").open("wb") as log:
        process = subprocess.Popen([str(executable)], cwd=here, stdin=subprocess.DEVNULL,
                                   stdout=log, stderr=log, start_new_session=True)
    receipt["pid"] = process.pid
    (cache / "launch.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"pid": process.pid, "app": str(app), "log": str(cache / "native.log")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
