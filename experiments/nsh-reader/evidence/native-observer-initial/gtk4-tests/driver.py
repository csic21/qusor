from pathlib import Path
import sys,os,subprocess,json,time,hashlib
r=Path('/workspace/qusor');b=r/'experiments/nsh-reader';p=Path('/home/user/.cache/qusor-native-release-observer');s=p/'nsh-913';sys.path.insert(0,str(b));from native_prefix import runtime_environment
out=p/'gtk-attempt-2';out.mkdir(exist_ok=True);env=runtime_environment(b/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(b/'target/nsh/target'),CARGO_NET_OFFLINE='true',TMPDIR=str(p/'tmp'))
common=['--manifest-path',str(s/'Cargo.toml'),'-p','nsh-platform-linux','--no-default-features','--features','native-retirement-observer','--locked','--offline','-j1'];cmd=['cargo','check',*common]
with (out/'check.log').open('w') as log:code=subprocess.call(cmd,cwd=s,env=env,stdout=log,stderr=subprocess.STDOUT)
record={'check_exit':code,'check_command':cmd,'source_sha256':hashlib.sha256((s/'crates/nsh-platform-linux/src/retirement.rs').read_bytes()).hexdigest()};(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(record),flush=True)
if code:raise SystemExit(code)
run=out/'isolated'
for n in ['home','runtime','config','data','cache']:(run/n).mkdir(parents=True,exist_ok=True,mode=0o700)
env.update(HOME=str(run/'home'),RUSTUP_HOME='/home/user/.rustup',CARGO_HOME='/home/user/.cargo',XDG_RUNTIME_DIR=str(run/'runtime'),XDG_CONFIG_HOME=str(run/'config'),XDG_DATA_HOME=str(run/'data'),XDG_CACHE_HOME=str(run/'cache'),GSETTINGS_BACKEND='memory',GIO_USE_VFS='local',GTK_USE_PORTAL='0',GSK_RENDERER='cairo')
import socket
for candidate in range(114,200):
 if Path(f'/tmp/.X{candidate}-lock').exists():continue
 probe=socket.socket(socket.AF_UNIX)
 try:probe.connect('\0/tmp/.X11-unix/X'+str(candidate))
 except OSError:display=str(candidate);break
 finally:probe.close()
else:raise RuntimeError('no free isolated display')
xenv=dict(env);xenv['LD_LIBRARY_PATH']='/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu';xenv['PATH']='/tmp/qusor-xvfb-prefix/usr/bin:'+xenv['PATH']
x=subprocess.Popen(['/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local',':'+display,'-screen','0','400x400x24','-nolisten','tcp'],env=xenv,stdout=(out/'xvfb.log').open('w'),stderr=subprocess.STDOUT)
time.sleep(1)
if x.poll() is not None:raise RuntimeError('isolated Xvfb startup failed')
env['DISPLAY']=':'+display;record['xvfb']={'pid':x.pid,'start_ticks':int(Path(f'/proc/{x.pid}/stat').read_text().rsplit(')',1)[1].split()[19]),'display':env['DISPLAY']};cmd=['cargo','test',*common,'--lib','retirement_','--','--test-threads=1','--nocapture'];record['test']={'command':cmd,'status':'running'};(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(record['xvfb']),flush=True);t=time.monotonic()
try:
 with (out/'test.log').open('w') as log:
  child=subprocess.Popen(cmd,cwd=s,env=env,stdout=log,stderr=subprocess.STDOUT);record['test']['cargo_pid']=child.pid;(out/'record.json').write_text(json.dumps(record,indent=2));code=child.wait()
 record['test'].update(exit_code=code,seconds=time.monotonic()-t,status='terminal')
finally:
 x.terminate();record['xvfb']['exit_code']=x.wait();record['xvfb']['terminal']=not Path(f'/proc/{x.pid}').exists();(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(record),flush=True)
raise SystemExit(code)
