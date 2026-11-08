"""Create deterministic MIT synthetic PDFs using only Python's standard library.

The logical lines below are a construction oracle, not an assertion about a
PDF engine's newline representation or character indices. The provider must
derive its own text/source map from the actual PDF bytes. Helvetica is a PDF
standard Type 1 font; no third-party font file is embedded or distributed.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class Pdf:
    def __init__(self):
        self.objects: list[bytes | None] = []

    def reserve(self) -> int:
        self.objects.append(None)
        return len(self.objects)

    def set(self, number: int, content: str | bytes):
        assert self.objects[number - 1] is None
        self.objects[number - 1] = content.encode('ascii') if isinstance(content, str) else content

    def add(self, content: str | bytes) -> int:
        number = self.reserve()
        self.set(number, content)
        return number

    def stream(self, content: bytes, extra: str = '') -> int:
        header = f'<< /Length {len(content)} {extra}>>\nstream\n'.encode('ascii')
        return self.add(header + content + b'\nendstream')

    def encode(self, root: int) -> bytes:
        result = bytearray(b'%PDF-1.7\n%\xe2\xe3\xcf\xd3\n')
        offsets = [0]
        for number, obj in enumerate(self.objects, 1):
            assert obj is not None
            offsets.append(len(result))
            result.extend(f'{number} 0 obj\n'.encode('ascii'))
            result.extend(obj)
            result.extend(b'\nendobj\n')
        xref = len(result)
        result.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode('ascii'))
        for offset in offsets[1:]:
            result.extend(f'{offset:010d} 00000 n \n'.encode('ascii'))
        result.extend((f'trailer\n<< /Size {len(offsets)} /Root {root} 0 R >>\n'
                       f'startxref\n{xref}\n%%EOF\n').encode('ascii'))
        return bytes(result)


def pdf_array(numbers: list[int]) -> str:
    return '[' + ' '.join(map(str, numbers)) + ']'


def literal(text: str) -> str:
    text.encode('ascii')  # Never misrepresent UTF-8 bytes as a PDF font encoding.
    return '(' + text.replace('\\', '\\\\').replace('(', '\\(').replace(')', '\\)') + ')'


def text_pdf() -> tuple[bytes, list[dict]]:
    pages = [
        {'page_index': 0, 'media_box': [0, 0, 612, 792],
         'crop_box': [18, 24, 594, 768], 'rotation_degrees': 0,
         'text_origin_pdf_user_space': [72, 700],
         'expected_text_lines': [
             'QUSOR native PDF qualification',
             'Repeated evidence stays local.',
             'Distinct first-page context.',
             'Repeated evidence stays local.',
             'Selection target: immutable originals.',
         ]},
        {'page_index': 1, 'media_box': [0, 0, 792, 612],
         'crop_box': [30, 18, 762, 594], 'rotation_degrees': 90,
         'text_origin_pdf_user_space': [72, 520],
         'expected_text_lines': [
             'QUSOR second source page',
             'Repeated evidence stays local.',
             'Second-page context is explicit.',
             'Selection target: immutable originals.',
         ]},
    ]
    pdf = Pdf()
    catalog, tree = pdf.reserve(), pdf.reserve()
    font = pdf.add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>')
    page_ids = []
    for page in pages:
        page_id = pdf.reserve()
        x, y = page['text_origin_pdf_user_space']
        commands = ['BT', '/F1 14 Tf', '24 TL', f'{x} {y} Td']
        for index, line in enumerate(page['expected_text_lines']):
            if index:
                commands.append('T*')
            commands.append(literal(line) + ' Tj')
        commands.append('ET')
        stream = pdf.stream(('\n'.join(commands) + '\n').encode('ascii'))
        pdf.set(page_id, f'<< /Type /Page /Parent {tree} 0 R '
                f'/MediaBox {pdf_array(page["media_box"])} '
                f'/CropBox {pdf_array(page["crop_box"])} /Rotate {page["rotation_degrees"]} '
                f'/Resources << /Font << /F1 {font} 0 R >> >> /Contents {stream} 0 R >>')
        page_ids.append(page_id)
    pdf.set(tree, f'<< /Type /Pages /Kids [{" ".join(f"{p} 0 R" for p in page_ids)}] /Count 2 >>')
    pdf.set(catalog, f'<< /Type /Catalog /Pages {tree} 0 R >>')
    return pdf.encode(catalog), pages


def image_pdf() -> bytes:
    pdf = Pdf()
    catalog, tree, page = pdf.reserve(), pdf.reserve(), pdf.reserve()
    # A real uncompressed raster XObject. No text objects or OCR text layer.
    pixels = bytes([255, 255, 255, 0, 0, 0, 255, 255, 255,
                    0, 0, 0, 255, 255, 255, 0, 0, 0,
                    255, 255, 255, 0, 0, 0, 255, 255, 255])
    image = pdf.stream(pixels, '/Type /XObject /Subtype /Image /Width 3 /Height 3 '
                       '/ColorSpace /DeviceRGB /BitsPerComponent 8 ')
    content = pdf.stream(b'q\n200 0 0 200 36 36 cm\n/Im0 Do\nQ\n')
    pdf.set(page, f'<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 300 300] '
            f'/CropBox [0 0 300 300] /Rotate 0 '
            f'/Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {content} 0 R >>')
    pdf.set(tree, f'<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>')
    pdf.set(catalog, f'<< /Type /Catalog /Pages {tree} 0 R >>')
    return pdf.encode(catalog)


def main():
    ordinary, pages = text_pdf()
    data = {
        'text-two-pages.pdf': ordinary,
        'image-only.pdf': image_pdf(),
        'malformed.pdf': b'%PDF-1.7\nThis is deliberately malformed, without objects, root or xref.\n%%EOF\n',
    }
    for name, content in data.items():
        (HERE / name).write_bytes(content)
    manifest = {
        'schema_version': 1,
        'license': 'MIT; synthetic fixtures created for QUSOR',
        'generator': {'path': 'generate.py', 'sha256': sha256(Path(__file__).read_bytes()),
                      'dependencies': 'Python standard library only'},
        'text_oracle': 'expected_text_lines are source construction order; newline representation and '
                       'engine character/scalar/byte indices must be measured from actual extraction',
        'coordinate_oracle': 'MediaBox/CropBox are PDF user space; rotation is page display rotation; '
                             'text origin is an operator position, not an expected glyph bounding box',
        'fixtures': [
            {'path': 'text-two-pages.pdf', 'sha256': sha256(ordinary), 'bytes': len(ordinary),
             'expected_status': 'text_available', 'expected_page_count': 2, 'pages': pages,
             'selections': [
                 {'id': 'page-zero-unique', 'page_index': 0, 'quote': 'immutable originals',
                  'expected_match_count_in_page': 1, 'expected_resolution': 'exact', 'source_line_index': 4},
                 {'id': 'page-zero-ambiguous', 'page_index': 0, 'quote': 'Repeated evidence stays local.',
                  'expected_match_count_in_page': 2, 'expected_resolution': 'ambiguous'},
                 {'id': 'page-one-exact', 'page_index': 1, 'quote': 'Repeated evidence stays local.',
                  'expected_match_count_in_page': 1, 'expected_resolution': 'exact', 'source_line_index': 1},
                 {'id': 'missing-quote', 'page_index': 0, 'quote': 'A quote that is not in this document.',
                  'expected_match_count_in_page': 0, 'expected_resolution': 'not_found'},
             ]},
            {'path': 'image-only.pdf', 'sha256': sha256(data['image-only.pdf']),
             'bytes': len(data['image-only.pdf']), 'expected_status': 'no_text',
             'expected_page_count': 1, 'pages': [
                 {'page_index': 0, 'media_box': [0, 0, 300, 300], 'crop_box': [0, 0, 300, 300],
                  'rotation_degrees': 0, 'expected_text_lines': []}],
             'note': 'Synthetic image-only PDF representing absence of an embedded text layer; no OCR claim'},
            {'path': 'malformed.pdf', 'sha256': sha256(data['malformed.pdf']),
             'bytes': len(data['malformed.pdf']), 'expected_status': 'invalid_document'},
        ],
        'unverified': ['Chinese PDF text extraction', 'embedded or CID fonts', 'Unicode ToUnicode mappings',
                       'RTL/vertical text', 'ligatures', 'multi-column reading order', 'OCR',
                       'encrypted PDFs', 'all-platform rendering'],
    }
    (HERE / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    for name, content in data.items():
        print(f'{name} {len(content)} {sha256(content)}')


if __name__ == '__main__':
    main()
