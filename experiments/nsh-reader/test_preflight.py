"""Real loader failures and readiness gating; uses only the Python standard library."""

from contextlib import ExitStack
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import preflight


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
