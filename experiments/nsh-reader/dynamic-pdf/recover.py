"""Reopen actual Core captures, restore a full archive, and compare original bytes."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / 'experiments/capture-core/target/debug/qusor-capture-probe'


def run(*args):
    value = json.loads(subprocess.check_output([str(BINARY), *map(str, args)], text=True))
    assert value['ok'], value
    return value


def recover(store, restored, output, capture_ids, documents):
    capture_ids = list(capture_ids)
    if not capture_ids:
        raise ValueError('capture recovery requires at least one saved capture')
    if restored.exists():
        raise ValueError('restore requires a new empty destination')
    work = restored.parent / (restored.name + '-artifacts'); work.mkdir()
    before = [run('pdf-compound-query', store, i) for i in capture_ids]
    archive = run('export-all', store)['archive']
    archive_file = work / 'archive.json'; archive_file.write_text(json.dumps(archive))
    restore = run('restore-all', restored, archive_file)
    after = [run('pdf-compound-query', restored, i) for i in capture_ids]
    assert before == after
    resolutions = []
    for i, value in zip(capture_ids, after):
        capture = value['capture']; draft = capture.get('compound', capture.get('draft'))
        draft_file = work / f'draft-{i}.json'; draft_file.write_text(json.dumps(draft))
        resolution = run('pdf-compound-resolve' if 'compound' in capture else 'pdf-resolve', restored, i, draft_file)
        assert resolution['selection'].get('draft', resolution['selection'].get('anchor')) == draft
        resolutions.append(resolution)
    originals = []
    for document, source in documents:
        recovered = work / f'original-{document}.pdf'; run('pdf-original', restored, document, recovered)
        assert recovered.read_bytes() == Path(source).read_bytes()
        originals.append({'document_id': document, 'source': str(source), 'sha256': hashlib.sha256(recovered.read_bytes()).hexdigest()})
    result = {'restore': restore, 'captures_equal_before_after_restore': True, 'records': after,
              'exact_resolutions': resolutions, 'originals': originals,
              'archive_sha256': hashlib.sha256(archive_file.read_bytes()).hexdigest()}
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('--store', type=Path, required=True)
    p.add_argument('--restored', type=Path, required=True); p.add_argument('--output', type=Path, required=True)
    p.add_argument('--capture', type=int, action='append', required=True)
    p.add_argument('--document', nargs=2, action='append', required=True, metavar=('ID', 'SOURCE'))
    a = p.parse_args(); print(json.dumps(recover(a.store, a.restored, a.output, a.capture, [(int(i), Path(s)) for i,s in a.document]), ensure_ascii=False))
