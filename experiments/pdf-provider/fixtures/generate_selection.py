"""Additional MIT native-selection rejection fixtures; no third-party inputs."""
import hashlib
import json
from pathlib import Path
from generate import Pdf, literal

HERE = Path(__file__).resolve().parent


def document(lines):
    pdf = Pdf()
    root = pdf.reserve()
    pages = pdf.reserve()
    font = pdf.add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
    operators = '\n'.join(f'BT /F1 14 Tf 1 0 0 1 {x} {y} Tm {literal(text)} Tj ET'
                          for x, y, text in lines).encode('ascii')
    contents = pdf.stream(operators)
    page = pdf.add(f'<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 612 792] '
                   f'/CropBox [0 0 612 792] /Resources << /Font << /F1 {font} 0 R >> >> '
                   f'/Contents {contents} 0 R >>')
    pdf.set(pages, f'<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>')
    pdf.set(root, f'<< /Type /Catalog /Pages {pages} 0 R >>')
    return pdf.encode(root)


def main():
    fixtures = [
        ('selection-two-columns.pdf', [(72, 700, 'Left column first.'), (330, 700, 'Right column first.'),
                                        (72, 670, 'Left column second.'), (330, 670, 'Right column second.')],
         'UnsupportedLayout', 'Distinct columns must not be guessed into a single paragraph.'),
        ('selection-overlap.pdf', [(72, 700, 'A'), (72, 700, 'B')],
         'Ambiguous', 'Different glyphs have overlapping physical boxes; never select the nearest one.'),
    ]
    records = []
    for name, lines, status, reason in fixtures:
        data = document(lines)
        (HERE / name).write_bytes(data)
        records.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest(),
                        'source_operators': lines, 'expected_rejection': status, 'reason': reason})
    (HERE / 'selection-manifest.json').write_text(json.dumps({'license': 'MIT synthetic QUSOR fixtures',
         'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
         'fixtures': records}, indent=2) + '\n')


if __name__ == '__main__':
    main()
