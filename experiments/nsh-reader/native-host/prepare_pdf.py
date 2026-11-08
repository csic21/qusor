"""Build the native PDF Surface and frozen display resources from the qualified fixture."""
from pathlib import Path
import hashlib,json,os,shutil,struct,subprocess,sys,tempfile,zlib
BASE=Path(__file__).resolve().parent
READER=BASE.parent
ROOT=READER.parent.parent
sys.path.insert(0,str(BASE))
import prepare

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def png(ppm,output,width,height):
    header,size,depth,raw=ppm.split(b'\n',3)
    w,h=size.split()
    if header!=b'P6' or int(w)!=width or int(h)!=height or depth!=b'255' or len(raw)!=width*height*3:
        raise ValueError('unexpected native bitmap')
    def chunk(k,v):return struct.pack('>I',len(v))+k+v+struct.pack('>I',zlib.crc32(k+v)&0xffffffff)
    scan=b''.join(b'\0'+raw[y*width*3:(y+1)*width*3] for y in range(height))
    output.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(scan))+chunk(b'IEND',b''))

def main():
    prep=prepare.prepare()
    env=prepare.runtime_environment(READER/'target/native-prefix')
    env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(prepare.SOURCE/'target'))
    provider=ROOT/'experiments/pdf-provider'
    subprocess.run(['cargo','build','--manifest-path',str(provider/'Cargo.toml'),'--locked','--offline'],check=True)
    output=READER/'target/pdf-surface-resources';output.mkdir(parents=True,exist_ok=True)
    temp=Path(tempfile.mkdtemp(prefix='pdf-render-',dir=READER/'target'))
    command=[str(provider/'target/debug/qusor-pdf-provider-probe'),'inspect',str(provider/'target/pdfium/lib/libpdfium.so'),str(provider/'fixtures/text-two-pages.pdf'),str(temp)]
    report=json.loads(subprocess.check_output(command))['report']
    (output/'report.json').write_text(json.dumps(report))
    (READER/'pdf-surface/frame.ts').write_text('export const reportDigest = '+json.dumps(digest(output/'report.json'))+';\n')
    resources=[]
    for page in report['pages']:
        source=temp/page['rendered_file']
        shutil.copyfile(source,output/page['rendered_file'])
        if digest(source)!=page['rendered_sha256']:raise ValueError('native page digest mismatch')
        target=READER/'pdf-surface/assets'/f"page-{page['page_index']}.png"
        png(source.read_bytes(),target,page['pixel_width'],page['pixel_height'])
        resources.append({'page':page['page_index'],'ppm_sha256':digest(source),'png_sha256':digest(target),'width':page['pixel_width'],'height':page['pixel_height']})
    shutil.rmtree(temp)
    helper=READER/'pdf-surface/.nsh/rust';helper.mkdir(parents=True,exist_ok=True)
    if not (helper/'Cargo.lock').exists():shutil.copyfile(prepare.SOURCE/'Cargo.lock',helper/'Cargo.lock')
    subprocess.run([str(prepare.SOURCE/'target/debug/nsh'),'build',str(READER/'pdf-surface'),'-o',str(READER/'target/pdf-surface-bundle'),'--verbose'],env=env,check=True)
    drifts=prepare.registry_drift(prepare.SOURCE/'Cargo.lock',helper/'Cargo.lock',provider/'Cargo.lock',ROOT/'Cargo.lock',ROOT/'experiments/extension-package/Cargo.lock')
    if drifts:raise ValueError({'registry_lock_drift':drifts})
    evidence={'preparation':prep,'inspect_command':command,'report_sha256':digest(output/'report.json'),'original_sha256':report['original_sha256'],'frame_report_sha256':report['extraction_sha256'],'resources':resources,'registry_lock_drift':drifts}
    (output/'preparation.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps(evidence,indent=2))
if __name__=='__main__':main()
