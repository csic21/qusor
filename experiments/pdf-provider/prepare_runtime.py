"""Verify and unpack the pinned native test runtime; no network or system install."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import tarfile

ARCHIVE_SHA256 = 'e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659'
LIBRARY_SHA256 = '1c251efce460d67437a4c76d4be9033ae2f17bf5603f5bff1af10209b94ca723'
URL = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-linux-x64.tgz'
RUNTIMES = {
    'linux-x86_64': {'archive_sha256': ARCHIVE_SHA256, 'library_sha256': LIBRARY_SHA256,
                     'url': URL, 'library': 'lib/libpdfium.so'},
    'macos-aarch64': {
        'archive_sha256': '4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d',
        'library_sha256': 'a45c44645523968525c710462691dd974f4efb1013ce1c249919a0939cfb373a',
        'url': 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-mac-arm64.tgz',
        'library': 'lib/libpdfium.dylib'},
    'macos-x86_64': {
        'archive_sha256': '8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0',
        'library_sha256': 'a9f428a16f46bab89646c8724b348e01c3fe707e3126d9356026cd876a74aecb',
        'url': 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-mac-x64.tgz',
        'library': 'lib/libpdfium.dylib'},
}



def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(archive, destination, target="linux-x86_64"):
    runtime = RUNTIMES[target]
    if archive.stat().st_size > 32 * 1024 * 1024 or sha256(archive) != runtime["archive_sha256"]:
        raise ValueError(f'archive does not match pinned PDFium 8125 {target} artifact')
    if destination.exists() and any(destination.iterdir()):
        raise ValueError('runtime destination must be empty')
    with tarfile.open(archive, 'r:gz') as bundle:
        members = bundle.getmembers()
        if len(members) > 1024 or sum(member.size for member in members) > 128 * 1024 * 1024:
            raise ValueError('runtime archive budget exceeded')
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
                raise ValueError('unexpected runtime archive entry')
        destination.mkdir(parents=True, exist_ok=True)
        bundle.extractall(destination, members=members, filter='data')
    if sha256(destination / runtime['library']) != runtime['library_sha256']:
        raise ValueError('extracted library does not match pinned runtime')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--destination', type=Path,
                        default=Path(__file__).parent / 'target/pdfium')
    parser.add_argument('--target', choices=sorted(RUNTIMES), default='linux-x86_64')
    args = parser.parse_args()
    prepare(args.archive, args.destination, args.target)
    print(args.destination / RUNTIMES[args.target]['library'])
