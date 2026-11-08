"""Send an explicit mouse trace only to an identified native probe X11 window."""
import argparse,ctypes as C,ctypes.util,json,re,subprocess,time

def trace(window,pid,events):
    props=subprocess.check_output(['xprop','-id',window,'_NET_WM_PID','WM_CLASS'],text=True)
    if f'= {pid}\n' not in props or 'org.qusor.nativeprobe' not in props:raise ValueError('not the expected probe')
    info=subprocess.check_output(['xwininfo','-id',window],text=True)
    if 'IsViewable' not in info:raise ValueError('probe not viewable')
    left=int(re.search(r'Absolute upper-left X:\s+(-?\d+)',info)[1]);top=int(re.search(r'Absolute upper-left Y:\s+(-?\d+)',info)[1])
    width=int(re.search(r'Width: (\d+)',info)[1]);height=int(re.search(r'Height: (\d+)',info)[1])
    x=C.CDLL(ctypes.util.find_library('X11'));t=C.CDLL(ctypes.util.find_library('Xtst'))
    x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p;x.XFlush.argtypes=[C.c_void_p];x.XCloseDisplay.argtypes=[C.c_void_p]
    t.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]
    t.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
    d=x.XOpenDisplay(None)
    if not d:raise RuntimeError('X11 unavailable')
    try:
        for event in events:
            kind=event['kind']
            if kind=='move':
                px,py=event['x'],event['y']
                if not (0<=px<width and 0<=py<height):raise ValueError('trace leaves identified client window')
                t.XTestFakeMotionEvent(d,-1,left+round(px),top+round(py),0)
            elif kind in ('down','up'):t.XTestFakeButtonEvent(d,1,kind=='down',0)
            else:raise ValueError('unknown trace event')
            x.XFlush(d);time.sleep(.15)
    finally:x.XCloseDisplay(d)
    return {'pid':pid,'window':window,'origin':[left,top],'size':[width,height],'events':events}
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--window',required=True);p.add_argument('--pid',type=int,required=True);p.add_argument('--events',required=True)
    a=p.parse_args();print(json.dumps(trace(a.window,a.pid,json.loads(a.events))))
