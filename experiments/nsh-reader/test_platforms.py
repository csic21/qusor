"""Six-target mapping, host/tool gating and non-claiming command plans."""

from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import unittest

import platforms


class MatrixTests(unittest.TestCase):
    def host(self, system: str) -> dict:
        return platforms.detect_host(system, "arm64")

    def tools(self, **overrides) -> dict:
        tools = {name: f"/test/bin/{name}" for name in platforms.TOOL_NAMES}
        tools["browser"] = "/test/bin/chromium"
        tools["nsh_source"] = "PATH"
        tools.update(overrides)
        return tools

    def native_ok(self) -> dict:
        return {
            "status": "checked",
            "packages": {
                package: {"usable": True, "minimum": minimum}
                for package, minimum in platforms.NATIVE_PACKAGES
            },
            "blockers": [],
        }

    def android_ok(self) -> dict:
        return {
            "found": True,
            "build_tools": ["35.0.1"],
            "platforms": ["android-35"],
            "adb": "/test/android/platform-tools/adb",
            "ndk": "/test/android/ndk/27",
            "usable": True,
            "reason": None,
        }

    def android_missing(self) -> dict:
        return platforms.detect_android_sdk(
            env={},
            home=Path("/nonexistent-qusor-android-sdk"),
            is_dir=lambda _path: False,
            is_file=lambda _path: False,
        )

    def test_six_target_mapping_is_canonical_and_host_mapped(self):
        self.assertEqual(
            platforms.TARGETS,
            ("macos", "windows", "linux", "ios", "android", "web"),
        )
        self.assertEqual(self.host("darwin")["target"], "macos")
        self.assertEqual(self.host("win32")["target"], "windows")
        self.assertEqual(self.host("linux")["target"], "linux")
        self.assertIsNone(self.host("freebsd")["target"])
        for name in platforms.TARGETS:
            with self.subTest(target=name):
                spec = platforms.target_spec(name)
                self.assertEqual(spec.name, name)
                self.assertEqual(spec.nsh_target, name)
        mapping = platforms.platform_mapping()
        self.assertEqual(set(mapping), set(platforms.TARGETS))
        self.assertEqual(mapping["ios"]["library_suffix"], ".a")
        self.assertTrue(mapping["ios"]["static_library"])
        self.assertTrue(mapping["web"]["wasm"])
        self.assertEqual(platforms.REPO_ROOT, platforms.BASE.parents[1])
        self.assertTrue(platforms.DEFAULT_APP.is_relative_to(platforms.REPO_ROOT))

    def test_windows_suffixes_exist_even_when_the_host_is_macos(self):
        report = platforms.collect(
            "windows",
            host=self.host("darwin"),
            tools=self.tools(),
            env={},
            native_packages=self.native_ok(),
        )
        target = report["targets"]["windows"]
        self.assertEqual(target["artifact"]["executable_suffix"], ".exe")
        self.assertEqual(target["artifact"]["library_suffix"], ".dll")
        self.assertEqual(target["artifact"]["pdfium_filename"], "pdfium.dll")
        self.assertEqual(target["status"], "unavailable")
        self.assertEqual(target["build"]["status"], "unavailable")
        self.assertEqual(target["start"]["status"], "unavailable")
        self.assertIn("windows", " ".join(target["build"]["blockers"]).lower())

    def test_missing_android_sdk_blocks_build_and_start(self):
        sdk = self.android_missing()
        self.assertFalse(sdk["found"])
        report = platforms.collect(
            "android",
            host=self.host("linux"),
            tools=self.tools(),
            env={"DISPLAY": ":test"},
            android_sdk=sdk,
            native_packages=self.native_ok(),
        )
        target = report["targets"]["android"]
        self.assertEqual(target["status"], "unavailable")
        self.assertEqual(target["build"]["status"], "unavailable")
        self.assertEqual(target["start"]["status"], "unavailable")
        self.assertIn("Android SDK", " ".join(target["build"]["blockers"]))
        self.assertEqual(target["artifact"]["library_suffix"], ".so")

    def test_ios_is_blocked_off_macos_even_with_fake_apple_tools(self):
        report = platforms.collect(
            "ios",
            host=self.host("linux"),
            tools=self.tools(),
            env={},
            native_packages=self.native_ok(),
        )
        target = report["targets"]["ios"]
        self.assertEqual(target["status"], "unavailable")
        self.assertEqual(target["build"]["status"], "unavailable")
        self.assertEqual(target["start"]["status"], "unavailable")
        self.assertEqual(target["artifact"]["library_suffix"], ".a")
        self.assertIn("macos", " ".join(target["build"]["blockers"]).lower())
        command = target["build"]["command"]
        self.assertEqual(command[command.index("--target") + 1], "ios")

    def test_web_has_no_gtk_or_pkg_config_dependency(self):
        def forbidden(*_args, **_kwargs):
            raise AssertionError("web preflight must not probe GTK or pkg-config")

        report = platforms.collect(
            "web",
            host=self.host("linux"),
            tools=self.tools(**{"pkg-config": None}),
            env={"DISPLAY": ":test"},
            android_sdk=self.android_missing(),
            pkg_config_runner=forbidden,
        )
        target = report["targets"]["web"]
        self.assertEqual(target["probe"]["status"], "available")
        self.assertEqual(target["build"]["status"], "not_run")
        self.assertEqual(target["start"]["status"], "not_run")
        self.assertEqual(target["status"], "not_run")
        self.assertEqual(target["artifact"]["library_suffix"], ".wasm")
        encoded = json.dumps(target).lower()
        self.assertNotIn("gtk", encoded)
        self.assertNotIn("gstreamer", encoded)
        self.assertEqual(
            target["build"]["command"][target["build"]["command"].index("--target") + 1],
            "web",
        )

    def test_repository_root_and_rust_android_ndk_requirement(self):
        self.assertTrue((platforms.REPO_ROOT / "Cargo.toml").is_file())
        self.assertTrue((platforms.DEFAULT_APP / "nsh.json").is_file())
        sdk = self.android_ok()
        sdk["ndk"] = []
        report = platforms.collect("android", host=self.host("darwin"),
            tools=self.tools(), env={}, android_sdk=sdk,
            app=platforms.DEFAULT_APP)
        self.assertEqual(report["targets"]["android"]["build"]["status"], "unavailable")
        self.assertTrue(any("NDK" in s for s in report["targets"]["android"]["probe"]["blockers"]))

    def test_unknown_target_is_rejected_before_probes(self):
        with self.assertRaises(ValueError):
            platforms.target_spec("tvos")
        with self.assertRaises(ValueError):
            platforms.collect("tvos", host=self.host("darwin"), tools=self.tools(), env={})
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
            platforms.parse_args(["--target", "tvos"])
        self.assertEqual(raised.exception.code, 2)

    def test_planned_commands_never_claim_a_pass(self):
        cases = {
            "macos": ("darwin", {}),
            "windows": ("win32", {}),
            "linux": ("linux", {"DISPLAY": ":test"}),
            "ios": ("darwin", {}),
            "android": ("linux", {"DISPLAY": ":test"}),
            "web": ("linux", {}),
        }
        for target_name, (system, env) in cases.items():
            with self.subTest(target=target_name):
                report = platforms.collect(
                    target_name,
                    host=self.host(system),
                    tools=self.tools(),
                    env=env,
                    android_sdk=(
                        self.android_ok()
                        if target_name == "android"
                        else self.android_missing()
                    ),
                    native_packages=self.native_ok(),
                )
                target = report["targets"][target_name]
                self.assertEqual(target["probe"]["status"], "available")
                for phase, command_name in (("build", "package"), ("start", "run")):
                    self.assertEqual(target[phase]["status"], "not_run")
                    command = target[phase]["command"]
                    self.assertEqual(command[0], "/test/bin/nsh")
                    self.assertEqual(command[1], command_name)
                    self.assertIn("--target", command)
                    self.assertEqual(
                        command[command.index("--target") + 1], target_name
                    )
                self.assertEqual(target["status"], "not_run")
                self.assertEqual(report["blockers"], [])
                self.assertNotIn('"passed"', json.dumps(report))

    def test_command_builders_are_plans_without_side_effects(self):
        for target in platforms.TARGETS:
            with self.subTest(target=target):
                build = platforms.build_command(
                    target, app=Path("/test/app"), out=Path("/test/out")
                )
                self.assertEqual(build[:2], ["nsh", "package"])
                self.assertNotIn("run", build)
                self.assertEqual(build[build.index("--target") + 1], target)
                start = platforms.start_command(target, app=Path("/test/app"))
                self.assertEqual(start[:2], ["nsh", "run"])
                self.assertEqual(start[start.index("--target") + 1], target)

    def test_parse_target_defaults_to_all_and_accepts_one_target(self):
        self.assertEqual(platforms.parse_args([]).target, "all")
        self.assertEqual(platforms.parse_args(["--target", "all"]).target, "all")
        self.assertEqual(platforms.parse_args(["--target", "web"]).target, "web")


if __name__ == "__main__":
    unittest.main()
