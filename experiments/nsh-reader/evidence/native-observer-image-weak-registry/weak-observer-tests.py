from pathlib import Path
import sys,os,subprocess,json,time,hashlib,socket,shutil
r=Path('/workspace/qusor');b=r/'experiments/nsh-reader';p=Path('/home/user/.cache/qusor-native-release-observer');s=p/'nsh-913-image';out=p/'image-weak-observer-tests';out.mkdir(exist_ok=True);sys.path.insert(0,str(b));from native_prefix import runtime_environment
h=lambda f:hashlib.sha256(f.read_bytes()).hexdigest();before=json.loads((p/'image-weak-final-validation/source-before.json').read_text());assert all(h(s/name)==digest for name,digest in before.items());shutil.copyfile(p/'image-weak-final-validation/source.zip',out/'source.zip');shutil.copyfile(p/'image-weak-final-validation/source.patch',out/'source.patch');(out/'source-before.json').write_text(json.dumps(before,indent=2))
env=runtime_environment(b/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(b/'target/nsh/target'),CARGO_NET_OFFLINE='true',TMPDIR=str(p/'tmp'));args=['--manifest-path',str(s/'Cargo.toml'),'-p','nsh-platform-linux','--no-default-features','--features','native-retirement-observer','--locked','--offline','-j1'];cmd=['cargo','test',*args,'--lib','--no-run','--message-format=json'];record={'source_before':before,'compile_command':cmd};t=time.monotonic()
with (out/'compile.jsonl').open('w') as stdout,(out/'compile.log').open('w') as stderr:code=subprocess.call(cmd,cwd=s,env=env,stdout=stdout,stderr=stderr)
record['compile']={'exit_code':code,'seconds':time.monotonic()-t};(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(record['compile']),flush=True)
if code:raise SystemExit(code)
artifacts=[json.loads(line) for line in (out/'compile.jsonl').read_text().splitlines() if line.startswith('{')];binary=Path(next(a['executable'] for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('executable') and a['target']['name']=='nsh_platform_linux'));dest=out/'observer-image-tests';subprocess.run(['cp','--reflink=auto',str(binary),str(dest)],check=True);record['binary']={'path':str(dest),'sha256':h(dest),'bytes':dest.stat().st_size}
run=out/'isolated'
for name in ['home','runtime','config','data','cache']:(run/name).mkdir(parents=True,exist_ok=True,mode=0o700)
env.update(HOME=str(run/'home'),RUSTUP_HOME='/home/user/.rustup',CARGO_HOME='/home/user/.cargo',XDG_RUNTIME_DIR=str(run/'runtime'),XDG_CONFIG_HOME=str(run/'config'),XDG_DATA_HOME=str(run/'data'),XDG_CACHE_HOME=str(run/'cache'),GSK_RENDERER='cairo',GSETTINGS_BACKEND='memory',GIO_USE_VFS='local',GTK_USE_PORTAL='0',GTK_IM_MODULE='simple',DBUS_SESSION_BUS_ADDRESS='unix:path='+str(run/'no-bus'))
for candidate in range(114,200):
 if Path(f'/tmp/.X{candidate}-lock').exists():continue
 probe=socket.socket(socket.AF_UNIX)
 try:probe.connect('\0/tmp/.X11-unix/X'+str(candidate))
 except OSError:display=str(candidate);break
 finally:probe.close()
else:raise RuntimeError('no free isolated display')
xenv=dict(env);xenv.update(LD_LIBRARY_PATH='/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu',PATH='/tmp/qusor-xvfb-prefix/usr/bin:'+env['PATH']);server=subprocess.Popen(['/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local',':'+display,'-screen','0','400x400x24','-nolisten','tcp'],env=xenv,stdout=(out/'xvfb.log').open('w'),stderr=subprocess.STDOUT);time.sleep(1)
if server.poll() is not None:raise RuntimeError('Xvfb startup failed')
env['DISPLAY']=':'+display;record['xvfb']={'pid':server.pid,'start_ticks':int(Path(f'/proc/{server.pid}/stat').read_text().rsplit(')',1)[1].split()[19]),'display':env['DISPLAY']};record['tests']=[]
try:
 names=[line.split(': test')[0] for line in subprocess.check_output([str(dest),'--list'],env=env,text=True).splitlines() if ': test' in line and 'retirement_' in line];assert len(names)==9,names
 for index,name in enumerate(names):
  cmd=[str(dest),'--exact',name,'--nocapture'];test={'name':name,'command':cmd};record['tests'].append(test);t=time.monotonic()
  with (out/f'test-{index+1}.log').open('w') as log:
   child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);test.update(pid=child.pid,start_ticks=int(Path(f'/proc/{child.pid}/stat').read_text().rsplit(')',1)[1].split()[19]));(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps({'started':name,'pid':child.pid}),flush=True)
   try:code=child.wait(timeout=90)
   except subprocess.TimeoutExpired:test['execution_deadline_seconds']=90;child.terminate();code=child.wait(timeout=5)
  test.update(exit_code=code,seconds=time.monotonic()-t);(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(test),flush=True)
  if code:break
finally:
 server.terminate();record['xvfb']['exit_code']=server.wait();record['xvfb']['terminal']=not Path(f'/proc/{server.pid}').exists();(out/'record.json').write_text(json.dumps(record,indent=2))
if code:raise SystemExit(code)
record['source_after']={name:h(s/name) for name in before};record['source_unchanged']=record['source_after']==before;record['status']='nine-independent-scenarios-terminal';(out/'record.json').write_text(json.dumps(record,indent=2));raise SystemExit(code)
