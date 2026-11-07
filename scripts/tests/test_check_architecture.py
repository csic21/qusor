"""Regression cases for dependency paths that can bypass host-only builds."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_architecture import validate_graph, validate_inventory  # noqa: E402


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


class ManifestInventoryTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.policy = {
            "layout": {
                "managed_roots": ["apps", "crates", "adapters", "plugins", "sdk", "experiments"],
                "production_roots": ["apps", "crates", "adapters"],
                "excluded_directories": ["target", "node_modules", "__pycache__", "build", "dist"],
                "workspace_manifests": ["Cargo.toml"],
            },
            "packages": {"core": {
                "manifest": "crates/core/Cargo.toml", "allowed_dependencies": [],
            }},
        }
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/core"]\n')
        self.write("crates/core/Cargo.toml", '[package]\nname = "core"\n')

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")
        return destination

    def errors(self):
        return validate_inventory(self.root, self.policy)

    def register_independent(self, section="experiments", name="probe"):
        manifest = f"{section}/{name}/Cargo.toml"
        self.policy.setdefault(section, {})[name] = {"packages": {name: {
            "manifest": manifest, "allowed_dependencies": [],
        }}}
        self.policy["layout"]["workspace_manifests"].append(manifest)
        self.write(manifest, f'[package]\nname = "{name}"\n[workspace]\n')
        return manifest

    def test_accepts_current_style_and_registered_independent_workspaces(self):
        for section in ("experiments", "plugins", "sdk"):
            self.register_independent(section, section + "-probe")
        self.write("experiments/virtual/Cargo.toml", "[workspace]\nmembers = []\n")
        self.policy["layout"]["workspace_manifests"].append("experiments/virtual/Cargo.toml")
        self.assertEqual(self.errors(), [])

    def test_detects_unregistered_packages_in_each_managed_root(self):
        for directory in self.policy["layout"]["managed_roots"]:
            with self.subTest(directory=directory):
                manifest = f"{directory}/unregistered/Cargo.toml"
                self.write(manifest, '[package]\nname = "hidden"\n')
                self.assertIn(f"Unregistered package manifest: {manifest}", self.errors())

    def test_rejects_undocumented_virtual_and_combined_workspaces(self):
        manifest = "experiments/hidden/Cargo.toml"
        self.write(manifest, "[workspace]\n")
        self.assertIn(f"Unregistered Workspace manifest: {manifest}", self.errors())
        self.register_independent(name="combined")
        combined = "experiments/combined/Cargo.toml"
        self.policy["layout"]["workspace_manifests"].remove(combined)
        self.assertIn(f"Unregistered Workspace manifest: {combined}", self.errors())

    def test_registered_path_does_not_allow_a_different_package(self):
        self.write("crates/core/Cargo.toml", '[package]\nname = "renamed"\n')
        self.assertTrue(any("identity does not match" in error for error in self.errors()))

    def test_rejects_missing_package_and_workspace_registrations(self):
        self.policy["packages"]["missing"] = {"manifest": "apps/missing/Cargo.toml"}
        self.policy["layout"]["workspace_manifests"].append("experiments/missing/Cargo.toml")
        errors = self.errors()
        self.assertTrue(any("Registered package manifest is missing" in error for error in errors))
        self.assertTrue(any("Workspace manifest is missing" in error for error in errors))

    def test_rejects_duplicate_registration_and_cross_scope_registration(self):
        self.policy["experiments"] = {"bad": {"packages": {
            "core": {"manifest": "crates/core/Cargo.toml"},
        }}}
        errors = self.errors()
        self.assertTrue(any("Duplicate package registration" in error for error in errors))
        self.assertTrue(any("Invalid experiments package" in error for error in errors))

    def test_rejects_production_registration_under_isolated_directory(self):
        self.policy["packages"]["core"]["manifest"] = "plugins/core/Cargo.toml"
        self.write("plugins/core/Cargo.toml", '[package]\nname = "core"\n')
        self.assertTrue(any("Invalid production package" in error for error in self.errors()))

    def test_skips_hidden_build_and_cache_trees_even_with_invalid_manifests(self):
        for directory in (".cache/nsh", ".local/scratch", "target", "node_modules", "__pycache__", "build", "dist"):
            self.write(f"experiments/probe/{directory}/Cargo.toml", "invalid toml[[")
        self.write(".cache/nsh/Cargo.toml", "invalid toml[[")
        self.assertEqual(self.errors(), [])

    def test_excluded_source_cannot_be_registered_as_a_real_package(self):
        self.policy["packages"]["hidden"] = {"manifest": "crates/.local/hidden/Cargo.toml"}
        self.write("crates/.local/hidden/Cargo.toml", '[package]\nname = "hidden"\n')
        self.assertTrue(any("Invalid production package" in error for error in self.errors()))

    def test_invalid_managed_manifest_is_reported_without_crashing(self):
        self.write("adapters/invalid/Cargo.toml", "[invalid")
        self.assertTrue(any("Cannot read managed manifest" in error for error in self.errors()))

    def test_rejects_manifest_with_no_package_or_workspace(self):
        self.write("sdk/empty/Cargo.toml", "[dependencies]\n")
        self.assertTrue(any("neither package nor workspace" in error for error in self.errors()))

    def test_does_not_follow_source_directory_or_manifest_symlinks(self):
        external_scratch = tempfile.TemporaryDirectory()
        self.addCleanup(external_scratch.cleanup)
        external = Path(external_scratch.name)
        (external / "Cargo.toml").write_text("malformed TOML [[", encoding="utf-8")
        linked = self.root / "plugins/linked"
        linked.parent.mkdir()
        try:
            linked.symlink_to(external, target_is_directory=True)
            (self.root / "crates/manifest-link").mkdir()
            (self.root / "crates/manifest-link/Cargo.toml").symlink_to(external / "Cargo.toml")
        except OSError:
            self.skipTest("This platform does not allow test symlinks")
        errors = self.errors()
        self.assertTrue(any("directory must not be a symlink" in error for error in errors))
        self.assertTrue(any("manifest must not be a symlink" in error for error in errors))
        self.assertFalse(any("Cannot read" in error for error in errors))

    def test_does_not_follow_a_managed_root_symlink(self):
        outside = self.root.parent / (self.root.name + "-missing")
        try:
            (self.root / "sdk").symlink_to(outside, target_is_directory=True)
        except OSError:
            self.skipTest("This platform does not allow test symlinks")
        self.assertTrue(any("directory must not be a symlink: sdk" in error for error in self.errors()))


class ProductionSourceBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[2]
        self.policy = {"layout": {"production_roots": ["apps", "crates", "adapters"]},
                       "packages": {"core": {
                           "manifest": "crates/core/Cargo.toml", "allowed_dependencies": ["helper"],
                       }}}
        self.package = {"id": "core-id", "name": "core", "source": None,
                        "manifest_path": str(self.root / "crates/core/Cargo.toml"),
                        "publish": [], "dependencies": []}
        self.metadata = {"packages": [self.package], "workspace_members": ["core-id"]}

    def errors(self):
        return validate_graph(self.metadata, self.policy, self.root)

    def test_accepts_registered_production_source(self):
        self.assertEqual(self.errors(), [])

    def test_root_workspace_cannot_absorb_isolated_packages_even_if_allowlisted(self):
        for directory in ("plugins", "sdk", "experiments"):
            with self.subTest(directory=directory):
                manifest = f"{directory}/core/Cargo.toml"
                self.package["manifest_path"] = str(self.root / manifest)
                self.policy["packages"]["core"]["manifest"] = manifest
                self.assertTrue(any("Workspace member: production cannot use" in error for error in self.errors()))

    def test_all_dependency_kinds_and_targets_cannot_use_isolated_sources(self):
        for directory in ("plugins", "sdk", "experiments"):
            for kind in (None, "dev", "build"):
                with self.subTest(directory=directory, kind=kind):
                    self.package["dependencies"] = [{
                        "name": "helper", "rename": "safe_alias", "kind": kind,
                        "target": 'cfg(target_os = "ios")', "optional": True,
                        "path": str(self.root / directory / "helper"),
                    }]
                    errors = self.errors()
                    self.assertTrue(any("path dependency helper" in error and "production cannot use" in error for error in errors))

    def test_inspects_path_declarations_of_resolved_helpers(self):
        self.policy["packages"]["helper"] = {
            "manifest": "crates/helper/Cargo.toml", "allowed_dependencies": [],
        }
        self.metadata["packages"].append({
            "id": "helper-id", "name": "helper", "source": None,
            "manifest_path": str(self.root / "crates/helper/Cargo.toml"),
            "dependencies": [{"name": "hidden", "kind": "build", "optional": True,
                              "target": 'cfg(target_os = "android")',
                              "path": str(self.root / "experiments/hidden")}],
        })
        errors = self.errors()
        self.assertTrue(any("helper: build path dependency hidden" in error for error in errors))

    def test_rejects_isolated_local_package_even_without_a_direct_path_edge(self):
        self.metadata["packages"].append({
            "id": "helper-id", "name": "helper", "source": None,
            "manifest_path": str(self.root / "plugins/helper/Cargo.toml"), "dependencies": [],
        })
        self.assertTrue(any("helper local dependency: production cannot use" in error for error in self.errors()))

    def test_rejects_unregistered_or_external_local_path_dependencies(self):
        for path, expected in ((self.root / "adapters/unknown", "unregistered production"),
                               (self.root.parent / "outside", "outside the repository"),
                               (self.root / ".local/hidden", "non-production")):
            with self.subTest(path=path):
                self.package["dependencies"] = [{"name": "helper", "path": str(path)}]
                self.assertTrue(any(expected in error for error in self.errors()))

    def test_parent_segments_cannot_disguise_an_isolated_dependency(self):
        self.package["dependencies"] = [{"name": "helper", "path": str(self.root / "crates/../experiments/helper")}]
        self.assertTrue(any("production cannot use" in error for error in self.errors()))

    def test_independent_validate_graph_api_does_not_apply_production_layout(self):
        self.policy.pop("layout")
        self.policy["packages"]["core"]["manifest"] = "experiments/core/Cargo.toml"
        self.package["manifest_path"] = str(self.root / "experiments/core/Cargo.toml")
        self.assertEqual(self.errors(), [])

if __name__ == "__main__":
    unittest.main()
