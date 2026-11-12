"""Reject Web/DOM UI integration in QUSOR apps and plugins.

This is deliberately stricter than a general source linter: NSH TSX is a native
UI description, while WebView/HTML/DOM are not an allowed plugin UI carrier.
Rust manifests and sources are checked too, so a native-looking TSX surface
cannot delegate to Tauri/wry/WebView underneath. Document-content engine
exceptions require their own reviewed adapter, not a webpage mounted as a
plugin workspace.
"""
from __future__ import annotations

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
ROOTS = (ROOT / "apps" / "qusor", ROOT / "plugins")
UI_SUFFIXES = {".ts", ".tsx", ".js", ".jsx"}
WEB_FILE_SUFFIXES = {".html", ".htm", ".css"}
RUST_SUFFIXES = {".rs"}
MANIFEST_SUFFIXES = {".toml", ".json"}
SKIP_DIRS = {"target", "node_modules", "dist", "build", "__pycache__"}

FORBIDDEN_IMPORTS = re.compile(
    r"""\bfrom\s+["'](?:nsh/web|nsh/ui/web|react-dom(?:/[^"']*)?|electron(?:/[^"']*)?|@tauri-apps/[^"']+|tauri(?:/[^"']*)?)["']"""
)
FORBIDDEN_MARKUP = re.compile(
    r"(?i)(?:<\s*(?:webview|iframe)\b|dangerouslySetInnerHTML|\.\s*(?:innerHTML|outerHTML)\b)"
)
FORBIDDEN_GLOBALS = re.compile(r"(?<![A-Za-z0-9_$])(?:document|window)\s*\.")
FORBIDDEN_RUST_DEPENDENCY = re.compile(
    r"""(?im)^\s*(?:
        (?:tauri(?:-[A-Za-z0-9_-]+)?|wry|webview2-com|webkit2gtk|electron)\s*=
        |
        \[(?:.*\.)?(?:tauri|wry|webview2-com|webkit2gtk|electron)\]
    )""",
    re.VERBOSE,
)
FORBIDDEN_RUST_API = re.compile(
    r"\b(?:tauri|wry|webkit2gtk|webview2_com)\s*::"
)
FORBIDDEN_MANIFEST_RENDERER = re.compile(
    r"""(?i)
        "(?:renderer|carrier|ui)"\s*:\s*"(?:web|webview|browser)"
        |
        "kind"\s*:\s*"(?:web|webview|browser|html)_[^"]*"
    """,
    re.VERBOSE,
)


def check_file(path: Path) -> list[str]:
    try:
        relative = path.relative_to(ROOT).as_posix()
    except ValueError:
        relative = path.name
    if path.suffix.lower() in WEB_FILE_SUFFIXES:
        return [f"{relative}: web UI file is not allowed in apps/plugins UI"]
    suffix = path.suffix.lower()
    if suffix not in UI_SUFFIXES | RUST_SUFFIXES | MANIFEST_SUFFIXES:
        return []
    source = path.read_text(encoding="utf-8-sig")
    errors = []
    checks = []
    if suffix in UI_SUFFIXES:
        checks.extend(
            (
                ("Web/DOM import", FORBIDDEN_IMPORTS),
                ("Web/DOM markup", FORBIDDEN_MARKUP),
                ("browser DOM global", FORBIDDEN_GLOBALS),
            )
        )
    if suffix in RUST_SUFFIXES | {".toml"}:
        checks.append(("WebView Rust dependency", FORBIDDEN_RUST_DEPENDENCY))
    if suffix in RUST_SUFFIXES:
        checks.append(("WebView Rust API", FORBIDDEN_RUST_API))
    if suffix in MANIFEST_SUFFIXES:
        checks.append(("Web UI manifest entry", FORBIDDEN_MANIFEST_RENDERER))
    for label, pattern in checks:
        if pattern.search(source):
            errors.append(f"{relative}: {label} is not allowed")
    return errors


def main() -> int:
    errors = []
    count = 0
    for root in ROOTS:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            parts = path.relative_to(root).parts
            if (
                not path.is_file()
                or any(part.startswith(".") or part in SKIP_DIRS for part in parts)
            ):
                continue
            count += 1
            errors.extend(check_file(path))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Native UI boundary passed in {count} app/plugin files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
