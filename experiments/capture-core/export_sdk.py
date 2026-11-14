"""Export authoritative public contracts, never the host/Core implementation."""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import shutil
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def value(item):
    if isinstance(item, str):
        return json.dumps(item)
    if isinstance(item, bool):
        return str(item).lower()
    if isinstance(item, (int, float)):
        return str(item)
    if isinstance(item, list):
        return '[' + ', '.join(map(value, item)) + ']'
    if isinstance(item, dict):
        return '{ ' + ', '.join(f'{key} = {value(val)}' for key, val in item.items()) + ' }'
    raise TypeError(item)


def manifest(package, dependencies, unsafe_lint='forbid'):
    return '\n'.join(['[workspace]', 'resolver = "3"', '', '[package]',
                      *(f'{key} = {value(val)}' for key, val in package.items()),
                      '', '[dependencies]',
                      *(f'{key} = {value(val)}' for key, val in dependencies.items()),
                      '', '[lints.rust]', f'unsafe_code = {json.dumps(unsafe_lint)}', ''])


def export_sdk(destination: Path):
    destination.mkdir(parents=True, exist_ok=True)
    fingerprints = {}
    for name, source, filename, allowed in (
        ('capture', ROOT / 'crates/core-api', 'contract.rs', {'serde', 'serde_json', 'sha2', 'unicode-segmentation'}),
        ('package', ROOT / 'crates/extension-package', 'package.rs',
         {'extension-api', 'ed25519-dalek', 'serde', 'serde_json', 'sha2'}),
    ):
        cargo = tomllib.loads((source / 'Cargo.toml').read_text())
        dependencies = {key: copy.deepcopy(val) for key, val in cargo['dependencies'].items()
                        if key in allowed}
        assert set(dependencies) == allowed
        if 'extension-api' in dependencies:
            dependencies['extension-api'] = {'path': '../api'}
        target = destination / name
        (target / 'src').mkdir(parents=True)
        shutil.copy2(source / 'src' / filename, target / 'src' / filename)
        (target / 'src/lib.rs').write_text(f'pub mod {Path(filename).stem};\n')
        package = copy.deepcopy(cargo['package'])
        if name in ('capture', 'package'):
            workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']
            package = {key: workspace[key] if isinstance(val, dict) and val.get('workspace') else val
                       for key, val in package.items()}
        (target / 'Cargo.toml').write_text(manifest(package, dependencies))
        fingerprints[f'{name}/{filename}'] = hashlib.sha256((target / 'src' / filename).read_bytes()).hexdigest()
        if name == 'capture':
            shutil.copy2(source / 'src/pdf_contract.rs', target / 'src/pdf_contract.rs')
            shutil.copy2(source / 'src/pdf_compound.rs', target / 'src/pdf_compound.rs')
            (target / 'src/lib.rs').write_text('pub mod contract;\npub mod pdf_contract;\npub mod pdf_compound;\n')
            fingerprints['capture/pdf_contract.rs'] = hashlib.sha256((target / 'src/pdf_contract.rs').read_bytes()).hexdigest()
            fingerprints['capture/pdf_compound.rs'] = hashlib.sha256((target / 'src/pdf_compound.rs').read_bytes()).hexdigest()

    api = ROOT / 'crates/extension-api'
    cargo = tomllib.loads((api / 'Cargo.toml').read_text())
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
    package = {key: workspace['package'][key] if isinstance(val, dict) and val.get('workspace')
               else val for key, val in cargo['package'].items()}
    shutil.copytree(api / 'src', destination / 'api/src')
    shutil.copytree(api / 'schema', destination / 'api/schema')
    for directory in ('src', 'schema'):
        for path in sorted((api / directory).rglob('*')):
            if path.is_file():
                relative = path.relative_to(api)
                exported = destination / 'api' / relative
                authority_hash = hashlib.sha256(path.read_bytes()).hexdigest()
                assert hashlib.sha256(exported.read_bytes()).hexdigest() == authority_hash
                fingerprints[f'api/{relative.as_posix()}'] = authority_hash
    (destination / 'api/Cargo.toml').write_text(manifest(package, cargo['dependencies']))

    sdk_root = ROOT / 'sdk/qusor-sdk'
    sdk_cargo = tomllib.loads((sdk_root / 'Cargo.toml').read_text())
    sdk_deps = copy.deepcopy(sdk_cargo['dependencies'])
    sdk_deps['extension-api'] = {'path': '../api'}
    shutil.copytree(sdk_root / 'src', destination / 'qusor-sdk/src')
    for path in sorted((sdk_root / 'src').rglob('*')):
        if path.is_file():
            relative = path.relative_to(sdk_root)
            exported = destination / 'qusor-sdk' / relative
            authority_hash = hashlib.sha256(path.read_bytes()).hexdigest()
            assert hashlib.sha256(exported.read_bytes()).hexdigest() == authority_hash
            fingerprints[f'qusor-sdk/{relative.as_posix()}'] = authority_hash
    sdk_unsafe_lint = sdk_cargo.get('lints', {}).get('rust', {}).get('unsafe_code', 'deny')
    (destination / 'qusor-sdk/Cargo.toml').write_text(
        manifest(sdk_cargo['package'], sdk_deps, unsafe_lint=sdk_unsafe_lint)
    )

    cargo = tomllib.loads((HERE / 'author/Cargo.toml').read_text())
    dependencies = copy.deepcopy(cargo['dependencies'])
    for name, path in [('extension-api', 'api'), ('qusor-capture-probe', 'capture'),
                       ('qusor-package-probe', 'package')]:
        dependencies[name] = {'path': f'../{path}'}
        if name == 'qusor-capture-probe':
            dependencies[name]['package'] = 'core-api'
        if name == 'qusor-package-probe':
            dependencies[name]['package'] = 'extension-package'
    shutil.copytree(HERE / 'author/src', destination / 'author/src')
    for path in sorted((HERE / 'author/src').rglob('*')):
        if path.is_file():
            relative = path.relative_to(HERE / 'author')
            exported = destination / 'author' / relative
            authority_hash = hashlib.sha256(path.read_bytes()).hexdigest()
            assert hashlib.sha256(exported.read_bytes()).hexdigest() == authority_hash
            fingerprints[f'author/{relative.as_posix()}'] = authority_hash
    (destination / 'author/Cargo.toml').write_text(manifest(cargo['package'], dependencies))
    forbidden = ('host.rs', 'pdf_host.rs', 'core.rs', 'store.rs', 'managed_runtime.rs')
    assert not any(path.name in forbidden for path in destination.rglob('*.rs'))
    (destination / 'README.md').write_text(
        '# Capture experimental public SDK\n\n'
        'Capture/anchor subset (contract, pdf_contract, pdf_compound) exported from core-api; knowledge storage envelopes/ports are excluded. Package format/signer comes from crates/extension-package.\n'
        'No Host, Core, database or Wasm runtime implementation is included.\n'
        'api/schema contains the authoritative 0.1-0.5 policy, Surface, Draft, Composition, Component Invocation, Context, Continuation and Paragraph Projection schemas; parsing does not grant authority.\n'
        'This is a validation artifact, not a production SDK release.\n')
    return fingerprints
