"""Exercise the pinned native PDF worker; does not qualify NSH or Core storage."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import signal
import struct
import subprocess
import sys
import tempfile
import time
import tomllib
import zlib

from prepare_runtime import LIBRARY_SHA256, ARCHIVE_SHA256, URL, sha256

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from check_architecture import validate_graph  # noqa: E402
CHECKS = []
SCENARIOS = []


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def command(argv, timeout=300):
    argv = list(map(str, argv))
    print('+', ' '.join(argv), flush=True)
    result = subprocess.run(argv, cwd=HERE, capture_output=True, text=True, timeout=timeout)
    CHECKS.append({'command': argv, 'exit': result.returncode,
                   'stdout': result.stdout, 'stderr': result.stderr})
    require(result.returncode == 0, result.stdout + result.stderr)
    return result.stdout


def child_limits():
    resource.setrlimit(resource.RLIMIT_AS, (1024 ** 3, 1024 ** 3))
    resource.setrlimit(resource.RLIMIT_CPU, (10, 10))
    resource.setrlimit(resource.RLIMIT_FSIZE, (64 * 1024 ** 2, 64 * 1024 ** 2))
    resource.setrlimit(resource.RLIMIT_NOFILE, (64, 64))


def worker(runner, name, args, ok=True):
    argv = list(map(str, [runner, *args]))
    started = time.monotonic()
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True,
                                   preexec_fn=child_limits)
        try:
            code = process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise RuntimeError(f'{name}: native worker exceeded wall deadline') from None
        stdout.seek(0)
        raw = stdout.read(64 * 1024 ** 2 + 1)
        stderr.seek(0)
        diagnostic = stderr.read(8192).decode('utf-8', errors='replace')
    require(len(raw) <= 64 * 1024 ** 2, 'worker output exceeds budget')
    require(code == (0 if ok else 1), f'{name}: exit {code}: {diagnostic}')
    report = json.loads(raw)
    require(report['ok'] == ok, f'{name}: unexpected status')
    SCENARIOS.append({'name': name, 'command': argv, 'exit': code,
                      'seconds': time.monotonic() - started, 'stderr': diagnostic, 'result': report})
    return report


def ppm(path):
    parts = path.read_bytes().split(b'\n', 3)
    require(parts[0] == b'P6' and parts[2] == b'255', 'unexpected page image encoding')
    width, height = map(int, parts[1].split())
    require(len(parts[3]) == width * height * 3, 'page image pixel count mismatch')
    return width, height, parts[3]


def png(path, width, height, rgb):
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))
    rows = b''.join(b'\0' + rgb[y * width * 3:(y + 1) * width * 3] for y in range(height))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def geometry_check(page, image):
    width, height, rgb = ppm(image)
    require((width, height) == (page['pixel_width'], page['pixel_height']), 'render extent mismatch')
    require(sha256(image) == page['rendered_sha256'], 'render output hash mismatch')
    checked = 0
    for character in page['characters']:
        quad = character['pixel_quad']
        if not quad or not chr(character['unicode_value']).isalpha():
            continue
        left = max(0, min(point[0] for point in quad) - 1)
        top = max(0, min(point[1] for point in quad) - 1)
        right = min(width, max(point[0] for point in quad) + 2)
        bottom = min(height, max(point[1] for point in quad) + 2)
        dark = any(min(rgb[(y * width + x) * 3:(y * width + x) * 3 + 3]) < 180
                   for y in range(top, bottom) for x in range(left, right))
        require(dark, f'character geometry misses rendered ink: page {page["page_index"]}, char {character["pdf_char_index"]}')
        checked += 1
    require(checked > 20, 'insufficient real glyph geometry samples')
    return checked, (width, height, rgb)


def run(runner, library, scratch, evidence_dir):
    fixture = HERE / 'fixtures/text-two-pages.pdf'
    oracle = json.loads((HERE / 'fixtures/manifest.json').read_text())['fixtures'][0]
    out = scratch / 'original'
    report = worker(runner, 'native two-page render and extraction', ['inspect', library, fixture, out])['report']
    require(report['original_sha256'] == oracle['sha256'], 'original version mismatch')
    require(report['extractor']['library_sha256'] == LIBRARY_SHA256, 'wrong runtime bound')
    require(len(report['pages']) == 2, 'page count mismatch')
    geometry = []
    for page, expected in zip(report['pages'], oracle['pages']):
        require(page['text_status'] == 'mapped', 'text mapping unavailable')
        actual_lines = page['page_text'].splitlines()
        require(sorted(actual_lines) == sorted(expected['expected_text_lines']), 'actual PDF extraction loses source lines')
        if expected['rotation_degrees'] == 0:
            require(actual_lines == expected['expected_text_lines'], 'unrotated line order changed')
        require(hashlib.sha256(page['page_text'].encode()).hexdigest() == page['page_text_sha256'], 'page text hash mismatch')
        for kind in ['crop_box', 'media_box']:
            box = page[kind]
            require([box[key] for key in ['left', 'bottom', 'right', 'top']] == expected[kind], 'page box mismatch')
        count, image = geometry_check(page, out / page['rendered_file'])
        geometry.append({'page_index': page['page_index'], 'glyphs_with_rendered_ink': count,
                         'rotation': page['rotation'],
                         'stream_matches_construction_order': actual_lines == expected['expected_text_lines'],
                         'actual_line_order': actual_lines})
        png(evidence_dir / f'page-{page["page_index"]}.png', *image)
    again = worker(runner, 'new worker reproducible extraction', ['inspect', library, fixture, scratch / 'reopen'])['report']
    require(again == report, 'reopen changed original/extraction/map/render version')
    page = report['pages'][0]
    quote = 'immutable originals'
    start = page['page_text'].index(quote)
    base = ['resolve', library, fixture, 0, start, start + len(quote), report['original_sha256'],
            report['extraction_sha256'], page['page_text_sha256'], 'unicode_scalar', quote]
    exact = worker(runner, 'exact original page selection', [*base, scratch / 'resolve'])['selection']
    width, height, pixels = ppm(out / page['rendered_file'])
    overlay = bytearray(pixels)
    for character in exact['characters']:
        quad = character['pixel_quad']
        if not quad:
            continue
        left, right = max(0, min(p[0] for p in quad)), min(width - 1, max(p[0] for p in quad))
        top, bottom = max(0, min(p[1] for p in quad)), min(height - 1, max(p[1] for p in quad))
        for x, y in ([(x, y) for x in range(left, right + 1) for y in [top, bottom]]
                     + [(x, y) for y in range(top, bottom + 1) for x in [left, right]]):
            offset = (y * width + x) * 3
            overlay[offset:offset + 3] = bytes([240, 120, 0])
    png(evidence_dir / 'selection-geometry.png', width, height, bytes(overlay))
    repeated = 'Repeated evidence stays local.'
    positions = [index for index in range(len(page['page_text'])) if page['page_text'].startswith(repeated, index)]
    require(len(positions) == 2, 'repeat-text fixture no longer exercises ambiguity')
    selected = []
    for ordinal, position in enumerate(positions):
        result = worker(runner, f'explicit range disambiguates repeated quote {ordinal}',
                        ['resolve', library, fixture, 0, position, position + len(repeated),
                         report['original_sha256'], report['extraction_sha256'], page['page_text_sha256'],
                         'unicode_scalar', repeated, scratch / f'repeat-{ordinal}'])['selection']
        selected.append(result['characters'][0]['pixel_quad'])
    require(selected[0] != selected[1], 'repeated quote silently chose the same occurrence')
    for field, replacement, name in [(6, '0' * 64, 'original'), (7, '0' * 64, 'extraction'),
                                      (8, '0' * 64, 'page-text'), (9, 'utf16', 'unit'),
                                      (10, 'similar but wrong', 'quote'), (3, 99, 'page')]:
        changed = base.copy()
        changed[field] = replacement
        worker(runner, f'reject mismatched {name}', [*changed, scratch / f'bad-{name}'], ok=False)
    rotated = report['pages'][1]
    text = 'Repeated evidence stays local.'
    offset = rotated['page_text'].index(text)
    worker(runner, 'exact rotated-page selection', ['resolve', library, fixture, 1, offset, offset + len(text),
           report['original_sha256'], report['extraction_sha256'], rotated['page_text_sha256'],
           'unicode_scalar', text, scratch / 'rotated-selection'])
    scan = worker(runner, 'image-only document remains viewable without invented text',
                  ['inspect', library, HERE / 'fixtures/image-only.pdf', scratch / 'scan'])['report']
    require(scan['pages'][0]['text_status'] == 'no_text' and not scan['pages'][0]['page_text'], 'scan acquired invented text')
    worker(runner, 'no-text page refuses exact selection',
           ['resolve', library, HERE / 'fixtures/image-only.pdf', 0, 0, 1, scan['original_sha256'],
            scan['extraction_sha256'], scan['pages'][0]['page_text_sha256'], 'unicode_scalar', 'x',
            scratch / 'scan-selection'], ok=False)
    worker(runner, 'malformed PDF rejected', ['inspect', library, HERE / 'fixtures/malformed.pdf', scratch / 'broken'], ok=False)
    symlink = scratch / 'alias.pdf'
    symlink.symlink_to(fixture)
    worker(runner, 'symbolic source rejected', ['inspect', library, symlink, scratch / 'symlink'], ok=False)
    oversized = scratch / 'oversized.pdf'
    with oversized.open('wb') as output:
        output.truncate(16 * 1024 ** 2 + 1)
    worker(runner, 'oversized input rejected before decode', ['inspect', library, oversized, scratch / 'oversized'], ok=False)
    return geometry


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--library', type=Path, default=HERE / 'target/pdfium/lib/libpdfium.so')
    parser.add_argument('--evidence', type=Path, default=HERE / 'evidence/linux-pdf-provider.json')
    args = parser.parse_args()
    require(sha256(args.library) == LIBRARY_SHA256, 'trusted runtime must match pinned library before execution')
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    command(['rustc', '-Vv'])
    command(['cargo', '-V'])
    command(['python3', HERE / 'fixtures/generate.py'])
    graph = json.loads(command(['cargo', 'metadata', '--locked', '--offline', '--all-features', '--format-version', '1']))
    policy = tomllib.loads((ROOT / 'config/architecture.toml').read_text())
    errors = validate_graph(graph, policy['experiments']['pdf-provider'], ROOT)
    require(not errors, '\n'.join(errors))
    command(['python3', ROOT / 'scripts/check_architecture.py'])
    command(['cargo', 'fmt', '--all', '--', '--check'])
    command(['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'])
    command(['cargo', 'test', '--locked', '--offline'])
    command(['cargo', 'build', '--locked', '--offline'])
    license_tree = command(['cargo', 'tree', '--locked', '--offline', '--target',
                            'x86_64-unknown-linux-gnu', '--edges', 'normal,build',
                            '--prefix', 'none', '--format', '{p}|{l}'])
    (args.evidence.parent / 'linux-native-dependencies.tsv').write_text('\n'.join(sorted(set(license_tree.splitlines()))) + '\n')
    runner = Path(graph['target_directory']) / 'debug/qusor-pdf-provider-probe'
    host_hash = sha256(runner)
    with tempfile.TemporaryDirectory(prefix='qusor-native-pdf-') as temporary:
        geometry = run(runner, args.library, Path(temporary), args.evidence.parent)
    require(sha256(runner) == host_hash, 'worker binary changed during qualification')
    paths = [path for path in HERE.rglob('*') if path.is_file()
             and not any(part in {'target', 'evidence', '__pycache__'} for part in path.relative_to(HERE).parts)]
    paths += [ROOT / 'config/architecture.toml', ROOT / 'rust-toolchain.toml']
    runtime = args.library.parent.parent
    runtime_files = [path for path in runtime.rglob('*') if path.is_file()]
    args.evidence.write_text(json.dumps({'platform': platform.platform(), 'worker_sha256': host_hash,
        'runtime': {'source': URL, 'archive_sha256': ARCHIVE_SHA256, 'library_sha256': LIBRARY_SHA256,
                    'files_sha256': {str(path.relative_to(runtime)): sha256(path) for path in sorted(runtime_files)}},
        'source_sha256': {str(path.relative_to(ROOT)): sha256(path) for path in sorted(paths)},
        'geometry_samples': geometry, 'checks': CHECKS, 'scenarios': SCENARIOS,
        'unverified': ['NSH window and selection UI', 'Core PDF retention/export', 'Chinese PDF mappings',
                       'Unicode-map-error API', 'reliable reading order for rotated/multicolumn pages',
                       'OS sandbox', 'other platforms']}, ensure_ascii=False, indent=2) + '\n')
    print(f'{len(CHECKS)} commands; {len(SCENARIOS)} worker scenarios; {args.evidence}')


if __name__ == '__main__':
    main()
