"""Trusted experiment adapter from a frozen native worker report to neutral DTOs.

This is host-side qualification code. Plugins cannot supply these snapshots.
"""
from __future__ import annotations

import os
from pathlib import Path


def freeze_pdf(path: Path) -> bytes:
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as source:
        import stat
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > 16 * 1024 * 1024:
            raise ValueError('PDF must be a bounded regular file')
        data = source.read(16 * 1024 * 1024 + 1)
    if len(data) > 16 * 1024 * 1024:
        raise ValueError('PDF input exceeds native profile')
    return data


def neutral_snapshot(report: dict) -> dict:
    rotations = {'None': 0, 'Degrees90': 90, 'Degrees180': 180, 'Degrees270': 270}
    character_keys = ('pdf_char_index', 'unicode_value', 'mapping', 'generated',
                      'scalar_start', 'scalar_end', 'utf8_start', 'utf8_end',
                      'utf16_start', 'utf16_end', 'pdf_bounds')
    return {
        'protocol_version': 1,
        'original_sha256': report['original_sha256'],
        'original_bytes': report['original_bytes'],
        'extraction': {'provider': 'pdfium', **report['extractor']},
        'engine_report_sha256': report['extraction_sha256'],
        'pages': [{'page_index': page['page_index'], 'engine_text': page['page_text'],
                   'text_sha256': page['page_text_sha256'], 'rotation_degrees': rotations[page['rotation']],
                   'media_box': page['media_box'], 'crop_box': page['crop_box'],
                   'characters': [{key: character[key] for key in character_keys}
                                  for character in page['characters']]}
                  for page in report['pages']],
    }


def selection_draft(document: dict, page_index: int, quote: str, occurrence: int = 0) -> dict:
    page = document['snapshot']['pages'][page_index]
    positions = [index for index in range(len(page['engine_text']))
                 if page['engine_text'].startswith(quote, index)]
    if occurrence < 0 or occurrence >= len(positions):
        raise ValueError('selected exact occurrence does not exist')
    start = positions[occurrence]
    return {'protocol_version': 1, 'original_sha256': document['receipt']['original_sha256'],
            'extraction_revision': document['receipt']['extraction_revision'],
            'page_index': page_index, 'page_text_sha256': page['text_sha256'],
            'unit': 'unicode_scalar', 'start': start, 'end': start + len(quote), 'exact': quote}


def native_selection_draft(document: dict, frame_report: dict, selection: dict) -> dict:
    """Bridge an authenticated native result, never a plugin-provided snapshot.

    Multi-segment results require a future compound Core operation. Reject before
    constructing any individual draft so callers cannot accidentally save half.
    """
    if not selection['committable_single_segment'] or len(selection['segments']) != 1:
        raise ValueError('compound selection needs a versioned atomic capture contract')
    page = _retained_native_page(document, frame_report, selection)
    return _native_segment_draft(document, page, selection['binding']['page_index'], selection['segments'][0])


def _retained_native_page(document: dict, frame_report: dict, selection: dict) -> dict:
    projected = neutral_snapshot(frame_report)
    stored = dict(document['snapshot'])
    projected.pop('engine_report_sha256')
    stored.pop('engine_report_sha256')
    if projected != stored:
        raise ValueError('native frame is not from the retained semantic document version')
    source = selection['binding']
    page_index = source['page_index']
    if type(page_index) is not int or not 0 <= page_index < len(projected['pages']):
        raise ValueError('native selection page is invalid')
    page = projected['pages'][page_index]
    if (source['original_sha256'] != projected['original_sha256']
            or source['frame_report_sha256'] != frame_report['extraction_sha256']
            or source['page_text_sha256'] != page['text_sha256']):
        raise ValueError('native selection is bound to another frame or source')
    return page


def _native_segment_draft(document: dict, page: dict, page_index: int, segment: dict) -> dict:
    start, end = segment['start'], segment['end']
    if (type(start) is not int or type(end) is not int or not 0 <= start < end <= len(page['engine_text'])
            or page['engine_text'][start:end] != segment['exact']):
        raise ValueError('native segment does not match retained engine text')
    return {'protocol_version': 1, 'original_sha256': document['receipt']['original_sha256'],
            'extraction_revision': document['receipt']['extraction_revision'], 'page_index': page_index,
            'page_text_sha256': page['text_sha256'], 'unit': 'unicode_scalar',
            'start': start, 'end': end, 'exact': segment['exact']}


def native_compound_draft(document: dict, frame_report: dict, selection: dict) -> dict:
    """Validate all native pieces before proposing one atomic compound capture."""
    import json
    if (selection['committable_single_segment'] or not 2 <= len(selection['segments']) <= 8
            or selection['layout_profile'] != 'qusor-plain-ltr-lines-v1'
            or selection['order_basis'] != 'unrotated_pdf_user_space_line_bands'
            or selection['separator'] != '\n'):
        raise ValueError('native selection does not qualify for the compound profile')
    page = _retained_native_page(document, frame_report, selection)
    page_index = selection['binding']['page_index']
    segments = [_native_segment_draft(document, page, page_index, segment) for segment in selection['segments']]
    for index, segment in enumerate(segments):
        for previous in segments[:index]:
            if segment['start'] < previous['end'] and previous['start'] < segment['end']:
                raise ValueError('native compound ranges overlap or repeat')
    exact = '\n'.join(segment['exact'] for segment in segments)
    if exact != selection['joined_text']:
        raise ValueError('native compound displayed text disagrees with its ordered source segments')
    draft = {'protocol_version': 2, 'original_sha256': document['receipt']['original_sha256'],
             'extraction_revision': document['receipt']['extraction_revision'],
             'selection_profile': selection['layout_profile'], 'separator': '\n', 'segments': segments, 'exact': exact}
    request = {'capture_protocol': 2, 'request_id': 'pdf-proposal', 'draft': draft}
    if len(json.dumps(request, ensure_ascii=False, separators=(',', ':')).encode('utf-8')) > 4096:
        raise ValueError('compound proposal exceeds the complete public wire budget')
    return draft
