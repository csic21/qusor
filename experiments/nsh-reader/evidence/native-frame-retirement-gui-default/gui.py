import pathlib,os,sys,json,subprocess,time,hashlib
root=pathlib.Path('/workspace/qusor');b=root/'experiments/nsh-reader';e=b/'evidence/native-frame-retirement-gui-default'
assert os.environ['DISPLAY']==':116' and os.environ.get('DBUS_SESSION_BUS_ADDRESS')
xenv=dict(os.environ);xenv['LD_LIBRARY_PATH']='/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu'
x=subprocess.Popen(['/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local',':116','-screen','0','1100x1100x24','-nolisten','tcp'],env=xenv,stdout=(e/'xvfb.log').open('w'),stderr=subprocess.STDOUT)
time.sleep(1)
sys.path[:0]=[str(b/'dynamic-pdf'),str(b/'native-host')];import generate_fixture;import run as launch
f=e/'fixtures';f.mkdir(exist_ok=True);pdf,texts=generate_fixture.make(3,'Frame retirement fresh source');(f/'input.pdf').write_bytes(pdf);(f/'manifest.json').write_text(json.dumps({'after_build':True,'lines':texts,'sha256':hashlib.sha256(pdf).hexdigest()},indent=2)+'\n')
p=launch.launch(e/'data',log=(e/'window.log').open('w'),picker_dir=f)
def identity(pid):return {'pid':pid,'start_ticks':int(pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])}
record={'app':identity(p.pid),'xvfb':identity(x.pid),'harness':identity(os.getpid()),'display':':116','bus':os.environ['DBUS_SESSION_BUS_ADDRESS'],'environment':{k:os.environ[k] for k in ['DISPLAY','HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME','GTK_USE_PORTAL','GTK_IM_MODULE']},'status':'running'};(e/'live.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)
code=p.wait();record['app_exit']=code;record['status']='terminal';x.terminate();record['xvfb_exit']=x.wait();(e/'live.json').write_text(json.dumps(record,indent=2)+'\n');print('APP_EXIT',code,flush=True)
