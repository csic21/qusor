#!/usr/bin/env python3
"""Enforce P0-1 stop-bleeding boundaries on apps/qusor.

Freezes the baseline so no new direct Core bypasses, hardcoded vault IDs,
hardcoded official plugin package/entry ID branches, or hardcoded workspace
kind renderers can be added to the application shell.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SKIP_DIRS = {".git", ".nsh", "target", "build", "node_modules"}

DIRECT_CORE_ALLOWLIST = frozenset({
    "apps/qusor/native/src/core_facade.rs",
    "apps/qusor/bridges/src/library_bridge.rs",
    "apps/qusor/bridges/src/notes_bridge.rs",
    "apps/qusor/bridges/src/orthogonal_bridge.rs",
    "apps/qusor/bridges/src/vocab_bridge.rs",
})

VAULT_ID_ALLOWLIST = frozenset({
    "apps/qusor/native/src/vault.rs",
})

FROZEN_CORE_FACADE_HOST_FNS = frozenset({
    "new_library_bridge",
    "new_notes_bridge",
    "new_orthogonal_bridge",
    "new_vocab_bridge",
    "export_backup_archives",
    "inspect_backup_state",
    "restore_knowledge_archive",
    "restore_study_archive",
    "ensure_starter_wordbook",
    "import_pdf_document",
    "query_pdf_document",
    "read_original_pdf",
    "import_epub_document",
    "import_wordbook",
})

FROZEN_LEGACY_BYPASS_FNS = frozenset({
    "delete_pdf_document",
    "delete_epub_document",
    "epub_documents_page",
    "epub_toc",
    "epub_chapter_text",
    "epub_saved_selections",
    "capture_epub_selection",
    "epub_notes_page",
    "epub_note_body",
    "update_pdf_capture_note",
    "update_epub_capture_note",
    "delete_pdf_capture",
    "delete_epub_capture",
})

FROZEN_WORKSPACE_RENDERER_KINDS = frozenset({
    "notes_workspace",
    "reader_workspace",
    "library_workspace",
})

DIRECT_CORE_RE = re.compile(r"\b(?:KnowledgeApplication|StudyApplication)\b")
VAULT_ID_RE = re.compile(r'"qusor-vault"')
QUOTED_PACKAGE_OR_ENTRY_ID_RE = re.compile(r'"((?:org|app)\.qusor\.[A-Za-z0-9_.-]+)"')
ALLOWED_QUOTED_APP_IDS = frozenset({
    "org.qusor.app",
    "org.qusor.app.QusorPlatform",
})

WORKSPACE_RENDERERS_BLOCK_RE = re.compile(
    r"const\s+workspaceRenderers\b[^=]*=\s*\{(?P<body>.*?)\n\};",
    re.DOTALL,
)
RENDERER_KEY_RE = re.compile(r"^([ \t]+)([a-z][a-z0-9_]*)\s*:", re.MULTILINE)
RUST_PUB_CRATE_FN_RE = re.compile(r"^\s*pub\(crate\)\s+fn\s+([A-Za-z0-9_]+)\s*\(", re.MULTILINE)
RUST_LEGACY_MOD_RE = re.compile(r"\bpub\(crate\)\s+mod\s+legacy_plugin_bypass\s*\{")
RUST_CFG_TEST_MOD_RE = re.compile(r"^\s*#\[cfg\([^\n]*\btest\b[^\n]*\)\]\s*mod\s+", re.MULTILINE)


def is_test_path(rel_path: str) -> bool:
    parts = Path(rel_path).parts
    return "tests" in parts or "test" in parts


def strip_rust_comments_and_cfg_test(text: str) -> str:
    """Return non-test Rust code with // comments and #[cfg(...test...)] mod blocks stripped."""
    cfg_test_match = RUST_CFG_TEST_MOD_RE.search(text)
    if cfg_test_match:
        text = text[: cfg_test_match.start()]
    lines = []
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        lines.append(line)
    return "\n".join(lines)


def strip_ts_comments(text: str) -> str:
    """Strip single-line // and block /* ... */ comments for boundary scans."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    lines = [line for line in text.splitlines() if not line.lstrip().startswith("//")]
    return "\n".join(lines)


def iter_app_source_files(app_root: Path) -> list[tuple[str, Path]]:
    if not app_root.is_dir():
        return []
    files: list[tuple[str, Path]] = []
    for path in sorted(app_root.rglob("*")):
        if not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.suffix not in {".rs", ".ts", ".tsx"}:
            continue
        rel = path.relative_to(app_root.parent.parent).as_posix()
        files.append((rel, path))
    return files


def check_core_facade_freeze(rel: str, text: str) -> list[str]:
    violations: list[str] = []
    mod_match = RUST_LEGACY_MOD_RE.search(text)
    if not mod_match:
        return [f"{rel}: missing `pub(crate) mod legacy_plugin_bypass`"]

    host_part = text[: mod_match.start()]
    legacy_part = text[mod_match.end() :]

    host_fns = set(RUST_PUB_CRATE_FN_RE.findall(host_part))
    legacy_fns = set(RUST_PUB_CRATE_FN_RE.findall(legacy_part))

    extra_host = sorted(host_fns - FROZEN_CORE_FACADE_HOST_FNS)
    if extra_host:
        violations.append(
            f"{rel}: unapproved functions added to core_facade host section: {extra_host}"
        )

    extra_legacy = sorted(legacy_fns - FROZEN_LEGACY_BYPASS_FNS)
    if extra_legacy:
        violations.append(
            f"{rel}: new legacy_plugin_bypass function(s) forbidden (baseline frozen): {extra_legacy}"
        )

    return violations


def check_workspace_renderers_freeze(rel: str, text: str) -> list[str]:
    violations: list[str] = []
    for match in WORKSPACE_RENDERERS_BLOCK_RE.finditer(text):
        pairs = RENDERER_KEY_RE.findall(match.group("body"))
        if not pairs:
            continue
        min_indent = min(len(indent) for indent, _ in pairs)
        keys = {key for indent, key in pairs if len(indent) == min_indent}
        extra = sorted(keys - FROZEN_WORKSPACE_RENDERER_KINDS)
        if extra:
            violations.append(
                f"{rel}: new hardcoded workspaceRenderers key(s) forbidden (baseline frozen): {extra}"
            )
    return violations


def check_app_boundary(root: Path = ROOT) -> list[str]:
    app_root = root / "apps" / "qusor"
    if not app_root.is_dir():
        return [f"missing directory: {app_root.relative_to(root)}"]

    violations: list[str] = []
    for rel, path in iter_app_source_files(app_root):
        if is_test_path(rel):
            continue
        raw = path.read_text(encoding="utf-8")
        non_test = (
            strip_rust_comments_and_cfg_test(raw)
            if path.suffix == ".rs"
            else strip_ts_comments(raw)
        )

        if DIRECT_CORE_RE.search(non_test) and rel not in DIRECT_CORE_ALLOWLIST:
            violations.append(
                f"{rel}: direct KnowledgeApplication/StudyApplication use outside core_facade/bridges"
            )

        if rel == "apps/qusor/native/src/core_facade.rs":
            violations.extend(check_core_facade_freeze(rel, non_test))

        if VAULT_ID_RE.search(non_test) and rel not in VAULT_ID_ALLOWLIST:
            violations.append(
                f'{rel}: hardcoded "qusor-vault" outside vault.rs DEFAULT_VAULT_ID'
            )

        for pkg_match in QUOTED_PACKAGE_OR_ENTRY_ID_RE.finditer(non_test):
            ident = pkg_match.group(1)
            if ident not in ALLOWED_QUOTED_APP_IDS:
                violations.append(
                    f'{rel}: hardcoded package/entry ID "{ident}" in non-test app code'
                )

        if path.suffix in {".ts", ".tsx"}:
            violations.extend(check_workspace_renderers_freeze(rel, non_test))

    return violations


def main() -> int:
    violations = check_app_boundary(ROOT)
    if violations:
        print("App boundary check failed:", file=sys.stderr)
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        return 1
    print("App boundary check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
