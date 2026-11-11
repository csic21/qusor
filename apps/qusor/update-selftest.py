#!/usr/bin/env python3
"""Local end-to-end self-update test for the packaged QUSOR app (macOS).

Builds the app twice from this checkout — 0.1.0 and 0.1.1 — with the update
source pointed at a loopback HTTP server, writes a signed `latest.json` with the
checkout's own update key, installs 0.1.0 into a scratch directory and starts it
with `QUSOR_UPDATE_EVIDENCE`. The app then checks, downloads, replaces itself and
restarts; the 0.1.1 build reports its own version and readable Core and quits.

What the test proves, and what it does not:

- proves: the packaged app accepts a manifest signed by the key baked into it,
  downloads the archive the manifest names (SHA-256 and size checked by NSH),
  replaces itself in place, restarts into 0.1.1 and opens the same app data.
- does not prove: Developer ID signing or notarization (the build is ad hoc),
  Windows/Linux replacement, GitHub Releases serving (the same manifest files are
  served from the loopback), mobile distribution, or an independent signature
  recomputation (the app's own acceptance is the check here).

The checkout's `nsh.json` is restored after every build; the test source writes a
temporary copy only.
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

APP = Path(__file__).resolve().parent
ROOT = APP.parents[1]
NSH = ROOT / "experiments/nsh-reader/target/nsh/target/debug/nsh"
KEYS = ROOT / ".local" / "keys"
CONFIG = APP / "nsh.json"
WORK = Path("/tmp/qusor-update-selftest")
DATA = Path.home() / "Library" / "Application Support" / "org.qusor.app"
EVIDENCE = ROOT / "experiments/nsh-reader/evidence/app-update"
SETTLE = 2.0


def run(command: list[str], **options) -> subprocess.CompletedProcess:
    print("+", " ".join(str(part) for part in command), flush=True)
    return subprocess.run(command, check=True, text=True, **options)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as reading:
        for block in iter(lambda: reading.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def serve(directory: Path, port: int) -> ThreadingHTTPServer:
    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(directory), **kwargs)

        def log_message(self, *_: object) -> None:
            return

    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


@contextlib.contextmanager
def test_config(updater: dict, version: str, build: int):
    """The app's nsh.json with the test's version and update source."""
    original = CONFIG.read_bytes()
    config = json.loads(original)
    config["version"] = version
    config["build"] = build
    config["updater"] = updater
    CONFIG.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
    try:
        yield
    finally:
        CONFIG.write_bytes(original)


def package(version: str, build: int, updater: dict, out: Path, log: Path) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    with test_config(updater, version, build):
        with log.open("w") as recording:
            run(
                [str(NSH), "package", "--target", "macos", "--out", str(out)],
                cwd=APP,
                env={**os.environ, "CARGO_PROFILE_RELEASE_STRIP": "none"},
                stdout=recording,
                stderr=subprocess.STDOUT,
            )
    archive = out / f"QUSOR-{version}-macos-aarch64.tar.gz"
    if not archive.is_file():
        raise SystemExit(f"packaged app has no archive at {archive}")
    return archive


def wait_for(path: Path, timeout: float, what: str) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            try:
                return read_json(path)
            except json.JSONDecodeError:
                pass
        time.sleep(0.15)
    raise SystemExit(f"timed out waiting for {what}: {path}")


def pids_of(executable: Path) -> list[int]:
    result = subprocess.run(
        ["pgrep", "-f", str(executable)], text=True, capture_output=True
    )
    if result.returncode != 0:
        return []
    return [int(line) for line in result.stdout.split() if line.strip().isdigit()]


def windows_of(owner: str, cache: Path) -> list[dict]:
    cache.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [
            "swift",
            "-module-cache-path",
            str(cache),
            str(APP / "window_ids.swift"),
            owner,
        ],
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        return []
    windows = []
    for line in result.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) >= 2 and parts[0].isdigit():
            windows.append({"id": int(parts[0]), "pid": int(parts[1])})
    return windows


def capture(path: Path, cache: Path) -> dict:
    """A capture attempt: a missing window is recorded, not fatal."""
    """`screencapture` of the app's window alone: no desktop, no other window."""
    deadline = time.monotonic() + 25.0
    while time.monotonic() < deadline:
        windows = windows_of("QUSOR", cache)
        if windows:
            window = windows[0]
            result = subprocess.run(
                ["screencapture", "-x", "-o", "-l", str(window["id"]), str(path)],
                capture_output=True,
            )
            if result.returncode == 0 and path.is_file() and path.stat().st_size > 1024:
                return {
                    "file": path.name,
                    "window_id": window["id"],
                    "pid": window["pid"],
                    "bytes": path.stat().st_size,
                }
        time.sleep(0.2)
    return {"file": path.name, "error": "no QUSOR window to capture"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--install-evidence",
        action="store_true",
        help="copy captures and record.json into experiments/nsh-reader/evidence/app-update",
    )
    args = parser.parse_args()

    if sys.platform != "darwin":
        raise SystemExit("the self-update test runs the packaged macOS app only")
    for required in (NSH, KEYS / "update.secret", KEYS / "update.pub", CONFIG):
        if not required.exists():
            raise SystemExit(f"missing {required}; build the app and run `nsh keygen keys/update`")

    if WORK.exists():
        shutil.rmtree(WORK)
    captures = WORK / "captures"
    evidence = WORK / "evidence"
    server_root = WORK / "server"
    install = WORK / "install"
    logs = WORK / "logs"
    for directory in (captures, evidence, server_root, install, logs):
        directory.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        ["pkill", "-f", "QUSOR.app/Contents/MacOS/QUSOR"], capture_output=True
    )
    time.sleep(0.5)
    before = WORK / "data-before"
    if DATA.exists():
        shutil.move(str(DATA), str(before))

    started_at = time.time()
    record: dict = {
        "tested_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "platform": "macos-aarch64",
        "app_id": "org.qusor.app",
        "from_version": "0.1.0",
        "to_version": "0.1.1",
        "update_source": "loopback http://127.0.0.1",
        "work_dir": str(WORK),
        "data_dir": str(DATA),
        "nsh": str(NSH),
        "nsh_json_sha256": sha256(CONFIG),
        "captures": [],
        "receipts": {},
        "checks": {},
        "logs": {},
    }

    try:
        print("== building the two versions", flush=True)
        run([sys.executable, str(APP / "prepare.py")], cwd=ROOT, stdout=subprocess.DEVNULL)
        port = free_port()
        public_key = (KEYS / "update.pub").read_text(encoding="utf-8").strip()
        updater = {"url": f"http://127.0.0.1:{port}/latest.json", "publicKey": public_key}
        archive_010 = package("0.1.0", 1, updater, WORK / "dist-0.1.0", logs / "package-0.1.0.log")
        archive_011 = package("0.1.1", 2, updater, WORK / "dist-0.1.1", logs / "package-0.1.1.log")
        record["archives"] = {
            "0.1.0": {"file": archive_010.name, "sha256": sha256(archive_010)},
            "0.1.1": {"file": archive_011.name, "sha256": sha256(archive_011)},
        }

        print("== writing the signed manifest", flush=True)
        notes = WORK / "notes.md"
        notes.write_text(
            "自测版本：只在等待更新状态行的措辞上不同。\n", encoding="utf-8"
        )
        with test_config(updater, "0.1.1", 2):
            run(
                [
                    str(NSH),
                    "update-manifest",
                    str(archive_011),
                    "--entry",
                    "apps/qusor",
                    "--notes",
                    str(notes),
                    "--key",
                    str(KEYS / "update.secret"),
                    "--base-url",
                    f"http://127.0.0.1:{port}",
                    "--out",
                    str(server_root),
                ],
                cwd=ROOT,
            )
        manifest = read_json(server_root / "latest.json")
        shutil.copy2(archive_011, server_root / archive_011.name)
        record["manifest"] = {
            "file": "latest.json",
            "sha256": sha256(server_root / "latest.json"),
            "signature": (server_root / "latest.json.sig")
            .read_text(encoding="utf-8")
            .strip(),
            "id": manifest["id"],
            "version": manifest["version"],
            "asset": manifest["platforms"]["macos-aarch64"],
        }
        serve(server_root, port)

        print("== installing 0.1.0 into the scratch directory", flush=True)
        app_010 = WORK / "dist-0.1.0" / "QUSOR.app"
        if not app_010.is_dir():
            raise SystemExit("nsh package wrote no QUSOR.app for the test")
        shutil.copytree(app_010, install / "QUSOR.app", symlinks=True)
        executable = install / "QUSOR.app" / "Contents" / "MacOS" / "QUSOR"
        installed_config = read_json(
            install / "QUSOR.app" / "Contents" / "Resources" / "nsh" / "assets" / "nsh-updater.json"
        )
        record["from_config"] = installed_config
        if installed_config.get("version") != "0.1.0":
            raise SystemExit(f"the installed test build is {installed_config}")

        # Warm the Swift module cache once; a cold compile can outlast the state
        # the first capture waits for.
        windows_of("QUSOR", WORK / "swift-cache")
        print("== running 0.1.0 itself", flush=True)
        # Direct launch keeps the evidence variable and this process model; the
        # test needs write access to the app data directory and the bundle, which
        # is why it runs with full file access rather than the workspace sandbox.
        log = (logs / "first-run.log").open("w")
        process = subprocess.Popen(
            [str(executable)],
            env={**os.environ, "QUSOR_UPDATE_EVIDENCE": str(evidence)},
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        first_pid = process.pid
        record["first_run_pid"] = first_pid
        cache = WORK / "swift-cache"

        started = wait_for(evidence / "started.json", 90, "the app to start")
        record["receipts"]["started"] = started
        time.sleep(SETTLE)
        checked = wait_for(evidence / "checked.json", 90, "the update check")
        record["receipts"]["checked"] = checked
        record["captures"].append(capture(captures / "available.png", cache))
        downloaded = wait_for(evidence / "downloaded.json", 180, "the download")
        record["receipts"]["downloaded"] = downloaded
        record["captures"].append(capture(captures / "installing.png", cache))

        print("== waiting for the restart", flush=True)
        try:
            first_exit = process.wait(timeout=180)
        except subprocess.TimeoutExpired:
            raise SystemExit("the 0.1.0 process did not exit after the update")
        record["first_exit_code"] = first_exit

        restarted = wait_for(evidence / "restarted.json", 120, "the restarted build")
        record["receipts"]["restarted"] = restarted
        deadline = time.monotonic() + 20
        second_pid = None
        while time.monotonic() < deadline:
            pids = [pid for pid in pids_of(executable) if pid != first_pid]
            if pids:
                second_pid = pids[0]
                break
            time.sleep(0.2)
        record["second_run_pid"] = second_pid
        record["captures"].append(capture(captures / "restarted.png", cache))
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline and pids_of(executable):
            time.sleep(0.2)

        print("== checking the result", flush=True)
        data_dir = Path(str(started["payload"]["data_dir"]))
        record["data_dir"] = str(data_dir)
        record["data_dir_entries"] = (
            sorted(entry.name for entry in data_dir.iterdir())
            if data_dir.is_dir()
            else []
        )
        if DATA.is_dir():
            record["data_dir_entries"] = sorted(entry.name for entry in DATA.iterdir())
        after_config = read_json(
            install / "QUSOR.app" / "Contents" / "Resources" / "nsh" / "assets" / "nsh-updater.json"
        )
        record["after_config"] = after_config
        record["checks"] = {
            "manifest_signed": (server_root / "latest.json.sig").is_file(),
            "manifest_names_0.1.1": manifest["version"] == "0.1.1",
            "manifest_sha256_matches_archive": manifest["platforms"]["macos-aarch64"][
                "sha256"
            ]
            == sha256(archive_011),
            "manifest_size_matches_archive": manifest["platforms"]["macos-aarch64"][
                "size"
            ]
            == archive_011.stat().st_size,
            "checked_found_0.1.1": checked["payload"]["found"]["version"] == "0.1.1",
            "checked_can_install": checked["payload"]["found"]["can_install"] is True,
            "download_completed": downloaded["payload"]["progress"] == 1,
            "first_process_exited": first_exit == 0,
            "restarted_build_is_0.1.1": restarted["payload"]["version"] == "0.1.1",
            "restarted_same_app_data": restarted["payload"]["data_dir"]
            == started["payload"]["data_dir"],
            "restarted_reads_vocab_core": restarted["payload"]["vocab_readable"] is True,
            "restarted_reads_notes_core": restarted["payload"]["notes_readable"] is True,
            "installed_app_is_0.1.1": after_config.get("version") == "0.1.1",
            "installed_build_is_2": after_config.get("build") == 2,
            "data_dir_preserved": data_dir.is_dir()
            and restarted["payload"]["data_dir"] == str(data_dir),
            "restart_marker_consumed": not (
                data_dir / "update-evidence.json"
            ).exists(),
            "second_process_observed": second_pid is not None,
        }
        record["duration_seconds"] = round(time.time() - started_at, 1)
        failed = [name for name, ok in record["checks"].items() if not ok]
        record["result"] = "passed" if not failed else "failed"
        record["failed_checks"] = failed
        (evidence / "record.json").write_text(
            json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        (logs / "second-run.log").write_text(
            "the restarted build is started by the platform; see receipts\n",
            encoding="utf-8",
        )
        print(json.dumps(record["checks"], indent=2), flush=True)
        print(f"\nresult: {record['result']} in {record['duration_seconds']}s", flush=True)
        if failed:
            print("failed: " + ", ".join(failed), flush=True)

        if args.install_evidence:
            EVIDENCE.mkdir(parents=True, exist_ok=True)
            for image in captures.glob("*.png"):
                shutil.copy2(image, EVIDENCE / image.name)
            shutil.copy2(evidence / "record.json", EVIDENCE / "record.json")
            print(f"evidence written to {EVIDENCE}", flush=True)
        return 0 if not failed else 1
    finally:
        subprocess.run(
            ["pkill", "-f", "QUSOR.app/Contents/MacOS/QUSOR"], capture_output=True
        )



if __name__ == "__main__":
    raise SystemExit(main())
