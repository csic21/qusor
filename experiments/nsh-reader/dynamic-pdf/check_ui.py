"""Type-check trusted TSX against the pinned NSH declarations without building Rust."""
from pathlib import Path
import json
import shutil
import subprocess

HERE=Path(__file__).resolve().parent
READER=HERE.parent

def main():
    target=READER/'target/dynamic-ui-typecheck';target.mkdir(parents=True,exist_ok=True)
    for pattern in ['*.tsx','*.ts','theme.json']:
        for source in HERE.glob(pattern):shutil.copyfile(source,target/source.name)
    # Reuse actual prior Rust-generated ABI declarations; no made-up module mock.
    abi=HERE/'.nsh/types/qusor/pdf-surface.d.ts'
    if not abi.exists():raise RuntimeError('actual pinned native module declarations are required')
    shutil.copyfile(abi,target/'native-pdf.d.ts')
    config=json.loads((HERE/'nsh.json').read_text());config.pop('rust',None)
    (target/'nsh.json').write_text(json.dumps(config))
    (target/'tsconfig.json').write_text(json.dumps({'extends':'./.nsh/tsconfig.json','compilerOptions':{'paths':{'nsh':['./.nsh/types/nsh/index.d.ts'],'nsh/*':['./.nsh/types/nsh/*.d.ts'],'qusor/pdf-surface':['./native-pdf.d.ts'],'*':['./.nsh/types/*.d.ts']}}},indent=2)+'\n')
    command=[str(READER/'target/nsh/target/debug/nsh'),'check',str(target/'App.tsx')]
    result=subprocess.run(command,text=True,capture_output=True)
    (target/'check.log').write_text(result.stdout+result.stderr)
    print(result.stdout+result.stderr,end='')
    if result.returncode:raise SystemExit(result.returncode)
    print('TSX checked with pinned NSH types and actual cached native ABI; no Rust build or native window run')
if __name__=='__main__':main()
