import pathlib,os,sys,json,subprocess,time,shutil,hashlib
r=pathlib.Path('/workspace/qusor');b=r/'experiments/nsh-reader';e=b/'evidence/native-editor-retirement-gui-default';run=pathlib.Path('/home/user/.cache/qusor-native-retirement/run');run.mkdir(exist_ok=True)
for name in ['home','config','data','cache','runtime']: (run/name).mkdir(exist_ok=True,mode=0o700)
os.environ.update(HOME=str(run/'home'),XDG_CONFIG_HOME=str(run/'config'),XDG_DATA_HOME=str(run/'data'),XDG_CACHE_HOME=str(run/'cache'),XDG_RUNTIME_DIR=str(run/'runtime'),DISPLAY=':112',GTK_USE_PORTAL='0',GIO_USE_VFS='local',GSETTINGS_BACKEND='memory',GTK_IM_MODULE='simple')
xenv=dict(os.environ);xenv['LD_LIBRARY_PATH']='/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu'
x=subprocess.Popen(['/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local',':112','-screen','0','1100x1100x24','-nolisten','tcp'],env=xenv,stdout=(e/'xvfb.log').open('w'),stderr=subprocess.STDOUT)
time.sleep(1)
sys.path[:0]=[str(b/'dynamic-pdf'),str(b/'native-host')];import generate_fixture;import run as launch
f=e/'fixtures';f.mkdir(exist_ok=True);pdf,texts=generate_fixture.make(3,'Retirement fresh source');(f/'input.pdf').write_bytes(pdf);(f/'manifest.json').write_text(json.dumps({'after_build':True,'lines':texts,'sha256':hashlib.sha256(pdf).hexdigest()},indent=2))
shutil.copyfile(b/'target/dynamic-pdf-bundle/bundle.nshc',e/'bundle.nshc')
p=launch.launch(e/'data',log=(e/'window.log').open('w'),picker_dir=f)
def identity(p):return {'pid':p.pid,'start_ticks':int(pathlib.Path(f'/proc/{p.pid}/stat').read_text().rsplit(')',1)[1].split()[19])}
record={'app':identity(p),'xvfb':identity(x),'display':':112','bus':os.environ['DBUS_SESSION_BUS_ADDRESS'],'environment':dict(os.environ),'status':'running'};(e/'live.json').write_text(json.dumps(record,indent=2));print(json.dumps({k:v for k,v in record.items() if k!='environment'}),flush=True)
code=p.wait();record['app_exit']=code;record['status']='terminal';x.terminate();record['xvfb_exit']=x.wait();(e/'live.json').write_text(json.dumps(record,indent=2));print('app exit',code,flush=True)
