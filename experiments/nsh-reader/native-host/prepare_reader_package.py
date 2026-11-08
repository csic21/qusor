"""Build one ordinary signed reader package solely from an exported SDK outside QUSOR."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,tempfile,tomllib
BASE=Path(__file__).resolve().parent
READER=BASE.parent
ROOT=READER.parent.parent
sys.path.insert(0,str(ROOT/'experiments/capture-core'))
from export_sdk import export_sdk,manifest
SEED='33'*32  # Public synthetic signing fixture; not a user credential.
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def prepare():
    result_dir=READER/'target/plugin-surface-resources';result_dir.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='qusor-reader-sdk-') as temporary:
        outside=Path(temporary);sdk=outside/'sdk';sources=export_sdk(sdk)
        src=sdk/'author/src'
        for name in ['main.rs','reader.wat']:shutil.copyfile(READER/'surface-author'/name,src/name)
        cargo=tomllib.loads((sdk/'author/Cargo.toml').read_text());cargo['package']['name']='build-surface-plugin';cargo['package']['default-run']='build-surface-plugin'
        (sdk/'author/Cargo.toml').write_text(manifest(cargo['package'],cargo['dependencies']))
        policy={'manifest_version':1,'package_id':'org.example.surface-reader','publisher_id':'org.example.author','package_version':{'major':1,'minor':0,'patch':0},'host_api':{'major':0,'min_minor':2,'max_minor':2},'required_capabilities':['reader.surface'],'optional_capabilities':['selection.read','capture.propose'],'contributions':[{'id':'org.example.surface-reader.workspace','title':'Read a local PDF','kind':'reader_workspace','required_capabilities':['reader.surface']}]}
        (outside/'manifest.json').write_text(json.dumps(policy))
        env=dict(os.environ,CARGO_TARGET_DIR=str(outside/'target'))
        command=['cargo','generate-lockfile','--manifest-path',str(sdk/'author/Cargo.toml'),'--offline'];subprocess.run(command,env=env,check=True)
        graph=json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',str(sdk/'author/Cargo.toml'),'--locked','--offline','--all-features','--format-version','1'],env=env))
        names={p['name'] for p in graph['packages']};forbidden={'extension-host','runtime-wasm','extension-runtime-api','qusor-package-probe','qusor-capture-probe','wasmi','rusqlite','fs2','rustix','learning-application','learning-ports','storage-sqlite','blob-store'}
        if names&forbidden or any(n.startswith('nsh-') for n in names):raise ValueError('SDK leaked private infrastructure')
        for package in graph['packages']:
            if package.get('source') is None and not Path(package['manifest_path']).resolve().is_relative_to(outside.resolve()):raise ValueError('SDK resolved repository source')
        for args in [['build'],['clippy','--all-targets']]:subprocess.run(['cargo',*args,'--manifest-path',str(sdk/'author/Cargo.toml'),'--locked','--offline',*(['--','-D','warnings'] if args[0]=='clippy' else [])],env=env,check=True)
        builder=outside/'target/debug/build-surface-plugin';target=result_dir/'reader.package'
        completed=subprocess.run([str(builder),str(outside/'manifest.json'),SEED,str(target)],text=True,capture_output=True,check=True)
        info=json.loads(completed.stdout);(result_dir/'publisher-key.txt').write_text(info['publisher_key']+'\n')
        (result_dir/'manifest.json').write_text(json.dumps(policy,indent=2)+'\n')
        proof={'sdk_sources':sources,'packages':sorted(names),'author_source_sha256':{name:digest(src/name) for name in ['main.rs','reader.wat']},'package_sha256':digest(target),'failure_packages':{variant:digest(target.with_suffix('.'+variant+'.package')) for variant in ['trap','timeout','wrong','repeat']},'publisher_key':info['publisher_key'],'local_sources_all_inside_sdk':True,'package_file':str(target)}
        (result_dir/'author-proof.json').write_text(json.dumps(proof,indent=2)+'\n');print(json.dumps(proof,indent=2));return proof
if __name__=='__main__':prepare()
