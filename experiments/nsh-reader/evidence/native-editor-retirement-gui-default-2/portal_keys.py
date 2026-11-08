"""Controlled ASCII fixture paths through a verified native GTK chooser and XTest."""
import argparse
import ctypes as C
import ctypes.util
import json
from pathlib import Path
import subprocess
import time


def send(window, pid, action, path=None):
    props = subprocess.check_output(['xprop', '-id', window, '_NET_WM_PID', 'WM_CLASS'], text=True)
    if f'= {pid}\n' not in props or 'Xdg-desktop-portal-gtk' not in props:
        raise ValueError('not the expected native probe window')
    if action == 'path':
        if path is None or not Path(path).is_absolute() or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789/.-' for c in path):
            raise ValueError('controlled lowercase ASCII fixture path required')
        if not Path(path).is_file():
            raise ValueError('fixture file missing')
    x = C.CDLL(ctypes.util.find_library('X11'))
    t = C.CDLL(ctypes.util.find_library('Xtst'))
    x.XOpenDisplay.argtypes = [C.c_char_p]; x.XOpenDisplay.restype = C.c_void_p
    x.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
    x.XGetInputFocus.argtypes = [C.c_void_p, C.POINTER(C.c_ulong), C.POINTER(C.c_int)]
    x.XKeysymToKeycode.argtypes = [C.c_void_p, C.c_ulong]; x.XKeysymToKeycode.restype = C.c_uint
    x.XFlush.argtypes = [C.c_void_p]; x.XCloseDisplay.argtypes = [C.c_void_p]
    t.XTestFakeKeyEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
    d = x.XOpenDisplay(None)
    if not d:
        raise RuntimeError('X11 unavailable')
    def key(symbol, pressed):
        code = x.XKeysymToKeycode(d, symbol)
        if not code:
            raise RuntimeError('native key mapping unavailable')
        t.XTestFakeKeyEvent(d, code, pressed, 0)
    def tap(symbol):
        key(symbol, 1); key(symbol, 0); x.XFlush(d); time.sleep(.02)
    try:
        xid = int(window, 0); x.XSetInputFocus(d, xid, 2, 0); x.XFlush(d); time.sleep(.2)
        focus = C.c_ulong(); revert = C.c_int(); x.XGetInputFocus(d, C.byref(focus), C.byref(revert))
        if focus.value != xid:
            raise RuntimeError('keyboard focus rejected; no text sent')
        if action == 'path':
            key(0xffe3, 1); tap(ord('l')); key(0xffe3, 0); x.XFlush(d); time.sleep(.2)
            key(0xffe3, 1); tap(ord('a')); key(0xffe3, 0)
            for char in path:
                tap(ord(char))
            tap(0xff0d)
        else:
            tap({'enter': 0xff0d, 'escape': 0xff1b, 'tab': 0xff09}[action])
        return {'window': window, 'pid': pid, 'action': action, 'fixture_path': path,
                'focus_verified': True, 'path': 'XTest→GTK native keyboard events; no IME claim'}
    finally:
        x.XCloseDisplay(d)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--window', required=True); p.add_argument('--pid', required=True, type=int)
    p.add_argument('action', choices=['path', 'enter', 'escape', 'tab']); p.add_argument('--path')
    a = p.parse_args(); print(json.dumps(send(a.window, a.pid, a.action, a.path)))
