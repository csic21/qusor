import importlib.util
from pathlib import Path
import tempfile
import unittest
spec=importlib.util.spec_from_file_location('native_prefix',Path(__file__).with_name('native_prefix.py'))
native=importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
class NativePrefixTests(unittest.TestCase):
    def test_relocation_preserves_raw_and_is_repeatable(self):
        with tempfile.TemporaryDirectory() as temporary:
            prefix=Path(temporary).resolve()
            pc=prefix/'usr/lib/x86_64-linux-gnu/pkgconfig/example.pc'
            pc.parent.mkdir(parents=True)
            raw='prefix=/usr\nincludedir=${prefix}/include\n'
            pc.write_text(raw)
            native.relocate(prefix)
            expected=f'prefix={prefix}/usr\nincludedir=${{prefix}}/include\n'
            self.assertEqual(pc.read_text(),expected)
            native.relocate(prefix)
            self.assertEqual(pc.read_text(),expected)
            self.assertEqual((prefix/'.original-pkgconfig/lib/x86_64-linux-gnu/pkgconfig/example.pc').read_text(),raw)
    def test_checksum_failure_precedes_extraction(self):
        with tempfile.TemporaryDirectory() as temporary:
            import json
            root=Path(temporary)
            (root/'bad.deb').write_bytes(b'bad')
            manifest=root/'manifest.json'
            manifest.write_text(json.dumps({'packages':[{'file':'bad.deb','sha256':'0'*64}]}))
            with self.assertRaises(ValueError):native.prepare(root,root/'prefix',manifest)
            self.assertFalse((root/'prefix').exists())
