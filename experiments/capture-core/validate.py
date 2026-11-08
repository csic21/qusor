"""Run C0 signed-author, real capture and restart qualification locally."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time
import tomllib

from export_sdk import export_sdk

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from check_architecture import validate_graph  # noqa: E402
CHECKS = []
SCENARIOS = []


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command(args, *, cwd=HERE, exits=(0,), env=None, timeout=300):
    argv = [str(arg) for arg in args]
    print('+', ' '.join(argv), flush=True)
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True,
                            text=True, encoding='utf-8', timeout=timeout)
    CHECKS.append({'command': argv, 'cwd': str(cwd), 'exit': result.returncode,
                   'stdout': result.stdout, 'stderr': result.stderr})
    require(result.returncode in exits, result.stdout + result.stderr)
    return result


def metadata(manifest, *, env=None):
    return json.loads(command(['cargo', 'metadata', '--manifest-path', manifest,
                              '--format-version', '1', '--locked', '--offline',
                              '--all-features'], env=env).stdout)


def independent_author(outside):
    fingerprints = export_sdk(outside)
    manifest = outside / 'author/Cargo.toml'
    env = dict(os.environ, CARGO_TARGET_DIR=str(outside / 'target'))
    command(['cargo', 'generate-lockfile', '--manifest-path', manifest, '--offline'], env=env)
    graph = metadata(manifest, env=env)
    names = {package['name'] for package in graph['packages']}
    forbidden = {'extension-host', 'qusor-wasm-probe', 'qusor-package-probe',
                 'qusor-capture-probe', 'extension-runtime-api', 'runtime-wasm',
                 'learning-application', 'learning-ports', 'storage-sqlite',
                 'blob-store', 'wasmi', 'rusqlite', 'fs2', 'rustix'}
    require(not names & forbidden, f'private dependencies leaked: {names & forbidden}')
    require(not any(name.startswith('nsh-') for name in names),
            'native UI dependency leaked into author SDK')
    for package in graph['packages']:
        if package.get('source') is None:
            path = Path(package['manifest_path']).resolve()
            require(path.is_relative_to(outside.resolve()), 'author depends on repository source')
    command(['cargo', 'build', '--manifest-path', manifest, '--locked', '--offline'], env=env)
    command(['cargo', 'clippy', '--manifest-path', manifest, '--all-targets', '--locked',
             '--offline', '--', '-D', 'warnings'], env=env)
    executable = Path(graph['target_directory']) / 'debug/build-capture-plugin'
    require(executable.is_file(), 'independent author executable missing')
    return executable, fingerprints, sorted(names)


def source_fingerprints():
    paths = {ROOT / 'config/architecture.toml',ROOT / 'Cargo.toml',ROOT / 'Cargo.lock'}
    for directory in [ROOT / 'experiments/extension-package', ROOT / 'experiments/extension-wasm',
                      ROOT / 'experiments/extension-wasm/runner', ROOT / 'crates/extension-api',
                      ROOT / 'crates/extension-host', ROOT / 'crates/extension-package',
                      ROOT / 'crates/extension-runtime-api', ROOT / 'adapters/runtime-wasm',
                      ROOT / 'adapters/extension-store', ROOT / 'adapters/runtime-process',
                      ROOT / 'crates/core-api', ROOT / 'crates/learning-ports',
                      ROOT / 'crates/learning-application',ROOT / 'adapters/storage-sqlite', ROOT / 'adapters/blob-store']:
        paths.update(path for path in [directory / 'Cargo.toml', directory / 'Cargo.lock'] if path.is_file())
    for directory in (HERE, ROOT / 'experiments/extension-package/src',
                      ROOT / 'experiments/extension-package/tests',
                      ROOT / 'experiments/extension-package/fixtures',
                      ROOT / 'experiments/extension-wasm/runner/src',
                      ROOT / 'crates/extension-api/src', ROOT / 'crates/extension-api/tests',
                      ROOT / 'crates/extension-api/fixtures', ROOT / 'crates/extension-api/schema',
                      ROOT / 'crates/extension-host/src', ROOT / 'crates/extension-host/tests',
                      ROOT / 'crates/extension-package/src', ROOT / 'crates/extension-package/tests',
                      ROOT / 'crates/extension-runtime-api/src', ROOT / 'crates/extension-runtime-api/tests',
                      ROOT / 'adapters/runtime-wasm/src', ROOT / 'adapters/runtime-wasm/tests',
                      ROOT / 'adapters/extension-store/src', ROOT / 'adapters/extension-store/tests',
                      ROOT / 'adapters/runtime-process/src', ROOT / 'adapters/runtime-process/tests',
                      ROOT / 'crates/core-api/src', ROOT / 'crates/core-api/tests', ROOT / 'crates/learning-ports/src',
                      ROOT / 'crates/learning-application/src', ROOT / 'crates/learning-application/tests',
                      ROOT / 'adapters/storage-sqlite/src', ROOT / 'adapters/storage-sqlite/tests',
                      ROOT / 'adapters/blob-store/src',ROOT / 'adapters/blob-store/tests'):
        for path in directory.rglob('*'):
            if path.is_file() and not any(part in {'target', '__pycache__', 'evidence'}
                                          for part in path.relative_to(directory).parts):
                if path.suffix in {'.rs', '.py', '.toml', '.lock', '.json', '.wat', '.c', '.s'} or path.name == 'Cargo.lock':
                    paths.add(path)
    return {str(path.relative_to(ROOT)): digest(path) for path in sorted(paths)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--evidence', type=Path, default=HERE / 'evidence/linux-capture-core.json')
    args = parser.parse_args()
    started = time.time()
    graph = metadata(HERE / 'Cargo.toml')
    policy = tomllib.loads((ROOT / 'config/architecture.toml').read_text())
    require(not validate_graph(graph, policy['experiments']['capture-core'], ROOT),
            'capture dependency policy failed')
    author_graph_local = metadata(HERE / 'author/Cargo.toml')
    require(not validate_graph(author_graph_local, policy['experiments']['capture-core-author'], ROOT),
            'capture author dependency policy failed')
    command(['python3', ROOT / 'scripts/check_architecture.py'])
    command(['cargo', 'fmt', '--all', '--', '--check'])
    command(['cargo', 'fmt', '--manifest-path', HERE / 'author/Cargo.toml', '--all', '--', '--check'])
    command(['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'])
    command(['cargo', 'test', '--locked', '--offline'])
    for manifest in [ROOT / 'experiments/extension-wasm/Cargo.toml',
                     ROOT / 'experiments/extension-package/Cargo.toml']:
        command(['cargo', 'test', '--manifest-path', manifest, '--locked', '--offline'])
    command(['cargo', 'build', '--locked', '--offline'])
    runner = Path(graph['target_directory']) / 'debug/qusor-capture-probe'
    host_hash = digest(runner)
    with tempfile.TemporaryDirectory(prefix='qusor-capture-') as scratch:
        scratch = Path(scratch)
        author, sdk_hashes, author_graph = independent_author(scratch / 'sdk')
        run_scenarios(runner, author, scratch)
        fixtures = {str(path.relative_to(scratch / 'fixtures')): digest(path)
                    for path in sorted((scratch / 'fixtures').rglob('*')) if path.is_file()}
    require(digest(runner) == host_hash, 'host binary changed during author validation')
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    args.evidence.write_text(json.dumps({
        'experiment': 'C0 real capture qualification', 'platform': platform.platform(),
        'started_unix': started, 'elapsed_seconds': time.time() - started,
        'host_sha256': host_hash, 'sdk_sources': sdk_hashes,
        'fixture_sha256': fixtures,
        'independent_author_packages': author_graph, 'source_sha256': source_fingerprints(),
        'checks': CHECKS, 'scenarios': SCENARIOS,
        'limitations': ['CLI only', 'synthetic UTF-8 source fixtures',
                        'process crashes are not power-loss tests', 'no NSH/platform qualification'],
    }, ensure_ascii=False, indent=2) + '\n')
    print(f'{len(CHECKS)} commands; {len(SCENARIOS)} scenarios; {args.evidence}')


def run_scenarios(runner, author, scratch):
    def action(name, *args, ok=True, crash=False, executable=runner):
        result = command([executable, *args], exits=(86,) if crash else ((0,) if ok else (1,)))
        report = {'crash_exit': 86} if crash else json.loads(result.stdout)
        if not crash:
            require(report['ok'] == ok, f'{name}: unexpected result')
        SCENARIOS.append({'name': name, 'arguments': list(map(str, args)), 'report': report})
        return report

    fixtures = scratch / 'fixtures'
    fixtures.mkdir()
    seed = fixtures / 'public-test-seed.hex'
    seed.write_text('11' * 32)
    packages = scratch / 'packages'
    core = scratch / 'knowledge'
    ids = ['org.example.archive-source', 'org.example.shared-text']
    snapshots = ['开头🙂 e\u0301，重复引文；中间；重复引文。\n' + '本地原文。' * 1000,
                 '第二份来源\n不同的原文与用户观点。']
    quotes = ['🙂 e\u0301', '不同的原文']
    requests = []
    for index, (package_id, text, quote) in enumerate(zip(ids, snapshots, quotes)):
        start = text.index(quote)
        draft = {'protocol_version': 1,
                 'source': {'namespace': f'source{index}', 'reference': f'fixture-{index}',
                            'title': f'来源 {index + 1}',
                            'public_uri': None if index == 0 else 'https://example.org/reference',
                            'external_return': 'none' if index == 0 else 'source_only'},
                 'snapshot_text': text,
                 'selector': {'unit': 'unicode_scalar', 'normalization': 'qusor-raw-text-v1',
                              'start': start, 'end': start + len(quote), 'exact': quote}}
        request = {'capture_protocol': 1, 'request_id': f'proposal-{index}', 'draft': draft}
        requests.append(request)
        request_path = fixtures / f'request-{index}.json'
        request_path.write_text(json.dumps(request, ensure_ascii=False))
        manifest = {'manifest_version': 1, 'package_id': package_id, 'publisher_id': 'org.example',
                    'package_version': {'major': 1, 'minor': 0, 'patch': 0},
                    'host_api': {'major': 0, 'min_minor': 1, 'max_minor': 1},
                    'required_capabilities': ['capture.propose'], 'optional_capabilities': [],
                    'contributions': [{'id': package_id + '.capture', 'title': 'Capture selected text',
                                       'kind': 'command', 'required_capabilities': ['capture.propose']}]}
        manifest_path = fixtures / f'manifest-{index}.json'
        manifest_path.write_text(json.dumps(manifest))
        package = fixtures / f'source-{index}.qpk'
        key = command([author, request_path, manifest_path, seed, package]).stdout.strip()
        action(f'install ordinary source {index}', 'install', packages, f'install-{index}', 0, package, key)

    captured = []
    for index, package_id in enumerate(ids):
        args = ['capture', packages, core, 'local-vault', f'collect-{index}', package_id, f'用户观点 {index}']
        preview = action(f'preview source {index}', *args)
        require(preview['committed'] is False, 'preview committed knowledge')
        require(action(f'preview has no receipt {index}', 'receipt', core,
                       f'collect-{index}')['receipt'] is None, 'preview left receipt')
        expected = preview['preview']['draft_hash']
        action(f'reject changed review hash {index}', *args, '--accept', '0' * 64, ok=False)
        saved = action(f'accept source {index}', *args, '--accept', expected)['receipt']
        replay = action(f'idempotent replay {index}', *args, '--accept', expected)['receipt']
        require(saved == replay, 'replay created another receipt')
        changed = args.copy()
        changed[-1] = '不同的用户观点'
        action(f'reject same intent changed note {index}', *changed, '--accept', expected, ok=False)
        record = action(f'restart read source {index}', 'query', core, saved['capture_id'])['capture']
        require(record['draft'] == requests[index]['draft'], 'immutable source changed')
        require(record['user_note'] == f'用户观点 {index}', 'user/source boundary lost')
        selection = action(f'exact Unicode resolution {index}', 'resolve', core, saved['capture_id'],
                           saved['snapshot_hash'], 'qusor-raw-text-v1', 'unicode_scalar')['selection']
        require(selection['text'] == quotes[index], 'wrong exact selection')
        action(f'wrong version hash {index}', 'resolve', core, saved['capture_id'], '0' * 64,
               'qusor-raw-text-v1', 'unicode_scalar', ok=False)
        action(f'wrong unit {index}', 'resolve', core, saved['capture_id'], saved['snapshot_hash'],
               'qusor-raw-text-v1', 'utf16', ok=False)
        action(f'wrong normalization {index}', 'resolve', core, saved['capture_id'], saved['snapshot_hash'],
               'nfc', 'unicode_scalar', ok=False)
        action(f'wrong document version {index}', 'resolve-version', core, saved['capture_id'],
               '999999', saved['snapshot_hash'], 'qusor-raw-text-v1', 'unicode_scalar', ok=False)
        captured.append((saved, expected))

    separate = action('distinct intent keeps identical content', 'capture', packages, core,
                      'local-vault', 'independent-intent', ids[0], '', '--accept', captured[0][1])['receipt']
    require(separate['capture_id'] != captured[0][0]['capture_id'] and
            separate['snapshot_hash'] == captured[0][0]['snapshot_hash'], 'content dedup swallowed intent')
    bundle = action('complete export', 'export', core)['bundle']
    require(len(bundle['entries']) == 3 and len(bundle['blobs']) == 2, 'export lacks knowledge or CAS sharing')
    archive = fixtures / 'knowledge.json'
    archive.write_text(json.dumps(bundle, ensure_ascii=False))
    for point in ['after_blob_write', 'before_blob_publish', 'after_blob_publish',
                  'before_transaction', 'after_entities', 'before_commit', 'after_commit']:
        crashed = scratch / f'capture-crash-{point}'
        args = ['capture', packages, crashed, 'local-vault', 'crash-intent', ids[0],
                '崩溃前确认', '--accept', captured[0][1]]
        action(f'real capture process crash {point}', *args, '--crash', point, crash=True)
        old = action(f'query receipt after crash {point}', 'receipt', crashed, 'crash-intent')['receipt']
        require((old is not None) == (point == 'after_commit'), 'crash crossed atomic boundary')
        retried = action(f'retry after crash {point}', *args)['receipt']
        require(retried['capture_id'] == 1 and (old is None or old == retried), 'lost-response retry duplicated save')
        require(len(action(f'one transaction after retry {point}', 'export', crashed)['bundle']['entries']) == 1,
                'crash retry left partial or duplicate knowledge')
    for point in ['after_blob_publish', 'after_entities', 'after_commit']:
        destination = scratch / f'restore-crash-{point}'
        action(f'real restore process crash {point}', 'restore', destination, archive,
               '--crash', point, crash=True)
        recovered = action(f'export after restore crash {point}', 'export', destination)['bundle']
        require(len(recovered['entries']) == (3 if point == 'after_commit' else 0),
                'restore crash left partial visible knowledge')
        if point != 'after_commit':
            action(f'retry empty restore {point}', 'restore', destination, archive)
        require(action(f'restore crash canonical result {point}', 'export', destination)['bundle'] == bundle,
                'restore crash recovery changed knowledge')
    restored = scratch / 'restored'
    action('restore empty Core', 'restore', restored, archive)
    require(action('restored canonical export', 'export', restored)['bundle'] == bundle, 'restore changed data')
    action('reject nonempty restore', 'restore', restored, archive, ok=False)
    for name, mutate in [('missing-blob', lambda b: b['blobs'].clear()),
                         ('corrupt-blob', lambda b: b['blobs'][0].update(text='损坏')),
                         ('unsupported-schema', lambda b: b.update(schema_version=99)),
                         ('injected-authorization', lambda b: b.update(grants=['capture.propose']))]:
        damaged = json.loads(json.dumps(bundle))
        mutate(damaged)
        path = fixtures / f'{name}.json'
        path.write_text(json.dumps(damaged, ensure_ascii=False))
        destination = scratch / f'restore-{name}'
        action(f'reject archive {name}', 'restore', destination, path, ok=False)
        require(action(f'archive rejection atomic {name}', 'export', destination)['bundle']['entries'] == [],
                'failed archive left visible records')

    duplicate = fixtures / 'duplicate-schema.json'
    duplicate.write_text('{"schema_version":1,' + json.dumps(bundle)[1:])
    action('reject duplicate archive keys', 'restore', scratch / 'duplicate-restore', duplicate, ok=False)

    # A signed hostile author still receives no authority to approve or locate by similarity.
    negatives = []
    for name in ['ambiguous', 'missing', 'grapheme', 'unit', 'self-approved']:
        request = json.loads(json.dumps(requests[0]))
        selector = request['draft']['selector']
        if name == 'ambiguous':
            selector.update(start=None, end=None, exact='重复引文')
        elif name == 'missing':
            selector.update(start=None, end=None, exact='不存在的引文')
        elif name == 'grapheme':
            selector.update(start=5, end=6, exact='\u0301')
        elif name == 'unit':
            selector['unit'] = 'utf16'
        else:
            request['approved'] = True
        negatives.append((name, json.dumps(request, ensure_ascii=False)))
    negatives.append(('duplicate-protocol', '{"capture_protocol":1,' + json.dumps(requests[0], ensure_ascii=False)[1:]))
    manifest = json.loads((fixtures / 'manifest-0.json').read_text())
    for name, raw in negatives:
        package_id = f'org.example.hostile-{name}'
        manifest['package_id'] = package_id
        manifest['contributions'][0]['id'] = package_id + '.capture'
        manifest_path = fixtures / f'negative-{name}-manifest.json'
        manifest_path.write_text(json.dumps(manifest))
        request_path = fixtures / f'negative-{name}-request.json'
        request_path.write_text(raw)
        package = fixtures / f'negative-{name}.qpk'
        key = command([author, request_path, manifest_path, seed, package,
                       '--unchecked-test-request']).stdout.strip()
        action(f'install signed negative {name}', 'install', packages, f'install-negative-{name}',
               0, package, key)
        rejected = action(f'reject signed hostile proposal {name}', 'capture', packages, core, 'local-vault',
                          f'negative-{name}', package_id, '', ok=False)
        expected_errors = {'ambiguous': 'Ambiguous', 'missing': 'NotFound', 'grapheme': 'InvalidBoundary',
                           'unit': 'InvalidSelector', 'self-approved': 'Malformed', 'duplicate-protocol': 'Malformed'}
        require(expected_errors[name] in rejected['error'], 'capture failure lost its explicit reason')
        require(action(f'hostile proposal has no receipt {name}', 'receipt', core,
                       f'negative-{name}')['receipt'] is None, 'hostile guest committed knowledge')

    # The package CLI manages the actual same installed-package database.
    package_manifest = ROOT / 'experiments/extension-package/Cargo.toml'
    command(['cargo', 'build', '--manifest-path', package_manifest, '--locked', '--offline'])
    package_graph = metadata(package_manifest)
    manager = Path(package_graph['target_directory']) / 'debug/qusor-package-probe'
    for index, package_id in enumerate(ids):
        action(f'uninstall source {index}', 'uninstall', packages, f'uninstall-{index}', 1,
               package_id, executable=manager)
        action(f'uninstalled source cannot propose {index}', 'capture', packages, core, 'local-vault',
               'after-uninstall', package_id, '', ok=False)
        saved = captured[index][0]
        action(f'knowledge survives uninstall {index}', 'query', core, saved['capture_id'])
    require(action('export survives all sources removed', 'export', core)['bundle'] == bundle,
            'uninstall changed Core knowledge')


if __name__ == '__main__':
    main()
