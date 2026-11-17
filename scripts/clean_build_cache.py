"""Remove disposable Cargo artifacts from the shared target directory.

Default: delete ``incremental`` directories and non-host target triples.
Keep the host ``debug/`` and ``release/`` trees, including the unsuffixed
copies Cargo writes for the host. ``--deps`` also removes ``deps`` and
``build`` under those host trees, but leaves final binaries that sit
directly in ``debug/`` or ``release/``. ``--all-targets`` also removes the
host artifacts.

This never uses ``git clean``. It only deletes inside ``.build/cargo-target``
and a leftover ``experiments/nsh-reader/target/nsh/target`` directory.
``.local/``, ``evidence/``, ``.git``, ``third_party/nsh`` source, and
``plugins/*/target`` packages stay untouched.
"""
from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

HOST_DIR_NAMES = {"debug", "release"}
# Top-level Cargo directories that are not target triples.
PLAIN_DIR_NAMES = HOST_DIR_NAMES | {"doc", "tmp", "incremental", "examples", ".fingerprint"}


def _host_triple(explicit: str | None) -> str:
    if explicit:
        return explicit
    result = subprocess.run(
        ["rustc", "-vV"], check=False, text=True, capture_output=True
    )
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or "rustc -vV failed")
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.split(": ", 1)[1].strip()
    raise SystemExit("rustc -vV did not report a host triple")


def _within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError:
        return False
    return True


def _remove_tree(path: Path, allowed: Path) -> None:
    if path.is_symlink() or not path.is_dir():
        raise SystemExit(f"refusing to remove {path}")
    resolved = path.resolve()
    if not _within(resolved, allowed) or resolved == allowed.resolve():
        raise SystemExit(f"refusing to remove {path}")
    blocked = {".git", ".local", "evidence"}
    if blocked.intersection(resolved.parts) or resolved.name in blocked:
        raise SystemExit(f"refusing to remove protected path {path}")
    shutil.rmtree(resolved)


def _directories(root: Path):
    """Yield real directories under ``root``, deepest first. Skip symlinks."""
    if not root.is_dir() or root.is_symlink():
        return
    children = []
    try:
        entries = list(root.iterdir())
    except OSError as error:
        raise SystemExit(f"cannot read {root}: {error}") from error
    for entry in entries:
        if entry.is_symlink() or not entry.is_dir():
            continue
        children.append(entry)
    for child in children:
        yield from _directories(child)
        yield child


def _is_triple(name: str) -> bool:
    return name not in PLAIN_DIR_NAMES and name.count("-") >= 2 and not name.startswith(".")


def clean(root: Path, *, deps: bool = False, all_targets: bool = False,
          host_triple: str | None = None) -> list[Path]:
    """Delete disposable artifacts under ``root``. Return removed paths."""
    root = root.resolve()
    host = _host_triple(host_triple)
    cargo_roots = [
        root / ".build" / "cargo-target",
        root / "experiments" / "nsh-reader" / "target" / "nsh" / "target",
    ]
    removed: list[Path] = []
    for cargo_root in cargo_roots:
        if not cargo_root.is_dir() or cargo_root.is_symlink():
            continue
        if not _within(cargo_root, root):
            raise SystemExit(f"refusing to clean outside {root}: {cargo_root}")
        incremental = [
            path for path in _directories(cargo_root) if path.name == "incremental"
        ]
        for path in incremental:
            _remove_tree(path, cargo_root)
            removed.append(path)
        for entry in list(cargo_root.iterdir()):
            if entry.is_symlink() or not entry.is_dir():
                continue
            foreign = _is_triple(entry.name) and entry.name != host
            host_tree = entry.name == host or entry.name in HOST_DIR_NAMES
            if foreign or (all_targets and host_tree):
                _remove_tree(entry, cargo_root)
                removed.append(entry)
        if not deps:
            continue
        host_profiles = [cargo_root / name for name in ("debug", "release")]
        host_profiles.extend(cargo_root / host / name for name in ("debug", "release"))
        for profile in host_profiles:
            if not profile.is_dir() or profile.is_symlink():
                continue
            for name in ("deps", "build"):
                candidate = profile / name
                if candidate.is_dir() and not candidate.is_symlink():
                    _remove_tree(candidate, cargo_root)
                    removed.append(candidate)
    return removed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=None,
        help="Repository root. Defaults to this checkout.",
    )
    parser.add_argument(
        "--deps", action="store_true",
        help="Also remove deps/ and build/ under host debug and release.",
    )
    parser.add_argument(
        "--all-targets", action="store_true",
        help="Also remove host debug, release, and the host triple.",
    )
    parser.add_argument(
        "--host-triple", default=None,
        help="Host triple to keep. Defaults to rustc -vV.",
    )
    args = parser.parse_args(argv)
    root = args.root if args.root is not None else Path(__file__).resolve().parents[1]
    removed = clean(
        root, deps=args.deps, all_targets=args.all_targets, host_triple=args.host_triple,
    )
    print(f"removed {len(removed)} paths under {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
