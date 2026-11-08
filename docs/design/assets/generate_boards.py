"""Generate original SVG design boards; optional PNG rendering uses local librsvg.

These are review artifacts, not application UI or browser implementation.
Run from any directory: python3 docs/design/assets/generate_boards.py [--png]
"""
from pathlib import Path
from html import escape
import sys

HERE = Path(__file__).resolve().parent
INK, MUTED, GREEN = '#242824', '#596159', '#315B46'
APP, NAV, BORDER = '#F4F3EF', '#ECEDE8', '#D9DDD5'
parts = []


def rect(x, y, w, h, fill, radius=0, stroke=None):
    parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" '
                 f'fill="{fill}"' + (f' stroke="{stroke}"' if stroke else '') + '/>')


def text(x, y, value, size=14, fill=INK, weight=400):
    parts.append(f'<text x="{x}" y="{y}" font-size="{size}" fill="{fill}" '
                 f'font-weight="{weight}">{escape(value)}</text>')


def line(x1, y1, x2, y2, color=BORDER):
    parts.append(f'<path d="M{x1} {y1}L{x2} {y2}" stroke="{color}" fill="none"/>')


def icon(x, y, kind, color=MUTED):
    paths = {
        'book': 'M2 3H9L12 5L15 3H22V20H15L12 22L9 20H2ZM12 5V22',
        'note': 'M4 2H17L21 6V22H4ZM17 2V7H21M8 11H17M8 15H17',
        'search': 'M15 15L22 22M18 10A8 8 0 1 1 2 10A8 8 0 1 1 18 10',
        'back': 'M15 4L7 12L15 20M7 12H23',
        'list': 'M5 5H22M5 12H22M5 19H22M1 5H2M1 12H2M1 19H2',
        'close': 'M6 6L18 18M18 6L6 18',
        'more': 'M4 12H5M11 12H12M18 12H19',
        'plus': 'M12 4V20M4 12H20',
    }
    parts.append(f'<path transform="translate({x} {y}) scale(.75)" d="{paths[kind]}" '
                 f'stroke="{color}" stroke-width="1.8" stroke-linecap="round" '
                 'stroke-linejoin="round" fill="none"/>')


def button(x, y, w, label, primary=False):
    rect(x, y, w, 36, GREEN if primary else '#FFFFFF', 6,
         None if primary else BORDER)
    text(x + 14, y + 24, label, 14, '#FFFFFF' if primary else INK, 500)


def begin(title, subtitle):
    parts.clear()
    parts.append('<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="1000" '
                 'viewBox="0 0 1440 1000" role="img">')
    parts.append(f'<title>{escape(title)}</title><desc>{escape(subtitle)}</desc>')
    parts.append('<defs><clipPath id="body"><rect x="120" y="168" width="1256" height="684"/></clipPath></defs>')
    parts.append('<g font-family="Noto Sans CJK SC, sans-serif">')
    rect(0, 0, 1440, 1000, '#E6E8E1')
    text(64, 49, title, 26, INK, 600)
    text(64, 78, subtitle, 14, MUTED)
    rect(64, 112, 1312, 792, APP, 12, '#C9CFC5')


def finish(name, notes):
    text(64, 942, notes, 14, MUTED)
    text(64, 970, '设计工作基线 v0.1 · 自拟示例内容 · 非运行截图 · 功能、长书与原生平台能力仍须分别验收', 12, MUTED)
    parts.append('</g></svg>')
    (HERE / f'{name}.svg').write_text('\n'.join(parts) + '\n')


def rail():
    rect(64, 112, 56, 792, NAV, 12)
    icon(82, 137, 'book', GREEN)
    rect(74, 187, 36, 36, '#DCE4D8', 8)
    icon(82, 196, 'book', GREEN)
    icon(82, 249, 'note')
    line(120, 112, 120, 904)


def reader_header():
    rail()
    rect(120, 112, 1256, 56, '#FBFBF8')
    icon(142, 131, 'back')
    text(168, 147, '资料库', 14, MUTED)
    line(229, 129, 229, 151)
    text(249, 147, '阅读与思考：如何理解材料', 15, INK, 500)
    button(1111, 123, 76, '目录')
    button(1199, 123, 153, '摘录与笔记')
    line(120, 168, 1376, 168)


PARAGRAPHS = [
    ['阅读时，我们常常把注意力放在信息本身：一句话说了什么，',
     '一段论证得出了怎样的结论。然而，理解还需要另一个过程。',
     '它要求我们把新材料放回自己的问题与经验之中。'],
    ['理解不是把句子存起来，而是知道它与已有经验如何联系。',
     '当我们能够说明这份联系，也就更容易发现遗漏与分歧。'],
    ['因此，一段摘录应当保留来处，一份想法应当表达自己的判断。',
     '两者相互关联，又能够分别被检查。回到原文核对时，我们',
     '需要看到当时的上下文，而不只是孤立的一句话。'],
    ['这种工作方式并不要求在每次阅读后写下很多文字。一个具体',
     '的问题、一条反例，或者一个尚未确定的联系，已经足以让',
     '阅读向前走一步。'],
]


def page(x, y, w, selected=False):
    rect(x, y, w, 970, '#FFFFFF', 1, '#E5E5DE')
    text(x + 56, y + 53, '第二章  阅读与理解', 12, MUTED)
    line(x + 56, y + 70, x + w - 56, y + 70)
    text(x + 56, y + 126, '理解，始于建立联系', 27, INK, 500)
    cursor = y + 175
    for index, paragraph in enumerate(PARAGRAPHS):
        for row_index, row in enumerate(paragraph):
            if selected and index == 1 and row_index == 0:
                rect(x + 52, cursor - 19, min(len(row) * 16.4, w - 100), 27, '#F5DF8B', 2)
            text(x + 56, cursor, row, 16, '#303630')
            cursor += 32
        cursor += 20
    text(x + 56, cursor + 20, '阅读札记', 18, INK, 500)
    text(x + 56, cursor + 56, '哪些地方与你原先的理解一致？哪些地方值得重新核对？', 16)


def controls(x=120, w=1256):
    rect(x, 852, w, 52, '#FBFBF8')
    line(x, 852, x + w, 852)
    text(x + 24, 884, '第 12 页 / 共 128 页', 13, MUTED)
    text(x + w // 2 - 68, 884, '‹     12     ›', 15)
    text(x + w - 192, 884, '适合宽度  ⌄', 13, MUTED)
    text(x + w - 70, 884, '−   ＋', 16, MUTED)


def library():
    begin('资料库 · 稳定入口与内容扫描', '① 工作区导航   ② 当前工作区工具   ③ 主内容；组件来源不改变区域职责')
    rect(64, 112, 208, 792, NAV, 12)
    icon(86, 139, 'book', GREEN)
    text(116, 157, 'QUSOR', 18, INK, 600)
    rect(80, 200, 176, 40, '#DCE4D8', 6)
    icon(95, 211, 'book', GREEN)
    text(125, 227, '资料库', 14, GREEN, 500)
    icon(95, 263, 'note')
    text(125, 279, '笔记', 14)
    text(94, 349, '阅读清单', 12, MUTED)
    text(96, 386, '稍后阅读', 14)
    text(96, 425, '已归档', 14)
    line(88, 807, 248, 807)
    text(95, 842, '本地资料', 13, MUTED)
    text(95, 877, '设置与扩展', 13, MUTED)
    line(272, 112, 272, 904)
    text(312, 174, '资料库', 24, INK, 600)
    text(312, 204, '12 份资料 · 保存在本机', 13, MUTED)
    button(1212, 147, 124, '导入资料…')
    rect(312, 232, 798, 40, '#FFFFFF', 6, BORDER)
    icon(326, 243, 'search')
    text(356, 258, '搜索标题、作者或内容', 14, MUTED)
    button(1126, 234, 210, '全部类型          ⌄')
    text(312, 320, '继续阅读', 16, INK, 500)
    rect(312, 340, 1024, 120, '#FFFFFF', 8, BORDER)
    rect(332, 358, 64, 84, '#E5E9DF', 4)
    text(344, 388, '阅读', 14, GREEN, 500)
    text(344, 411, '思考', 14, GREEN, 500)
    text(420, 378, '阅读与思考：如何理解材料', 17, INK, 500)
    text(420, 406, '第二章 · 第 12 页', 13, MUTED)
    rect(420, 426, 300, 3, '#E7EAE4', 1)
    rect(420, 426, 102, 3, '#789477', 1)
    text(737, 432, '继续上次的位置', 12, MUTED)
    button(1194, 380, 120, '继续阅读')
    text(312, 515, '全部资料', 16, INK, 500)
    text(1150, 515, '最近阅读  ↓', 13, MUTED)
    line(312, 536, 1336, 536)
    text(328, 563, '标题', 12, MUTED)
    text(1066, 563, '类型', 12, MUTED)
    text(1178, 563, '最近阅读', 12, MUTED)
    entries = [('阅读与思考：如何理解材料', '独立文档 · 128 页', '今天'),
               ('证据与判断', '学习笔记选编 · 64 页', '昨天'),
               ('知识如何相互连接', '研究材料 · 92 页', '11 月 6 日'),
               ('观察、问题与解释', '个人收藏 · 48 页', '11 月 5 日')]
    for index, (title, detail, date) in enumerate(entries):
        y = 585 + index * 72
        if index == 0:
            rect(312, y - 5, 1024, 70, '#E7EDE5', 4)
        icon(328, y + 16, 'book')
        text(365, y + 23, title, 15, INK, 500)
        text(365, y + 47, detail, 12, MUTED)
        text(1066, y + 31, 'PDF', 12, MUTED)
        text(1178, y + 31, date, 13, MUTED)
        icon(1297, y + 16, 'more')
        line(312, y + 65, 1336, y + 65, '#E4E7E0')
    finish('library', '资料库由工作区插件提供；导航、动作样式和焦点由宿主统一。导入进度与阅读位置须连接真实持久数据。')


def reading():
    begin('阅读 · 内容优先，工具按需', '默认收起目录与编辑区；保留返回路径、文档身份和阅读控制')
    reader_header()
    parts.append('<g clip-path="url(#body)">')
    page(380, 192, 736)
    parts.append('</g>')
    controls()
    finish('reading', '工作区内容区承载 document.reader 角色；PDF保留原页，布局示例不代表长书或可重排格式已验收。')


def capture():
    begin('写想法 · 引用、观点与状态分开', '④ 辅助面板由用户主动打开；当前编辑器可通过更多菜单选择兼容提供者')
    reader_header()
    parts.append('<g clip-path="url(#body)">')
    page(213, 192, 736, selected=True)
    parts.append('</g>')
    rect(1040, 168, 336, 736, '#FFFFFF')
    line(1040, 168, 1040, 904)
    text(1064, 207, '想法', 17, INK, 500)
    icon(1290, 191, 'more')
    icon(1333, 191, 'close')
    line(1064, 229, 1352, 229)
    text(1064, 263, '阅读与思考', 14, INK, 500)
    text(1064, 288, '第 12 页', 13, MUTED)
    rect(1142, 267, 114, 32, '#F1F4EE', 4)
    text(1154, 288, '回到选段', 13, GREEN)
    rect(1064, 309, 288, 148, '#F7F7F2', 6)
    rect(1064, 321, 3, 122, '#D4BA62', 1)
    for i, row in enumerate(['理解不是把句子存起来，', '而是知道它与已有经验', '如何联系。']):
        text(1082, 343 + i * 28, row, 16)
    text(1064, 498, '我的想法', 14, INK, 500)
    rect(1064, 514, 288, 184, '#FFFFFF', 6, GREEN)
    for i, row in enumerate(['可以把一个新概念和已有的', '问题放在一起核对：它改变了', '什么，又留下了哪些疑问？']):
        text(1078, 545 + i * 26, row, 14)
    line(1260, 580, 1260, 600, GREEN)
    text(1064, 729, '未保存', 12, MUTED)
    button(1242, 708, 110, '保存想法', True)
    line(1064, 775, 1352, 775)
    text(1064, 807, '保存后可继续阅读或查看笔记', 12, MUTED)
    controls(120, 920)
    finish('capture', '引用与来源归Core，编辑器提供者可替换；保存成功须有真实回执。关闭面板回到原选段，不丢输入与位置。')


def composition():
    begin('分区与扩展 · 稳定区域，可替换提供者', '区域负责放置，角色负责能力，提供者负责实现；正式数据与来源始终归 Core')
    rect(88, 140, 188, 572, '#DFE7DC', 8)
    text(108, 175, '工作区导航', 18, GREEN, 500)
    text(108, 206, 'shell.workspaces', 13, MUTED)
    for i, row in enumerate(['资料库', '阅读', '笔记', '其他已安装工作区']):
        text(108, 258 + i * 49, row, 15)
    text(108, 637, '插件贡献入口', 13, MUTED)
    text(108, 663, '宿主统一呈现与路由', 13, MUTED)
    rect(296, 140, 1056, 76, '#DFE7DC', 8)
    text(318, 171, '工作区工具栏', 17, GREEN, 500)
    text(318, 198, 'workspace.toolbar · 当前对象 / 返回 / 常用动作', 13, MUTED)
    rect(296, 236, 176, 418, '#F1F2EC', 8, BORDER)
    text(316, 270, '局部导航', 17, INK, 500)
    text(316, 300, 'workspace.leading', 12, MUTED)
    text(316, 349, '目录 / 筛选 / 书签', 13, MUTED)
    text(316, 580, '按需展开', 13, GREEN)
    text(316, 609, '兼容提供者可选择', 12, MUTED)
    rect(492, 236, 550, 418, '#FFFFFF', 8, BORDER)
    text(516, 270, '主内容', 18, INK, 500)
    text(516, 301, 'workspace.content', 13, MUTED)
    text(516, 351, '角色：文档阅读 / 笔记编辑', 16)
    text(516, 382, '实现：兼容阅读器 / 编辑器提供者', 14, MUTED)
    text(516, 412, '输入：实体、版本与来源锚点', 14, MUTED)
    rect(548, 465, 436, 64, '#EDF0E7', 6, BORDER)
    text(568, 491, '上下文动作', 15, GREEN, 500)
    text(568, 516, 'workspace.context-actions · 选段后出现', 12, MUTED)
    text(516, 609, '替换提供者时保留正文、来源与兼容状态', 14, MUTED)
    rect(1062, 236, 290, 418, '#F1F2EC', 8, BORDER)
    text(1084, 270, '辅助面板', 17, INK, 500)
    text(1084, 301, 'workspace.auxiliary', 13, MUTED)
    text(1084, 350, '引用 / 想法 / 资料信息', 15)
    text(1084, 385, '默认关闭，按任务打开', 13, MUTED)
    text(1084, 580, '一个当前工具', 13, GREEN)
    text(1084, 609, '允许选择兼容组件', 13, MUTED)
    rect(296, 674, 1056, 38, '#DFE7DC', 6)
    text(316, 699, '对象状态：跟随当前对象就地呈现 · workspace.object-status（示意位置，非常驻全局状态条）', 13, GREEN)
    line(88, 741, 1352, 741)
    for x, title, detail in [(88, '工作区插件', '组织流程与区域'),
                             (412, '组件提供者', '按角色、格式与版本匹配'),
                             (736, 'Host 公开能力', '原生承载 / 权限 / 资源'),
                             (1060, 'Core', '实体 / 引用 / 持久事务')]:
        rect(x, 768, 268, 106, '#FFFFFF', 8, BORDER)
        text(x + 18, 803, title, 17, INK, 500)
        text(x + 18, 842, detail, 13, MUTED)
        if x < 1060:
            text(x + 285, 824, '→', 22, GREEN)
    finish('composition', '宿主管理整体布局、焦点与受信提示；普通插件经同一公开契约接入。图中名称为设计语义，尚非已发布 API。')


def render_png(path):
    """Optional local native renderer, with no network or Python dependencies."""
    import ctypes as c
    import ctypes.util
    rsvg = c.CDLL(ctypes.util.find_library('rsvg-2'))
    cairo = c.CDLL(ctypes.util.find_library('cairo'))
    gobject = c.CDLL(ctypes.util.find_library('gobject-2.0'))
    rsvg.rsvg_handle_new_from_data.argtypes = [c.c_void_p, c.c_size_t, c.c_void_p]
    rsvg.rsvg_handle_new_from_data.restype = c.c_void_p
    rsvg.rsvg_handle_render_document.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p]
    rsvg.rsvg_handle_render_document.restype = c.c_int
    cairo.cairo_image_surface_create.argtypes = [c.c_int, c.c_int, c.c_int]
    cairo.cairo_image_surface_create.restype = c.c_void_p
    cairo.cairo_create.argtypes = [c.c_void_p]
    cairo.cairo_create.restype = c.c_void_p
    cairo.cairo_surface_write_to_png.argtypes = [c.c_void_p, c.c_char_p]
    cairo.cairo_surface_write_to_png.restype = c.c_int
    cairo.cairo_destroy.argtypes = [c.c_void_p]
    cairo.cairo_surface_destroy.argtypes = [c.c_void_p]
    gobject.g_object_unref.argtypes = [c.c_void_p]
    class Box(c.Structure):
        _fields_ = [(k, c.c_double) for k in ['x', 'y', 'width', 'height']]
    data = path.read_bytes()
    buffer = c.create_string_buffer(data)
    handle = rsvg.rsvg_handle_new_from_data(buffer, len(data), None)
    if not handle:
        raise RuntimeError(f'Invalid SVG: {path}')
    surface = cairo.cairo_image_surface_create(0, 1440, 1000)
    context = cairo.cairo_create(surface)
    try:
        box = Box(0, 0, 1440, 1000)
        if not rsvg.rsvg_handle_render_document(handle, context, c.byref(box), None):
            raise RuntimeError(f'SVG render failed: {path}')
        status = cairo.cairo_surface_write_to_png(surface, bytes(path.with_suffix('.png')))
        if status:
            raise RuntimeError(f'PNG write failed: {status}')
    finally:
        cairo.cairo_destroy(context)
        cairo.cairo_surface_destroy(surface)
        gobject.g_object_unref(handle)


if __name__ == '__main__':
    library()
    reading()
    capture()
    composition()
    if '--png' in sys.argv:
        for filename in ['library', 'reading', 'capture', 'composition']:
            render_png(HERE / f'{filename}.svg')
    print('Generated 4 original design boards (not application screenshots).')
