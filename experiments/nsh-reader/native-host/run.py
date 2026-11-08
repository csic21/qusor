"""Launch the offline native bridge probe in its own data directory."""
from pathlib import Path
import subprocess
import sys

BASE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BASE))
from native_prefix import runtime_environment

if __name__ == '__main__':
    env = runtime_environment(BASE / 'target/native-prefix')
    env['GDK_BACKEND'] = 'x11'
    env['XDG_DATA_DIRS'] = str(BASE / 'target/native-prefix/usr/share') + ':' + env.get('XDG_DATA_DIRS', '/usr/local/share:/usr/share')
    command = [str(BASE / 'target/nsh/target/debug/qusor-nsh-native-host-probe'),
               '--bundle', str(BASE / 'target/native-bundle/bundle.nshc'),
               '--trace', '--app-id', 'org.qusor.nativeprobe', '--data-dir',
               str(BASE / 'target/native-probe-data'), '--window-size', '600x900']
    raise SystemExit(subprocess.run(command, env=env).returncode)
