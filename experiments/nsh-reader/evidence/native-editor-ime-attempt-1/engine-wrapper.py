#!/usr/bin/python3
"""Not a product sandbox. Experimental engine-only read-only path overlays.
Prepared, not executed: namespace/overlay/DBus operation still needs qualification.
"""
import os,pathlib,sys
root=pathlib.Path('/home/user/.cache/qusor-ime')
run=pathlib.Path(os.environ['QUSOR_IME_RUN']).resolve()
if run.parent != root/'runs' or not run.is_dir():
 raise SystemExit('explicit isolated QUSOR_IME_RUN required')
prefix=root/'prefix'
args=['/usr/bin/bwrap','--unshare-user','--uid',str(os.getuid()),'--gid',str(os.getgid()),'--unshare-pid','--unshare-net','--die-with-parent','--new-session','--ro-bind','/','/','--proc','/proc','--dev','/dev','--tmpfs','/home','--dir','/home/user/.cache','--ro-bind',str(root),str(root),'--bind',str(run),str(run),'--chdir',str(run)]
for location in ['/usr/lib/x86_64-linux-gnu','/usr/share','/usr/libexec']:
 args+=['--overlay-src',location,'--overlay-src',str(prefix/location.lstrip('/')),'--ro-overlay',location]
args+=['--',str(prefix/'usr/libexec/ibus-engine-libpinyin'),*sys.argv[1:]]
os.execv(args[0],args)
