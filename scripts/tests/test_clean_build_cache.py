"""Fixture tests for the shared Cargo cache cleaner. Never touches the real tree."""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import build_layout
import clean_build_cache


HOST = "aarch64-apple-darwin"


def _write(path: Path, text: str = "x") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def _fixture(root: Path) -> Path:
    cargo = root / ".build" / "cargo-target"
    _write(cargo / "debug" / "incremental" / "nsh" / "s")
    _write(cargo / "debug" / "nsh", "host-bin")
    _write(cargo / "debug" / "deps" / "lib.rlib", "dep")
    _write(cargo / "debug" / "build" / "foo" / "out")
    _write(cargo / "release" / "nsh", "host-release")
    _write(cargo / HOST / "debug" / "nsh", "host-triple")
    _write(cargo / "aarch64-apple-ios" / "debug" / "lib")
    _write(cargo / "aarch64-apple-ios-sim" / "debug" / "lib")
    _write(cargo / "aarch64-linux-android" / "debug" / "lib")
    _write(cargo / "x86_64-pc-windows-gnu" / "debug" / "lib")
    _write(cargo / "x86_64-pc-windows-msvc" / "debug" / "lib")
    _write(cargo / "x86_64-unknown-linux-gnu" / "debug" / "lib")
    leftover = root / "experiments" / "nsh-reader" / "target" / "nsh" / "target"
    _write(leftover / "debug" / "incremental" / "old")
    _write(leftover / "debug" / "nsh", "leftover-host")
    _write(leftover / "aarch64-apple-ios" / "debug" / "lib")
    _write(root / ".local" / "keys" / "update.secret", "secret")
    _write(root / "evidence" / "keep.md", "evidence")
    _write(root / "plugins" / "reader" / "target" / "reader.qpk", "qpk")
    _write(root / "plugins" / "reader" / "target" / "author-proof.json", "{}")
    _write(root / "third_party" / "nsh" / "crates" / "nsh-core" / "src" / "lib.rs", "fn main() {}")
    _write(root / ".git" / "HEAD", "ref: refs/heads/main")
    return cargo


class CleanBuildCacheTest(unittest.TestCase):
    def test_layout_constants(self) -> None:
        self.assertEqual(build_layout.NSH_SRC, build_layout.REPO / "third_party" / "nsh")
        self.assertEqual(build_layout.NSH_SHA, "913fd76e1f8434ec6cdf248f4476c1ebe08af19f")
        self.assertEqual(build_layout.CARGO_TARGET, build_layout.REPO / ".build" / "cargo-target")
        self.assertEqual(build_layout.nsh_cli(), build_layout.CARGO_TARGET / "debug" / "nsh")

    def test_default_keeps_host_and_protected_files(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qusor-cache-fixture-") as temporary:
            root = Path(temporary)
            cargo = _fixture(root)
            clean_build_cache.clean(root, host_triple=HOST)
            self.assertFalse((cargo / "debug" / "incremental").exists())
            self.assertEqual((cargo / "debug" / "nsh").read_text(), "host-bin")
            self.assertEqual((cargo / "release" / "nsh").read_text(), "host-release")
            self.assertTrue((cargo / HOST / "debug" / "nsh").is_file())
            for name in (
                "aarch64-apple-ios", "aarch64-apple-ios-sim", "aarch64-linux-android",
                "x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu",
            ):
                self.assertFalse((cargo / name).exists(), name)
            leftover = root / "experiments" / "nsh-reader" / "target" / "nsh" / "target"
            self.assertFalse((leftover / "debug" / "incremental").exists())
            self.assertEqual((leftover / "debug" / "nsh").read_text(), "leftover-host")
            self.assertFalse((leftover / "aarch64-apple-ios").exists())
            self.assertTrue((root / ".local" / "keys" / "update.secret").is_file())
            self.assertTrue((root / "evidence" / "keep.md").is_file())
            self.assertEqual((root / "plugins" / "reader" / "target" / "reader.qpk").read_text(), "qpk")
            self.assertTrue((root / "plugins" / "reader" / "target" / "author-proof.json").is_file())
            self.assertTrue((root / "third_party" / "nsh" / "crates" / "nsh-core" / "src" / "lib.rs").is_file())
            self.assertTrue((root / ".git" / "HEAD").is_file())
            self.assertTrue((cargo / "debug" / "deps" / "lib.rlib").is_file())

    def test_deps_keeps_final_binaries(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qusor-cache-deps-") as temporary:
            root = Path(temporary)
            cargo = _fixture(root)
            clean_build_cache.clean(root, deps=True, host_triple=HOST)
            self.assertFalse((cargo / "debug" / "deps").exists())
            self.assertFalse((cargo / "debug" / "build").exists())
            self.assertEqual((cargo / "debug" / "nsh").read_text(), "host-bin")
            self.assertEqual((cargo / "release" / "nsh").read_text(), "host-release")
            self.assertTrue((root / "plugins" / "reader" / "target" / "reader.qpk").is_file())

    def test_all_targets_removes_host_artifacts(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qusor-cache-all-") as temporary:
            root = Path(temporary)
            cargo = _fixture(root)
            clean_build_cache.clean(root, all_targets=True, host_triple=HOST)
            self.assertFalse((cargo / "debug").exists())
            self.assertFalse((cargo / "release").exists())
            self.assertFalse((cargo / HOST).exists())
            self.assertTrue((root / ".local" / "keys" / "update.secret").is_file())
            self.assertTrue((root / "plugins" / "reader" / "target" / "author-proof.json").is_file())

    def test_cli_cleans_only_the_given_root(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qusor-cache-cli-") as temporary:
            root = Path(temporary)
            cargo = _fixture(root)
            script = ROOT / "scripts" / "clean_build_cache.py"
            subprocess.run(
                [sys.executable, str(script), "--root", str(root), "--host-triple", HOST],
                check=True, text=True, capture_output=True,
            )
            self.assertFalse((cargo / "aarch64-apple-ios").exists())
            self.assertTrue((cargo / "debug" / "nsh").is_file())
            self.assertTrue((ROOT / ".local").exists())


if __name__ == "__main__":
    unittest.main()
