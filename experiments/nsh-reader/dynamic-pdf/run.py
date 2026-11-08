"""Launch the asset-free dynamic native reader with explicitly local private data."""
from pathlib import Path
import argparse,os,subprocess,sys
HERE=Path(__file__).resolve().parent;BASE=HERE.parent;ROOT=BASE.parent.parent
sys.path.insert(0,str(BASE));from native_prefix import runtime_environment

def launch(data,log=None,picker_dir=None):
    data=Path(data).resolve();data.mkdir(parents=True,exist_ok=True);data.chmod(0o700)
    env=runtime_environment(BASE/'target/native-prefix');env.update(GDK_BACKEND='x11',QUSOR_READER_RESOURCES=str(BASE/'target/dynamic-pdf-resources'),QUSOR_READER_DATA=str(data),QUSOR_PDF_LIBRARY=str(ROOT/'experiments/pdf-provider/target/pdfium/lib/libpdfium.so'))
    env['GSETTINGS_SCHEMA_DIR']=str(BASE/'target/native-prefix/usr/share/glib-2.0/schemas')
    env['XDG_DATA_DIRS']=str(BASE/'target/native-prefix/usr/share')+':'+env.get('XDG_DATA_DIRS','/usr/local/share:/usr/share')
    if picker_dir:env['QUSOR_DYNAMIC_PICKER_DIR']=str(Path(picker_dir).resolve())
    command=[str(BASE/'target/nsh/target/debug/qusor-nsh-dynamic-pdf-probe'),'--bundle',str(BASE/'target/dynamic-pdf-bundle/bundle.nshc'),'--trace','--app-id','org.qusor.nativeprobe','--data-dir',str(data/'ui'),'--window-size','1000x1000']
    return subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--data',type=Path,required=True);p.add_argument('--picker-dir',type=Path);a=p.parse_args();raise SystemExit(launch(a.data,picker_dir=a.picker_dir).wait())
