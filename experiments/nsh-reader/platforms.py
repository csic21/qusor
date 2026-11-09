#!/usr/bin/env python3
"""Read-only QUSOR NSH six-target build/launch preflight matrix.

Plans NSH CLI commands and checks host, tool and target prerequisites. Build
and start phases stay not_run: this utility never installs, downloads, builds,
launches a host or opens a window. A planned command is not execution evidence.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform as host_platform
import shutil
import subprocess
import sys

TARGETS = ("macos", "windows", "linux", "ios", "android", "web")
BASE = Path(__file__).resolve().parent
REPO_ROOT = BASE.parents[1]
DEFAULT_APP = BASE / "dynamic-pdf"
DEFAULT_OUTPUT = BASE / "target" / "platforms"
HOST_TARGETS = {
    "darwin": "macos", "linux": "linux", "win32": "windows",
    "macos": "macos", "windows": "windows",
}
BROWSER_NAMES = (
    "google-chrome", "google-chrome-stable", "chromium", "chromium-browser",
    "msedge", "chrome",
)
TOOL_NAMES = (
    "nsh", "cargo", "rustup", "rustc", "git", "clang", "cl", "link", "cc", "gcc",
    "pkg-config", "xcrun", "xcodebuild", "xcode-select", "adb", "java", "sdkmanager",
) + BROWSER_NAMES
# Fixed Linux development packages from the pinned NSH Linux profile.
NATIVE_PACKAGES = (
    ("gtk4", "4.14"), ("pango", "1.50"),
    ("gstreamer-1.0", None), ("gstreamer-app-1.0", None),
    ("gstreamer-audio-1.0", None),
)


@dataclass(frozen=True)
class TargetSpec:
    """Canonical QUSOR target, artifact names and declared prerequisites."""

    name: str
    nsh_target: str
    hosts: tuple[str, ...]
    executable_suffix: str
    library_suffix: str
    pdfium_filename: str
    build_requirements: tuple[str, ...]
    start_requirements: tuple[str, ...]
    notes: str

    @property
    def static_library(self) -> bool:
        return self.library_suffix == ".a"

    @property
    def wasm(self) -> bool:
        return self.name == "web"

    def artifact(self) -> dict:
        return {
            "executable_suffix": self.executable_suffix,
            "library_suffix": self.library_suffix,
            "pdfium_filename": self.pdfium_filename,
            "static_library": self.static_library,
            "wasm": self.wasm,
        }

    def as_dict(self) -> dict:
        return {
            "target": self.name,
            "nsh_target": self.nsh_target,
            "hosts": list(self.hosts),
            **self.artifact(),
            "build_requirements": list(self.build_requirements),
            "start_requirements": list(self.start_requirements),
            "notes": self.notes,
        }


TARGET_SPECS = {
    "macos": TargetSpec(
        name="macos", nsh_target="macos", hosts=("macos",),
        executable_suffix="", library_suffix=".dylib",
        pdfium_filename="libpdfium.dylib",
        build_requirements=("NSH CLI", "Xcode command-line tools (xcrun)"),
        start_requirements=("NSH CLI", "macOS desktop session"),
        notes="AppKit development host; PDFium is a dynamic library.",
    ),
    "windows": TargetSpec(
        name="windows", nsh_target="windows", hosts=("windows",),
        executable_suffix=".exe", library_suffix=".dll",
        pdfium_filename="pdfium.dll",
        build_requirements=(
            "NSH CLI", "MSVC Build Tools (cl or link)", "LLVM/clang for bindgen",
        ),
        start_requirements=("NSH CLI", "Windows desktop session"),
        notes="Win32 development host; executables and PDFium use Windows suffixes.",
    ),
    "linux": TargetSpec(
        name="linux", nsh_target="linux", hosts=("linux",),
        executable_suffix="", library_suffix=".so",
        pdfium_filename="libpdfium.so",
        build_requirements=(
            "NSH CLI", "pkg-config", "GTK 4 >= 4.14",
            "GStreamer 1.x development packages",
        ),
        start_requirements=("NSH CLI", "DISPLAY or WAYLAND_DISPLAY"),
        notes="GTK 4 development host; no WebKit requirement for the native profile.",
    ),
    "ios": TargetSpec(
        name="ios", nsh_target="ios", hosts=("macos",),
        executable_suffix="", library_suffix=".a",
        pdfium_filename="libpdfium.a",
        build_requirements=(
            "macOS host", "NSH CLI", "Xcode + iOS SDK (xcodebuild/xcrun)",
        ),
        start_requirements=(
            "NSH CLI", "Xcode iOS Simulator (xcrun simctl)",
        ),
        notes="iOS requires a separately qualified native document provider; no PDFium archive is prepared by this tool.",
    ),
    "android": TargetSpec(
        name="android", nsh_target="android", hosts=("macos", "windows", "linux"),
        executable_suffix="", library_suffix=".so",
        pdfium_filename="libpdfium.so",
        build_requirements=(
            "NSH CLI", "Android SDK build-tools + platform", "JDK",
        ),
        start_requirements=("NSH CLI", "adb device or emulator"),
        notes="NDK is additionally needed for Rust crates, not for a TSX-only package.",
    ),
    "web": TargetSpec(
        name="web", nsh_target="web", hosts=("macos", "windows", "linux"),
        executable_suffix=".wasm", library_suffix=".wasm",
        pdfium_filename="pdfium.wasm",
        build_requirements=("NSH CLI", "WebAssembly runtime packaged by NSH"),
        start_requirements=("NSH CLI", "Browser for interactive use (optional)"),
        notes="Browser target; native desktop toolkit and pkg-config packages are not prerequisites.",
    ),
}


def canonical_target(value: str) -> str:
    """Return a canonical target name, rejecting anything outside the six."""
    name = str(value).strip().lower()
    if name not in TARGET_SPECS:
        raise ValueError(
            f"Unknown target: {value!r}; expected one of {', '.join(TARGETS)}"
        )
    return name


def target_spec(target: str) -> TargetSpec:
    """Return the immutable specification for one canonical target."""
    return TARGET_SPECS[canonical_target(target)]


def platform_mapping() -> dict[str, dict]:
    """Return the six-target artifact/toolchain mapping as plain data."""
    return {name: TARGET_SPECS[name].as_dict() for name in TARGETS}


def _normalize_host_system(system: str) -> str | None:
    value = str(system).lower()
    if value in HOST_TARGETS:
        return HOST_TARGETS[value]
    if value.startswith("darwin"):
        return "macos"
    if value.startswith("win"):
        return "windows"
    if value.startswith("linux"):
        return "linux"
    return None


def detect_host(system: str | None = None, machine: str | None = None) -> dict:
    """Detect the current host without recording user or environment data."""
    raw_system = sys.platform if system is None else system
    raw_machine = host_platform.machine() if machine is None else machine
    target = _normalize_host_system(raw_system)
    return {
        "system": raw_system,
        "machine": raw_machine,
        "target": target,
        "supported": target is not None,
    }


def detect_tools(
    env: dict | None = None,
    which=None,
    is_file=None,
) -> dict:
    """Detect executables by name only; no tool is executed or installed."""
    env = os.environ if env is None else env
    which = shutil.which if which is None else which
    is_file = (lambda path: Path(path).is_file()) if is_file is None else is_file
    tools = {name: which(name) for name in TOOL_NAMES}
    tools["browser"] = next(
        (tools[name] for name in BROWSER_NAMES if tools.get(name)), None
    )
    tools["nsh_source"] = "PATH" if tools.get("nsh") else None
    if not tools.get("nsh"):
        override = env.get("NSH_CLI")
        if override and is_file(override):
            tools["nsh"] = str(override)
            tools["nsh_source"] = "NSH_CLI"
        else:
            candidates = (
                BASE / "target/nsh/target/debug/nsh",
                BASE / "target/nsh/target/debug/nsh.exe",
                BASE / "target/nsh/target/release/nsh",
                BASE / "target/nsh/target/release/nsh.exe",
            )
            for candidate in candidates:
                if is_file(candidate):
                    tools["nsh"] = str(candidate)
                    tools["nsh_source"] = "repository build"
                    break
    if not tools.get("java"):
        java_home = env.get("JAVA_HOME")
        if java_home:
            for name in ("java", "java.exe"):
                candidate = Path(java_home) / "bin" / name
                if is_file(candidate):
                    tools["java"] = str(candidate)
                    break
    return tools


def detect_android_sdk(
    env: dict | None = None,
    home: Path | None = None,
    is_dir=None,
    is_file=None,
) -> dict:
    """Locate the Android SDK and its build-tools/platforms; no SDK is changed."""
    env = os.environ if env is None else env
    home = Path.home() if home is None else Path(home)
    is_dir = (lambda path: Path(path).is_dir()) if is_dir is None else is_dir
    is_file = (lambda path: Path(path).is_file()) if is_file is None else is_file
    candidates = []
    source = None
    for name in ("ANDROID_HOME", "ANDROID_SDK_ROOT"):
        value = env.get(name)
        if value:
            candidates.append(Path(value).expanduser())
            source = source or name
    system = _normalize_host_system(sys.platform)
    if system == "macos":
        candidates.append(home / "Library/Android/sdk")
    elif system == "windows":
        local = env.get("LOCALAPPDATA")
        candidates.append(
            Path(local) / "Android/Sdk" if local else home / "AppData/Local/Android/Sdk"
        )
    else:
        candidates.append(home / "Android/Sdk")
    root = next((path for path in candidates if is_dir(path)), None)
    if root is None:
        return {
            "found": False,
            "source": source or "default locations",
            "reason": (
                "Android SDK not found; install it and set ANDROID_HOME or "
                "ANDROID_SDK_ROOT (no install attempted)."
            ),
        }

    def entries(directory: Path) -> list[str]:
        if not is_dir(directory):
            return []
        try:
            return sorted(path.name for path in directory.iterdir() if is_dir(path))
        except OSError:
            return []

    build_tools = entries(root / "build-tools")
    platforms = entries(root / "platforms")
    adb = next(
        (
            root / "platform-tools" / name
            for name in ("adb", "adb.exe")
            if is_file(root / "platform-tools" / name)
        ),
        None,
    )
    ndk_home = env.get("ANDROID_NDK_HOME")
    if ndk_home and is_dir(Path(ndk_home)):
        ndk = Path(ndk_home)
    else:
        found_ndks = entries(root / "ndk")
        ndk = root / "ndk" / found_ndks[-1] if found_ndks else None
    missing = []
    if not build_tools:
        missing.append("Android SDK build-tools are missing")
    if not platforms:
        missing.append("Android SDK platform is missing")
    return {
        "found": True,
        "root": str(root),
        "source": source or "default location",
        "build_tools": build_tools,
        "platforms": platforms,
        "adb": str(adb) if adb is not None else None,
        "ndk": str(ndk) if ndk is not None else None,
        "usable": not missing,
        "reason": "; ".join(missing) if missing else None,
    }


def _run(argv: list[str], timeout: int = 10) -> dict:
    """Run one bounded read-only pkg-config query."""
    try:
        completed = subprocess.run(
            argv, capture_output=True, text=True, encoding="utf-8",
            errors="replace", timeout=timeout, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"command": argv, "exit_code": None, "error": type(exc).__name__}
    return {
        "command": argv,
        "exit_code": completed.returncode,
        "stdout": completed.stdout.strip(),
        "stderr": completed.stderr.strip(),
    }


def probe_native_packages(pkg_config: str | None = None, runner=None) -> dict:
    """Check the fixed Linux profile packages without installing anything."""
    pkg_config = pkg_config or shutil.which("pkg-config")
    if not pkg_config:
        return {
            "status": "unavailable",
            "pkg_config": None,
            "packages": {},
            "blockers": ["pkg-config is not available; Linux native packages were not probed."],
        }
    runner = _run if runner is None else runner
    packages = {}
    blockers = []
    for package, minimum in NATIVE_PACKAGES:
        version_result = runner([pkg_config, "--modversion", package])
        version = (version_result.get("stdout") or "").strip() or None
        usable = version_result.get("exit_code") == 0 and bool(version)
        minimum_result = None
        if usable and minimum:
            minimum_result = runner(
                [pkg_config, f"--atleast-version={minimum}", package]
            )
            usable = minimum_result.get("exit_code") == 0
        packages[package] = {
            "minimum": minimum,
            "version": version,
            "version_exit_code": version_result.get("exit_code"),
            "minimum_exit_code": (
                minimum_result.get("exit_code") if minimum_result is not None else None
            ),
            "usable": usable,
        }
        if not usable:
            blockers.append(
                f"Missing or too old Linux development package: {package}"
                + (f" >= {minimum}" if minimum else "")
            )
    return {
        "status": "checked" if not blockers else "unavailable",
        "pkg_config": pkg_config,
        "packages": packages,
        "blockers": blockers,
    }


def _display_available(env: dict) -> bool:
    return bool(env.get("DISPLAY") or env.get("WAYLAND_DISPLAY"))


def build_command(
    target: str,
    app: Path | str = DEFAULT_APP,
    out: Path | str = DEFAULT_OUTPUT,
    cli: str = "nsh",
) -> list[str]:
    """Plan the packaging step; it neither runs the app nor launches a host."""
    spec = target_spec(target)
    return [cli, "package", str(app), "--target", spec.nsh_target, "--out", str(out)]


def start_command(
    target: str,
    app: Path | str = DEFAULT_APP,
    cli: str = "nsh",
    release: bool = False,
    device: str | None = None,
) -> list[str]:
    """Plan the development-host launch step without starting it."""
    spec = target_spec(target)
    command = [cli, "run", str(app), "--target", spec.nsh_target]
    if release:
        command.append("--release")
    if device and spec.name == "ios":
        command.extend(["--device", device])
    return command


def qualify_target(
    target: str,
    host: dict,
    tools: dict,
    *,
    app: Path | str = DEFAULT_APP,
    out: Path | str = DEFAULT_OUTPUT,
    android_sdk: dict | None = None,
    native_packages: dict | None = None,
    env: dict | None = None,
) -> dict:
    """Report probe/build/start status for one target.

    Missing prerequisites yield unavailable. Present prerequisites still yield
    not_run for build and start because this function performs no build or
    launch. Only the probe phase can be available.
    """
    spec = target_spec(target)
    env = os.environ if env is None else env
    host_target = host.get("target")
    probe_blockers = []
    checked = ["host", "tools"]

    if host_target not in spec.hosts:
        actual = host_target or host.get("system") or "unknown"
        probe_blockers.append(
            f"{spec.name} requires a {' or '.join(spec.hosts)} host; "
            f"current host is {actual}."
        )
    if not tools.get("nsh"):
        probe_blockers.append(
            "NSH CLI is not on PATH, NSH_CLI, or the repository build; "
            "no install attempted."
        )

    if spec.name == "macos":
        if not tools.get("xcrun"):
            probe_blockers.append(
                "macOS requires Xcode command-line tools (xcrun)."
            )
    elif spec.name == "windows":
        if not (tools.get("cl") or tools.get("link")):
            probe_blockers.append(
                "Windows requires MSVC Build Tools (cl or link on PATH)."
            )
        if not tools.get("clang"):
            probe_blockers.append(
                "Windows requires LLVM/clang for bindgen; no install attempted."
            )
    elif spec.name == "linux":
        checked.append("native_packages")
        if not tools.get("pkg-config"):
            probe_blockers.append(
                "Linux requires pkg-config and GTK/GStreamer development packages."
            )
        elif native_packages is None:
            probe_blockers.append(
                "Linux native package probe did not run; GTK/GStreamer readiness is unknown."
            )
        else:
            package_blockers = native_packages.get("blockers") or []
            if not package_blockers and native_packages.get("status") == "unavailable":
                package_blockers = ["Linux native package probe reported unavailable."]
            probe_blockers.extend(package_blockers)
        if not (tools.get("cc") or tools.get("gcc")):
            probe_blockers.append(
                "Linux requires a C compiler (cc or gcc) for native dependencies."
            )
    elif spec.name == "ios":
        checked.extend(["xcode", "ios_sdk"])
        if not tools.get("xcodebuild"):
            probe_blockers.append("iOS requires Xcode (xcodebuild).")
        if not tools.get("xcrun"):
            probe_blockers.append("iOS requires Xcode command-line tools (xcrun).")
    elif spec.name == "android":
        checked.extend(["android_sdk", "jdk"])
        if not android_sdk or not android_sdk.get("found"):
            reason = (
                android_sdk.get("reason")
                if isinstance(android_sdk, dict)
                else None
            )
            probe_blockers.append(
                reason
                or "Android SDK is missing or unusable; set ANDROID_HOME or "
                "ANDROID_SDK_ROOT (no install attempted)."
            )
        else:
            if not android_sdk.get("build_tools"):
                probe_blockers.append("Android SDK build-tools are missing.")
            if not android_sdk.get("platforms"):
                probe_blockers.append("Android SDK platform is missing.")
        if not tools.get("java"):
            probe_blockers.append("Android requires a JDK (java on PATH or JAVA_HOME).")
        try:
            rust_app = bool(json.loads((Path(app) / "nsh.json").read_text()).get("rust"))
        except (OSError, ValueError):
            rust_app = False
        if rust_app and not (android_sdk or {}).get("ndk"):
            probe_blockers.append("This QUSOR app includes Rust; an Android NDK is required.")
    elif spec.name == "web":
        # Web must stay independent of GTK, WebKit, pkg-config and native PDFium.
        checked.append("web_runtime")

    probe_status = "available" if not probe_blockers else "unavailable"
    cli = tools.get("nsh") or "nsh"
    build = build_command(spec.name, app, out, cli)
    start = start_command(spec.name, app, cli)
    build_status = "not_run" if probe_status == "available" else "unavailable"
    start_blockers = list(probe_blockers)
    if spec.name == "linux" and not _display_available(env):
        start_blockers.append(
            "No DISPLAY or WAYLAND_DISPLAY; native window startup was not attempted."
        )
    elif spec.name == "android":
        if android_sdk and android_sdk.get("found") and not android_sdk.get("adb"):
            start_blockers.append(
                "adb is missing from the Android SDK; device/emulator launch is unavailable."
            )
    # Web starts the dev server; a browser is recorded but not required to plan it.
    start_status = "not_run" if not start_blockers else "unavailable"
    statuses = (probe_status, build_status, start_status)
    overall = "unavailable" if "unavailable" in statuses else "not_run"
    return {
        "target": spec.name,
        "nsh_target": spec.nsh_target,
        "status": overall,
        "artifact": spec.artifact(),
        "required_toolchains": list(spec.build_requirements),
        "notes": spec.notes,
        "probe": {"status": probe_status, "checked": checked, "blockers": probe_blockers},
        "build": {
            "status": build_status,
            "command": build,
            "requirements": list(spec.build_requirements),
            "blockers": list(probe_blockers),
        },
        "start": {
            "status": start_status,
            "command": start,
            "requirements": list(spec.start_requirements),
            "blockers": start_blockers,
        },
    }


def collect(
    target: str = "all",
    *,
    app: Path | str = DEFAULT_APP,
    out: Path | str = DEFAULT_OUTPUT,
    host: dict | None = None,
    tools: dict | None = None,
    env: dict | None = None,
    android_sdk: dict | None = None,
    native_packages: dict | None = None,
    pkg_config_runner=None,
) -> dict:
    """Build a read-only report for all six targets or one canonical target."""
    selection = str(target).strip().lower()
    requested = TARGETS if selection == "all" else (canonical_target(selection),)
    env = os.environ if env is None else env
    host = detect_host() if host is None else host
    tools = detect_tools(env) if tools is None else tools
    if "android" in requested and android_sdk is None:
        android_sdk = detect_android_sdk(env)
    if (
        "linux" in requested
        and native_packages is None
        and host.get("target") == "linux"
    ):
        native_packages = probe_native_packages(
            tools.get("pkg-config"), pkg_config_runner
        )

    targets = {
        name: qualify_target(
            name, host, tools, app=app, out=out,
            android_sdk=android_sdk, native_packages=native_packages, env=env,
        )
        for name in requested
    }
    blockers = []
    for name in requested:
        for phase in ("build", "start"):
            for blocker in targets[name][phase]["blockers"]:
                labelled = f"{name}: {blocker}"
                if labelled not in blockers:
                    blockers.append(labelled)
    app_path = Path(app).expanduser()
    return {
        "schema_version": 1,
        "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "task_ids": ["P01", "P02"],
        "scope": (
            "Six-target NSH build/launch prerequisite matrix. Read-only: no "
            "installs, downloads, builds, host launches or GUI startup. Build "
            "and start remain not_run unless separately executed and recorded. "
            "Only named executable/SDK paths are recorded; no environment dump "
            "or credentials."
        ),
        "selection": "all" if selection == "all" else requested[0],
        "repository_root": str(REPO_ROOT),
        "host": host,
        "app": {"path": str(app_path), "exists": app_path.exists()},
        "tools": tools,
        "android_sdk": android_sdk,
        "native_packages": native_packages,
        "targets": targets,
        "blockers": blockers,
        "unverified": [
            "toolchain/SDK versions beyond executable and platform presence",
            "build execution and artifact inspection",
            "launch/window startup and interaction",
            "packaging/install on each target",
            "iOS simulator and Android device/emulator runs",
            "reading, input, selection and close/reopen recovery",
        ],
    }


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target", default="all", choices=("all",) + TARGETS,
        metavar="all|TARGET",
        help="Target to report (default: all six canonical targets).",
    )
    parser.add_argument(
        "--output", type=Path,
        help="Optional JSON evidence file; stdout always receives the report.",
    )
    parser.add_argument(
        "--app", type=Path, default=DEFAULT_APP,
        help="NSH app entry/directory used only for command planning.",
    )
    parser.add_argument(
        "--out", type=Path, default=DEFAULT_OUTPUT,
        help="Package output directory used only for command planning.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    report = collect(args.target, app=args.app, out=args.out)
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return 1 if report["blockers"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
