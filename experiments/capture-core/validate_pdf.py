"""Qualify real PDF retention, ordinary plugin capture and mixed Core recovery."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import platform
import sys
import tempfile
import tomllib

import validate as capture
from pdf_adapter import freeze_pdf, neutral_snapshot, selection_draft

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PROVIDER = ROOT / 'experiments/pdf-provider'
sys.path.insert(0, str(PROVIDER))
spec = importlib.util.spec_from_file_location('native_pdf_validation', PROVIDER / 'validate.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False))
    return path


def action(runner, name, *args, ok=True, crash=False):
    completed = capture.command([runner, *args], exits=(86,) if crash else ((0,) if ok else (1,)))
    report = {'crash_exit': 86} if crash else json.loads(completed.stdout)
    if not crash:
        capture.require(report['ok'] == ok, f'{name}: unexpected status')
    capture.SCENARIOS.append({'name': name, 'arguments': list(map(str, args)), 'report': report})
    return report


def scenarios(runner, author, worker, library, scratch):
    vault = 'pdf-vault'
    core = scratch / 'knowledge'
    packages = scratch / 'packages'
    original = scratch / 'frozen-original.pdf'
    original.write_bytes(freeze_pdf(PROVIDER / 'fixtures/text-two-pages.pdf'))
    before = capture.digest(original)
    report = native.worker(worker, 'real frozen PDF native extraction',
                           ['inspect', library, original, scratch / 'native-render'])['report']
    capture.require(report['original_sha256'] == before == capture.digest(original), 'native source input changed')
    snapshot = neutral_snapshot(report)
    snapshot_file = write(scratch / 'neutral-snapshot.json', snapshot)
    imported = action(runner, 'import real PDF original and neutral map', 'pdf-import', core, vault,
                      'pdf-import', 'Local original', original, snapshot_file)['receipt']
    same = action(runner, 'idempotent PDF import', 'pdf-import', core, vault, 'pdf-import',
                  'Local original', original, snapshot_file)['receipt']
    capture.require(same == imported, 'import duplicated original intent')
    action(runner, 'import same operation changed title conflict', 'pdf-import', core, vault,
           'pdf-import', 'Changed title', original, snapshot_file, ok=False)
    document = action(runner, 'restart reads retained PDF map', 'pdf-document', core,
                      imported['document_id'])['document']
    capture.require(document['snapshot'] == snapshot, 'persisted PDF map changed')
    draft = selection_draft(document, 0, 'immutable originals')
    draft_file = write(scratch / 'draft.json', draft)
    seed = scratch / 'public-test-seed.hex'
    seed.write_text('11' * 32)
    identity = 'org.example.native-pdf-reader'
    manifest = {'manifest_version': 1, 'package_id': identity, 'publisher_id': 'org.example',
                'package_version': {'major': 1, 'minor': 0, 'patch': 0},
                'host_api': {'major': 0, 'min_minor': 1, 'max_minor': 1},
                'required_capabilities': ['selection.read', 'capture.propose'], 'optional_capabilities': [],
                'contributions': [{'id': identity + '.capture', 'title': 'Save PDF selection', 'kind': 'command',
                                   'required_capabilities': ['selection.read', 'capture.propose']}]}
    manifest_file = write(scratch / 'manifest.json', manifest)
    package = scratch / 'reader.qpk'
    lifecycle = write(scratch / 'lifecycle-v1.json', {'protocol': 1, 'private_schema': 1,
                       'migration_module': None, 'accepted_from_schemas': []})
    key = capture.command([author, '--pdf-relay', vault, manifest_file, seed, package, lifecycle, '-']).stdout.strip()
    action(runner, 'install ordinary outside-SDK PDF reader', 'install', packages, 'install-reader', 0, package, key)
    manager = ROOT / 'experiments/extension-package/target/debug/qusor-package-probe'
    private_bytes = scratch / 'private-preferences.bin'
    private_bytes.write_bytes(b'theme=dark')
    initialized = action(manager, 'initialize ordinary reader private state', 'initialize-private', packages,
                         'initialize-reader', 1, identity, private_bytes)['receipt']
    reader_revision = initialized['installation']['revision']
    args = ['pdf-capture', packages, core, vault, 'pdf-note', identity,
            imported['document_id'], draft_file, 'This is my own interpretation.']
    preview = action(runner, 'ordinary guest reads selection and constructs preview', *args)
    capture.require(preview['committed'] is False, 'preview saved a PDF note')
    capture.require(preview['preview']['title'] == 'Local original' and preview['preview']['page_number'] == 1
                    and preview['preview']['user_note'] == args[-1], 'review omits source/page/user note')
    capture.require(action(runner, 'preview has no PDF receipt', 'pdf-receipt', core,
                           'pdf-note')['receipt'] is None, 'preview wrote a receipt')
    expected = preview['preview']['draft_hash']
    action(runner, 'wrong reviewed PDF hash rejected', *args, '--accept', '0' * 64, ok=False)
    saved = action(runner, 'accept ordinary PDF plugin proposal', *args, '--accept', expected)['receipt']
    capture.require(action(runner, 'same PDF note retry returns receipt', *args,
                           '--accept', expected)['receipt'] == saved, 'PDF retry duplicated note')
    changed = args.copy()
    changed[-1] = 'Changed user note'
    action(runner, 'same PDF intent changed note conflicts', *changed, '--accept', expected, ok=False)
    selection = action(runner, 'restart exact PDF source resolution', 'pdf-resolve', core,
                       saved['capture_id'], draft_file)['selection']
    capture.require(selection['exact'] == draft['exact'], 'wrong saved PDF quote')
    for field, replacement in [('original_sha256', '0' * 64), ('extraction_revision', '0' * 64),
                               ('page_text_sha256', '0' * 64), ('unit', 'utf16'), ('exact', 'similar original')]:
        bad = dict(draft, **{field: replacement})
        path = write(scratch / f'bad-{field}.json', bad)
        action(runner, f'no saved-anchor fallback for {field}', 'pdf-resolve', core,
               saved['capture_id'], path, ok=False)
    action(runner, 'wrong vault cannot borrow PDF selection', 'pdf-capture', packages, core,
           'other-vault', 'cross-vault', identity, imported['document_id'], draft_file, '', ok=False)

    # A legacy text capture after PDF exercises independent local IDs/global revisions.
    text_request = {'capture_protocol': 1, 'request_id': 'text-after-pdf', 'draft': {
        'protocol_version': 1, 'source': {'namespace': 'neutral.text', 'reference': 'one', 'title': 'Text source',
                                       'public_uri': None, 'external_return': 'none'},
        'snapshot_text': 'A separate immutable text source.',
        'selector': {'unit': 'unicode_scalar', 'normalization': 'qusor-raw-text-v1',
                     'start': 11, 'end': 20, 'exact': 'immutable'}}}
    text_manifest = json.loads(json.dumps(manifest))
    text_manifest.update(package_id='org.example.text-after-pdf', required_capabilities=['capture.propose'])
    text_manifest['contributions'][0].update(id='org.example.text-after-pdf.capture', required_capabilities=['capture.propose'])
    text_key = capture.command([author, write(scratch / 'text-request.json', text_request),
                               write(scratch / 'text-manifest.json', text_manifest), seed,
                               scratch / 'text.qpk']).stdout.strip()
    action(runner, 'install text alongside PDF', 'install', packages, 'install-text', 0, scratch / 'text.qpk', text_key)
    text_args = ['capture', packages, core, vault, 'text-after-pdf', 'org.example.text-after-pdf', 'Text note']
    text_preview = action(runner, 'legacy text preview in mixed Core', *text_args)
    text_receipt = action(runner, 'legacy text saves after PDF revision', *text_args, '--accept',
                          text_preview['preview']['draft_hash'])['receipt']
    capture.require(text_receipt['capture_id'] == 1 and text_receipt['commit_rev'] > saved['commit_rev'],
                    'local text entity ID confused with global commit revision')
    action(runner, 'legacy export cannot silently omit PDF', 'export', core, ok=False)
    archive = action(runner, 'unified complete mixed archive', 'export-all', core)['archive']
    capture.require(len(archive['pdf_documents']) == 1 and len(archive['pdf_captures']) == 1
                    and len(archive['text_entries']) == 1, 'mixed archive misses knowledge')
    capture.require([entry['commit_rev'] for entry in archive['commits']] == [1, 2, 3], 'noncontiguous global history')
    archive_file = write(scratch / 'archive.json', archive)
    restored = scratch / 'restored'
    action(runner, 'restore unified mixed Core', 'restore-all', restored, archive_file)
    capture.require(action(runner, 'restored unified archive equals original', 'export-all', restored)['archive'] == archive,
                    'restore changed canonical mixed knowledge')
    restored_pdf = scratch / 'restored.pdf'
    action(runner, 'read original from restored binary CAS', 'pdf-original', restored,
           imported['document_id'], restored_pdf)
    capture.require(restored_pdf.read_bytes() == original.read_bytes(), 'PDF original bytes changed')
    rebuilt = native.worker(worker, 'restored original renders in a new native worker',
                            ['inspect', library, restored_pdf, scratch / 'restored-render'])['report']
    capture.require(neutral_snapshot(rebuilt) == snapshot, 'restored original remaps to another extraction')
    capture.require([page['rendered_sha256'] for page in rebuilt['pages']] ==
                    [page['rendered_sha256'] for page in report['pages']], 'restored page raster changed')
    action(runner, 'restored PDF anchor resolves', 'pdf-resolve', restored, saved['capture_id'], draft_file)
    action(runner, 'nonempty mixed restore refused', 'restore-all', restored, archive_file, ok=False)

    # All faults use actual process exit, then a fresh Core process observes state.
    for point in ['after_blob_write', 'before_blob_publish', 'after_blob_publish', 'before_transaction',
                  'after_entities', 'before_commit', 'after_commit']:
        destination = scratch / f'import-crash-{point}'
        import_args = ['pdf-import', destination, vault, 'interrupted-import', 'Original', original, snapshot_file]
        action(runner, f'actual PDF import crash {point}', *import_args, '--crash', point, crash=True)
        prior = action(runner, f'PDF import receipt after {point}', 'document-receipt', destination,
                       'interrupted-import')['receipt']
        capture.require((prior is not None) == (point == 'after_commit'), 'import crash atomicity failed')
        retried = action(runner, f'PDF import retry {point}', *import_args)['receipt']
        capture.require(retried['document_id'] == 1 and (prior is None or retried == prior), 'import retry duplicates document')
    for point in ['before_transaction', 'after_entities', 'before_commit', 'after_commit']:
        op = f'capture-crash-{point}'
        crash_args = list(args)
        crash_args[4] = op
        action(runner, f'actual approved PDF note crash {point}', *crash_args, '--accept', expected,
               '--crash', point, crash=True)
        prior = action(runner, f'PDF note receipt after {point}', 'pdf-receipt', core, op)['receipt']
        capture.require((prior is not None) == (point == 'after_commit'), 'capture crash atomicity failed')
        retried = action(runner, f'PDF note retry {point}', *crash_args, '--accept', expected)['receipt']
        capture.require(prior is None or retried == prior, 'capture lost-response retry changes receipt')
    for point in ['after_blob_publish', 'after_entities', 'after_commit']:
        destination = scratch / f'restore-crash-{point}'
        action(runner, f'actual mixed restore crash {point}', 'restore-all', destination, archive_file,
               '--crash', point, crash=True)
        state = action(runner, f'visible mixed state after restore crash {point}', 'export-all', destination)['archive']
        capture.require(len(state['commits']) == (3 if point == 'after_commit' else 0), 'partial mixed restore visible')
        if point != 'after_commit':
            action(runner, f'retry mixed restore {point}', 'restore-all', destination, archive_file)
    for name, mutate in [('missing-binary', lambda a: a['blobs'].clear()),
                         ('wrong-binary', lambda a: a['blobs'][0].update(base64='AAAA')),
                         ('wrong-page-map', lambda a: a['pdf_documents'][0]['snapshot']['pages'][0].update(engine_text='similar')),
                         ('commit-gap', lambda a: a['commits'][0].update(commit_rev=9)),
                         ('authorization-injection', lambda a: a.update(grants=['capture.propose']))]:
        damaged = json.loads(json.dumps(archive))
        mutate(damaged)
        path = write(scratch / f'archive-{name}.json', damaged)
        destination = scratch / f'reject-{name}'
        action(runner, f'reject damaged mixed archive {name}', 'restore-all', destination, path, ok=False)
        capture.require(not action(runner, f'archive rejection leaves no commits {name}', 'export-all', destination)['archive']['commits'],
                        'rejected archive left partial knowledge')
    duplicate = scratch / 'duplicate-archive.json'
    duplicate.write_text('{"schema_version":2,' + json.dumps(archive)[1:])
    action(runner, 'duplicate archive keys rejected before restore', 'restore-all', scratch / 'duplicate', duplicate, ok=False)

    # The same ordinary reader really upgrades/migrates and returns to its retained pair.
    stable = action(runner, 'Core state before reader package migration', 'export-all', core)['archive']
    manifest_v2 = json.loads(json.dumps(manifest))
    manifest_v2['package_version']['major'] = 2
    lifecycle_v2 = write(scratch / 'lifecycle-v2.json', {'protocol': 1, 'private_schema': 2,
                         'migration_module': 'migration.wasm', 'accepted_from_schemas': [1]})
    migration = scratch / 'private-migrate-good.wat'
    migration.write_bytes((ROOT / 'experiments/extension-package/fixtures/private-migrate-good.wat').read_bytes())
    upgraded_package = scratch / 'reader-v2.qpk'
    capture.command([author, '--pdf-relay', vault, write(scratch / 'manifest-v2.json', manifest_v2),
                     seed, upgraded_package, lifecycle_v2, migration])
    upgraded = action(manager, 'real PDF reader private migration upgrade', 'upgrade-private', packages,
                      'upgrade-reader', reader_revision, upgraded_package, key)['receipt']
    capture.require(upgraded['outcome'] == 'succeeded', 'real reader migration did not succeed')
    status = action(manager, 'inspect migrated PDF reader pair', 'private-status', packages, identity)['private']
    capture.require(status['current']['schema'] == 2 and len(status['retained']) == 1, 'paired reader migration missing')
    capture.require(action(runner, 'PDF knowledge survives real reader upgrade', 'export-all', core)['archive'] == stable,
                    'upgrade changed Core knowledge')
    rolled = action(manager, 'real PDF reader paired rollback', 'rollback-private', packages,
                    'rollback-reader', upgraded['installation']['revision'], identity,
                    status['retained'][0]['pair_id'])['receipt']
    capture.require(rolled['outcome'] == 'succeeded', 'paired rollback failed')
    status = action(manager, 'inspect restored PDF reader private bytes', 'private-status', packages, identity)['private']
    capture.require(status['current']['schema'] == 1 and status['current']['state_hex'] == b'theme=dark'.hex(),
                    'rollback failed to recover original reader preferences')
    capture.require(action(runner, 'PDF knowledge survives reader rollback', 'export-all', core)['archive'] == stable,
                    'rollback changed Core PDF knowledge')
    action(runner, 'rolled-back reader can read retained PDF selection', *args)
    before_uninstall = action(runner, 'snapshot before reader uninstall', 'export-all', core)['archive']
    action(manager, 'uninstall ordinary PDF reader', 'uninstall', packages, 'uninstall-reader',
           rolled['installation']['revision'], identity)
    action(runner, 'removed reader cannot execute again', *args, ok=False)
    capture.require(action(runner, 'all PDF knowledge survives uninstall', 'export-all', core)['archive'] == before_uninstall,
                    'uninstall deleted Core PDF state')
    action(runner, 'PDF anchor survives reader uninstall', 'pdf-resolve', core, saved['capture_id'], draft_file)
    return {'original_sha256': before, 'extraction_revision': imported['extraction_revision'],
            'restored_render_sha256': [page['rendered_sha256'] for page in rebuilt['pages']]}


def main():
    library = PROVIDER / 'target/pdfium/lib/libpdfium.so'
    capture.require(capture.digest(library) == native.LIBRARY_SHA256, 'native library differs from qualified runtime')
    graph = capture.metadata(HERE / 'Cargo.toml')
    policy = tomllib.loads((ROOT / 'config/architecture.toml').read_text())
    capture.require(not capture.validate_graph(graph, policy['experiments']['capture-core'], ROOT), 'Core dependency policy failed')
    author_graph = capture.metadata(HERE / 'author/Cargo.toml')
    capture.require(not capture.validate_graph(author_graph, policy['experiments']['capture-core-author'], ROOT), 'author policy failed')
    for args in [['cargo', 'fmt', '--all', '--', '--check'],
                 ['cargo', 'fmt', '--manifest-path', HERE / 'author/Cargo.toml', '--all', '--', '--check'],
                 ['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'],
                 ['cargo', 'test', '--locked', '--offline'], ['cargo', 'build', '--locked', '--offline']]:
        capture.command(args)
    capture.command(['cargo', 'build', '--manifest-path', PROVIDER / 'Cargo.toml', '--locked', '--offline'])
    provider_graph = capture.metadata(PROVIDER / 'Cargo.toml')
    worker = Path(provider_graph['target_directory']) / 'debug/qusor-pdf-provider-probe'
    runner = Path(graph['target_directory']) / 'debug/qusor-capture-probe'
    host_hash = capture.digest(runner)
    worker_hash = capture.digest(worker)
    with tempfile.TemporaryDirectory(prefix='qusor-pdf-core-') as temporary:
        scratch = Path(temporary)
        author, sdk, sdk_packages = capture.independent_author(scratch / 'sdk')
        legacy = scratch / 'legacy'
        legacy.mkdir()
        capture.run_scenarios(runner, author, legacy)
        pdf = scratch / 'pdf'
        pdf.mkdir()
        result = scenarios(runner, author, worker, library, pdf)
    capture.require(capture.digest(runner) == host_hash and capture.digest(worker) == worker_hash, 'native authority binary changed during validation')
    evidence = HERE / 'evidence/linux-pdf-core.json'
    evidence.write_text(json.dumps({'platform': platform.platform(), 'host_sha256': host_hash,
        'provider_sha256': worker_hash, 'runtime_sha256': native.LIBRARY_SHA256,
        'sdk_sources': sdk, 'independent_author_packages': sdk_packages,
        'source_sha256': capture.source_fingerprints(), 'pdf': result, 'checks': capture.CHECKS,
        'provider_source_sha256': {str(path.relative_to(ROOT)): capture.digest(path)
                                  for path in sorted((PROVIDER / 'src').rglob('*.rs'))},
        'scenarios': capture.SCENARIOS, 'native_scenarios': native.SCENARIOS,
        'unverified': ['production Core', 'library import plugin', 'NSH Surface',
                       'reliable reading order', 'Chinese PDF/fonts', 'other platforms']}, ensure_ascii=False, indent=2) + '\n')
    print(f'{len(capture.CHECKS)} commands; {len(capture.SCENARIOS)} scenarios; {evidence}')


if __name__ == '__main__':
    main()
