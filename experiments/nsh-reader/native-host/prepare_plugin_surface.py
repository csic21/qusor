"""Prepare the ordinary-package, Core and native Surface qualification from verified local inputs."""
from pathlib import Path
import json,shutil,subprocess,sys
BASE=Path(__file__).resolve().parent;READER=BASE.parent;ROOT=READER.parent.parent
sys.path.insert(0,str(BASE));import prepare,prepare_pdf,prepare_reader_package
sys.path.insert(0,str(ROOT/'experiments/capture-core'));from pdf_adapter import neutral_snapshot

def main():
    prepare_pdf.main();prepare_reader_package.prepare()
    resources=READER/'target/plugin-surface-resources'
    report=json.loads((READER/'target/pdf-surface-resources/report.json').read_text())
    shutil.copyfile(READER/'target/pdf-surface-resources/report.json',resources/'report.json')
    (resources/'snapshot.json').write_text(json.dumps(neutral_snapshot(report)))
    shutil.copyfile(ROOT/'experiments/pdf-provider/fixtures/text-two-pages.pdf',resources/'original.pdf')
    shutil.copyfile(READER/'pdf-surface/frame.ts',READER/'plugin-surface/frame.ts')
    for page in [0,1]:shutil.copyfile(READER/f'pdf-surface/assets/page-{page}.png',READER/f'plugin-surface/assets/page-{page}.png')
    helper=READER/'plugin-surface/.nsh/rust';helper.mkdir(parents=True,exist_ok=True)
    if not (helper/'Cargo.lock').exists():shutil.copyfile(READER/'pdf-surface/.nsh/rust/Cargo.lock',helper/'Cargo.lock')
    env=prepare.runtime_environment(READER/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(prepare.SOURCE/'target'))
    subprocess.run([str(prepare.SOURCE/'target/debug/nsh'),'build',str(READER/'plugin-surface'),'-o',str(READER/'target/plugin-surface-bundle'),'--verbose'],env=env,check=True)
    # The probe adds only already-qualified Core/package/runtime dependencies.
    if prepare.registry_drift(prepare.SOURCE/'Cargo.lock',BASE/'Cargo.lock',ROOT/'experiments/pdf-provider/Cargo.lock',ROOT/'Cargo.lock',ROOT/'experiments/extension-package/Cargo.lock'):raise ValueError('unqualified host dependency lock drift')
    if prepare.registry_drift(prepare.SOURCE/'Cargo.lock',helper/'Cargo.lock',ROOT/'experiments/pdf-provider/Cargo.lock',ROOT/'Cargo.lock',ROOT/'experiments/extension-package/Cargo.lock'):raise ValueError('unqualified helper dependency lock drift')
if __name__=='__main__':main()
