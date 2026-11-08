"""Build a TSX author fixture outside the repository with the cached NSH CLI.

This records build evidence only, never loads downloaded QuickJS bytecode,
starts a native window, installs a plugin, or calls the fixture's host bridge.
"""
from pathlib import Path
import argparse
from datetime import datetime, timezone
import hashlib
import json
import shutil
import subprocess
import tempfile

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
CLI = ROOT / "experiments/nsh-reader/target/nsh/target/debug/nsh"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    version = subprocess.run(
        [str(CLI), "--version"], check=True, text=True, capture_output=True
    ).stdout.strip()
    if version != "nsh 0.4.0 (913fd76)":
        raise SystemExit(f"Unexpected CLI: {version}; review the pin before running")
    with tempfile.TemporaryDirectory(prefix="qusor-tsx-author-") as temporary:
        author = Path(temporary) / "author"
        author.mkdir()
        for name in ("App.tsx", "nsh.json"):
            shutil.copyfile(BASE / "fixtures" / name, author / name)
        out = Path(temporary) / "bundle"
        process = subprocess.run(
            [str(CLI), "build", str(author), "--release", "-o", str(out)],
            cwd=author, check=False, capture_output=True, text=True, timeout=60,
        )
        evidence = {
            "recorded_at": datetime.now(timezone.utc).isoformat(),
            "scope": "outside-repository TSX build only",
            "cli_version": version,
            "cli_sha256": digest(CLI),
            "author_directory_outside_repository": not author.is_relative_to(ROOT),
            "fixture_sha256": {
                name: digest(author / name) for name in ("App.tsx", "nsh.json")
            },
            "exit_code": process.returncode,
            "stdout": process.stdout,
            "stderr": process.stderr,
            "artifacts": {
                path.name: {"bytes": path.stat().st_size, "sha256": digest(path)}
                for path in sorted(out.iterdir()) if path.is_file()
            } if out.exists() else {},
            "native_window": "not executed",
            "host_call": "not executed",
            "package_installation": "not executed",
            "vm_isolation": "not executed",
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n")
        if process.returncode:
            raise SystemExit(process.returncode)
        required = {"bundle.js", "bundle.nshc", "capabilities.json"}
        if not required.issubset(evidence["artifacts"]):
            raise SystemExit("Expected bundle outputs missing; inspect evidence")
        print(json.dumps({"version": version, "exit_code": process.returncode,
                          "artifacts": list(evidence["artifacts"])}, ensure_ascii=False))


if __name__ == "__main__":
    main()
