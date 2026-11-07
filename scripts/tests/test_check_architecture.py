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


    def enable_closure(self, allowed=None):
        rule = self.policy["packages"]["learning-domain"]
        rule["allowed_dependencies"] = ["helper"]
        rule["allowed_transitive_dependencies"] = ["helper"] if allowed is None else allowed
        self.package["dependencies"] = [{"name": "helper", "kind": None}]
        self.metadata["packages"].append({"id": "helper-id", "name": "helper"})
        self.metadata["resolve"] = {"nodes": [
            {"id": "domain-id", "dependencies": ["helper-id"], "deps": [{
                "name": "helper", "pkg": "helper-id",
                "dep_kinds": [{"kind": None, "target": None}],
            }]},
            {"id": "helper-id", "dependencies": [], "deps": []},
        ]}

    def add_resolved_dependency(self, name, kind=None, target=None, alias=None):
        identifier = name + "-id"
        self.metadata["packages"].append({"id": identifier, "name": name})
        nodes = self.metadata["resolve"]["nodes"]
        helper = next(node for node in nodes if node["id"] == "helper-id")
        helper["dependencies"].append(identifier)
        helper["deps"].append({
            "name": alias or name, "pkg": identifier,
            "dep_kinds": [{"kind": kind, "target": target}],
        })
        nodes.append({"id": identifier, "dependencies": [], "deps": []})

    def test_accepts_explicitly_allowed_resolved_closure(self):
        self.enable_closure(["helper", "serde"])
        self.add_resolved_dependency("serde")
        self.assertEqual(self.errors(), [])

    def test_rejects_forbidden_transitive_dependency_through_allowed_helper(self):
        for name in ("nsh", "storage"):
            with self.subTest(name=name):
                self.setUp()
                self.enable_closure()
                self.add_resolved_dependency(name)
                self.assertTrue(any(
                    f"learning-domain -> helper -> {name}" in error
                    for error in self.errors()
                ))

    def test_resolved_closure_does_not_filter_dev_build_or_foreign_target_edges(self):
        for kind, target in (
            ("dev", None), ("build", None),
            (None, 'cfg(target_os = "ios")'),
            ("build", 'cfg(target_os = "windows")'),
        ):
            with self.subTest(kind=kind, target=target):
                self.setUp()
                self.enable_closure()
                self.add_resolved_dependency("nsh", kind=kind, target=target)
                self.assertTrue(any("unapproved resolved dependency nsh" in error for error in self.errors()))

    def test_resolved_alias_cannot_hide_actual_package_name(self):
        self.enable_closure(["helper", "safe_alias"])
        self.add_resolved_dependency("storage", alias="safe_alias")
        self.assertTrue(any("unapproved resolved dependency storage" in error for error in self.errors()))

    def test_direct_allowlist_uses_package_name_even_when_closure_is_allowed(self):
        self.enable_closure()
        self.package["dependencies"][0].update(name="nsh", rename="helper")
        self.assertTrue(any("unapproved normal dependency nsh" in error for error in self.errors()))

    def test_transitive_allowlist_includes_directly_resolved_packages(self):
        self.enable_closure([])
        self.assertTrue(any("unapproved resolved dependency helper" in error for error in self.errors()))

    def test_rejects_missing_resolve_when_closure_policy_is_present(self):
        self.enable_closure()
        for resolve in (None, {}, {"nodes": None}):
            with self.subTest(resolve=resolve):
                self.metadata["resolve"] = resolve
                self.assertTrue(any("resolve.nodes is missing" in error for error in self.errors()))
        self.metadata.pop("resolve")
        self.assertTrue(any("resolve.nodes is missing" in error for error in self.errors()))

    def test_rejects_missing_root_or_reachable_resolve_nodes(self):
        for identifier in ("domain-id", "helper-id"):
            with self.subTest(identifier=identifier):
                self.setUp()
                self.enable_closure()
                self.metadata["resolve"]["nodes"] = [
                    node for node in self.metadata["resolve"]["nodes"]
                    if node["id"] != identifier
                ]
                self.assertTrue(any(
                    f"resolved dependency node is missing: {identifier}" in error
                    for error in self.errors()
                ))

    def test_rejects_resolved_edge_without_package_metadata(self):
        self.enable_closure()
        self.metadata["packages"] = [self.package]
        self.assertTrue(any("resolved package metadata is missing: helper-id" in error for error in self.errors()))

    def test_rejects_missing_workspace_package_metadata(self):
        self.metadata["workspace_members"].append("missing-id")
        self.assertTrue(any("Workspace package metadata is missing: missing-id" in error for error in self.errors()))

    def test_dependency_cycle_terminates_and_does_not_hide_forbidden_package(self):
        self.enable_closure()
        helper = self.metadata["resolve"]["nodes"][1]
        helper["dependencies"].extend(["domain-id", "helper-id"])
        self.assertEqual(self.errors(), [])
        self.add_resolved_dependency("nsh")
        errors = self.errors()
        self.assertEqual(sum("unapproved resolved dependency nsh" in error for error in errors), 1)

    def test_either_cargo_edge_representation_still_covers_all_dependencies(self):
        for removed in ("dependencies", "deps"):
            with self.subTest(removed=removed):
                self.setUp()
                self.enable_closure()
                self.add_resolved_dependency("nsh")
                for node in self.metadata["resolve"]["nodes"]:
                    node.pop(removed)
                self.assertTrue(any("unapproved resolved dependency nsh" in error for error in self.errors()))

    def test_inconsistent_edge_representations_cannot_hide_a_dependency(self):
        self.enable_closure()
        self.add_resolved_dependency("nsh")
        self.metadata["resolve"]["nodes"][1]["dependencies"] = []
        self.assertTrue(any("unapproved resolved dependency nsh" in error for error in self.errors()))

    def test_rejects_incomplete_and_duplicate_nodes(self):
        self.enable_closure()
        self.metadata["resolve"]["nodes"][1] = {"id": "helper-id"}
        self.assertTrue(any("no dependency edges" in error for error in self.errors()))
        self.metadata["resolve"]["nodes"].append({"id": "helper-id", "dependencies": []})
        self.assertTrue(any("duplicate node" in error for error in self.errors()))

    def test_closure_check_is_applied_independently_to_each_workspace_member(self):
        self.enable_closure()
        helper = self.metadata["packages"][1]
        helper.update(
            manifest_path=str(self.root / "crates/helper/Cargo.toml"),
            publish=[], dependencies=[{"name": "nsh", "kind": None}],
        )
        self.metadata["workspace_members"].append("helper-id")
        self.policy["packages"]["helper"] = {
            "manifest": "crates/helper/Cargo.toml", "allowed_dependencies": ["nsh"],
            "allowed_transitive_dependencies": ["nsh"],
        }
        self.add_resolved_dependency("nsh")
        errors = self.errors()
        self.assertTrue(any(error.startswith("learning-domain: unapproved resolved dependency nsh") for error in errors))
        self.assertFalse(any(error.startswith("helper:") for error in errors))

if __name__ == "__main__":
    unittest.main()
