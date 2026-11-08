"""Qualify native PDF pointer selection using real glyph pixels and C1 storage."""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import platform
import sys
import tempfile

import validate as native

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CORE = ROOT / 'experiments/capture-core'
sys.path.insert(0, str(CORE))
from pdf_adapter import neutral_snapshot, native_selection_draft  # noqa: E402
spec = importlib.util.spec_from_file_location('capture_validation_helpers', CORE / 'validate.py')
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


def write(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False))
    return path


def binding(report, page):
    return {'original_sha256': report['original_sha256'], 'frame_report_sha256': report['extraction_sha256'],
            'page_text_sha256': report['pages'][page]['page_text_sha256'], 'page_index': page}


def viewport(**changes):
    return {'page_origin_dip': [0., 0.], 'scroll_dip': [0., 0.], 'zoom': 1., 'device_scale': 1.,
            'generation': 1, **changes}


def dip(point, view):
    return {'x': point[0] * view['zoom'] / view['device_scale'] + view['page_origin_dip'][0] - view['scroll_dip'][0],
            'y': point[1] * view['zoom'] / view['device_scale'] + view['page_origin_dip'][1] - view['scroll_dip'][1]}


def ink_point(page, image, index):
    width, height, pixels = native.ppm(image)
    quad = page['characters'][index]['pixel_quad']
    native.require(quad is not None, 'point oracle requires a visible glyph')
    left, right = min(p[0] for p in quad), max(p[0] for p in quad)
    top, bottom = min(p[1] for p in quad), max(p[1] for p in quad)
    candidates = [(x+.5, y+.5) for y in range(max(0, top+1), min(height, bottom))
                  for x in range(max(0, left+1), min(width, right))
                  if min(pixels[(y*width+x)*3:(y*width+x)*3+3]) < 128]
    native.require(candidates, 'real glyph has no visible interior ink')
    return min(candidates, key=lambda p: (p[0]-(left+right)/2)**2 + (p[1]-(top+bottom)/2)**2)


def core_action(runner, name, *args, ok=True):
    result = capture.command([runner, *args], exits=(0,) if ok else (1,))
    value = json.loads(result.stdout)
    native.require(value['ok'] == ok, f'{name}: wrong Core status')
    capture.SCENARIOS.append({'name': name, 'arguments': list(map(str, args)), 'result': value})
    return value


def overlay(image, selection, output):
    width, height, pixels = native.ppm(image)
    rgb = bytearray(pixels)
    for index, segment in enumerate(selection['segments']):
        color = bytes([230,110,0] if index % 2 == 0 else [0,100,210])
        for character in segment['characters']:
            quad = character['pixel_quad']
            if not quad:
                continue
            left, right = max(0,min(p[0] for p in quad)), min(width-1,max(p[0] for p in quad))
            top, bottom = max(0,min(p[1] for p in quad)), min(height-1,max(p[1] for p in quad))
            edges = ([(x,y) for x in range(left,right+1) for y in [top,bottom]]
                     + [(x,y) for y in range(top,bottom+1) for x in [left,right]])
            for x,y in edges:
                offset = (y*width+x)*3
                rgb[offset:offset+3] = color
    native.png(output,width,height,bytes(rgb))


def integrate_core(runner, native_runner, library, scratch, report, selection, multiline):
    snapshot = write(scratch / 'snapshot.json', neutral_snapshot(report))
    core = scratch / 'knowledge'
    original = HERE / 'fixtures/text-two-pages.pdf'
    imported = core_action(runner, 'import actual pointer source', 'pdf-import', core, 'selection-vault',
                           'source', 'Native pointer source', original, snapshot)['receipt']
    document = core_action(runner, 'read retained semantic version before native bridge', 'pdf-document', core,
                           imported['document_id'])['document']
    part = selection['segments'][0]
    draft = native_selection_draft(document, report, selection)
    draft_file = write(scratch / 'pointer-draft.json', draft)
    author, sdk, packages_graph = capture.independent_author(scratch / 'sdk')
    manifest = {'manifest_version': 1, 'package_id': 'org.example.pointer-reader', 'publisher_id': 'org.example',
                'package_version': {'major': 1, 'minor': 0, 'patch': 0},
                'host_api': {'major': 0, 'min_minor': 1, 'max_minor': 1},
                'required_capabilities': ['selection.read', 'capture.propose'], 'optional_capabilities': [],
                'contributions': [{'id': 'org.example.pointer-reader.capture', 'kind': 'command',
                                   'title': 'Save native selection', 'required_capabilities': ['selection.read', 'capture.propose']}]}
    seed = scratch / 'seed.hex'
    seed.write_text('11' * 32)
    package = scratch / 'reader.qpk'
    key = capture.command([author, '--pdf-relay', 'selection-vault', write(scratch / 'manifest.json', manifest),
                           seed, package]).stdout.strip()
    installed = scratch / 'packages'
    core_action(runner, 'ordinary pointer reader installation', 'install', installed, 'install', 0, package, key)
    args = ['pdf-capture', installed, core, 'selection-vault', 'pointer-note', 'org.example.pointer-reader',
            imported['document_id'], draft_file, 'Note from a native geometric selection.']
    review = core_action(runner, 'pointer result becomes ordinary plugin preview', *args)
    native.require(core_action(runner, 'pointer release alone has no receipt', 'pdf-receipt', core,
                               'pointer-note')['receipt'] is None, 'pointer action wrote knowledge without approval')
    saved = core_action(runner, 'explicitly accept pointer-derived selection', *args, '--accept',
                        review['preview']['draft_hash'])['receipt']
    resolved = core_action(runner, 'saved pointer anchor resolves after restart', 'pdf-resolve', core,
                           saved['capture_id'], draft_file)['selection']
    native.require(resolved['exact'] == part['exact'], 'pointer saved another range')
    archive = core_action(runner, 'archive pointer note and original', 'export-all', core)['archive']
    restored = scratch / 'restored'
    core_action(runner, 'restore pointer note to empty Core', 'restore-all', restored,
                write(scratch / 'archive.json', archive))
    core_action(runner, 'restored pointer source remains exact', 'pdf-resolve', restored, saved['capture_id'], draft_file)
    restored_pdf = scratch / 'restored.pdf'
    core_action(runner, 'recover pointer source original', 'pdf-original', restored, imported['document_id'], restored_pdf)
    rerendered = native.worker(native_runner, 'restored pointer source native render',
                              ['inspect', library, restored_pdf, scratch / 'restored-image'])['report']
    native.require(neutral_snapshot(rerendered) == neutral_snapshot(report), 'restored native source changed')
    native.require(not multiline['committable_single_segment'] and len(multiline['segments']) > 1,
                   'multiline selection must require a compound contract')
    try:
        native_selection_draft(document, report, multiline)
    except ValueError as error:
        native.require('atomic' in str(error), 'compound bridge failure lost its reason')
    else:
        raise RuntimeError('multi-line native result silently flattened into one Core draft')
    native.require(len(core_action(runner, 'multi-segment preview was not partially saved', 'export-all', core)
                       ['archive']['pdf_captures']) == 1, 'multi-segment preview created partial captures')
    return {'sdk_sources': sdk, 'author_packages': packages_graph, 'saved_exact': part['exact']}


def run(runner, core_runner, library, scratch):
    original = HERE / 'fixtures/text-two-pages.pdf'
    images = scratch / 'source'
    report = native.worker(runner, 'freeze real pointer pages', ['inspect', library, original, images])['report']
    counter = 0
    def select(name, page, operation, *, view=None, source=original, source_report=report, ok=True, bad_binding=None):
        nonlocal counter
        counter += 1
        script = {'binding': bad_binding or binding(source_report, page), 'viewport': view or viewport(), 'operation': operation}
        script_file = write(scratch / f'script-{counter}.json', script)
        return native.worker(runner, name, ['select', library, source, scratch / f'output-{counter}', script_file], ok=ok)
    single = None
    for page_index in [0, 1]:
        page = report['pages'][page_index]
        quote = 'immutable originals'
        start = page['page_text'].index(quote)
        first = ink_point(page, images / page['rendered_file'], start)
        last = ink_point(page, images / page['rendered_file'], start+len(quote)-1)
        for index, view in enumerate([viewport(), viewport(zoom=1.75,device_scale=2.,page_origin_dip=[90.,40.],scroll_dip=[13.,27.])]):
            a, f = dip(first, view), dip(last, view)
            hit = select(f'actual ink point hit page {page_index} viewport {index}', page_index,
                         {'kind': 'hit', 'point': a}, view=view)['hit']
            native.require(hit['char_index'] == start, 'physical glyph hit another character')
            result = select(f'native single-line drag page {page_index} viewport {index}', page_index,
                            {'kind': 'drag', 'anchor': a, 'focus': f}, view=view)['selection']
            native.require(result['joined_text'] == quote and result['committable_single_segment'], 'wrong native selected quote')
            backward = select(f'reverse native drag page {page_index} viewport {index}', page_index,
                              {'kind': 'drag', 'anchor': f, 'focus': a}, view=view)['selection']
            native.require(backward['segments'] == result['segments'] and backward['direction'] == 'Backward',
                           'reverse drag changed selected content')
            if page_index == 0 and index == 0:
                single = result
        first_line = 'Repeated evidence stays local.'
        next_line = 'Distinct first-page context.' if page_index == 0 else 'Second-page context is explicit.'
        s = page['page_text'].index(first_line)
        e = page['page_text'].index(next_line) + len(next_line) - 1
        multi = select(f'explicit ordered multi-line page {page_index}', page_index,
                       {'kind': 'drag', 'anchor': dip(ink_point(page, images/page['rendered_file'], s), viewport()),
                        'focus': dip(ink_point(page, images/page['rendered_file'], e), viewport())})['selection']
        native.require(multi['joined_text'] == first_line+'\n'+next_line and len(multi['segments']) == 2
                       and not multi['committable_single_segment'], 'multi-line source order guessed incorrectly')
        overlay(images/page['rendered_file'],multi,HERE/f'evidence/multiline-page-{page_index}.png')
        if page_index == 1:
            native.require(multi['segments'][0]['start'] > multi['segments'][1]['start'],
                           'rotated page engine-order difference was hidden')
        if page_index == 0:
            multiline = multi
    page = report['pages'][0]
    quote = 'immutable originals'
    start = page['page_text'].index(quote)
    a = dip(ink_point(page, images/page['rendered_file'], start), viewport())
    f = dip(ink_point(page, images/page['rendered_file'], start+len(quote)-1), viewport())
    blank = {'x': 800., 'y': 1100.}
    select('blank does not snap to nearby text', 0, {'kind': 'hit', 'point': blank}, ok=False)
    for field in ['original_sha256', 'frame_report_sha256', 'page_text_sha256']:
        bad = binding(report, 0)
        bad[field] = '0' * 64
        select(f'wrong {field} cannot hit', 0, {'kind':'hit','point':a}, bad_binding=bad, ok=False)
    down = {'kind':'down','pointer_id':1,'point':a}
    move = {'kind':'move','pointer_id':1,'point':f}
    up = {'kind':'up','pointer_id':1,'point':f}
    trace = select('native pointer release creates Ready once', 0, {'kind':'trace','events':[down,move,up,up]})['trace']
    native.require('Ready' in trace[2]['outcome'] and trace[3]['error']['code']=='NotDragging', 'duplicate up committed another gesture')
    trace = select('blank move clears previous selectable preview', 0, {'kind':'trace','events':
                   [down,move,{'kind':'move','pointer_id':1,'point':blank},{'kind':'up','pointer_id':1,'point':blank}]})['trace']
    native.require(trace[2]['preview'] is None and not trace[3]['ok'], 'invalid endpoint reused last good selection')
    trace = select('cancel blocks late pointer release', 0, {'kind':'trace','events':[down,move,{'kind':'cancel'},up]})['trace']
    native.require(trace[3]['error']['code']=='NotDragging', 'cancel leaked a ready selection')
    trace = select('second pointer cannot steal native gesture', 0, {'kind':'trace','events':
                   [down,{'kind':'down','pointer_id':2,'point':f},move,up]})['trace']
    native.require(not trace[1]['ok'] and 'Ready' in trace[3]['outcome'], 'second pointer replaced original ownership')
    for name, change in [('zoom', {'zoom':2.}),('scroll',{'scroll_dip':[2.,0.]}),
                         ('dpi',{'device_scale':2.}),('generation',{'generation':2})]:
        trace = select(f'{name} change cancels active pointer gesture', 0, {'kind':'trace','events':
                       [down,move,{**move,'viewport':viewport(**change)},up]})['trace']
        native.require(trace[2]['error']['code']=='Cancelled' and trace[2]['preview'] is None
                       and not trace[3]['ok'], 'viewport change left a committable preview')
    for name in ['selection-two-columns.pdf', 'selection-overlap.pdf']:
        source = HERE/'fixtures'/name
        output = scratch/name
        unusual = native.worker(runner, f'render actual negative {name}', ['inspect',library,source,output])['report']
        chars = [c for c in unusual['pages'][0]['characters'] if c['pixel_quad']]
        if 'columns' in name:
            p = unusual['pages'][0]
            # Source positions are real glyph pixels on the left column; a whole-page
            # paragraph classifier must not silently swallow the right column.
            first, last = chars[0], chars[-1]
            op = {'kind':'drag','anchor':dip(ink_point(p,output/p['rendered_file'],first['scalar_start']),viewport()),
                  'focus':dip(ink_point(p,output/p['rendered_file'],last['scalar_start']),viewport())}
        else:
            native.require(len(chars)>=2, 'PDF engine collapsed overlap fixture')
            boxes = [c['pixel_quad'] for c in chars[:2]]
            x = (max(min(v[0] for v in box) for box in boxes)+min(max(v[0] for v in box) for box in boxes))/2
            y = (max(min(v[1] for v in box) for box in boxes)+min(max(v[1] for v in box) for box in boxes))/2
            op = {'kind':'hit','point':{'x':x,'y':y}}
        failure = select(f'no heuristic fallback for {name}',0,op,source=source,source_report=unusual,ok=False)
        native.require(('UnsupportedLayout' if 'columns' in name else 'Ambiguous') in failure['error'], 'layout rejection lost reason')
    scan = HERE/'fixtures/image-only.pdf'
    scan_report = native.worker(runner, 'actual no-text source remains displayable',
                                ['inspect',library,scan,scratch/'scan-image'])['report']
    failure = select('actual no-text source refuses pointer text selection',0,{'kind':'hit','point':a},
                     source=scan,source_report=scan_report,ok=False)
    native.require('NoText' in failure['error'],'no-text selection lost explicit state')
    script = {'binding':binding(report,0),'viewport':viewport(),'operation':{'kind':'hit','point':a}}
    for name, raw in [('unknown-field',json.dumps({**script,'approved':True})),
                      ('duplicate-binding','{"binding":'+json.dumps(script['binding'])+','+json.dumps(script)[1:]),
                      ('event-budget',json.dumps({**script,'operation':{'kind':'trace','events':[down]*65}}))]:
        path = scratch/f'{name}.json'
        path.write_text(raw)
        native.worker(runner,f'selection protocol rejects {name}',
                       ['select',library,original,scratch/f'bad-{name}',path],ok=False)
    return integrate_core(core_runner, runner, library, scratch, report, single, multiline)


def main():
    library = HERE/'target/pdfium/lib/libpdfium.so'
    native.require(native.sha256(library)==native.LIBRARY_SHA256,'unexpected native runtime')
    native.command(['python3',HERE/'fixtures/generate_selection.py'])
    paths = [path for directory in [HERE/'src',HERE/'tests',HERE/'fixtures',CORE/'src']
             for path in directory.rglob('*') if path.is_file() and '__pycache__' not in path.parts]
    paths += [Path(__file__), HERE/'Cargo.toml',HERE/'Cargo.lock',ROOT/'config/architecture.toml',
              CORE/'pdf_adapter.py',CORE/'export_sdk.py',CORE/'author/src/main.rs']
    fingerprints = {str(path.relative_to(ROOT)):native.sha256(path) for path in sorted(paths)}
    for manifest in [HERE/'Cargo.toml',CORE/'Cargo.toml']:
        native.command(['cargo','fmt','--manifest-path',manifest,'--all','--','--check'])
    native.command(['cargo','clippy','--all-targets','--locked','--offline','--','-D','warnings'])
    native.command(['cargo','test','--locked','--offline'])
    native.command(['cargo','build','--locked','--offline'])
    capture.command(['cargo','build','--locked','--offline'])
    graph = json.loads(native.command(['cargo','metadata','--locked','--offline','--all-features','--format-version','1']))
    core_graph = capture.metadata(CORE/'Cargo.toml')
    runner = Path(graph['target_directory'])/'debug/qusor-pdf-provider-probe'
    core_runner = Path(core_graph['target_directory'])/'debug/qusor-capture-probe'
    hashes = [native.sha256(path) for path in [runner,core_runner]]
    with tempfile.TemporaryDirectory(prefix='qusor-native-selection-') as temporary:
        result = run(runner,core_runner,library,Path(temporary))
    native.require([native.sha256(path) for path in [runner,core_runner]]==hashes,'authority binaries changed during test')
    native.require(all(native.sha256(ROOT/path)==value for path,value in fingerprints.items()),
                   'source changed during qualification')
    evidence = HERE/'evidence/linux-native-selection.json'
    evidence.write_text(json.dumps({'platform':platform.platform(),'native_sha256':hashes[0],'core_sha256':hashes[1],
        'runtime_sha256':native.LIBRARY_SHA256,'source_sha256':fingerprints,
        'checks':native.CHECKS,'native_scenarios':native.SCENARIOS,'core_checks':capture.CHECKS,
        'core_scenarios':capture.SCENARIOS,'integration':result,
        'unverified':['NSH pointer capture/window','compound atomic capture','cross-page selection','Chinese PDF/fonts','accessibility/touch/keyboard']},ensure_ascii=False,indent=2)+'\n')
    print(f'{len(native.SCENARIOS)} native cases; {len(capture.SCENARIOS)} Core cases; {evidence}')


if __name__=='__main__':
    main()
