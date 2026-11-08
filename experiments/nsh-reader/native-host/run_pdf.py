"""Launch the offline native PDF Surface in an isolated data directory."""
from pathlib import Path
import subprocess,sys
BASE=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(BASE))
from native_prefix import runtime_environment

def launch(log=None):
    env=runtime_environment(BASE/'target/native-prefix')
    env.update(GDK_BACKEND='x11',QUSOR_PDF_REPORT=str(BASE/'target/pdf-surface-resources/report.json'))
    env['XDG_DATA_DIRS']=str(BASE/'target/native-prefix/usr/share')+':'+env.get('XDG_DATA_DIRS','/usr/local/share:/usr/share')
    command=[str(BASE/'target/nsh/target/debug/qusor-nsh-pdf-surface-probe'),'--bundle',str(BASE/'target/pdf-surface-bundle/bundle.nshc'),'--trace','--app-id','org.qusor.nativeprobe','--data-dir',str(BASE/'target/pdf-surface-data'),'--window-size','1000x1000']
    return subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
if __name__=='__main__':raise SystemExit(launch().wait())
