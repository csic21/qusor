"""Prepare an isolated native build prefix from verified local Ubuntu debs."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

BASE = Path(__file__).resolve().parent

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def environment(prefix):
    prefix = Path(prefix).resolve()
    return {'PKG_CONFIG_PATH': os.pathsep.join(str(prefix / p) for p in ['usr/lib/x86_64-linux-gnu/pkgconfig', 'usr/share/pkgconfig']),
            'LD_LIBRARY_PATH': str(prefix / 'usr/lib/x86_64-linux-gnu')}

def runtime_environment(prefix, base_env=None):
    prefix = Path(prefix).resolve()
    if not (prefix / 'usr/lib/x86_64-linux-gnu/pkgconfig/gtk4.pc').is_file():
        raise ValueError('native prefix is missing genuine GTK pkg-config metadata')
    result = dict(os.environ if base_env is None else base_env)
    for name, value in environment(prefix).items():
        result[name] = value + (os.pathsep + result[name] if result.get(name) else '')
    return result

def relocate(prefix):
    prefix = Path(prefix).resolve()
    raw = prefix / '.original-pkgconfig'
    for pc in sorted((prefix / 'usr').rglob('*.pc')):
        relative = pc.relative_to(prefix / 'usr')
        original = raw / relative
        original.parent.mkdir(parents=True, exist_ok=True)
        if not original.exists():
            original.write_bytes(pc.read_bytes())
        text = original.read_text()
        pc.write_text(text.replace('prefix=/usr\n', f'prefix={prefix}/usr\n'))

def prepare(downloads, prefix, manifest):
    downloads, prefix = Path(downloads).resolve(), Path(prefix).resolve()
    records = json.loads(Path(manifest).read_text())['packages']
    if prefix.exists() and any(prefix.iterdir()):
        raise ValueError('destination must be empty')
    for record in records:
        filename = record['file']
        if Path(filename).name != filename or digest(downloads / filename) != record['sha256']:
            raise ValueError('invalid filename or package checksum')
    prefix.mkdir(parents=True, exist_ok=True)
    for record in records:
        subprocess.run(['dpkg-deb', '-x', str(downloads / record['file']), str(prefix)], check=True)
    relocate(prefix)
    return environment(prefix)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--downloads', type=Path, required=True)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, default=BASE / 'evidence/linux-native-prefix.json')
    args = parser.parse_args()
    print(json.dumps(prepare(args.downloads,args.prefix,args.manifest),indent=2))
