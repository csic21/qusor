"""Send ASCII XTest keyboard events only after verifying the native probe's focus."""
import argparse,ctypes as C,ctypes.util,json,subprocess,time

def type_text(window,pid,text):
    props=subprocess.check_output(['xprop','-id',window,'_NET_WM_PID','WM_CLASS'],text=True)
    if f'= {pid}\n' not in props or 'org.qusor.nativeprobe' not in props:raise ValueError('not the expected native probe')
    if not text or any(not ('a'<=ch<='z' or ch==' ') for ch in text):raise ValueError('fixture accepts lowercase ASCII and spaces only; no IME claim')
    x=C.CDLL(ctypes.util.find_library('X11'));t=C.CDLL(ctypes.util.find_library('Xtst'))
    x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p
    x.XSetInputFocus.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_ulong]
    x.XGetInputFocus.argtypes=[C.c_void_p,C.POINTER(C.c_ulong),C.POINTER(C.c_int)]
    x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong];x.XKeysymToKeycode.restype=C.c_uint
    x.XFlush.argtypes=[C.c_void_p];x.XCloseDisplay.argtypes=[C.c_void_p]
    t.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
    d=x.XOpenDisplay(None)
    if not d:raise RuntimeError('X11 unavailable')
    try:
        xid=int(window,0);x.XSetInputFocus(d,xid,2,0);x.XFlush(d);time.sleep(.3)
        focus=C.c_ulong();revert=C.c_int();x.XGetInputFocus(d,C.byref(focus),C.byref(revert))
        if focus.value!=xid:raise RuntimeError(f'keyboard focus rejected: {hex(focus.value)}; no text sent')
        for ch in text:
            key=x.XKeysymToKeycode(d,ord(ch))
            if not key:raise RuntimeError('fixture character has no native key mapping')
            t.XTestFakeKeyEvent(d,key,1,0);t.XTestFakeKeyEvent(d,key,0,0);x.XFlush(d);time.sleep(.08)
        return {'window':window,'pid':pid,'text':text,'focus_verified':True,'path':'XTest→GDK/GTK native Input→onChange; no IME'}
    finally:x.XCloseDisplay(d)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--window',required=True);p.add_argument('--pid',type=int,required=True);p.add_argument('--text',required=True);a=p.parse_args();print(json.dumps(type_text(a.window,a.pid,a.text)))
