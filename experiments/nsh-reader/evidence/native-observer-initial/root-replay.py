from pathlib import Path
import os,sys,subprocess,json,time,hashlib,socket
root=Path('/workspace/qusor'); evidence=root/'experiments/nsh-reader/evidence/native-observer-initial'; record=json.loads((evidence/'record.json').read_text())
for name,meta in record['artifacts'].items():
 path=evidence/name
 assert hashlib.file_digest(path.open('rb'),'sha256').hexdigest()==meta['sha256'],name
binary=Path(record['test_executable']);assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==record['test_executable_sha256']
server_path=Path(record['xvfb_binary']);assert hashlib.file_digest(server_path.open('rb'),'sha256').hexdigest()==record['xvfb_sha256']
work=Path('/home/user/.cache/qusor-native-release-observer/root-replay');work.mkdir(exist_ok=True)
env=os.environ.copy();env.update(record['runtime_env'])
for key,name in [('HOME','home'),('XDG_RUNTIME_DIR','runtime'),('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache')]:
 path=work/name;path.mkdir(exist_ok=True,mode=0o700);env[key]=str(path)
for number in range(116,200):
 if Path(f'/tmp/.X{number}-lock').exists():continue
 probe=socket.socket(socket.AF_UNIX);probe.settimeout(.1)
 try:probe.connect('\0/tmp/.X11-unix/X'+str(number))
 except OSError:display=':'+str(number);break
 finally:probe.close()
else:raise RuntimeError('no free display')
xenv=env.copy();xenv['LD_LIBRARY_PATH']='/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu';xenv['PATH']='/tmp/qusor-xvfb-prefix/usr/bin:'+xenv['PATH']
def identity(pid):return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])
result={'scope':'Independent frozen GTK4 partial observer tests; no Host ACK/decoder/native child retirement proof.','executable_sha256':record['test_executable_sha256'],'artifacts_checked':len(record['artifacts'])}
with (evidence/'root-xvfb.log').open('w') as out:
 server=subprocess.Popen([str(server_path),display,'-screen','0','400x400x24','-nolisten','tcp'],env=xenv,stdout=out,stderr=subprocess.STDOUT)
 result['xvfb']={'pid':server.pid,'display':display,'start_ticks':identity(server.pid)}
 try:
  time.sleep(.5)
  assert server.poll() is None,'owned Xvfb startup failed'
  env['DISPLAY']=display
  with (evidence/'root-test.log').open('w') as log:
   test=subprocess.Popen([str(binary),*record['replay_test_arguments']],env=env,stdout=log,stderr=subprocess.STDOUT)
   result['test']={'pid':test.pid,'start_ticks':identity(test.pid)}
   code=test.wait();result['test']['exit_code']=code
  assert code==0 and '3 passed' in (evidence/'root-test.log').read_text()
 finally:
  if server.poll() is None:
   assert identity(server.pid)==result['xvfb']['start_ticks'];server.terminate()
  result['xvfb']['exit_code']=server.wait(timeout=10);result['xvfb']['terminal']=not Path(f'/proc/{server.pid}').exists()
  result['test_log_sha256']=hashlib.sha256((evidence/'root-test.log').read_bytes()).hexdigest() if (evidence/'root-test.log').exists() else None
  (evidence/'root-independent.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result));print((evidence/'root-test.log').read_text()[-2000:])
