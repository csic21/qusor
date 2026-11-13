#!/usr/bin/env python3
"""Add the pinned PDFium runtime to an NSH-built Android APK.

`nsh package` writes only the app's `libnsh.so`. This script inserts the
matching PDFium `.so` under `lib/<abi>/`, page-aligns the archive and signs it
again with the same debug key NSH uses by default. A production Android build
must pass its release keystore instead.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

APP = Path(__file__).resolve().parent
RUNTIMES = {
    "arm64-v8a": (
        APP / "native/assets/pdfium/android-arm64/libpdfium.so",
        "de19db1c0fb33579bca6ace903af7ccb9d03ad1d3f6c5202970d8d79d7538303",
    ),
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def android_sdk() -> Path:
    value = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if value:
        return Path(value)
    home = Path.home()
    return home / "Library/Android/sdk"


def build_tool(name: str) -> Path:
    versions = sorted((android_sdk() / "build-tools").iterdir(), reverse=True)
    for version in versions:
        tool = version / name
        if tool.is_file():
            return tool
    raise SystemExit(f"Android build-tools has no {name}")


def repack(apk: Path, output: Path, native: Path, abi: str) -> None:
    entry = f"lib/{abi}/libpdfium.so"
    with zipfile.ZipFile(apk, "r") as source:
        names = set(source.namelist())
        if f"lib/{abi}/libnsh.so" not in names:
            raise SystemExit(f"{apk} has no lib/{abi}/libnsh.so")
        if entry in names:
            raise SystemExit(f"{apk} already contains {entry}")
        with zipfile.ZipFile(output, "w", allowZip64=True) as target:
            for info in source.infolist():
                data = source.read(info.filename)
                copied = zipfile.ZipInfo(info.filename, info.date_time)
                copied.compress_type = info.compress_type
                copied.external_attr = info.external_attr
                copied.create_system = info.create_system
                copied.comment = info.comment
                target.writestr(copied, data)
            stored = zipfile.ZipInfo(entry, (1981, 1, 1, 0, 0, 0))
            stored.compress_type = zipfile.ZIP_STORED
            stored.external_attr = 0o100644 << 16
            target.writestr(stored, native.read_bytes())


def run(*args: str | Path) -> None:
    subprocess.run([str(arg) for arg in args], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--abi", default="arm64-v8a", choices=sorted(RUNTIMES))
    parser.add_argument("--keystore", type=Path)
    parser.add_argument("--alias", default="androiddebugkey")
    parser.add_argument("--password", default="android")
    args = parser.parse_args()

    native, expected = RUNTIMES[args.abi]
    if sha256(native) != expected:
        raise SystemExit("packaged PDFium runtime does not match its pinned digest")
    if not args.apk.is_file():
        raise SystemExit(f"APK does not exist: {args.apk}")
    keystore = args.keystore or Path.home() / ".android/debug.keystore"
    if not keystore.is_file():
        raise SystemExit(f"Android key store is missing: {keystore}")

    apk = args.apk.resolve()
    with tempfile.TemporaryDirectory(prefix="qusor-apk-") as directory:
        work = Path(directory)
        unsigned = work / "unsigned.apk"
        aligned = work / "aligned.apk"
        repack(apk, unsigned, native, args.abi)
        run(build_tool("zipalign"), "-f", "-P", "16", "4", unsigned, aligned)
        run(
            build_tool("apksigner"),
            "sign",
            "--ks",
            keystore,
            "--ks-key-alias",
            args.alias,
            "--ks-pass",
            f"pass:{args.password}",
            "--key-pass",
            f"pass:{args.password}",
            aligned,
        )
        run(build_tool("zipalign"), "-c", "-P", "16", "4", aligned)
        run(build_tool("apksigner"), "verify", "--verbose", aligned)
        shutil.copyfile(aligned, apk)
    print(f"{apk}: added {args.abi} PDFium {expected}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
