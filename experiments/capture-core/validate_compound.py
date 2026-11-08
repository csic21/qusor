"""Exercise real multi-line native selection as one ordinary-plugin Core commit."""
from __future__ import annotations
import json
from pathlib import Path
import platform
import tempfile
import tomllib

import validate_pdf as baseline
from pdf_adapter import native_compound_draft
from selection_oracle import binding, viewport, ink_point

capture, native = baseline.capture, baseline.native
HERE, ROOT, PROVIDER = baseline.HERE, baseline.ROOT, baseline.PROVIDER
write, action = baseline.write, baseline.action


def native_multiline(worker, library, original, report, images, page_index, scratch):
    page = report['pages'][page_index]
    first = 'Repeated evidence stays local.'
    last = 'Distinct first-page context.' if page_index == 0 else 'Second-page context is explicit.'
    start = page['page_text'].index(first)
    end = page['page_text'].index(last) + len(last) - 1
    script = {'binding': binding(report, page_index), 'viewport': viewport(), 'operation': {
        'kind': 'drag', 'anchor': ink_point(page, images/page['rendered_file'], start),
        'focus': ink_point(page, images/page['rendered_file'], end)}}
    result = native.worker(worker, f'actual multi-line native selection page {page_index}',
        ['select',library,original,scratch/f'selected-{page_index}',write(scratch/f'script-{page_index}.json',script)])['selection']
    capture.require(result['joined_text'] == first+'\n'+last and len(result['segments']) == 2,
                    'native selection changed full multi-line source')
    if page_index == 1:
        capture.require(result['segments'][0]['start'] > result['segments'][1]['start'],
                        'rotated engine-order difference disappeared')
    return result


def scenarios(runner, author, worker, library, scratch, legacy_core):
    vault = 'pdf-vault'
    core = scratch/'knowledge'
    legacy = action(runner,'old mixed V2 archive before compound','export-all',legacy_core)['archive']
    capture.require(legacy['schema_version'] == 2,'old record serialization no longer exports V2')
    action(runner,'restore untouched old V2 archive to schema3 Core','restore-all',core,write(scratch/'legacy.json',legacy))
    capture.require(action(runner,'old V2 bytes and receipts preserved','export-all',core)['archive'] == legacy,
                    'schema3 changed old record/archive hashes')
    document = action(runner,'read old source for new compound intent','pdf-document',core,1)['document']
    original = PROVIDER/'fixtures/text-two-pages.pdf'
    images = scratch/'native'
    report = native.worker(worker,'native source for compound capture',['inspect',library,original,images])['report']
    manifest = {'manifest_version':1,'package_id':'org.example.compound-reader','publisher_id':'org.example',
                'package_version':{'major':1,'minor':0,'patch':0},'host_api':{'major':0,'min_minor':1,'max_minor':1},
                'required_capabilities':['selection.read','capture.propose'],'optional_capabilities':[],
                'contributions':[{'id':'org.example.compound-reader.capture','title':'Save complete selection',
                                  'kind':'command','required_capabilities':['selection.read','capture.propose']}]}
    identity = manifest['package_id']
    manifest_file = write(scratch/'manifest.json',manifest)
    seed = scratch/'seed.hex'
    seed.write_text('11'*32)
    lifecycle = write(scratch/'lifecycle.json',{'protocol':1,'private_schema':1,'migration_module':None,'accepted_from_schemas':[]})
    package = scratch/'reader.qpk'
    key = capture.command([author,'--pdf-compound-relay',vault,manifest_file,seed,package,lifecycle,'-']).stdout.strip()
    packages = scratch/'packages'
    action(runner,'install ordinary compound reader','install',packages,'install',0,package,key)
    manager = ROOT/'experiments/extension-package/target/debug/qusor-package-probe'
    preferences = scratch/'preferences.bin'
    preferences.write_bytes(b'theme=dark')
    installed = action(manager,'initialize compound reader state','initialize-private',packages,'initialize',1,identity,preferences)['receipt']
    revision = installed['installation']['revision']
    drafts, saved, arguments, hashes = [],[],[],[]
    for page_index in [0,1]:
        selected = native_multiline(worker,library,original,report,images,page_index,scratch)
        draft = native_compound_draft(document,report,selected)
        drafts.append(draft)
        path = write(scratch/f'draft-{page_index}.json',draft)
        args = ['pdf-compound-capture',packages,core,vault,f'compound-{page_index}',identity,1,path,f'Whole selection note {page_index}']
        arguments.append(args)
        before = action(runner,f'before compound preview {page_index}','export-all',core)['archive']
        review = action(runner,f'ordinary guest constructs full compound preview {page_index}',*args)
        capture.require(not review['committed'] and review['preview']['draft']==draft
                        and review['preview']['exact']==draft['exact'] and review['preview']['segment_count']==2,
                        'review omitted a source segment')
        capture.require(action(runner,f'compound preview has no receipt {page_index}','pdf-receipt',core,
                               f'compound-{page_index}')['receipt'] is None,'preview persisted compound knowledge')
        digest = review['preview']['draft_hash']
        hashes.append(digest)
        action(runner,f'wrong whole-selection review hash rejected {page_index}',*args,'--accept','0'*64,ok=False)
        receipt = action(runner,f'one approval saves entire compound {page_index}',*args,'--accept',digest)['receipt']
        saved.append(receipt)
        capture.require(action(runner,f'idempotent full compound replay {page_index}',*args,'--accept',digest)['receipt']==receipt,
                        'compound intent duplicated')
        after = action(runner,f'compound is exactly one transaction {page_index}','export-all',core)['archive']
        capture.require(len(after['commits'])==len(before['commits'])+1 and
                        len(after['pdf_captures'])==len(before['pdf_captures'])+1 and after['schema_version']==3,
                        'compound was split into multiple captures/revisions')
        record = action(runner,f'query complete compound {page_index}','pdf-compound-query',core,receipt['capture_id'])['capture']
        capture.require(record['compound']==draft and record['draft']==draft['segments'][0], 'compound/projection mismatch')
        action(runner,f'old query cannot hide later segments {page_index}','pdf-query',core,receipt['capture_id'],ok=False)
        primary = write(scratch/f'primary-{page_index}.json',draft['segments'][0])
        action(runner,f'old resolver cannot return only first segment {page_index}','pdf-resolve',core,receipt['capture_id'],primary,ok=False)
        resolved = action(runner,f'all compound segments resolve after restart {page_index}','pdf-compound-resolve',core,
                          receipt['capture_id'],path)['selection']
        capture.require(resolved['exact']==draft['exact'] and len(resolved['segments'])==2,'resolution lost a segment')
        changed = json.loads(json.dumps(draft))
        changed['segments'].reverse()
        changed['exact']='\n'.join(segment['exact'] for segment in changed['segments'])
        changed_path = write(scratch/f'reordered-{page_index}.json',changed)
        stale = args.copy()
        stale[7]=changed_path
        action(runner,f'valid reorder cannot reuse old reviewed hash {page_index}',*stale,'--accept',digest,ok=False)
        action(runner,f'changed whole ordering conflicts with same operation {page_index}',*stale,'--accept',
               action(runner,f'preview explicitly reordered content {page_index}',*stale)['preview']['draft_hash'],ok=False)
    # Distinct user intent with identical source is retained as one more logical note.
    distinct = arguments[0].copy()
    distinct[4]='separate-user-intent'
    second = action(runner,'distinct full-selection intent is retained',*distinct,'--accept',hashes[0])['receipt']
    capture.require(second['capture_id']!=saved[0]['capture_id'],'content hash swallowed distinct intent')
    archive = action(runner,'complete mixed legacy and compound archive','export-all',core)['archive']
    capture.require(archive['text_entries']==legacy['text_entries'] and archive['pdf_captures'][:len(legacy['pdf_captures'])]==legacy['pdf_captures'],
                    'new compound data rewrote old records')
    archive_file = write(scratch/'archive-v3.json',archive)
    restored = scratch/'restored'
    action(runner,'restore complete V3 archive','restore-all',restored,archive_file)
    capture.require(action(runner,'restored mixed archive is identical','export-all',restored)['archive']==archive,'V3 recovery changed data')
    recovered = scratch/'recovered.pdf'
    action(runner,'recover original for all compound sources','pdf-original',restored,1,recovered)
    capture.require(recovered.read_bytes()==original.read_bytes(),'restored compound original differs')
    restored_images = scratch/'restored-native'
    rebuilt = native.worker(worker,'render restored compound original',['inspect',library,recovered,restored_images])['report']
    restored_document = action(runner,'restored semantic source for selection bridge','pdf-document',restored,1)['document']
    for index in [0,1]:
        selected = native_multiline(worker,library,recovered,rebuilt,restored_images,index,scratch/'restore-drag')
        capture.require(native_compound_draft(restored_document,rebuilt,selected)==drafts[index], 'restored native geometry resolves another compound')
        action(runner,f'restored complete compound source {index}','pdf-compound-resolve',restored,
               saved[index]['capture_id'],scratch/f'draft-{index}.json')
    for point in ['before_transaction','after_entities','before_commit','after_commit']:
        args = arguments[1].copy()
        args[4]=f'compound-crash-{point}'
        action(runner,f'actual compound transaction crash {point}',*args,'--accept',hashes[1],'--crash',point,crash=True)
        old = action(runner,f'query one compound receipt after {point}','pdf-receipt',core,args[4])['receipt']
        capture.require((old is not None)==(point=='after_commit'),'partial compound transaction became visible')
        retry = action(runner,f'retry entire compound after {point}',*args,'--accept',hashes[1])['receipt']
        capture.require(old is None or retry==old,'lost-response compound receipt changed')
        record = action(runner,f'compound retains every segment after {point}','pdf-compound-query',core,retry['capture_id'])['capture']
        capture.require(record['compound']==drafts[1],'crash retry left partial compound')
    for point in ['after_blob_publish','after_entities','after_commit']:
        destination = scratch/f'restore-crash-{point}'
        action(runner,f'actual V3 restore crash {point}','restore-all',destination,archive_file,'--crash',point,crash=True)
        state = action(runner,f'V3 restore visibility after {point}','export-all',destination)['archive']
        capture.require(len(state['commits'])==(len(archive['commits']) if point=='after_commit' else 0),'partial V3 restore is visible')
        if point!='after_commit':
            action(runner,f'retry V3 restore {point}','restore-all',destination,archive_file)
        capture.require(action(runner,f'all V3 records preserved after {point}','export-all',destination)['archive']==archive,'V3 restart lost records')
    compound_index = next(i for i,record in enumerate(archive['pdf_captures']) if 'compound' in record)
    def remove_second(a):
        a['pdf_captures'][compound_index]['compound']['segments'].pop()
    def reorder(a):
        compound=a['pdf_captures'][compound_index]['compound']
        compound['segments'].reverse()
        compound['exact']='\n'.join(segment['exact'] for segment in compound['segments'])
    def projection(a):
        record=a['pdf_captures'][compound_index]
        record['draft']=record['compound']['segments'][1]
    for name, mutate in [('false-v2',lambda a:a.update(schema_version=2)),('missing-second',remove_second),
                         ('changed-order',reorder),('primary-projection',projection),
                         ('authorization',lambda a:a.update(approved=True))]:
        damaged=json.loads(json.dumps(archive))
        mutate(damaged)
        destination=scratch/f'bad-{name}'
        action(runner,f'reject damaged V3 archive {name}','restore-all',destination,write(scratch/f'bad-{name}.json',damaged),ok=False)
        capture.require(not action(runner,f'no partial data for rejected {name}','export-all',destination)['archive']['commits'],
                        'bad compound archive left partial state')
    stable=action(runner,'knowledge before compound reader lifecycle','export-all',core)['archive']
    manifest2=json.loads(json.dumps(manifest))
    manifest2['package_version']['major']=2
    lifecycle2=write(scratch/'lifecycle2.json',{'protocol':1,'private_schema':2,'migration_module':'migration.wasm','accepted_from_schemas':[1]})
    migration=scratch/'migration.wat'
    migration.write_bytes((ROOT/'experiments/extension-package/fixtures/private-migrate-good.wat').read_bytes())
    package2=scratch/'reader2.qpk'
    capture.command([author,'--pdf-compound-relay',vault,write(scratch/'manifest2.json',manifest2),seed,package2,lifecycle2,migration])
    upgrade=action(manager,'real compound reader upgrade','upgrade-private',packages,'upgrade',revision,package2,key)['receipt']
    capture.require(upgrade['outcome']=='succeeded','compound reader migration failed')
    status=action(manager,'inspect compound reader migration','private-status',packages,identity)['private']
    rolled=action(manager,'real compound reader rollback','rollback-private',packages,'rollback',upgrade['installation']['revision'],
                  identity,status['retained'][0]['pair_id'])['receipt']
    capture.require(rolled['outcome']=='succeeded','compound reader rollback failed')
    capture.require(action(runner,'compound knowledge survives migration and rollback','export-all',core)['archive']==stable,
                    'reader lifecycle changed compound knowledge')
    action(manager,'uninstall compound reader','uninstall',packages,'remove',rolled['installation']['revision'],identity)
    action(runner,'removed reader cannot process compound',*arguments[0],ok=False)
    capture.require(action(runner,'all compound knowledge survives uninstall','export-all',core)['archive']==stable,'uninstall deleted segments')
    action(runner,'compound source opens after reader removal','pdf-compound-resolve',core,saved[1]['capture_id'],scratch/'draft-1.json')
    return {'legacy_commits':len(legacy['commits']),'first_compound_capture_ids':[r['capture_id'] for r in saved],
            'normal_exact':drafts[0]['exact'],'rotated_exact':drafts[1]['exact']}


def main():
    library=PROVIDER/'target/pdfium/lib/libpdfium.so'
    capture.require(capture.digest(library)==native.LIBRARY_SHA256,'unexpected native library')
    fingerprints=capture.source_fingerprints()
    graph=capture.metadata(HERE/'Cargo.toml')
    author_graph=capture.metadata(HERE/'author/Cargo.toml')
    policy=tomllib.loads((ROOT/'config/architecture.toml').read_text())
    for metadata,name in [(graph,'capture-core'),(author_graph,'capture-core-author')]:
        capture.require(not capture.validate_graph(metadata,policy['experiments'][name],ROOT),'dependency policy failed')
    for args in [['cargo','fmt','--all','--','--check'],
                 ['cargo','fmt','--manifest-path',HERE/'author/Cargo.toml','--all','--','--check'],
                 ['cargo','clippy','--all-targets','--locked','--offline','--','-D','warnings'],
                 ['cargo','test','--locked','--offline'],['cargo','build','--locked','--offline']]:
        capture.command(args)
    capture.command(['cargo','build','--manifest-path',PROVIDER/'Cargo.toml','--locked','--offline'])
    native_graph=capture.metadata(PROVIDER/'Cargo.toml')
    runner=Path(graph['target_directory'])/'debug/qusor-capture-probe'
    worker=Path(native_graph['target_directory'])/'debug/qusor-pdf-provider-probe'
    hashes=[capture.digest(path) for path in [runner,worker]]
    with tempfile.TemporaryDirectory(prefix='qusor-compound-') as temporary:
        scratch=Path(temporary)
        author,sdk,sdk_graph=capture.independent_author(scratch/'sdk')
        for name in ['legacy','pdf','compound']:
            (scratch/name).mkdir()
        capture.run_scenarios(runner,author,scratch/'legacy')
        baseline.scenarios(runner,author,worker,library,scratch/'pdf')
        (scratch/'compound/restore-drag').mkdir()
        result=scenarios(runner,author,worker,library,scratch/'compound',scratch/'pdf/knowledge')
    capture.require([capture.digest(path) for path in [runner,worker]]==hashes,'authority binary changed during validation')
    capture.require(capture.source_fingerprints()==fingerprints,'source changed during qualification')
    evidence=HERE/'evidence/linux-compound-pdf.json'
    evidence.write_text(json.dumps({'platform':platform.platform(),'host_sha256':hashes[0],'native_sha256':hashes[1],
        'runtime_sha256':native.LIBRARY_SHA256,'source_sha256':fingerprints,'sdk_sources':sdk,'sdk_packages':sdk_graph,
        'checks':capture.CHECKS,'scenarios':capture.SCENARIOS,'native_scenarios':native.SCENARIOS,'compound':result,
        'unverified':['NSH UI','cross-page pointer gestures','all PDF layout/Unicode','production integration']},
        ensure_ascii=False,indent=2)+'\n')
    print(f'{len(capture.CHECKS)} commands; {len(capture.SCENARIOS)} CLI scenarios; {len(native.SCENARIOS)} native cases; {evidence}')


if __name__=='__main__':
    main()
