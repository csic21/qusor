import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from check_app_boundary import (
    FROZEN_CORE_FACADE_HOST_FNS,
    FROZEN_LEGACY_BYPASS_FNS,
    check_app_boundary,
)


def make_clean_app(root: Path) -> None:
    src = root / "apps" / "qusor" / "native" / "src"
    src.mkdir(parents=True)

    host_fns = "\n".join(
        f"pub(crate) fn {name}() {{}}" for name in sorted(FROZEN_CORE_FACADE_HOST_FNS)
    )
    (src / "core_facade.rs").write_text(
        f"use learning_application::KnowledgeApplication;\n{host_fns}\n",
        encoding="utf-8",
    )
    (src / "vault.rs").write_text(
        'const DEFAULT_VAULT_ID: &str = "qusor-vault";\n'
        '#[cfg(test)]\nmod tests {\n    const ID: &str = "org.qusor.notes";\n}\n',
        encoding="utf-8",
    )
    (root / "apps" / "qusor" / "android-jni" / "src").mkdir(parents=True)
    (root / "apps" / "qusor" / "android-jni" / "src" / "lib.rs").write_text(
        'const PLATFORM_CLASS: &str = "org.qusor.app.QusorPlatform";\n',
        encoding="utf-8",
    )
    (root / "apps" / "qusor" / "shell").mkdir(parents=True)
    (root / "apps" / "qusor" / "shell" / "catalog.tsx").write_text(
        "export function refreshWorkspaceCatalog() { return true; }\n",
        encoding="utf-8",
    )


class CheckAppBoundaryTest(unittest.TestCase):
    def test_repo_passes(self) -> None:
        repo_root = Path(__file__).resolve().parent.parent.parent
        self.assertEqual(check_app_boundary(repo_root), [])

    def test_rejects_direct_core_call_outside_facade(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            (root / "apps" / "qusor" / "native" / "src" / "notes_native.rs").write_text(
                "pub fn bad() { let _ = KnowledgeApplication::new(()); }\n",
                encoding="utf-8",
            )
            violations = check_app_boundary(root)
            self.assertEqual(len(violations), 1)
            self.assertIn("notes_native.rs", violations[0])

    def test_rejects_new_legacy_bypass_function_in_facade(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            facade = root / "apps" / "qusor" / "native" / "src" / "core_facade.rs"
            facade.write_text(
                facade.read_text(encoding="utf-8")
                + "\npub(crate) mod legacy_plugin_bypass {\n    pub(crate) fn extra_bypass() {}\n}\n",
                encoding="utf-8",
            )
            violations = check_app_boundary(root)
            self.assertEqual(len(violations), 1)
            self.assertIn("extra_bypass", violations[0])

    def test_rejects_hardcoded_vault_and_package_id_branch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            (root / "apps" / "qusor" / "native" / "src" / "runtime.rs").write_text(
                'pub fn bad() { let _ = ("qusor-vault", "org.qusor.vocab"); }\n',
                encoding="utf-8",
            )
            violations = check_app_boundary(root)
            self.assertEqual(len(violations), 2)
            joined = "\n".join(violations)
            self.assertIn("qusor-vault", joined)
            self.assertIn("org.qusor.vocab", joined)

    def test_rejects_new_hardcoded_workspace_renderer_kind(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            catalog = root / "apps" / "qusor" / "shell" / "catalog.tsx"
            catalog.write_text(
                "const workspaceRenderers = {\n"
                "  notes_workspace: () => null,\n"
                "  ai_workspace: () => null,\n"
                "};\n",
                encoding="utf-8",
            )
            violations = check_app_boundary(root)
            self.assertEqual(len(violations), 1)
            self.assertIn("ai_workspace", violations[0])

    def test_allows_cfg_test_and_integration_tests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            (root / "apps" / "qusor" / "native" / "src" / "extra_test_mod.rs").write_text(
                'pub fn clean() {}\n#[cfg(all(test, unix))]\nmod tests {\n    const ID: &str = "org.qusor.vocab";\n}\n',
                encoding="utf-8",
            )
            tests_dir = root / "apps" / "qusor" / "native" / "tests"
            tests_dir.mkdir(parents=True)
            (tests_dir / "integration.rs").write_text(
                'fn t() { let _ = ("qusor-vault", "org.qusor.notes"); KnowledgeApplication::new(()); }\n',
                encoding="utf-8",
            )
            self.assertEqual(check_app_boundary(root), [])

    def test_allows_shrinking_legacy_bypass_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_clean_app(root)
            facade = root / "apps" / "qusor" / "native" / "src" / "core_facade.rs"
            facade.write_text(
                facade.read_text(encoding="utf-8").replace(
                    "    pub(crate) fn vocab_delete_wordbook() {}\n",
                    "",
                ),
                encoding="utf-8",
            )
            self.assertEqual(check_app_boundary(root), [])


if __name__ == "__main__":
    unittest.main()
