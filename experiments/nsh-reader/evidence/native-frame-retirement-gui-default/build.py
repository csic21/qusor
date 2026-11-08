from pathlib import Path
import sys,os,json,hashlib,zipfile,subprocess,time,urllib.request,xml.etree.ElementTree as ET
root=Path('/workspace/qusor');base=root/'experiments/nsh-reader';e=base/'evidence/native-frame-retirement-gui-default'
sys.path.insert(0,str(base/'native-host'));import prepare
files=set(json.loads((base/'evidence/native-editor-retirement-gui-default-2/source-before.json').read_text()))
for folder in [base/'native-host',base/'dynamic-pdf']:
 for p in folder.glob('*.rs' if folder.name=='native-host' else '*.ts*'):files.add(str(p.relative_to(root)))
for p in (base/'native-host/tests').glob('*.rs'):files.add(str(p.relative_to(root)))
hashes={n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(files)};(e/'source-before.json').write_text(json.dumps(hashes,indent=2)+'\n')
with zipfile.ZipFile(e/'build-sources.zip','w',zipfile.ZIP_DEFLATED) as z:
 for n in sorted(files):z.write(root/n,n)
upstream={'checked_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'pin':subprocess.check_output(['git','-C',str(prepare.SOURCE),'rev-parse','HEAD'],text=True).strip(),'release':'official releases verified v0.4.0 stable/nightly f81b965','docs':'official fixed-pin docs/dev/linux.md reread','impact':'no upgrade; frozen Linux native qualification only'}
try:
 raw=urllib.request.urlopen('https://github.com/longbridge/nsh/commits/main.atom',timeout=20).read();(e/'upstream.atom').write_bytes(raw);ns={'a':'http://www.w3.org/2005/Atom'};upstream['recent']=[{'id':x.find('a:id',ns).text,'title':x.find('a:title',ns).text.strip(),'updated':x.find('a:updated',ns).text} for x in ET.fromstring(raw).findall('a:entry',ns)[:5]]
except Exception as ex:upstream['recent_check_error']=str(ex)
(e/'upstream-check.json').write_text(json.dumps(upstream,indent=2)+'\n')
env=prepare.runtime_environment(base/'target/native-prefix');env.update(RUSTUP_TOOLCHAIN='1.97.1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(prepare.SOURCE/'target'),TMPDIR='/home/user/.cache/qusor-validation-tmp',CARGO_BUILD_JOBS='1')
cmd=['cargo','build','-j1','--manifest-path',str(base/'native-host/Cargo.toml'),'--locked','--offline','--bin','qusor-nsh-dynamic-pdf-probe'];(e/'build-command.json').write_text(json.dumps({'native':cmd,'features':'default only; no native-qualification','jobs':1},indent=2)+'\n')
with (e/'native-build.log').open('w') as log:
 p=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);print('NATIVE_BUILD_PID',p.pid,flush=True);code=p.wait()
print('NATIVE_BUILD_EXIT',code,flush=True)
if code:sys.exit(code)
cmd=[str(prepare.SOURCE/'target/debug/nsh'),'build',str(base/'dynamic-pdf'),'-o',str(base/'target/dynamic-pdf-bundle'),'--verbose']
with (e/'bundle-build.log').open('w') as log:p=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
print('BUNDLE_BUILD_EXIT',p.returncode,flush=True)
if p.returncode:sys.exit(p.returncode)
binary=prepare.SOURCE/'target/debug/qusor-nsh-dynamic-pdf-probe';dest=base/'target/qualified-native-frame-retirement-gui-default';dest.mkdir(exist_ok=True);os.link(binary,dest/'native-binary')
import shutil
shutil.copyfile(base/'target/dynamic-pdf-bundle/bundle.nshc',e/'bundle.nshc')
(e/'build-artifacts.json').write_text(json.dumps({'native_binary':str(dest/'native-binary'),'native_binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'bundle_sha256':hashlib.sha256((e/'bundle.nshc').read_bytes()).hexdigest(),'source_fingerprints':len(hashes)},indent=2)+'\n')
