"""Run the ordinary reader package in a native offline host with private local data."""
from pathlib import Path
import argparse,os,subprocess,sys
BASE=Path(__file__).resolve().parent.parent;sys.path.insert(0,str(BASE))
from native_prefix import runtime_environment

def launch(data=None,log=None):
    data=Path(data) if data else BASE/'target/plugin-surface-data'
    data.mkdir(parents=True,exist_ok=True);data.chmod(0o700)
    env=runtime_environment(BASE/'target/native-prefix');env.update(GDK_BACKEND='x11',QUSOR_READER_RESOURCES=str(BASE/'target/plugin-surface-resources'),QUSOR_READER_DATA=str(data.resolve()))
    env['XDG_DATA_DIRS']=str(BASE/'target/native-prefix/usr/share')+':'+env.get('XDG_DATA_DIRS','/usr/local/share:/usr/share')
    command=[str(BASE/'target/nsh/target/debug/qusor-nsh-plugin-surface-probe'),'--bundle',str(BASE/'target/plugin-surface-bundle/bundle.nshc'),'--trace','--app-id','org.qusor.nativeprobe','--data-dir',str(data/'ui'),'--window-size','1000x1000']
    return subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--data',type=Path);args=p.parse_args();raise SystemExit(launch(args.data).wait())
