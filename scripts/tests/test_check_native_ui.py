import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_native_ui import check_file  # noqa: E402


class NativeUiBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_accepts_nsh_native_tsx(self):
        path = self.write(
            "Plugin.tsx",
            'import { View } from "nsh";\nexport const app = <View />;\n',
        )
        self.assertEqual(check_file(path), [])

    def test_rejects_webview_and_dom_imports(self):
        for source in (
            'import { WebView } from "nsh/web";',
            'import { createRoot } from "react-dom/client";',
            'import { app } from "electron";',
        ):
            with self.subTest(source=source):
                path = self.write("Plugin.tsx", source + "\n")
                self.assertTrue(check_file(path))

    def test_rejects_iframe_and_html_injection(self):
        for source in (
            "const view = <iframe src=\"https://example.test\" />;",
            "const view = <WebView source={url} />;",
            "element.innerHTML = value;",
            "element.dangerouslySetInnerHTML = value;",
        ):
            with self.subTest(source=source):
                path = self.write("Plugin.tsx", source + "\n")
                self.assertTrue(check_file(path))

    def test_rejects_browser_globals_and_web_assets(self):
        for name, source in (
            ("Plugin.tsx", "document.body.append(node);\n"),
            ("Plugin.tsx", "window.location.reload();\n"),
            ("Plugin.html", "<main>web plugin</main>\n"),
            ("Plugin.css", "main { color: red; }\n"),
        ):
            with self.subTest(name=name):
                path = self.write(name, source)
                self.assertTrue(check_file(path))

    def test_rejects_rust_webview_dependencies_and_apis(self):
        for name, source in (
            (
                "Cargo.toml",
                '[dependencies]\ntauri = { version = "2" }\n',
            ),
            (
                "Cargo.toml",
                '[dependencies.wry]\nversion = "0.50"\n',
            ),
            (
                "host.rs",
                "fn mount() { wry::WebViewBuilder::new(); }\n",
            ),
        ):
            with self.subTest(name=name, source=source):
                path = self.write(name, source)
                self.assertTrue(check_file(path))

    def test_rejects_web_renderer_manifest_entries(self):
        for source in (
            '{"renderer": "web"}\n',
            '{"carrier": "webview"}\n',
            '{"kind": "web_workspace"}\n',
        ):
            with self.subTest(source=source):
                path = self.write("manifest.json", source)
                self.assertTrue(check_file(path))


if __name__ == "__main__":
    unittest.main()
