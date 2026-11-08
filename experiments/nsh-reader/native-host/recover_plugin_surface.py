"""Verify the native Surface captures survive a complete Core archive restore."""
from pathlib import Path
import argparse,hashlib,json,subprocess
BASE=Path(__file__).resolve().parent.parent;BINARY=BASE.parent/'capture-core/target/debug/qusor-capture-probe'
def run(*args):
    result=json.loads(subprocess.check_output([str(BINARY),*map(str,args)],text=True))
    assert result['ok'];return result

def main(interaction,restored,evidence):
    if restored.exists():raise ValueError('restore must start with an empty destination')
    work=restored.parent/(restored.name+'-artifacts');work.mkdir()
    original=json.loads(interaction.read_text());store=Path(original['data'])/'knowledge'
    archive=run('export-all',store)['archive'];archive_file=work/'archive.json';archive_file.write_text(json.dumps(archive))
    restore=run('restore-all',restored,archive_file)
    records=[];resolutions=[]
    for index,expected in enumerate(original['core_queries_after_process_exit'],1):
        query=run('pdf-compound-query',restored,index);assert query==expected;records.append(query)
        capture=query['capture'];draft=capture.get('compound',capture.get('draft'));draft_file=work/f'draft-{index}.json';draft_file.write_text(json.dumps(draft))
        resolutions.append(run('pdf-compound-resolve' if 'compound' in capture else 'pdf-resolve',restored,capture['receipt']['capture_id'],draft_file))
    for resolution,record in zip(resolutions,records):
        saved=record['capture'];expected=saved.get('compound',saved.get('draft'));selected=resolution['selection']
        assert selected.get('draft',selected.get('anchor'))==expected
    pdf=work/'original.pdf';run('pdf-original',restored,1,pdf)
    sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
    fixture=BASE.parent/'pdf-provider/fixtures/text-two-pages.pdf';assert sha(pdf)==sha(fixture)
    result={'restore':restore,'captures_equal_before_after_restore':True,'original_sha256':sha(pdf),'archive_sha256':sha(archive_file),'records':records,'exact_resolutions':resolutions}
    evidence.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--interaction',type=Path,required=True);p.add_argument('--restored',type=Path,required=True);p.add_argument('--evidence',type=Path,required=True);a=p.parse_args();main(a.interaction,a.restored,a.evidence)
