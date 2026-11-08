"""Request a normal close of an identified probe X11 window."""
import argparse
import ctypes as C
import ctypes.util
import subprocess


def close(window, pid):
    props = subprocess.check_output(['xprop', '-id', window, '_NET_WM_PID', 'WM_CLASS'], text=True)
    if f'= {pid}\n' not in props or 'org.qusor.nativeprobe' not in props:
        raise ValueError('window is not the expected native probe process')
    class Data(C.Union):
        _fields_ = [('b', C.c_char * 20), ('s', C.c_short * 10), ('l', C.c_long * 5)]
    class Client(C.Structure):
        _fields_ = [('type', C.c_int), ('serial', C.c_ulong), ('send_event', C.c_int),
                    ('display', C.c_void_p), ('window', C.c_ulong), ('message_type', C.c_ulong),
                    ('format', C.c_int), ('data', Data)]
    class Event(C.Union):
        _fields_ = [('client', Client), ('pad', C.c_long * 24)]
    x = C.CDLL(ctypes.util.find_library('X11'))
    x.XOpenDisplay.argtypes = [C.c_char_p]
    x.XOpenDisplay.restype = C.c_void_p
    x.XInternAtom.argtypes = [C.c_void_p, C.c_char_p, C.c_int]
    x.XInternAtom.restype = C.c_ulong
    x.XSendEvent.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_long, C.POINTER(Event)]
    x.XFlush.argtypes = [C.c_void_p]
    x.XCloseDisplay.argtypes = [C.c_void_p]
    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError('X11 display connection failed')
    try:
        event = Event()
        event.client.type = 33
        event.client.display = display
        event.client.window = int(window, 0)
        event.client.message_type = x.XInternAtom(display, b'WM_PROTOCOLS', 0)
        event.client.format = 32
        event.client.data.l[0] = x.XInternAtom(display, b'WM_DELETE_WINDOW', 0)
        event.client.data.l[1] = 0
        if not x.XSendEvent(display, int(window, 0), 0, 0, C.byref(event)):
            raise RuntimeError('X11 rejected the normal close request')
        x.XFlush(display)
    finally:
        x.XCloseDisplay(display)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', required=True)
    parser.add_argument('--pid', type=int, required=True)
    args = parser.parse_args()
    close(args.window, args.pid)
