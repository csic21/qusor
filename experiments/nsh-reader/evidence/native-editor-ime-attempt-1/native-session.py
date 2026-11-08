#!/usr/bin/python3
import os,sys,pathlib,subprocess,time,json,shutil,hashlib,xml.etree.ElementTree as ET
root=pathlib.Path('/workspace/qusor');cache=pathlib.Path('/home/user/.cache/qusor-ime');run=pathlib.Path(sys.argv[1]);evidence=root/'experiments/nsh-reader/evidence/native-editor-ime-attempt-1';evidence.mkdir(parents=True,exist_ok=True);prefix=cache/'prefix';base=root/'experiments/nsh-reader';native=base/'target/native-prefix'
def ident(p):return {'pid':p.pid,'start_ticks':int(pathlib.Path(f'/proc/{p.pid}/stat').read_text().rsplit(')',1)[1].split()[19])}
def save(name,v):(evidence/name).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n')
record={'status':'preparing','run':str(run),'display':':111','session_bus':os.environ['DBUS_SESSION_BUS_ADDRESS'],'engine_wrapper':'bwrap engine only; not a product sandbox','personal_configuration_read':False}
save('live.json',record)
try:
 # Derived public schemas; no user dconf or word lists.
 for directory in [native/'usr/share/glib-2.0/schemas',prefix/'usr/share/glib-2.0/schemas']:
  for f in directory.glob('*.gschema.xml'):shutil.copyfile(f,run/'schemas'/f.name)
 override='\n'.join(f'[{name}]\nenable-cloud-input=false\nlua-extension=false\n'for name in ['com.github.libpinyin.ibus-libpinyin.libpinyin','com.github.libpinyin.ibus-libpinyin.libbopomofo'])
 (run/'schemas/99-qusor-ime.gschema.override').write_text(override)
 subprocess.run(['/usr/bin/glib-compile-schemas',str(run/'schemas')],check=True)
 tree=ET.parse(prefix/'usr/share/ibus/component/libpinyin.xml');c=tree.getroot();c.find('exec').text=str(cache/'bin/engine-wrapper.py')+' --ibus';c.find('engines').set('exec',str(cache/'bin/engine-wrapper.py')+' --xml');tree.write(run/'components/libpinyin.xml',encoding='utf-8',xml_declaration=True)
 env=dict(os.environ);env.update(DISPLAY=':111',QUSOR_IME_RUN=str(run),IBUS_COMPONENT_PATH=str(run/'components'),IBUS_ADDRESS='unix:path='+str(run/'runtime/ibus.sock'),GTK_IM_MODULE='ibus',GTK_PATH=str(prefix/'usr/lib/x86_64-linux-gnu/gtk-4.0')+':'+str(prefix/'usr/lib/x86_64-linux-gnu/gtk-4.0/4.0.0/immodules'),GSETTINGS_SCHEMA_DIR=str(run/'schemas'),GSETTINGS_BACKEND='memory',GDK_BACKEND='x11',GTK_USE_PORTAL='0',GIO_USE_VFS='local',LANG='C.utf8',LD_LIBRARY_PATH=str(prefix/'usr/lib/x86_64-linux-gnu')+':'+str(native/'usr/lib/x86_64-linux-gnu'),PATH=str(prefix/'usr/bin')+':/usr/bin:/bin',GI_TYPELIB_PATH=str(prefix/'usr/lib/x86_64-linux-gnu/girepository-1.0'),PYTHONPATH=str(prefix/'usr/lib/python3/dist-packages'))
 for k in ['WAYLAND_DISPLAY','XAUTHORITY']:env.pop(k,None)
 (run/'environment.json').write_text(json.dumps(env,indent=2)+'\n')
 xvfb_env={'PATH':'/usr/bin:/bin','LD_LIBRARY_PATH':'/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu'}
 xvfb=subprocess.Popen(['/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local',':111','-screen','0','1200x1100x24','-nolisten','tcp','-ac'],env=xvfb_env,stdout=(evidence/'xvfb.log').open('w'),stderr=subprocess.STDOUT);record['xvfb']=ident(xvfb);save('live.json',record);time.sleep(1)
 assert xvfb.poll() is None,'own Xvfb exited'
 daemon=subprocess.Popen([str(prefix/'usr/bin/ibus-daemon'),'--single','--address='+env['IBUS_ADDRESS']],env=env,stdout=(evidence/'ibus.log').open('w'),stderr=subprocess.STDOUT);record['ibus']=ident(daemon);save('live.json',record)
 for i in range(40):
  p=subprocess.run([str(prefix/'usr/bin/ibus'),'engine','libpinyin'],env=env,capture_output=True,text=True)
  (evidence/'ibus-engine-select.log').write_text(p.stdout+p.stderr)
  if p.returncode==0:break
  if daemon.poll() is not None:raise RuntimeError('own IBus daemon exited')
  time.sleep(.25)
 else:raise RuntimeError('engine activation not ready; no app started')
 p=subprocess.run([str(prefix/'usr/bin/ibus'),'engine'],env=env,capture_output=True,text=True,check=True);record['selected_engine']=p.stdout.strip()
 sys.path.insert(0,str(base/'dynamic-pdf'));import generate_fixture
 fixture=evidence/'fixtures';fixture.mkdir(exist_ok=True);pdf,texts=generate_fixture.make(3,'Ime witness afterbuild');(fixture/'ime-input.pdf').write_bytes(pdf);save('fixtures/manifest.json',{'generated_after_build':True,'license':'MIT synthetic','sha256':hashlib.sha256(pdf).hexdigest(),'lines':texts})
 shutil.copyfile(base/'evidence/native-editor-ui-attempt-3/bundle.nshc',evidence/'bundle.nshc')
 data=evidence/'data';data.mkdir(mode=0o700,exist_ok=True)
 env.update(QUSOR_READER_RESOURCES=str(base/'target/dynamic-pdf-resources'),QUSOR_READER_DATA=str(data),QUSOR_PDF_LIBRARY=str(root/'experiments/pdf-provider/target/pdfium/lib/libpdfium.so'),QUSOR_DYNAMIC_PICKER_DIR=str(fixture))
 (run/'environment.json').write_text(json.dumps(env,indent=2)+'\n')
 cmd=[str(base/'target/nsh/target/debug/qusor-nsh-dynamic-pdf-probe'),'--bundle',str(evidence/'bundle.nshc'),'--trace','--app-id','org.qusor.nativeprobe','--data-dir',str(data/'ui'),'--window-size','1000x1000']
 app=subprocess.Popen(cmd,env=env,stdout=(evidence/'window.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True);record['app']=ident(app);record['status']='native-running';record['command']=cmd;save('live.json',record);print(json.dumps(record),flush=True)
 while app.poll() is None and not(run/'stop').exists():time.sleep(.5)
 record['app_exit']=app.poll();record['status']='app-terminal'if app.poll()is not None else'stop-requested';save('live.json',record)
 # Stop only owned Popen children; NativeApp should first receive normal WM close.
 if app.poll()is None:app.terminate();app.wait(timeout=15)
 daemon.terminate();record['ibus_exit']=daemon.wait(timeout=15);xvfb.terminate();record['xvfb_exit']=xvfb.wait(timeout=15);save('live.json',record)
except Exception as error:
 record['status']='failed-preparation';record['error']=repr(error);save('live.json',record);print(json.dumps(record),flush=True)
 # Preserve actual live state for inspection; never auto-relaunch on observation timeout.
 while not(run/'stop').exists():time.sleep(.5)
 for name in ['daemon','xvfb']:
  p=locals().get(name)
  if p is not None and p.poll()is None:p.terminate();p.wait(timeout=15)
 raise
