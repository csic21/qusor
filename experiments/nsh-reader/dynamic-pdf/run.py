"""Launch the asset-free dynamic native reader with explicitly local private data."""
from pathlib import Path
import argparse,hashlib,os,platform,plistlib,subprocess,sys
HERE=Path(__file__).resolve().parent;BASE=HERE.parent;ROOT=BASE.parent.parent
sys.path.insert(0,str(BASE));from native_prefix import runtime_environment

def macos_app(executable):
    """Give this locally built native executable an immutable, inspectable app identity."""
    digest=hashlib.sha256(executable.read_bytes()).hexdigest()[:16]
    bundle=BASE/'target/macos-apps'/digest/'QUSOR Preview.app'
    binary=bundle/'Contents/MacOS/qusor-reader'
    binary.parent.mkdir(parents=True,exist_ok=True)
    if not binary.exists():os.link(executable,binary)
    info={'CFBundleIdentifier':'org.qusor.preview','CFBundleExecutable':'qusor-reader',
          'CFBundleName':'QUSOR Preview','CFBundleDisplayName':'QUSOR 阅读预览',
          'CFBundlePackageType':'APPL','CFBundleVersion':'0.1.0',
          'LSMinimumSystemVersion':'14.0','NSHighResolutionCapable':True}
    (bundle/'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    return binary

def launch(data,log=None,picker_dir=None):
    data=Path(data).resolve();data.mkdir(parents=True,exist_ok=True);data.chmod(0o700)
    if sys.platform == 'darwin':
        env=dict(os.environ)
        arch='aarch64' if platform.machine() in ('arm64','aarch64') else 'x86_64'
        library=ROOT/f'experiments/pdf-provider/target/pdfium-macos-{arch}/lib/libpdfium.dylib'
    elif sys.platform.startswith('linux'):
        env=runtime_environment(BASE/'target/native-prefix');env['GDK_BACKEND']='x11'
        env['GSETTINGS_SCHEMA_DIR']=str(BASE/'target/native-prefix/usr/share/glib-2.0/schemas')
        env['XDG_DATA_DIRS']=str(BASE/'target/native-prefix/usr/share')+':'+env.get('XDG_DATA_DIRS','/usr/local/share:/usr/share')
        library=ROOT/'experiments/pdf-provider/target/pdfium/lib/libpdfium.so'
    else:
        raise RuntimeError('reader runtime profile has not yet passed qualification on this platform')
    env.update(QUSOR_READER_RESOURCES=str(BASE/'target/dynamic-pdf-resources'),QUSOR_READER_DATA=str(data),QUSOR_PDF_LIBRARY=str(library))
    if picker_dir:env['QUSOR_DYNAMIC_PICKER_DIR']=str(Path(picker_dir).resolve())
    executable=BASE/'target/nsh/target/debug/qusor-nsh-dynamic-pdf-probe'
    if sys.platform=='darwin':executable=macos_app(executable)
    command=[str(executable),'--bundle',str(BASE/'target/dynamic-pdf-bundle/bundle.nshc'),'--trace','--app-id','org.qusor.nativeprobe','--data-dir',str(data/'ui'),'--window-size','1000x1000']
    if sys.platform=='darwin':command+=['--name','QUSOR 阅读预览']
    return subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--data',type=Path,required=True);p.add_argument('--picker-dir',type=Path);a=p.parse_args();raise SystemExit(launch(a.data,picker_dir=a.picker_dir).wait())
