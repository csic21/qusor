"""Read only the isolated qualification display's CLIPBOARD after an explicit copy action."""
import ctypes as C,ctypes.util,time

def read():
 x=C.CDLL(ctypes.util.find_library('X11'));D=C.c_void_p;U=C.c_ulong
 x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=D;x.XDefaultRootWindow.argtypes=[D];x.XDefaultRootWindow.restype=U;x.XCreateSimpleWindow.argtypes=[D,U,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_uint,U,U];x.XCreateSimpleWindow.restype=U;x.XInternAtom.argtypes=[D,C.c_char_p,C.c_int];x.XInternAtom.restype=U;x.XConvertSelection.argtypes=[D,U,U,U,U,U];x.XPending.argtypes=[D];x.XNextEvent.argtypes=[D,D];x.XFlush.argtypes=[D];x.XGetWindowProperty.argtypes=[D,U,U,C.c_long,C.c_long,C.c_int,U,C.POINTER(U),C.POINTER(C.c_int),C.POINTER(U),C.POINTER(U),C.POINTER(C.POINTER(C.c_ubyte))];x.XFree.argtypes=[D];x.XDestroyWindow.argtypes=[D,U];x.XCloseDisplay.argtypes=[D]
 class Event(C.Union):_fields_=[('type',C.c_int),('pad',C.c_long*24)]
 d=x.XOpenDisplay(None);assert d;w=x.XCreateSimpleWindow(d,x.XDefaultRootWindow(d),0,0,1,1,0,0,0)
 try:
  clip=x.XInternAtom(d,b'CLIPBOARD',0);utf=x.XInternAtom(d,b'UTF8_STRING',0);prop=x.XInternAtom(d,b'QUSOR_CONTROLLED_CLIPBOARD',0);x.XConvertSelection(d,clip,utf,prop,w,0);x.XFlush(d);end=time.monotonic()+3
  while time.monotonic()<end:
   if not x.XPending(d):time.sleep(.01);continue
   event=Event();x.XNextEvent(d,C.byref(event))
   if event.type==31:
    actual=U();fmt=C.c_int();n=U();after=U();data=C.POINTER(C.c_ubyte)();code=x.XGetWindowProperty(d,w,prop,0,65536,0,0,C.byref(actual),C.byref(fmt),C.byref(n),C.byref(after),C.byref(data));assert code==0 and fmt.value==8 and n.value<65536
    try:return C.string_at(data,n.value).decode('utf8')
    finally:x.XFree(data)
  raise RuntimeError('native clipboard selection timed out')
 finally:x.XDestroyWindow(d,w);x.XCloseDisplay(d)
if __name__=='__main__':print(read())
