import pathlib,sys,os,json,time,subprocess,importlib.util
root=pathlib.Path('/workspace/qusor');b=root/'experiments/nsh-reader';e=b/'evidence/native-frame-retirement-gui-default';live=json.loads((e/'live.json').read_text());os.environ.update(DISPLAY=live['display'],DBUS_SESSION_BUS_ADDRESS=live['bus'])
sys.path[:0]=[str(b/'dynamic-pdf'),str(b/'native-host')]
from ui_driver import windows,click
from capture_window import capture
from keyboard_probe import type_text
from pointer_probe import trace
pid=live['app']['pid'];matches=windows(pid);w=matches[0]['window'] if matches else None
if __name__=='__main__':
 action=sys.argv[1]
 if action=='windows':print(json.dumps(matches))
 if action=='shot':print(capture(w,pid,e/sys.argv[2]))
 if action=='click':print(json.dumps(click(w,pid,int(sys.argv[2]),int(sys.argv[3]))))
 if action=='type':print(json.dumps(type_text(w,pid,sys.argv[2])))
 if action=='drag':print(json.dumps(trace(w,pid,[{'kind':'move','x':244,'y':196},{'kind':'down'},{'kind':'move','x':300,'y':196},{'kind':'move','x':370,'y':196},{'kind':'move','x':420,'y':196},{'kind':'up'}])))
