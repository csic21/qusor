"""Regression cases for dependency paths that can bypass host-only builds."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_architecture import validate_graph  # noqa: E402


class ArchitectureGuardTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[2]
        self.policy = {"packages": {"learning-domain": {
            "manifest": "crates/learning-domain/Cargo.toml",
            "allowed_dependencies": [],
        }}}
        self.package = {
            "id": "domain-id",
            "name": "learning-domain",
            "manifest_path": str(self.root / "crates/learning-domain/Cargo.toml"),
            "publish": [],
            "dependencies": [],
        }
        self.metadata = {
            "packages": [self.package],
            "workspace_members": ["domain-id"],
        }

    def errors(self):
        return validate_graph(self.metadata, self.policy, self.root)

    def test_accepts_dependency_free_domain(self):
        self.assertEqual(self.errors(), [])

    def test_rejects_aliased_optional_target_dependency(self):
        self.package["dependencies"] = [{
            "name": "nsh", "rename": "innocent_helper",
            "optional": True, "target": 'cfg(target_os = "ios")', "kind": None,
        }]
        self.assertTrue(any("nsh" in error for error in self.errors()))

    def test_rejects_dev_and_build_dependency_paths(self):
        for kind in ("dev", "build"):
            with self.subTest(kind=kind):
                self.package["dependencies"] = [{
                    "name": "gpui", "kind": kind, "target": None,
                }]
                self.assertTrue(any(kind in error for error in self.errors()))

    def test_rejects_unregistered_workspace_member(self):
        other = copy.deepcopy(self.package)
        other.update(id="adapter-id", name="nsh-adapter")
        self.metadata["packages"].append(other)
        self.metadata["workspace_members"].append("adapter-id")
        self.assertTrue(any("Unregistered" in error for error in self.errors()))

    def test_rejects_manifest_outside_repository(self):
        self.package["manifest_path"] = str(self.root.parent / "Cargo.toml")
        self.assertTrue(any("outside" in error for error in self.errors()))

    def test_rejects_accidental_publishing(self):
        self.package["publish"] = None
        self.assertTrue(any("publish" in error for error in self.errors()))


if __name__ == "__main__":
    unittest.main()
