"""Capture only an identified probe X11 client window, never the desktop."""
import argparse
import ctypes as C
import ctypes.util
from pathlib import Path
import re
import struct
import subprocess
import zlib


def capture(window, pid, output):
    props = subprocess.check_output(['xprop', '-id', window, '_NET_WM_PID', 'WM_CLASS'], text=True)
    if f'= {pid}\n' not in props or 'org.qusor.nativeprobe' not in props:
        raise ValueError('window is not the expected native probe process')
    info = subprocess.check_output(['xwininfo', '-id', window], text=True)
    if 'IsViewable' not in info:
        raise ValueError('probe window is not viewable')
    width = int(re.search(r'Width: (\d+)', info)[1])
    height = int(re.search(r'Height: (\d+)', info)[1])
    x = C.CDLL(ctypes.util.find_library('X11'))
    x.XOpenDisplay.argtypes = [C.c_char_p]
    x.XOpenDisplay.restype = C.c_void_p
    x.XGetImage.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int]
    x.XGetImage.restype = C.c_void_p
    x.XGetPixel.argtypes = [C.c_void_p, C.c_int, C.c_int]
    x.XGetPixel.restype = C.c_ulong
    x.XDestroyImage.argtypes = [C.c_void_p]
    x.XCloseDisplay.argtypes = [C.c_void_p]
    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError('X11 display connection failed')
    image = x.XGetImage(display, int(window, 0), 0, 0, width, height, 0xffffffff, 2)
    if not image:
        x.XCloseDisplay(display)
        raise RuntimeError('X11 client image unavailable')
    raw = bytearray()
    try:
        for y in range(height):
            raw.append(0)
            for column in range(width):
                pixel = x.XGetPixel(image, column, y)
                raw.extend(((pixel >> 16) & 255, (pixel >> 8) & 255, pixel & 255))
    finally:
        x.XDestroyImage(image)
        x.XCloseDisplay(display)
    def chunk(kind, value):
        return struct.pack('>I', len(value)) + kind + value + struct.pack('>I', zlib.crc32(kind + value) & 0xffffffff)
    # This X11 probe uses the TrueColor RGB 0xff0000/0x00ff00/0x0000ff visual.
    if 'Visual Class: TrueColor' not in info:
        raise ValueError('unsupported X11 visual')
    output.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))
    return width, height


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', required=True)
    parser.add_argument('--pid', type=int, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(capture(args.window, args.pid, args.output))
