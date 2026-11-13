#!/usr/bin/env python3
"""Add the pinned PDFium runtime and QusorPlatform DEX to an NSH-built Android APK.

`nsh package` writes only the app's `libnsh.so` and `classes.dex`. This script
inserts the matching PDFium `.so` under `lib/<abi>/`, compiles
`apps/qusor/android/QusorPlatform.java` into `classes2.dex` when present,
page-aligns the archive and signs it with the configured keystore (or the
default debug keystore).
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
PLATFORM_JAVA = APP / "android/QusorPlatform.java"
RUNTIMES = {
    "arm64-v8a": (
        APP / "native/assets/pdfium/android-arm64/libpdfium.so",
        "de19db1c0fb33579bca6ace903af7ccb9d03ad1d3f6c5202970d8d79d7538303",
    ),
}
FILES_PROVIDER_STUB = """\
package com.longbridge.nsh;

import android.content.Context;
import android.net.Uri;
import java.io.File;

public final class FilesProvider {
    public static Uri uriFor(Context context, File file) {
        return null;
    }

    public static String typeOf(Context context, File file) {
        return null;
    }
}
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def android_sdk() -> Path:
    value = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if value:
        return Path(value)
    home = Path.home()
    return home / "Library/Android/sdk"


def build_tool(name: str) -> Path:
    build_tools_dir = android_sdk() / "build-tools"
    if not build_tools_dir.is_dir():
        raise SystemExit(f"Android build-tools directory missing: {build_tools_dir}")
    versions = sorted(build_tools_dir.iterdir(), reverse=True)
    candidates = [name, f"{name}.bat", f"{name}.exe"] if os.name == "nt" else [name]
    for version in versions:
        for candidate in candidates:
            tool = version / candidate
            if tool.is_file():
                return tool
    raise SystemExit(f"Android build-tools has no {name}")


def android_jar() -> Path:
    platforms_dir = android_sdk() / "platforms"
    if not platforms_dir.is_dir():
        raise SystemExit(f"Android platforms directory missing: {platforms_dir}")

    def api_key(path: Path) -> tuple[int, str]:
        suffix = path.name.removeprefix("android-")
        return (int(suffix) if suffix.isdigit() else -1, path.name)

    for platform in sorted(platforms_dir.iterdir(), key=api_key, reverse=True):
        jar = platform / "android.jar"
        if jar.is_file():
            return jar
    raise SystemExit(f"Android SDK platforms has no android.jar under {platforms_dir}")


def java_tool(name: str) -> Path:
    candidates: list[Path] = []
    java_home = os.environ.get("JAVA_HOME")
    if java_home:
        candidates.append(Path(java_home) / "bin" / name)
    candidates.extend(
        [
            Path("/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin") / name,
            Path("/opt/android-studio/jbr/bin") / name,
            Path("/opt/homebrew/opt/openjdk/bin") / name,
            Path("/usr/local/opt/openjdk/bin") / name,
        ]
    )
    jvm_dir = Path("/Library/Java/JavaVirtualMachines")
    if jvm_dir.is_dir():
        for vm in sorted(jvm_dir.iterdir(), reverse=True):
            candidates.append(vm / "Contents/Home/bin" / name)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    found = shutil.which(name)
    if found:
        return Path(found)
    raise SystemExit(f"JDK tool not found: {name} (set JAVA_HOME or install a JDK)")


def compile_platform_dex(java_source: Path, work: Path, min_api: int) -> Path:
    sdk_jar = android_jar()
    javac = java_tool("javac")
    d8 = build_tool("d8")

    stub_src_dir = work / "stub-src"
    stub_file = stub_src_dir / "com/longbridge/nsh/FilesProvider.java"
    stub_file.parent.mkdir(parents=True, exist_ok=True)
    stub_file.write_text(FILES_PROVIDER_STUB, encoding="utf-8")

    stub_classes = work / "stub-classes"
    stub_classes.mkdir(parents=True, exist_ok=True)
    run(
        javac,
        "-source",
        "1.8",
        "-target",
        "1.8",
        "-classpath",
        sdk_jar,
        "-d",
        stub_classes,
        stub_file,
    )

    app_classes = work / "app-classes"
    app_classes.mkdir(parents=True, exist_ok=True)
    classpath = f"{sdk_jar}{os.pathsep}{stub_classes}"
    run(
        javac,
        "-source",
        "1.8",
        "-target",
        "1.8",
        "-classpath",
        classpath,
        "-d",
        app_classes,
        java_source,
    )

    class_files = sorted(app_classes.rglob("*.class"))
    if not class_files:
        raise SystemExit(f"javac produced no .class files for {java_source}")

    dex_out = work / "dex-out"
    dex_out.mkdir(parents=True, exist_ok=True)
    run(
        d8,
        "--lib",
        sdk_jar,
        "--min-api",
        str(min_api),
        "--output",
        dex_out,
        *class_files,
    )
    classes_dex = dex_out / "classes.dex"
    if not classes_dex.is_file():
        raise SystemExit(f"d8 did not produce classes.dex in {dex_out}")
    return classes_dex


def ensure_keystore(
    keystore: Path,
    *,
    generate: bool,
    alias: str,
    password: str,
    dname: str,
    validity: int,
) -> None:
    if keystore.is_file():
        return
    if not generate:
        raise SystemExit(f"Android key store is missing: {keystore}")
    keystore.parent.mkdir(parents=True, exist_ok=True)
    run(
        java_tool("keytool"),
        "-genkeypair",
        "-v",
        "-keystore",
        keystore,
        "-storetype",
        "PKCS12",
        "-alias",
        alias,
        "-keyalg",
        "RSA",
        "-keysize",
        "2048",
        "-validity",
        str(validity),
        "-storepass",
        password,
        "-keypass",
        password,
        "-dname",
        dname,
    )


def repack(
    apk: Path,
    output: Path,
    native: Path,
    abi: str,
    classes2_dex: Path | None = None,
) -> None:
    entry = f"lib/{abi}/libpdfium.so"
    with zipfile.ZipFile(apk, "r") as source:
        names = set(source.namelist())
        if f"lib/{abi}/libnsh.so" not in names:
            raise SystemExit(f"{apk} has no lib/{abi}/libnsh.so")
        if entry in names:
            raise SystemExit(f"{apk} already contains {entry}")
        if classes2_dex is not None and "classes2.dex" in names:
            raise SystemExit(f"{apk} already contains classes2.dex")
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
            if classes2_dex is not None:
                dex_info = zipfile.ZipInfo("classes2.dex", (1981, 1, 1, 0, 0, 0))
                dex_info.compress_type = zipfile.ZIP_DEFLATED
                dex_info.external_attr = 0o100644 << 16
                target.writestr(dex_info, classes2_dex.read_bytes())


def run(*args: str | Path) -> None:
    subprocess.run([str(arg) for arg in args], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--abi", default="arm64-v8a", choices=sorted(RUNTIMES))
    parser.add_argument("--keystore", type=Path)
    parser.add_argument("--alias", default="androiddebugkey")
    parser.add_argument("--password", default="android")
    parser.add_argument(
        "--generate-keystore",
        action="store_true",
        help="Generate --keystore with keytool if it does not already exist.",
    )
    parser.add_argument(
        "--dname",
        default="CN=QUSOR, O=QUSOR, C=US",
        help="Distinguished name used when --generate-keystore creates a keystore.",
    )
    parser.add_argument(
        "--validity",
        type=int,
        default=10000,
        help="Certificate validity in days when --generate-keystore creates a keystore.",
    )
    parser.add_argument(
        "--min-api",
        type=int,
        default=26,
        help="Minimum Android API level passed to d8 for QusorPlatform.java.",
    )
    args = parser.parse_args()

    native, expected = RUNTIMES[args.abi]
    if sha256(native) != expected:
        raise SystemExit("packaged PDFium runtime does not match its pinned digest")
    if not args.apk.is_file():
        raise SystemExit(f"APK does not exist: {args.apk}")
    if args.generate_keystore and args.keystore is None:
        raise SystemExit("--generate-keystore requires --keystore <path>")

    keystore = args.keystore or Path.home() / ".android/debug.keystore"
    ensure_keystore(
        keystore,
        generate=args.generate_keystore,
        alias=args.alias,
        password=args.password,
        dname=args.dname,
        validity=args.validity,
    )

    apk = args.apk.resolve()
    with tempfile.TemporaryDirectory(prefix="qusor-apk-") as directory:
        work = Path(directory)
        classes2_dex = (
            compile_platform_dex(PLATFORM_JAVA, work, args.min_api)
            if PLATFORM_JAVA.is_file()
            else None
        )
        unsigned = work / "unsigned.apk"
        aligned = work / "aligned.apk"
        repack(apk, unsigned, native, args.abi, classes2_dex=classes2_dex)
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
    dex_note = " + classes2.dex" if PLATFORM_JAVA.is_file() else ""
    print(f"{apk}: added {args.abi} PDFium {expected}{dex_note}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
