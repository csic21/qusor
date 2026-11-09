"""Render original shell-layout study boards; these are not application screenshots.

python3 docs/design/assets/generate_shell_boards.py [--png]
PNG rendering uses an existing rsvg-convert or macOS Quick Look; no packages are installed.
"""
from html import escape
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
COLORS = {
    "sheet": "#f2f3f5", "surface": "#ffffff", "chrome": "#f7f8f9",
    "ink": "#20262e", "muted": "#586472", "line": "#d6dce2",
    "selected": "#e7edf4", "accent": "#315c85", "soft": "#eef3f8",
    "paper": "#faf9f5", "warning": "#805222", "warning_fill": "#faf2e6",
}


class Board:
    def __init__(self, title, subtitle, height):
        self.parts = [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="{height}" '
            f'viewBox="0 0 1440 {height}" role="img">',
            f'<title>{escape(title)}</title><desc>{escape(subtitle)}</desc>',
            '<g font-family="-apple-system, BlinkMacSystemFont, PingFang SC, '
            'Noto Sans CJK SC, sans-serif">',
        ]
        self.rect(0, 0, 1440, height, "sheet")
        self.text(48, 52, title, 28, weight=600)
        self.text(48, 84, subtitle, 15, "muted")

    def rect(self, x, y, w, h, fill="surface", radius=0, stroke=None):
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" '
            f'fill="{COLORS[fill]}"' +
            (f' stroke="{COLORS[stroke]}"' if stroke else "") + '/>'
        )

    def text(self, x, y, value, size=15, color="ink", weight=400):
        self.parts.append(
            f'<text x="{x}" y="{y}" font-size="{size}" fill="{COLORS[color]}" '
            f'font-weight="{weight}">{escape(value)}</text>'
        )

    def line(self, x1, y1, x2, y2, color="line", width=1):
        self.parts.append(
            f'<path d="M{x1} {y1}L{x2} {y2}" fill="none" '
            f'stroke="{COLORS[color]}" stroke-width="{width}"/>'
        )

    def chip(self, x, y, w, label, selected=False):
        self.rect(x, y, w, 30, "selected" if selected else "chrome", 5)
        self.text(x + 10, y + 21, label, 13, "accent" if selected else "muted", 500)

    def rows(self, x, y, w, labels, gap=39):
        for i, label in enumerate(labels):
            self.text(x, y + i * gap, label)
            self.line(x, y + i * gap + 14, x + w, y + i * gap + 14)

    def arrow(self, x, y):
        self.line(x, y, x + 24, y, "muted", 1.5)
        self.line(x + 18, y - 5, x + 24, y, "muted", 1.5)
        self.line(x + 18, y + 5, x + 24, y, "muted", 1.5)

    def save(self, name):
        self.parts.append('</g></svg>')
        path = HERE / f"{name}.svg"
        path.write_text("\n".join(self.parts) + "\n", encoding="utf-8")
        return path


def layout_board():
    b = Board("同一个基座，按当前工作分配空间", "布局研究 v0.3 · 中性线框，非最终视觉稿 · 内容与状态均为自拟示例", 1190)
    x, y, w, h = 48, 120, 1344, 615
    b.rect(x, y, w, h, radius=10, stroke="line")
    b.rect(x + 1, y + 1, w - 2, 51, "chrome", 9)
    b.text(x + 18, y + 33, "个人空间 ▾", 16, weight=500)
    b.text(x + 170, y + 33, "日常工作 ▾", 14, "muted")
    b.rect(x + 475, y + 11, 420, 31, "surface", 5, "line")
    b.text(x + 491, y + 32, "搜索、打开或执行…", 14, "muted")
    b.text(x + 1198, y + 33, "活动  1", 14)
    b.line(x, y + 52, x + w, y + 52)
    nav, support = 208, 320
    main_x, main_y, main_w = x + nav + 1, y + 52, w - nav - support - 2
    b.rect(x + 1, y + 53, nav - 1, h - 54, "chrome")
    b.line(x + nav, y + 52, x + nav, y + h)
    b.chip(x + 12, y + 67, 80, "空间", True)
    b.chip(x + 97, y + 67, 99, "当前内容")
    b.rect(x + 10, y + 118, nav - 20, 34, "selected", 5)
    b.text(x + 25, y + 141, "当前工作区", 15, weight=500)
    b.text(x + 24, y + 191, "固定入口", 12, "muted")
    for i, label in enumerate(["资料库", "流程", "开发"]):
        b.text(x + 24, y + 228 + i * 38, label)
    b.text(x + 24, y + 367, "全部功能…", 14)
    b.text(x + 24, y + 398, "仅固定常用入口", 12, "muted")
    b.line(x + 16, y + h - 105, x + nav - 16, y + h - 105)
    b.text(x + 24, y + h - 71, "扩展…", 14)
    b.text(x + 24, y + h - 34, "设置…", 14)
    b.rect(main_x, main_y, w - nav - 1, 42, "chrome")
    for offset, label, active in [(8, "发布清单", True), (157, "设计草稿", False), (306, "资料 A", False)]:
        b.rect(main_x + offset, main_y + 6, 140, 35, "surface" if active else "chrome", 4)
        b.text(main_x + offset + 12, main_y + 28, label, 14, "ink" if active else "muted", 500 if active else 400)
        b.text(main_x + offset + 117, main_y + 28, "×", 14, "muted")
    b.line(main_x, main_y + 42, x + w, main_y + 42)
    b.text(main_x + 22, main_y + 76, "发布清单", 18, weight=600)
    b.text(main_x + 147, main_y + 76, "表格视图", 13, "muted")
    b.text(main_x + main_w - 205, main_y + 76, "检查…     工具…    ···", 14)
    b.line(main_x, main_y + 94, main_x + main_w, main_y + 94)
    b.text(main_x + 28, main_y + 137, "内容采用自己的结构与密度", 23, weight=600)
    b.text(main_x + 28, main_y + 166, "表格占满工作面；文稿、画布、会话可以选择不同呈现。", 14, "muted")
    table_y = main_y + 199
    b.rect(main_x + 27, table_y, main_w - 54, 36, "chrome", 3)
    for offset, label in [(40, "项目"), (348, "类型"), (520, "负责人")]:
        b.text(main_x + offset, table_y + 24, label, 13, "muted")
    for i, row in enumerate([("桌面入口", "应用", "我"), ("插件兼容", "契约", "我"), ("备份恢复", "数据", "我"), ("发布说明", "文稿", "我")]):
        ry = table_y + 68 + i * 48
        for offset, label in zip([40, 348, 520], row):
            b.text(main_x + offset, ry, label, 14)
        b.line(main_x + 28, ry + 17, main_x + main_w - 28, ry + 17)
    b.text(main_x + 28, y + h - 32, "示例记录 · 内容状态放在对应对象附近", 13, "muted")
    sx = x + w - support
    b.rect(sx, main_y + 42, support - 1, h - 95, "chrome")
    b.line(sx, main_y + 42, sx, y + h)
    b.text(sx + 20, main_y + 76, "检查规则", 16, weight=500)
    b.text(sx + support - 30, main_y + 76, "×", 17, "muted")
    b.line(sx, main_y + 94, sx + support, main_y + 94)
    b.chip(sx + 16, main_y + 112, support - 32, "跟随当前：发布清单")
    b.rows(sx + 20, main_y + 187, support - 40, ["每项有明确负责人", "来源与结果可以打开", "必需项不能遗漏"])
    b.text(sx + 20, main_y + 350, "用户打开后才占用此区域", 13, "muted")
    b.text(sx + 20, main_y + 376, "可固定目标或移到主区域", 13, "muted")
    b.text(48, 775, "组合示例：支持工具已被用户打开。默认只有导航与主区域，Agent 不常驻。", 15, "muted")
    b.text(48, 827, "主区域的资格相同，工作方式不同", 21, weight=600)
    for i, (title, kind) in enumerate([("阅读插件", "reading"), ("流程插件", "form"), ("开发插件", "code"), ("会话插件", "chat")]):
        mx, my, mw, mh = 48 + i * 342, 852, 318, 216
        b.rect(mx, my, mw, mh, radius=7, stroke="line")
        b.text(mx + 16, my + 28, title, 15, weight=500)
        b.line(mx, my + 42, mx + mw, my + 42)
        if kind == "reading":
            b.rect(mx + 1, my + 43, 69, mh - 44, "chrome")
            b.text(mx + 12, my + 69, "目录", 12, "muted")
            b.rect(mx + 83, my + 57, 220, 145, "paper", 3)
            b.text(mx + 100, my + 84, "内容自己的版心", 14)
            for j in range(5):
                b.line(mx + 100, my + 106 + j * 15, mx + 281, my + 106 + j * 15)
        elif kind == "form":
            b.text(mx + 17, my + 71, "准备一次资料整理", 15)
            b.text(mx + 17, my + 103, "输入范围", 12, "muted")
            b.rect(mx + 17, my + 113, 283, 30, "chrome", 3, "line")
            b.text(mx + 26, my + 133, "当前选择的 3 项", 13)
            b.text(mx + 17, my + 172, "校验输入 → 检查计划 → 执行", 13)
            b.chip(mx + 17, my + 180, 100, "检查计划…")
        elif kind == "code":
            b.rect(mx + 1, my + 43, 68, mh - 44, "chrome")
            b.text(mx + 10, my + 71, "文件", 12, "muted")
            for j, text in enumerate(["fn main() {", "    build();", "}"]):
                b.text(mx + 84, my + 78 + 25 * j, text, 14)
            b.line(mx + 69, my + 150, mx + mw, my + 150)
            b.text(mx + 83, my + 176, "按需输出 / 检查", 13, "muted")
        else:
            b.rect(mx + 16, my + 59, 224, 39, "chrome", 5)
            b.text(mx + 27, my + 84, "讨论目标与材料", 14)
            b.rect(mx + 65, my + 111, 237, 39, "soft", 5)
            b.text(mx + 77, my + 136, "会话本身就是主内容", 14)
            b.rect(mx + 16, my + 171, 286, 30, "surface", 4, "line")
            b.text(mx + 27, my + 192, "输入消息…", 13, "muted")
    b.text(48, 1112, "宿主统一：打开 / 标签 / 布局 / 返回 / 焦点 / 活动     插件提供：对象 / 主交互 / 适用动作 / 紧凑呈现", 15)
    b.text(48, 1146, "静态结构候选 · 不代表普通包接入、原生控件行为、持久恢复或六端资格已通过", 13, "muted")
    return b.save("shell-layout-study")


def flow_board():
    b = Board("切换工作，保留对象、输入和返回路径", "交互研究 v0.3 · 先定义状态转移，再做视觉表现 · 所有流程均待原生验证", 1030)
    b.text(48, 130, "写给 A 的输入，不能因为切到 B 就改了目标", 22, weight=600)
    steps = [
        ("打开 A", "主视图 A", "工具：跟随 A", "还没有未提交输入"),
        ("开始输入", "主视图 A", "工具：固定到 A", "草稿绑定 A 与其版本"),
        ("激活 B", "主视图 B", "草稿仍属于 A", "不自动把文字发给 B"),
        ("明确处理", "返回 A / 保存后切换", "保留或提交原草稿", "只有用户动作能改变目标"),
    ]
    for i, (label, main, tool, note) in enumerate(steps):
        x, y, w, h = 48 + i * 345, 159, 309, 235
        b.text(x, y + 3, f"{i + 1}. {label}", 15, weight=500)
        b.rect(x, y + 22, w, h - 22, radius=6, stroke="line")
        b.rect(x + 1, y + 23, w - 2, 35, "chrome", 5)
        b.text(x + 13, y + 47, main, 14)
        b.rect(x + 15, y + 79, w - 30, 41, "selected" if i < 2 else "warning_fill", 5)
        b.text(x + 27, y + 106, tool, 14, "accent" if i < 2 else "warning", 500)
        b.text(x + 15, y + 157, note, 13, "muted")
        b.line(x + 15, y + 180, x + w - 15, y + 180)
        b.text(x + 15, y + 207, "同一视图 / 草稿身份保持", 12, "muted")
        if i < 3:
            b.arrow(x + w + 6, y + 123)
    b.text(48, 449, "空间不足时改变安放，不重建业务、不重发执行", 22, weight=600)
    dx, dy, dw, dh = 48, 478, 770, 295
    b.rect(dx, dy, dw, dh, radius=7, stroke="line")
    b.rect(dx + 1, dy + 1, dw - 2, 38, "chrome", 6)
    b.text(dx + 16, dy + 26, "宽窗：当前对象 A + 支持工具", 15, weight=500)
    b.rect(dx + 1, dy + 39, 78, dh - 40, "chrome")
    b.text(dx + 14, dy + 77, "导航", 13, "muted")
    b.line(dx + 79, dy + 39, dx + 79, dy + dh)
    b.text(dx + 106, dy + 79, "对象 A", 20, weight=500)
    for i in range(6):
        b.line(dx + 106, dy + 111 + i * 23, dx + 473, dy + 111 + i * 23)
    b.line(dx + 500, dy + 39, dx + 500, dy + dh)
    b.text(dx + 520, dy + 79, "工具：A 的想法", 15, weight=500)
    b.rect(dx + 520, dy + 99, 226, 110, "soft", 4, "line")
    b.text(dx + 533, dy + 128, "正在写的草稿…", 14)
    b.text(dx + 520, dy + 245, "继续当前输入", 13, "muted")
    b.arrow(835, 619)
    for i, (title, detail, footer) in enumerate([
        ("‹ 对象 A", "A 的想法", "返回恢复 A 的原位置"),
        ("‹ 工作区", "对象 A", "工具可从同一入口再打开"),
    ]):
        px, py, pw, ph = 880 + i * 268, 478, 244, 345
        b.rect(px, py, pw, ph, radius=15, stroke="line")
        b.rect(px + 1, py + 1, pw - 2, 47, "chrome", 14)
        b.text(px + 14, py + 31, title, 15)
        b.line(px, py + 48, px + pw, py + 48)
        b.text(px + 17, py + 86, detail, 19, weight=500)
        if i == 0:
            b.chip(px + 17, py + 105, 210, "目标：A · 未保存")
            b.rect(px + 17, py + 151, 210, 104, "soft", 4, "line")
            b.text(px + 28, py + 182, "同一份草稿…", 14)
            b.text(px + 17, py + 290, "键盘与输入焦点继续", 13, "muted")
        else:
            for j in range(7):
                b.line(px + 17, py + 118 + j * 21, px + pw - 17, py + 118 + j * 21)
        b.text(px + 17, py + ph - 19, footer, 12, "muted")
    b.text(880, 855, "紧凑：工具页  →  系统返回  →  原对象", 14)
    b.text(48, 824, "导航、主内容与工具均须满足最小可用空间。", 15, "muted")
    b.text(48, 854, "字号增大或窗口缩小时，保留正在输入的区域与其目标。", 15, "muted")
    b.line(48, 893, 1392, 893)
    for x, title, caption in [(48, "返回", "回到当前视图的上一位置"), (393, "关闭视图", "保留对象；处理未保存输入"), (738, "隐藏工具", "不等于停止活动"), (1083, "停止活动", "等待真实停止结果")]:
        b.text(x, 931, title, 17, weight=500)
        b.text(x, 962, caption, 13, "muted")
    b.text(48, 1001, "静态流程说明 · 不代表草稿落盘、后台寿命、手机返回或输入法已经验收", 13, "muted")
    return b.save("shell-context-flow")


def main():
    paths = [layout_board(), flow_board()]
    if "--png" in sys.argv:
        renderer = shutil.which("rsvg-convert")
        for path in paths:
            output = path.with_suffix(".png")
            if renderer:
                subprocess.run([renderer, str(path), "-o", str(output)], check=True)
            elif shutil.which("qlmanage") and shutil.which("swift"):
                # Quick Look fills a square thumbnail, cropping a non-square SVG.
                # Wrap the authored board in a square, then remove only the extra bottom.
                svg = ET.parse(path).getroot()
                width, height = int(svg.attrib["width"]), int(svg.attrib["height"])
                side = max(width, height)
                content = path.read_text(encoding="utf-8")
                square = content.replace(
                    f'width="{width}" height="{height}" viewBox="0 0 {width} {height}"',
                    f'width="{side}" height="{side}" viewBox="0 0 {side} {side}"', 1,
                )
                with tempfile.TemporaryDirectory(prefix="qusor-shell-board-") as directory:
                    source = Path(directory) / path.name
                    source.write_text(square, encoding="utf-8")
                    subprocess.run(["qlmanage", "-t", "-s", str(side), "-o", directory, str(source)],
                                   check=True, capture_output=True)
                    thumbnail = Path(directory) / (path.name + ".png")
                    subprocess.run(["swift", str(HERE / "crop_shell_board.swift"), str(thumbnail),
                                    str(output), str(width), str(height)],
                                   check=True, capture_output=True)
            else:
                raise SystemExit("No supported PNG renderer; SVG boards generated, rendering not run")
    for path in paths:
        print(path)


if __name__ == "__main__":
    main()
