"""Check managed Cargo sources, declarations and resolved dependency closures.

Declarations cover every workspace normal/dev/build/optional/target dependency.
Resolved closure checks cover edges emitted by Cargo metadata --all-features;
they do not prove anything about third-party features Cargo did not resolve.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def resolved_nodes(metadata: dict) -> tuple[dict | None, list[str]]:
    """Require a complete resolve section when transitive policy is enabled."""
    resolve = metadata.get("resolve")
    if not isinstance(resolve, dict) or not isinstance(resolve.get("nodes"), list):
        return None, ["Cargo resolve.nodes is missing; cannot check dependency closure"]
    nodes = {}
    errors = []
    for node in resolve["nodes"]:
        if not isinstance(node, dict) or not isinstance(node.get("id"), str):
            errors.append("Cargo resolve contains a node without a package ID")
            continue
        identifier = node["id"]
        if identifier in nodes:
            errors.append(f"Cargo resolve contains a duplicate node: {identifier}")
        nodes[identifier] = node
    return nodes, errors


def resolved_edges(node: dict) -> tuple[set[str], list[str]]:
    """Inspect every resolved kind/target; edge names can be renamed aliases."""
    errors = []
    edges = set()
    # Current Cargo emits both arrays. Use their union so missing or inconsistent
    # information in one representation cannot silently hide a resolved edge.
    if not isinstance(node.get("dependencies"), list) and not isinstance(node.get("deps"), list):
        return edges, [f"Cargo resolve node has no dependency edges: {node['id']}"]
    identifiers = node.get("dependencies", [])
    if not isinstance(identifiers, list):
        errors.append(f"Cargo resolve node has invalid dependency IDs: {node['id']}")
        identifiers = []
    for identifier in identifiers:
        if not isinstance(identifier, str):
            errors.append(f"Cargo resolve node has an invalid dependency: {node['id']}")
        else:
            edges.add(identifier)
    dependencies = node.get("deps", [])
    if not isinstance(dependencies, list):
        errors.append(f"Cargo resolve node has invalid dependency edges: {node['id']}")
        dependencies = []
    for dependency in dependencies:
        if not isinstance(dependency, dict) or not isinstance(dependency.get("pkg"), str):
            errors.append(f"Cargo resolve node has an invalid dependency edge: {node['id']}")
        else:
            edges.add(dependency["pkg"])
    return edges, errors


def validate_closure(package: dict, rule: dict, packages: dict, nodes: dict) -> list[str]:
    """Walk by package ID and authorize actual package names, never edge aliases."""
    root_name = package["name"]
    allowed = set(rule["allowed_transitive_dependencies"])
    errors = []
    pending = [(package["id"], [root_name])]
    visited = set()
    while pending:
        identifier, path = pending.pop()
        if identifier in visited:
            continue
        visited.add(identifier)
        resolved_package = packages.get(identifier)
        if resolved_package is None:
            errors.append(f"{root_name}: resolved package metadata is missing: {identifier}")
        elif identifier != package["id"] and resolved_package["name"] not in allowed:
            errors.append(
                f"{root_name}: unapproved resolved dependency "
                f"{resolved_package['name']} (path: {' -> '.join(path)})"
            )
        node = nodes.get(identifier)
        if node is None:
            errors.append(f"{root_name}: resolved dependency node is missing: {identifier}")
            continue
        edges, edge_errors = resolved_edges(node)
        errors.extend(edge_errors)
        for target in sorted(edges, reverse=True):
            target_name = packages.get(target, {}).get("name", target)
            pending.append((target, [*path, target_name]))
    return errors


def validate_graph(metadata: dict, policy: dict, root: Path) -> list[str]:
    rules = policy.get("packages", {})
    packages = {package["id"]: package for package in metadata["packages"]}
    errors = []
    members = []
    for identifier in metadata["workspace_members"]:
        if identifier not in packages:
            errors.append(f"Workspace package metadata is missing: {identifier}")
        else:
            members.append(packages[identifier])
    names = {package["name"] for package in members}
    for name in sorted(set(rules) - names):
        errors.append(f"Policy package is missing from workspace: {name}")
    nodes = None
    if any("allowed_transitive_dependencies" in rule for rule in rules.values()):
        nodes, resolve_errors = resolved_nodes(metadata)
        errors.extend(resolve_errors)
    for package in members:
        name = package["name"]
        if name not in rules:
            errors.append(f"Unregistered workspace package: {name}")
            continue
        rule = rules[name]
        try:
            actual = Path(package["manifest_path"]).resolve().relative_to(root.resolve())
        except ValueError:
            errors.append(f"{name}: manifest is outside the repository")
            continue
        if actual.as_posix() != rule["manifest"]:
            errors.append(f"{name}: unexpected manifest path {actual.as_posix()}")
        if package.get("publish") != []:
            errors.append(f"{name}: foundation package must set publish = false")
        allowed = set(rule["allowed_dependencies"])
        # Cargo declarations include optional and target-specific dependencies,
        # even when they are inactive on the machine running this check.
        # 'name' is the package name, not a possibly misleading renamed alias.
        for dependency in package["dependencies"]:
            if dependency["name"] not in allowed:
                kind = dependency.get("kind") or "normal"
                target = dependency.get("target") or "all targets"
                errors.append(
                    f"{name}: unapproved {kind} dependency "
                    f"{dependency['name']} ({target})"
                )
        if "allowed_transitive_dependencies" in rule and nodes is not None:
            errors.extend(validate_closure(package, rule, packages, nodes))
    if "layout" in policy:
        errors.extend(validate_production_layout(metadata, policy, root))
    return errors


def registered_manifests(policy: dict) -> tuple[dict, list[str]]:
    """Collect package identities without importing other Workspace policies."""
    registered = {}
    errors = []
    groups = [("production", policy.get("packages", {}))]
    for section in ("experiments", "plugins", "sdk"):
        for group in policy.get(section, {}).values():
            groups.append((section, group.get("packages", {})))
    for section, packages in groups:
        for name, rule in packages.items():
            manifest = rule["manifest"]
            if manifest in registered:
                errors.append(f"Duplicate package registration: {manifest}")
            registered[manifest] = (name, section)
    return registered, errors


def validate_inventory(root: Path, policy: dict) -> list[str]:
    """Inventory managed Cargo sources; do not follow symlinks or scan caches.

    This checks registration, not independent Workspace dependency resolution.
    Their own runners must still call validate_graph with their own metadata.
    """
    root = root.resolve()
    layout = policy["layout"]
    managed = set(layout["managed_roots"])
    production = set(layout["production_roots"])
    excluded = set(layout["excluded_directories"])
    workspaces = set(layout["workspace_manifests"])
    registered, errors = registered_manifests(policy)
    discovered = {}

    def read_manifest(path):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            errors.append(f"Managed manifest must not be a symlink: {relative}")
            return
        try:
            # Check before opening; this guard is not a race-proof sandbox.
            path.resolve().relative_to(root)
            with path.open("rb") as stream:
                discovered[relative] = tomllib.load(stream)
        except (OSError, ValueError) as error:
            errors.append(f"Cannot read managed manifest {relative}: {error}")

    read_manifest(root / "Cargo.toml")
    for directory in sorted(managed):
        if Path(directory).name != directory or directory.startswith("."):
            errors.append(f"Invalid managed root: {directory}")
            continue
        base = root / directory
        if base.is_symlink():
            errors.append(f"Managed source directory must not be a symlink: {directory}")
            continue
        if not base.exists():
            continue
        for current, directories, files in os.walk(
            base, followlinks=False,
            onerror=lambda error: errors.append(f"Cannot scan managed sources: {error}"),
        ):
            current = Path(current)
            kept = []
            for child in sorted(directories):
                if child.startswith(".") or child in excluded:
                    continue
                candidate = current / child
                if candidate.is_symlink():
                    errors.append(
                        "Managed source directory must not be a symlink: "
                        + candidate.relative_to(root).as_posix()
                    )
                else:
                    kept.append(child)
            directories[:] = kept
            if "Cargo.toml" in files:
                read_manifest(current / "Cargo.toml")

    for manifest, (name, section) in sorted(registered.items()):
        path = Path(manifest)
        allowed_roots = production if section == "production" else {section}
        if (path.is_absolute() or ".." in path.parts or not path.parts
                or path.parts[0] not in allowed_roots
                or path.name != "Cargo.toml"
                or any(part.startswith(".") or part in excluded for part in path.parts)):
            errors.append(f"Invalid {section} package manifest registration: {manifest}")
        document = discovered.get(manifest)
        if document is None:
            errors.append(f"Registered package manifest is missing from managed sources: {manifest}")
        elif document.get("package", {}).get("name") != name:
            errors.append(f"Registered package identity does not match {manifest}: {name}")
    for manifest in sorted(workspaces):
        if "workspace" not in discovered.get(manifest, {}):
            errors.append(f"Registered Workspace manifest is missing or has no workspace: {manifest}")
    for manifest, document in sorted(discovered.items()):
        if "package" in document and manifest not in registered:
            errors.append(f"Unregistered package manifest: {manifest}")
        if "workspace" in document and manifest not in workspaces:
            errors.append(f"Unregistered Workspace manifest: {manifest}")
        if "package" not in document and "workspace" not in document:
            errors.append(f"Cargo manifest has neither package nor workspace: {manifest}")
    return errors


def validate_production_layout(metadata: dict, policy: dict, root: Path) -> list[str]:
    """Reject production source dependencies on isolated or unregistered paths.

    Inspect every declaration Cargo reports, including those of resolved helpers.
    This does not inspect unresolved third-party optional dependency declarations,
    Rust include!/build-script file access, or independent plugin dependency graphs.
    """
    root = root.resolve()
    layout = policy["layout"]
    allowed_roots = set(layout["production_roots"])
    registered = {rule["manifest"] for rule in policy.get("packages", {}).values()}
    errors = []

    def check_manifest(manifest, context):
        try:
            relative = Path(manifest).resolve().relative_to(root)
        except ValueError:
            errors.append(f"{context}: local source manifest is outside the repository")
            return
        if not relative.parts or relative.parts[0] not in allowed_roots:
            errors.append(f"{context}: production cannot use isolated/non-production source {relative.as_posix()}")
        elif relative.as_posix() not in registered:
            errors.append(f"{context}: unregistered production source manifest {relative.as_posix()}")

    members = set(metadata["workspace_members"])
    for package in metadata["packages"]:
        name = package["name"]
        if package["id"] in members:
            check_manifest(package["manifest_path"], f"{name} Workspace member")
        elif package.get("source") is None and "manifest_path" in package:
            check_manifest(package["manifest_path"], f"{name} local dependency")
        for dependency in package.get("dependencies", []):
            if dependency.get("path") is not None:
                kind = dependency.get("kind") or "normal"
                target = dependency.get("target") or "all targets"
                check_manifest(
                    Path(dependency["path"]) / "Cargo.toml",
                    f"{name}: {kind} path dependency {dependency['name']} ({target})",
                )
    return errors


def main() -> int:
    with (ROOT / "config/architecture.toml").open("rb") as stream:
        policy = tomllib.load(stream)
    errors = validate_inventory(ROOT, policy)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked",
         "--offline", "--all-features"],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8",
        check=False,
    )
    if result.returncode:
        print(result.stderr, file=sys.stderr)
        return result.returncode
    errors = validate_graph(json.loads(result.stdout), policy, ROOT)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Architecture guard passed: managed manifest inventory, production source boundaries, workspace declarations and configured resolved closures.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
