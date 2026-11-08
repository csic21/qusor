"""Build one asset-free native bundle; PDFs are selected only after this finishes."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys
HERE=Path(__file__).resolve().parent;BASE=HERE.parent;ROOT=BASE.parent.parent
sys.path.insert(0,str(BASE/'native-host'));import prepare as native_prepare,prepare_reader_package

def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def prepare_editor_package(resources):
    # The UI uses an ordinary independently built, signed SDK package. Its test
    # publisher key is public; no signing seed or private API enters the bundle.
    source=ROOT/'experiments/capture-core/evidence/component-author-a'
    record=json.loads((source/'record.json').read_text())
    package=source/'provider-a.qpk'
    if digest(package)!=record['artifacts']['provider-a.qpk']['sha256']:
        raise ValueError('ordinary editor A differs from its qualified author artifact')
    public_key=(source/'publisher-public-key.txt').read_text().strip()
    if len(public_key)!=64 or any(c not in '0123456789abcdef' for c in public_key):
        raise ValueError('invalid ordinary editor public key')
    shutil.copyfile(package,resources/'editor-a.package')
    (resources/'editor-a-publickey.txt').write_text(public_key+'\n')
    proof={'package_sha256':digest(package),'publisher_public_key':public_key,
           'author_record_sha256':digest(source/'record.json'),
           'authority':'ordinary SDK; exact Draft approval still required at use'}
    (resources/'editor-a-proof.json').write_text(json.dumps(proof,indent=2)+'\n')
    return proof

def main():
    native_prepare.prepare(native_binary='qusor-nsh-dynamic-pdf-probe')
    schemas=BASE/'target/native-prefix/usr/share/glib-2.0/schemas'
    subprocess.run(['glib-compile-schemas','--strict',str(schemas)],check=True)
    proof=prepare_reader_package.prepare();source=BASE/'target/plugin-surface-resources';resources=BASE/'target/dynamic-pdf-resources';resources.mkdir(parents=True,exist_ok=True)
    for name in ['reader.package','publisher-key.txt','manifest.json','author-proof.json']:shutil.copyfile(source/name,resources/name)
    editor_proof=prepare_editor_package(resources)
    if any(p.suffix in {'.pdf','.png','.ppm'} or p.name in {'report.json','snapshot.json'} for p in resources.iterdir()):raise ValueError('dynamic resources must not contain a document or frame fixture')
    helper=HERE/'.nsh/rust';helper.mkdir(parents=True,exist_ok=True)
    if not (helper/'Cargo.lock').exists():shutil.copyfile(BASE/'plugin-surface/.nsh/rust/Cargo.lock',helper/'Cargo.lock')
    env=native_prepare.runtime_environment(BASE/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(native_prepare.SOURCE/'target'))
    subprocess.run([str(native_prepare.SOURCE/'target/debug/nsh'),'build',str(HERE),'-o',str(BASE/'target/dynamic-pdf-bundle'),'--verbose'],env=env,check=True)
    drift=native_prepare.registry_drift(native_prepare.SOURCE/'Cargo.lock',helper/'Cargo.lock',ROOT/'experiments/pdf-provider/Cargo.lock',ROOT/'Cargo.lock',ROOT/'experiments/extension-package/Cargo.lock')
    if drift:raise ValueError({'unqualified_helper_registry_drift':drift})
    artifacts=[native_prepare.SOURCE/'target/debug/qusor-nsh-dynamic-pdf-probe',BASE/'target/dynamic-pdf-bundle/bundle.nshc',resources/'reader.package']
    record={'artifacts':{str(p.relative_to(ROOT)):digest(p) for p in artifacts},'author_proof':proof,'editor_author_proof':editor_proof,'helper_registry_drift':drift,'document_resources':[],'local_gtk_schema_sha256':digest(schemas/'gschemas.compiled'),'qualification_requires_post_build_fixtures':True}
    (BASE/'target/dynamic-pdf-preparation.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record,indent=2));return record
if __name__=='__main__':main()
