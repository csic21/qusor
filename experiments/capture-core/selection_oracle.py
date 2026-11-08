"""Pixel-derived pointer inputs for native qualification, never product selection."""


def binding(report, index):
    return {'original_sha256': report['original_sha256'], 'frame_report_sha256': report['extraction_sha256'],
            'page_text_sha256': report['pages'][index]['page_text_sha256'], 'page_index': index}


def viewport():
    return {'page_origin_dip': [0., 0.], 'scroll_dip': [0., 0.], 'zoom': 1., 'device_scale': 1., 'generation': 1}


def ink_point(page, path, index):
    magic, dimensions, maximum, pixels = path.read_bytes().split(b'\n', 3)
    width, height = map(int, dimensions.split())
    if magic != b'P6' or maximum != b'255' or len(pixels) != width * height * 3:
        raise ValueError('unexpected native page image')
    quad = page['characters'][index]['pixel_quad']
    left, right = min(p[0] for p in quad), max(p[0] for p in quad)
    top, bottom = min(p[1] for p in quad), max(p[1] for p in quad)
    candidates = [(x+.5, y+.5) for y in range(max(0, top+1), min(height, bottom))
                  for x in range(max(0, left+1), min(width, right))
                  if min(pixels[(y*width+x)*3:(y*width+x)*3+3]) < 128]
    if not candidates:
        raise ValueError('no native glyph ink for pointer oracle')
    x, y = min(candidates, key=lambda p: (p[0]-(left+right)/2)**2 + (p[1]-(top+bottom)/2)**2)
    return {'x': x, 'y': y}
