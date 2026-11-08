import sys,time,json,os
from pathlib import Path
ROOT=Path('/workspace/qusor');reader=ROOT/'experiments/nsh-reader';sys.path.insert(0,str(reader/'dynamic-pdf'));from ui_driver import *
import importlib.util
spec=importlib.util.spec_from_file_location("dynamic_run",reader/"dynamic-pdf/run.py");run=importlib.util.module_from_spec(spec);spec.loader.exec_module(run)
sys.path.insert(0,str(reader/'native-host'));from keyboard_probe import type_text
out=reader/'evidence/dynamic-pdf/final-vault-bound';data=reader/'target/dynamic-pdf-final-data';fixtures=reader/'target/dynamic-pdf-final-fixtures'
logpath=out/'window.log';log=logpath.open('w');proc=run.launch(data,log,fixtures);pid=proc.pid
records=[]
(out/'launch.json').write_text(json.dumps({'pid':pid,'data':str(data),'fixtures':str(fixtures),'display':os.environ.get('DISPLAY'),'environment':{k:os.environ.get(k) for k in ['GTK_USE_PORTAL','GIO_USE_VFS','GSETTINGS_BACKEND','DBUS_SESSION_BUS_ADDRESS']}},indent=2)+'\n')
print('PID',pid,flush=True)
def offset():return len(logpath.read_text(errors='replace'))
def win(chooser=False):
 end=time.monotonic()+5
 while time.monotonic()<end:
  ws=[v['window'] for v in windows(pid) if ('打开' in v['props'])==chooser]
  if ws:return ws[0]
  time.sleep(.1)
 raise RuntimeError('window unavailable')
main=win();capture(main,pid,out/'empty.png')
def choose(row,name):
 start=offset();records.append(click(main,pid,60,35));w=win(True);capture(w,pid,out/(name+'-picker.png'));records.append(click(w,pid,275,row));records.append(click(w,pid,782,349));wait_log(logpath,'DYNAMIC_IMPORTED ',start);wait_log(logpath,'"ready":true',start)
def nextpage():
 start=offset();records.append(click(main,pid,820,35));wait_log(logpath,'"ready":true',start)
def selectsave(page,note,label):
 report=json.loads(next((data/'native-jobs').glob('*/report.json')).read_text());p=report['pages'][page];title=p['page_text'].split('\r')[0];cs=[c for c in p['characters'] if c['pixel_quad'] and c['scalar_start']<len(title)];mid=lambda q:(sum(x for x,y in q)/4,sum(y for x,y in q)/4);a=mid(cs[0]['pixel_quad']);b=mid(cs[-1]['pixel_quad']);events=[{'kind':'move','x':16+a[0]*.6,'y':63+a[1]*.6},{'kind':'down'},{'kind':'move','x':16+b[0]*.6,'y':63+b[1]*.6},{'kind':'up'}];records.append(trace(main,pid,events));records.append(click(main,pid,820,270));records.append(type_text(main,pid,note));capture(main,pid,out/(label+'-selected.png'));start=offset();records.append(click(main,pid,820,334));wait_log(logpath,'"saved":true',start);capture(main,pid,out/(label+'-saved.png'))
 import shutil
 shutil.copyfile(next((data/'native-jobs').glob('*/report.json')),out/(label+'-report.json'))
choose(150,'three');nextpage();nextpage();selectsave(2,'final native note','third');choose(118,'four');nextpage();nextpage();nextpage();selectsave(3,'final second note','fourth')
(out/'interaction.json').write_text(json.dumps({'pid':pid,'data':str(data),'events':records,'frames':log_values(logpath,'DYNAMIC_FRAME '),'imports':log_values(logpath,'DYNAMIC_IMPORTED ')},ensure_ascii=False,indent=2)+'\n')
print('COMPLETE',pid,flush=True)
