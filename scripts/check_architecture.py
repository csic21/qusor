"""Check Cargo declarations and resolved dependency closures against policy.

Declarations cover every workspace normal/dev/build/optional/target dependency.
Resolved closure checks cover edges emitted by Cargo metadata --all-features;
they do not prove anything about third-party features Cargo did not resolve.
"""
from __future__ import annotations

import json
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
    return errors


def main() -> int:
    with (ROOT / "config/architecture.toml").open("rb") as stream:
        policy = tomllib.load(stream)
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
    print("Architecture guard passed: workspace declarations and configured resolved closures.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
