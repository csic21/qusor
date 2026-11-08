import sys,time,ctypes as C,ctypes.util,json,subprocess
from pathlib import Path
ROOT=Path('/workspace/qusor');R=ROOT/'experiments/nsh-reader';OUT=R/'evidence/reading-workspace/final-focus';PID=287912
sys.path.insert(0,str(R/'dynamic-pdf'));from ui_driver import windows,click,capture,log_values,trace
from dialog_keys import send

def record(value):
 with (OUT/'events.jsonl').open('a') as f:f.write(json.dumps(value,ensure_ascii=False)+'\n')
 return value

def mainwindow():return [v['window'] for v in windows(PID) if 'nsh"' in v['props']][0]
def resize(width,height):
 window=mainwindow();props=subprocess.check_output(['xprop','-id',window,'_NET_WM_PID','WM_CLASS'],text=True);assert f'= {PID}\n' in props and 'org.qusor.nativeprobe' in props
 x=C.CDLL(ctypes.util.find_library('X11'));x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p;x.XResizeWindow.argtypes=[C.c_void_p,C.c_ulong,C.c_uint,C.c_uint];x.XFlush.argtypes=[C.c_void_p];x.XCloseDisplay.argtypes=[C.c_void_p];d=x.XOpenDisplay(None);assert d;x.XResizeWindow(d,int(window,0),width,height);x.XFlush(d);x.XCloseDisplay(d);time.sleep(.3);record({'action':'resize','pid':PID,'window':window,'width':width,'height':height})
def press(x,y):return record(click(mainwindow(),PID,x,y))
def snap(name):return capture(mainwindow(),PID,OUT/(name+'.png'))
def keys(action):return record(send(mainwindow(),PID,action))
def chooser():return [v['window'] for v in windows(PID) if '打开' in v['props']][0]
def choose_row(y):
 w=chooser();record(click(w,PID,275,y));record(click(w,PID,782,349))
if __name__=='__main__':resize(1280,1000);snap('default-empty');print(windows(PID))

def ready_frame(page=None):
 end=time.monotonic()+20
 while time.monotonic()<end:
  fs=log_values(OUT/'window.log','DYNAMIC_FRAME ')
  if fs:
   f=fs[-1]
   if page is None or f['page']==page:
    replies=log_values(OUT/'window.log','READER_NATIVE ')
    if any(v.get('ok') and v.get('value',{}).get('ready') is True and v['value'].get('generation')==f['generation'] and v['value'].get('handle')==f['handle'] for v in replies):return f
  time.sleep(.1)
 raise RuntimeError('latest frame not ready')
