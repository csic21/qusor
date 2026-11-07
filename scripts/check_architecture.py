"""Check all declared Cargo dependency kinds against the foundation allowlist."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def validate_graph(metadata: dict, policy: dict, root: Path) -> list[str]:
    rules = policy.get("packages", {})
    packages = {package["id"]: package for package in metadata["packages"]}
    members = [packages[identifier] for identifier in metadata["workspace_members"]]
    errors = []
    names = {package["name"] for package in members}
    for name in sorted(set(rules) - names):
        errors.append(f"Policy package is missing from workspace: {name}")
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
    print("Architecture guard passed: workspace and all declared dependency kinds.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
