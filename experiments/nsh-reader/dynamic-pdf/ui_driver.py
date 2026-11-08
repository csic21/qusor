"""Small verified-X11 driver for the native dynamic PDF qualification."""
from pathlib import Path
import json
import os
import re
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'native-host'))
from pointer_probe import trace
from capture_window import capture
from dialog_keys import send


def windows(pid):
    tree = subprocess.check_output(['xwininfo', '-root', '-tree'], text=True)
    found = []
    for xid in dict.fromkeys(re.findall(r'^\s+(0x[0-9a-f]+)', tree, re.M)):
        props = subprocess.run(['xprop', '-id', xid, '_NET_WM_PID', 'WM_CLASS', 'WM_NAME'], capture_output=True, text=True).stdout
        if f'= {pid}\n' in props and 'org.qusor.nativeprobe' in props:
            info = subprocess.check_output(['xwininfo', '-id', xid], text=True)
            if 'IsViewable' in info:
                found.append({'window': xid, 'props': props})
    return found


def click(window, pid, x, y):
    return trace(window, pid, [{'kind': 'move', 'x': x, 'y': y}, {'kind': 'down'}, {'kind': 'up'}])


def wait_log(path, marker, after=0, seconds=20):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        content = Path(path).read_text(errors='replace') if Path(path).exists() else ''
        index = content.find(marker, after)
        if index >= 0:
            return content[index:content.find('\n', index) if '\n' in content[index:] else None]
        time.sleep(.1)
    raise RuntimeError(f'native log did not reach {marker!r}')


def log_values(path, prefix):
    values = []
    for line in Path(path).read_text(errors='replace').splitlines():
        if prefix in line:
            raw = line.split(prefix, 1)[1].strip()
            try:
                values.append(json.loads(raw))
            except json.JSONDecodeError:
                pass
    return values


if __name__ == '__main__':
    import argparse
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('--pid', type=int, required=True)
    a = p.parse_args(); print(json.dumps(windows(a.pid), indent=2))
