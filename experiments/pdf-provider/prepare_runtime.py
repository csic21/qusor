"""Verify and unpack the pinned native test runtime; no network or system install."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import tarfile

ARCHIVE_SHA256 = 'e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659'
LIBRARY_SHA256 = '1c251efce460d67437a4c76d4be9033ae2f17bf5603f5bff1af10209b94ca723'
URL = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-linux-x64.tgz'


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(archive, destination):
    if archive.stat().st_size > 32 * 1024 * 1024 or sha256(archive) != ARCHIVE_SHA256:
        raise ValueError('archive does not match pinned PDFium 8125 Linux x64 artifact')
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
    if sha256(destination / 'lib/libpdfium.so') != LIBRARY_SHA256:
        raise ValueError('extracted library does not match pinned runtime')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--destination', type=Path,
                        default=Path(__file__).parent / 'target/pdfium')
    args = parser.parse_args()
    prepare(args.archive, args.destination)
    print(args.destination / 'lib/libpdfium.so')
