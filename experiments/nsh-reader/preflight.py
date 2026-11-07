#!/usr/bin/env python3
"""Read-only Linux prerequisites; optional offline Cargo check. No installs or network."""

from __future__ import annotations

import argparse
import ctypes
import ctypes.util
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys

NSH_SHA = "913fd76e1f8434ec6cdf248f4476c1ebe08af19f"
RUST_TOOLCHAIN = "1.97.1"
BASE = Path(__file__).resolve().parent
SOURCE_FILES = (
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
    "crates/nsh-platform-linux/Cargo.toml",
    "examples/rust-exports/nsh.json", "examples/rust-exports/App.tsx",
)
# Fixed docs/dev/linux.md and platform manifests, not guessed from OS version.
PKG_REQUIREMENTS = (
    ("gtk4", "4.14"), ("pango", "1.50"), ("pangocairo", None),
    ("cairo", None), ("graphene-gobject-1.0", None),
    ("gstreamer-1.0", None), ("gstreamer-app-1.0", None),
    ("gstreamer-audio-1.0", None), ("webkitgtk-6.0", "2.40"),
)


def run(argv: list[str], cwd: Path | None = None, timeout: int = 15) -> dict:
    """Capture evidence; a timed-out Cargo check also stops its compiler children."""
    try:
        process = subprocess.Popen(
            argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", errors="replace", start_new_session=os.name == "posix",
        )
    except OSError as exc:
        return {"command": argv, "exit_code": None, "error": str(exc)}
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return {"command": argv, "exit_code": process.returncode,
                "stdout": stdout.strip(), "stderr": stderr.strip()}
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        stdout, stderr = process.communicate()
        return {"command": argv, "exit_code": None, "timed_out": True,
                "timeout_seconds": timeout, "stdout": stdout.strip(), "stderr": stderr.strip()}


def digest(path: Path) -> str | None:
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def probe_libclang() -> dict:
    """Load the Linux C API, without treating the clang executable as its library.

    An explicit LIBCLANG_PATH must work itself: no fallback hides a stale override.
    Its value and loader exception text are deliberately excluded from evidence.
    """
    if not sys.platform.startswith("linux"):
        return {"usable": False, "status": "unknown", "reason": "Linux probe only"}
    override = os.environ.get("LIBCLANG_PATH")
    candidates: list[str] = []
    source = "LIBCLANG_PATH" if override is not None else "Linux system library search"
    try:
        if override is not None:
            path = Path(override).expanduser() if override else None
            if path is not None and path.is_file():
                candidates.append(str(path))
            elif path is not None and path.is_dir():
                candidates.extend(str(item) for item in sorted(path.glob("libclang*.so*"))
                                  if "libclang-cpp" not in item.name and item.is_file())
        else:
            found = ctypes.util.find_library("clang")
            if found:
                candidates.append(found)
            # Debian/Ubuntu multilib and LLVM layouts; custom installs use LIBCLANG_PATH.
            for root in (Path("/usr/lib"), Path("/lib"), Path("/usr/local/lib")):
                for pattern in ("libclang*.so*", "*-linux-gnu/libclang*.so*", "llvm-*/lib/libclang*.so*"):
                    candidates.extend(str(item) for item in sorted(root.glob(pattern))
                                      if "libclang-cpp" not in item.name and item.is_file())
    except OSError:
        return {"usable": False, "status": "unknown", "source": source,
                "reason": "Library search failed; override paths are redacted"}

    attempts = []
    for candidate in dict.fromkeys(candidates):
        label = "<LIBCLANG_PATH candidate>" if override is not None else Path(candidate).name
        try:
            library = ctypes.CDLL(candidate)
            for symbol in ("clang_getClangVersion", "clang_getCString", "clang_disposeString",
                           "clang_createIndex", "clang_disposeIndex"):
                getattr(library, symbol)
        except (OSError, AttributeError):
            attempts.append({"library": label, "status": "load_or_required_symbol_failed"})
            continue

        class CXString(ctypes.Structure):
            _fields_ = [("data", ctypes.c_void_p), ("private_flags", ctypes.c_uint)]

        library.clang_getClangVersion.restype = CXString
        library.clang_getClangVersion.argtypes = []
        library.clang_getCString.argtypes = [CXString]
        library.clang_getCString.restype = ctypes.c_char_p
        library.clang_disposeString.argtypes = [CXString]
        library.clang_disposeString.restype = None
        version_string = library.clang_getClangVersion()
        try:
            version_bytes = library.clang_getCString(version_string)
            version = version_bytes.decode("utf-8", errors="replace") if version_bytes else None
        finally:
            library.clang_disposeString(version_string)
        return {"usable": True, "status": "loaded", "source": source, "library": label,
                "version": version, "required_symbols_checked": True,
                "bindgen_generation_verified": False, "previous_attempts": attempts}
    return {"usable": False, "status": "unavailable", "source": source,
            "reason": "No loadable library with the required libclang C API was found",
            "attempts": attempts}


def collect(source: Path, cargo_check: bool) -> dict:
    blockers: list[str] = []
    linux = sys.platform.startswith("linux")
    if not linux:
        blockers.append("This probe checks Linux only; run it inside Linux/WSL.")
    tools = {name: shutil.which(name) for name in ("git", "rustup", "clang", "cc", "pkg-config")}
    blockers.extend(f"Required tool is not on PATH: {name}" for name, path in tools.items() if not path)

    revision = run(["git", "-C", str(source), "rev-parse", "HEAD"])
    cleanliness = run(["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"])
    source_ok = revision["exit_code"] == 0 and revision.get("stdout") == NSH_SHA
    source_ok = source_ok and cleanliness["exit_code"] == 0 and not cleanliness.get("stdout")
    if not source_ok:
        blockers.append("NSH source must be at the fixed SHA with no tracked edits.")
    source_hashes = {name: digest(source / name) for name in SOURCE_FILES}
    if not all(source_hashes.values()):
        source_ok = False
        blockers.append("Required pinned NSH source files are missing.")

    # rustup run fails if absent; it does not auto-install a toolchain or mobile targets.
    rust = run(["rustup", "run", RUST_TOOLCHAIN, "rustc", "-Vv"])
    cargo = run(["rustup", "run", RUST_TOOLCHAIN, "cargo", "-V"])
    for name, result in (("Rust", rust), ("Cargo", cargo)):
        if result["exit_code"] != 0:
            blockers.append(f"Installed {name} {RUST_TOOLCHAIN} is unavailable; no install attempted.")

    libclang = probe_libclang()
    if not libclang["usable"]:
        blockers.append("Loadable libclang C API could not be verified; clang executable alone is insufficient.")

    packages = {}
    for package, minimum in PKG_REQUIREMENTS:
        version = run(["pkg-config", "--modversion", package])
        check = run(["pkg-config", f"--atleast-version={minimum}", package]) if minimum else None
        usable = version["exit_code"] == 0 and (check is None or check["exit_code"] == 0)
        packages[package] = {"minimum": minimum, "version": version,
                             "minimum_check": check, "usable": usable}
        if not usable:
            blockers.append(f"Missing or too old native development package: {package}"
                            + (f" >= {minimum}" if minimum else ""))

    display = {name: os.environ.get(name) for name in ("DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR")}
    display["wslg_present"] = Path("/mnt/wslg").is_dir()
    display["x11_socket_present"] = Path("/tmp/.X11-unix/X0").exists()
    runtime, wayland = display["XDG_RUNTIME_DIR"], display["WAYLAND_DISPLAY"]
    display["wayland_socket_present"] = bool(runtime and wayland and (Path(runtime) / wayland).exists())
    display["connection_tested"] = False
    if not (display["DISPLAY"] or display["WAYLAND_DISPLAY"]):
        blockers.append("No display advertised; native startup needs a real or explicitly recorded virtual display.")

    compile_check: dict = {"status": "not_run"}
    if cargo_check and source_ok and linux and rust["exit_code"] == 0 and cargo["exit_code"] == 0:
        command = ["rustup", "run", RUST_TOOLCHAIN, "cargo", "check", "-p", "nsh-platform-linux",
                   "--no-default-features", "--features", "js,webview", "--locked", "--offline"]
        result = run(command, cwd=source, timeout=45)
        compile_check = {"status": "passed" if result["exit_code"] == 0 else "failed", **result}
        if result["exit_code"] != 0:
            blockers.append("Offline Cargo check failed; recorded output identifies the first failure.")
    elif cargo_check:
        compile_check = {"status": "skipped", "reason": "Requires fixed clean source, Linux and installed toolchain."}

    os_release = Path("/etc/os-release")
    return {
        "schema_version": 1, "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "task_ids": ["P01", "P02"],
        "status": "blocked" if blockers else "prerequisites_ready",
        "scope": "Linux native + JS + WebView prerequisites; not reading or platform acceptance",
        "system": {
            "system": platform.system(), "kernel": platform.release(), "machine": platform.machine(),
            "os_release": os_release.read_text(encoding="utf-8") if os_release.is_file() else None,
            "logical_cpu_count": os.cpu_count(),
        },
        "source": {"directory": str(source), "expected_sha": NSH_SHA, "revision": revision,
                   "tracked_cleanliness": cleanliness, "sha256": source_hashes},
        "tools": tools, "rust": rust, "cargo": cargo, "libclang": libclang,
        "native_packages": packages, "display": display,
        "fixture": {"path": "fixtures/selection.txt", "sha256": digest(BASE / "fixtures/selection.txt"),
                    "status": "prepared_only_not_rendered"},
        "cargo_check": compile_check, "blockers": blockers,
        "unverified": ["full application compile/link", "package/install", "window startup",
                       "Rust/TSX round trip", "IME", "PDF/EPUB rendering", "selection/anchors",
                       "offline subresources/bridge isolation", "close/reopen recovery", "other platforms"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=BASE / "target/nsh")
    parser.add_argument("--output", type=Path, help="Optional JSON evidence file; stdout always receives the report.")
    parser.add_argument("--cargo-check", action="store_true", help="Attempt bounded offline check in pinned upstream workspace.")
    args = parser.parse_args()
    report = collect(args.source.resolve(), args.cargo_check)
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return 1 if report["blockers"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
