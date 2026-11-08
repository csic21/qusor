from pathlib import Path
import sys,os,subprocess,time,json,hashlib
r=Path('/workspace/qusor');b=r/'experiments/nsh-reader';p=Path('/home/user/.cache/qusor-native-release-observer');s=p/'nsh-913-image';out=p/'image-weak-final-validation';sys.path.insert(0,str(b));from native_prefix import runtime_environment
h=lambda f:hashlib.sha256(f.read_bytes()).hexdigest();before=json.loads((out/'source-before.json').read_text());assert all(h(s/name)==v for name,v in before.items());env=runtime_environment(b/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(b/'target/nsh/target'),CARGO_NET_OFFLINE='true',TMPDIR=str(p/'tmp'));record={'status':'running','steps':[],'source_before':before};commands=[('core-image',['cargo','test','--manifest-path',str(s/'Cargo.toml'),'-p','nsh-core','--no-default-features','--features','svg,loader','--locked','--offline','-j1','--lib','image::','--','--test-threads=1','--nocapture'])]
for feature in ['on','off']:
 cmd=['cargo','check','--manifest-path',str(s/'Cargo.toml'),'-p','nsh-platform-linux','--no-default-features','--locked','--offline','-j1'];cmd+=['--features','native-retirement-observer'] if feature=='on' else [];commands.append(('check-'+feature,cmd))
for name,cmd in commands:
 step={'name':name,'command':cmd};record['steps'].append(step);(out/'record.json').write_text(json.dumps(record,indent=2));print('START '+name,flush=True);t=time.monotonic()
 with (out/(name+'.log')).open('w') as log:
  child=subprocess.Popen(cmd,cwd=s,env=env,stdout=log,stderr=subprocess.STDOUT);step['pid']=child.pid;(out/'record.json').write_text(json.dumps(record,indent=2));code=child.wait()
 step.update(exit_code=code,seconds=time.monotonic()-t);(out/'record.json').write_text(json.dumps(record,indent=2));print(json.dumps(step),flush=True)
 if code:record['status']='failed';(out/'record.json').write_text(json.dumps(record,indent=2));raise SystemExit(code)
# Reuse the real isolated driver, but bind it to this source and omit its
# already completed core phase. Observer test executable is rebuilt/frozen.
driver=(p/'image-tests.py').read_text().replace("out=p/'image-tests-attempt-1'","out=p/'image-weak-observer-tests'").replace("p/'image-check-attempt-3/","p/'image-weak-final-validation/")
cut=driver.index("print('GTK/pool nine subprocesses terminal;")
driver=driver[:cut]+"record['source_after']={name:h(s/name) for name in before};record['source_unchanged']=record['source_after']==before;record['status']='nine-independent-scenarios-terminal';(out/'record.json').write_text(json.dumps(record,indent=2));raise SystemExit(code)\n"
(p/'weak-observer-tests.py').write_text(driver);print('START rebuilt observer compile/independent tests',flush=True);code=subprocess.call(['python3',str(p/'weak-observer-tests.py')],cwd=r,env=os.environ)
record['observer_exit']=code;record['source_after']={name:h(s/name) for name in before};record['source_unchanged']=record['source_after']==before;record['status']='passed-core-checks-nine-observer-subprocesses' if code==0 else 'observer-failed';(out/'record.json').write_text(json.dumps(record,indent=2));print('FINAL '+record['status'],flush=True);raise SystemExit(code)
