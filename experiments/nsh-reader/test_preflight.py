"""Real loader failures and readiness gating; uses only the Python standard library."""

from contextlib import ExitStack
from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import preflight


class ProfileTests(unittest.TestCase):
    def collect_profile(self, profile="native", *, cargo_check=False, missing=None, native_prefix=None):
        calls = []
        environments = []

        def other_checks(argv, **_kwargs):
            calls.append(argv)
            environments.append((argv, _kwargs.get("env")))
            absent = argv[0] == "pkg-config" and argv[-1] == missing
            stdout = preflight.NSH_SHA if argv[-1] == "HEAD" else ""
            return {"command": argv, "exit_code": 1 if absent else 0,
                    "stdout": stdout, "stderr": "fixture package unavailable" if absent else ""}

        with ExitStack() as context:
            context.enter_context(patch.dict(preflight.os.environ, {"DISPLAY": ":test"}))
            context.enter_context(patch.object(preflight, "run", side_effect=other_checks))
            context.enter_context(patch.object(preflight.sys, "platform", "linux"))
            context.enter_context(patch.object(preflight.shutil, "which", return_value="/test/tool"))
            context.enter_context(patch.object(preflight, "digest", return_value="fixture-hash"))
            context.enter_context(patch.object(preflight, "probe_libclang", return_value={"usable": True}))
            report = preflight.collect(Path("/test/pinned-source"), cargo_check, profile, native_prefix)
        self.environments = environments
        return report, calls

    def test_local_prefix_environment_reaches_real_probe_boundaries_without_global_mutation(self):
        env = {"PKG_CONFIG_PATH": "/test/native/pkgconfig", "LD_LIBRARY_PATH": "/test/native/lib"}
        before = dict(preflight.os.environ)
        with patch("native_prefix.runtime_environment", return_value=env) as prepare:
            report, _ = self.collect_profile(cargo_check=True, native_prefix=Path("/test/native"))
        prepare.assert_called_once_with(Path("/test/native"))
        self.assertEqual(dict(preflight.os.environ), before)
        dependency_calls = [(argv, actual) for argv, actual in self.environments
                            if argv[0] == "pkg-config" or "check" in argv]
        self.assertTrue(dependency_calls)
        self.assertTrue(all(actual is env for _, actual in dependency_calls))
        self.assertTrue(all(actual is None for argv, actual in self.environments if argv[0] == "git"))
        self.assertEqual(report["native_prefix"]["environment"], env)
        self.assertEqual(report["cargo_check"]["status"], "passed")

    def test_cli_defaults_native_and_explicit_webview_is_preserved(self):
        self.assertEqual(preflight.parse_args([]).profile, "native")
        self.assertEqual(preflight.parse_args(["--profile", "webview", "--cargo-check"]).profile, "webview")
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
            preflight.parse_args(["--profile", "browser"])
        self.assertEqual(raised.exception.code, 2)

    def test_missing_webkit_does_not_gate_native_or_disappear_from_unverified_scope(self):
        report, calls = self.collect_profile(missing="webkitgtk-6.0")
        self.assertEqual(report["status"], "prerequisites_ready")
        self.assertEqual(report["profile"], "native")
        self.assertEqual(report["cargo_features"], ["js"])
        self.assertNotIn("webkitgtk-6.0", report["native_packages"])
        self.assertFalse(any("webkitgtk-6.0" in call for call in calls))
        self.assertIn("WebView excluded", report["scope"])
        self.assertIn("WebView build/embedding (excluded by native profile)", report["unverified"])
        self.assertIn("PDF/EPUB rendering", report["unverified"])
        self.assertEqual(report["cargo_check"]["status"], "not_run")

    def test_missing_webkit_blocks_webview_profile(self):
        report, _calls = self.collect_profile("webview", missing="webkitgtk-6.0")
        self.assertEqual(report["status"], "blocked")
        self.assertEqual(report["cargo_features"], ["js", "webview"])
        self.assertFalse(report["native_packages"]["webkitgtk-6.0"]["usable"])
        self.assertEqual(len(report["blockers"]), 1)
        self.assertIn("webkitgtk-6.0 >= 2.40", report["blockers"][0])

    def test_native_keeps_required_gtk_graphene_and_gstreamer_gates(self):
        report, _calls = self.collect_profile(missing="gstreamer-audio-1.0")
        self.assertEqual(set(report["native_packages"]), {
            "gtk4", "pango", "pangocairo", "cairo", "graphene-gobject-1.0",
            "gstreamer-1.0", "gstreamer-app-1.0", "gstreamer-audio-1.0",
        })
        self.assertEqual(report["status"], "blocked")
        self.assertIn("gstreamer-audio-1.0", report["blockers"][0])

    def test_optional_cargo_check_uses_exact_selected_feature_set(self):
        for profile, expected in [("native", "js"), ("webview", "js,webview")]:
            with self.subTest(profile=profile):
                report, calls = self.collect_profile(profile, cargo_check=True)
                command = report["cargo_check"]["command"]
                self.assertEqual(command[command.index("--features") + 1], expected)
                self.assertIn("--no-default-features", command)
                self.assertIn("--offline", command)
                self.assertIn("--locked", command)
                self.assertEqual(sum(call == command for call in calls), 1)

    def test_unknown_programmatic_profile_is_rejected_before_probes(self):
        with patch.object(preflight, "run") as run:
            with self.assertRaises(ValueError):
                preflight.collect(Path("/test/source"), False, "browser")
            run.assert_not_called()


@unittest.skipUnless(preflight.sys.platform.startswith("linux"), "Linux libclang probe")
class LibclangProbeTests(unittest.TestCase):
    def test_explicit_empty_directory_does_not_fall_back_to_system_libclang(self):
        with tempfile.TemporaryDirectory(prefix="qusor-private-libclang-") as directory:
            with patch.dict(preflight.os.environ, {"LIBCLANG_PATH": directory}):
                result = preflight.probe_libclang()
            self.assertFalse(result["usable"])
            self.assertEqual(result["status"], "unavailable")
            self.assertNotIn(directory, json.dumps(result))

    def test_existing_non_library_file_fails_loading_without_path_disclosure(self):
        with tempfile.TemporaryDirectory(prefix="qusor-private-libclang-") as directory:
            library = Path(directory) / "libclang.so"
            library.write_text("not a shared library", encoding="utf-8")
            with patch.dict(preflight.os.environ, {"LIBCLANG_PATH": str(library)}):
                result = preflight.probe_libclang()
            self.assertFalse(result["usable"])
            self.assertEqual(result["attempts"][0]["status"], "load_or_required_symbol_failed")
            self.assertNotIn(directory, json.dumps(result))

    def test_missing_libclang_alone_blocks_otherwise_ready_prerequisites(self):
        def successful_other_checks(argv, **_kwargs):
            stdout = preflight.NSH_SHA if argv[-1] == "HEAD" else ""
            return {"command": argv, "exit_code": 0, "stdout": stdout, "stderr": ""}

        with tempfile.TemporaryDirectory() as directory, ExitStack() as context:
            context.enter_context(patch.dict(preflight.os.environ, {
                "LIBCLANG_PATH": directory, "DISPLAY": ":test",
            }))
            context.enter_context(patch.object(preflight, "run", side_effect=successful_other_checks))
            context.enter_context(patch.object(preflight.shutil, "which", return_value="/test/tool"))
            context.enter_context(patch.object(preflight, "digest", return_value="test-hash"))
            report = preflight.collect(Path(directory), cargo_check=False)
        self.assertFalse(report["libclang"]["usable"])
        self.assertEqual(report["status"], "blocked")
        self.assertEqual(len(report["blockers"]), 1)
        self.assertIn("libclang", report["blockers"][0])


if __name__ == "__main__":
    unittest.main()
