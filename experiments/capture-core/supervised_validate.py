"""Qualify the production process controller with its real same-executable Core consumer."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
EVIDENCE = HERE / 'evidence'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    paths = []
    for directory in ['crates', 'adapters', 'experiments/capture-core/src',
                      'experiments/capture-core/tests', 'experiments/extension-package/src',
                      'experiments/extension-wasm/runner/src']:
        paths.extend((ROOT / directory).rglob('*.rs'))
    for directory in ['crates', 'adapters']:
        paths.extend((ROOT / directory).rglob('Cargo.toml'))
    for name in ['Cargo.toml', 'Cargo.lock', 'config/architecture.toml',
                 'experiments/capture-core/Cargo.toml', 'experiments/capture-core/Cargo.lock',
                 'experiments/capture-core/supervised_validate.py',
                 'experiments/extension-package/Cargo.toml', 'experiments/extension-package/Cargo.lock',
                 'experiments/extension-wasm/runner/Cargo.toml', 'experiments/extension-wasm/Cargo.lock']:
        paths.append(ROOT / name)
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(set(paths))}


def command(args, label, env, timeout=120):
    start = time.monotonic()
    result = subprocess.run(args, cwd=ROOT, env=env, text=True, capture_output=True, timeout=timeout)
    stdout = EVIDENCE / f'supervised-{label}.stdout'
    stderr = EVIDENCE / f'supervised-{label}.stderr'
    stdout.write_text(result.stdout); stderr.write_text(result.stderr)
    record = {'command': args, 'exit_code': result.returncode, 'seconds': round(time.monotonic()-start, 3),
              'stdout': str(stdout.relative_to(ROOT)), 'stdout_sha256': sha(stdout),
              'stderr': str(stderr.relative_to(ROOT)), 'stderr_sha256': sha(stderr)}
    if result.returncode:
        raise RuntimeError(json.dumps(record))
    return result, record


def main():
    EVIDENCE.mkdir(exist_ok=True)
    env = os.environ.copy(); env['RUSTUP_TOOLCHAIN'] = '1.97.1'; env['CARGO_NET_OFFLINE'] = 'true'
    before = inputs()
    base = ['cargo', 'test', '--manifest-path', str(HERE/'Cargo.toml'), '--test', 'supervised_consumer', '--locked', '--offline']
    build, build_record = command(base+['--no-run', '--message-format=json'], 'build', env)
    executables = [json.loads(line)['executable'] for line in build.stdout.splitlines()
                   if line.startswith('{') and json.loads(line).get('reason') == 'compiler-artifact'
                   and json.loads(line).get('target', {}).get('name') == 'supervised_consumer'
                   and json.loads(line).get('executable')]
    assert len(executables) == 1
    executable = Path(executables[0]); binary_before = sha(executable)
    _, clippy = command(['cargo', 'clippy', '--manifest-path', str(HERE/'Cargo.toml'), '--test',
                         'supervised_consumer', '--locked', '--offline', '--', '-D', 'warnings'], 'clippy', env)
    # No compiler runs in this qualification while its default-250ms VM cases execute.
    run, run_record = command([str(executable)], 'run', env, timeout=30)
    result = json.loads(run.stdout.strip()); assert result['ok']
    assert inputs() == before, 'build inputs changed during qualification'
    assert sha(executable) == binary_before, 'consumer executable changed during qualification'
    record = {'ok': True, 'qualification': 'Linux production Controller/worker_main + ManagedRun + Store/DurableHost + actual Core',
              'source_inputs': before, 'source_inputs_unchanged': True,
              'executable': str(executable.relative_to(ROOT)), 'executable_sha256': binary_before,
              'executable_unchanged': True, 'commands': [build_record, clippy, run_record], 'result': result,
              'limits': ['not a sandbox', 'not NSH or other-platform qualification',
                         '64KiB is request-only; response remains 4096 bytes',
                         'unknown-spawn observer cleanup does not automatically clear the production lease or claim original-parent reaping']}
    output = EVIDENCE/'linux-supervised-controller.json'
    output.write_text(json.dumps(record, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps({'ok': True, 'evidence': str(output.relative_to(ROOT)),
                      'cases': len(result['process_controller_consumer']), 'sha256': sha(output)}))


if __name__ == '__main__':
    main()
