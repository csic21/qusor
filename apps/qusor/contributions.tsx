import { Canvas, Clipboard, effect, Image, keymap, Path2D, Picker, Scroll, signal, Storage, Text, View, type InputRef, type JSX } from "nsh";
import { Button, Input, Menu, MenuItem, MenuSeparator, TextView } from "nsh/base";
import { Menu as DesktopMenu, type MenuItem as DesktopMenuItem } from "nsh/desktop";
import { BackHandler } from "nsh/mobile";
import { CompositionHost, LibraryHost, NotesHost, VocabHost } from "qusor/pdf-surface";
import {
    Badge,
    BottomSheet,
    Copy,
    focus,
    Glyph,
    ink,
    menuItem,
    menuPopup,
    Rule,
    TextButton,
    Tool,
} from "./ui";
import { phoneticOf, pronunciationOf } from "./pronunciation";
import {
    loadReadingPosition,
    saveReadingPosition,
    type ReadingPosition,
} from "./reading_position";
import {
    DEFAULT_EPUB_FONT_SIZE,
    DEFAULT_EPUB_LINE_HEIGHT,
    formatEpubPosition,
    loadReadingZoom,
    MAX_EPUB_FONT_SIZE,
    MAX_EPUB_LINE_HEIGHT,
    MIN_EPUB_FONT_SIZE,
    MIN_EPUB_LINE_HEIGHT,
    nextReadingZoom,
    normalizeEpubFontSize,
    normalizeEpubLineHeight,
    normalizeReadingZoom,
    parseEpubPosition,
    readingZoomLabel,
    saveReadingZoom,
    type EpubPosition,
} from "./reading_view";
import {
    adjustSelectionEnd,
    adjustSelectionStart,
    boxesForRange,
    boxesForSavedRanges,
    glyphAt,
    parsePageGeometry,
    selectionBetween,
    selectionHandles,
    type PageGeometry,
    type SavedSelectionRange,
} from "./reading_selection";

// Content fixtures plus the manifest-backed workspace contribution catalog.
// This module owns rendering and local draft state; the shell owns composition
// and Core facts.
export type Entry = {
    id: string;
    title: string;
    kind: string;
    provider: string;
    icon: string;
    description: string;
    render: () => JSX.Element;
};
export type PluginWorkspace = {
    id: string;
    package: string;
    title: string;
    kind: string;
    role: string;
    icon: string;
    description: string;
    render: (props: { compact: boolean }) => JSX.Element;
};
export type PluginPackage = {
    id: string;
    title: string;
    version: string;
    enabled: boolean;
};
let pluginWorkspaceOpener: ((role: string) => boolean) | null = null;
export function setPluginWorkspaceOpener(
    opener: (role: string) => boolean,
) {
    pluginWorkspaceOpener = opener;
}
export const draft = signal(
    "让工作区围绕正在处理的内容展开。工具可以随时加入，也可以安静地退到一旁。",
);
function Paragraph(props: { children: JSX.Element }) {
    return (
        <Text
            style={{
                fontSize: 16,
                lineHeight: 29,
                fontWeight: "400",
                color: ink("text-primary"),
                userSelect: "text",
            }}
        >
            {props.children}
        </Text>
    );
}
function Sheet(props: { children: JSX.Element }) {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    paddingHorizontal: 48,
                    paddingTop: 40,
                    paddingBottom: 70,
                    alignItems: "center",
                }}
            >
                <View style={{ width: "100%", maxWidth: 670, gap: 23 }}>
                    {props.children}
                </View>
            </View>
        </Scroll>
    );
}
function NoteView() {
    return (
        <Sheet>
            <Copy size={12} muted>
                产品设计 / 文稿
            </Copy>
            <Text
                style={{
                    fontSize: 29,
                    lineHeight: 41,
                    fontWeight: "600",
                    color: ink("text-primary"),
                }}
            >
                工作区设计备忘
            </Text>
            <Copy size={12} muted>
                2026 年 11 月 9 日 · 窗口内草稿
            </Copy>
            <Rule />
            <Paragraph>
                一天的工作常常从一个想法开始：找到几份资料，试着写下来，再把零散的线索连在一起。好的工作区，应该让这些动作自然地发生。
            </Paragraph>
            <Copy size={18} strong>
                先把内容放在手边
            </Copy>
            <Paragraph>
                我想要一个可以长期停留的地方。文稿、图表和讨论各自有合适的形状，切换时仍能认出自己在哪里。
            </Paragraph>
            <View
                style={{
                    borderLeftWidth: 3,
                    borderColor: ink("text-accent"),
                    paddingLeft: 18,
                    paddingVertical: 3,
                }}
            >
                <Paragraph>共同的是工作区，变化的是看待内容的方式。</Paragraph>
            </View>
            <Copy size={18} strong>
                随手记
            </Copy>
            <Input
                accessibilityLabel="工作区设计随手记"
                multiline
                value={draft()}
                onChange={({ value }) => draft.set(value)}
                style={{
                    minHeight: 132,
                    padding: 14,
                    fontSize: 16,
                    lineHeight: 28,
                    fontWeight: "400",
                    color: ink("text-primary"),
                    backgroundColor: ink("surface-inset"),
                    borderWidth: 1,
                    borderColor: ink("border-subtle"),
                    borderRadius: 8,
                }}
                focusStyle={focus()}
            />
            <Copy size={12} muted>
                此处输入保留到当前窗口关闭
            </Copy>
        </Sheet>
    );
}

export function Diagram(props: { small?: boolean }) {
    return (
        <Canvas
            accessibilityLabel="从想法到成果：收集线索与材料，整理并建立联系，形成文稿与行动"
            style={{ width: "100%", height: props.small ? 132 : 290 }}
            draw={(cx, { width, height }) => {
                const scale = Math.min(width / 650, height / 270);
                const dx = (width - 650 * scale) / 2;
                cx.translate(dx, (height - 270 * scale) / 2);
                cx.scale(scale, scale);
                cx.strokeStyle = ink("canvas-line");
                cx.lineWidth = 1.3;
                cx.stroke(
                    new Path2D("M190 135H245 M405 135H460 M325 102V52H540V102"),
                );
                const nodes = [
                    {
                        x: 30,
                        y: 102,
                        w: 160,
                        t: "收集",
                        s: "线索与材料",
                        icon: "01",
                    },
                    {
                        x: 245,
                        y: 102,
                        w: 160,
                        t: "整理",
                        s: "建立联系",
                        icon: "02",
                    },
                    {
                        x: 460,
                        y: 102,
                        w: 160,
                        t: "形成结果",
                        s: "文稿与行动",
                        icon: "03",
                    },
                ];
                for (let i = 0; i < nodes.length; i++) {
                    const n = nodes[i];
                    cx.fillStyle = ink(
                        i === 1 ? "canvas-selected" : "surface-canvas-node",
                    );
                    cx.beginPath();
                    cx.roundRect(n.x, n.y, n.w, 100, 9);
                    cx.fill();
                    cx.strokeStyle = ink(
                        i === 1 ? "text-accent" : "border-control",
                    );
                    cx.stroke();
                    cx.fillStyle = ink("text-secondary");
                    cx.font = "11px sans-serif";
                    cx.fillText(n.icon, n.x + 16, n.y + 21);
                    cx.fillStyle = ink("text-primary");
                    cx.font = "500 17px sans-serif";
                    cx.fillText(n.t, n.x + 16, n.y + 48);
                    cx.fillStyle = ink("text-secondary");
                    cx.font = "12px sans-serif";
                    cx.fillText(n.s, n.x + 16, n.y + 74);
                }
                cx.fillStyle = ink("text-secondary");
                cx.font = "12px sans-serif";
                cx.fillText("继续补充", 414, 41);
            }}
        />
    );
}
function CanvasView() {
    return (
        <View
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-canvas"),
            }}
        >
            <View
                style={{
                    height: 48,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    paddingHorizontal: 24,
                }}
            >
                <Copy strong>从想法到成果</Copy>
                <View style={{ flex: 1 }} />
                <Copy size={12} muted>
                    流程草案 · 只读样例
                </Copy>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{ paddingHorizontal: 28, paddingTop: 42, gap: 22 }}
                >
                    <Copy size={12} muted>
                        一次研究的三个阶段
                    </Copy>
                    <Diagram />
                    <View
                        style={{
                            alignSelf: "center",
                            maxWidth: 480,
                            paddingTop: 4,
                            gap: 10,
                        }}
                    >
                        <Copy size={15} strong>
                            让线索长成自己的结构
                        </Copy>
                        <Copy muted lines={3}>
                            资料、想法和下一步行动可以放在同一处。图上的节点只表达关系，不代表已经运行的任务。
                        </Copy>
                    </View>
                </View>
            </Scroll>
        </View>
    );
}
function SessionView() {
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        maxWidth: 700,
                        width: "100%",
                        alignSelf: "center",
                        padding: 40,
                        gap: 30,
                    }}
                >
                    <Copy size={12} muted>
                        产品设计 / 新会话
                    </Copy>
                    <Copy size={23} strong>
                        一起想一想下一步
                    </Copy>
                    <Paragraph>
                        准备好当前材料，再选择适合的 Agent。这次会话还没有开始。
                    </Paragraph>
                    <View
                        style={{
                            flexDirection: "row",
                            gap: 12,
                            alignItems: "center",
                            paddingVertical: 14,
                            borderTopWidth: 1,
                            borderBottomWidth: 1,
                            borderColor: ink("border-subtle"),
                        }}
                    >
                        <Badge icon="document" />
                        <View style={{ flex: 1, gap: 4 }}>
                            <Copy>工作区设计备忘</Copy>
                            <Copy size={12} muted>
                                可以选择的上下文 · 尚未发送
                            </Copy>
                        </View>
                        <Glyph name="plus" size={16} />
                    </View>
                </View>
            </Scroll>
            <View
                style={{
                    padding: 24,
                    maxWidth: 760,
                    width: "100%",
                    alignSelf: "center",
                    gap: 10,
                }}
            >
                <View
                    style={{
                        borderRadius: 13,
                        borderCurve: "continuous",
                        borderWidth: 1,
                        borderColor: ink("border-control"),
                        backgroundColor: ink("surface-inset"),
                        padding: 16,
                        gap: 18,
                    }}
                >
                    <Copy muted>先连接 Agent，再开始会话</Copy>
                    <View
                        style={{ flexDirection: "row", alignItems: "center" }}
                    >
                        <Glyph name="plus" size={17} />
                        <View style={{ flex: 1 }} />
                        <Copy size={12} muted>
                            未连接
                        </Copy>
                        <View style={{ width: 8 }} />
                        <Tool
                            label="尚未连接，无法发送"
                            icon="arrow"
                            disabled
                        />
                    </View>
                </View>
                <Copy size={12} muted>
                    界面样例 · 没有生成或发送消息
                </Copy>
            </View>
        </View>
    );
}
function ReaderView() {
    return (
        <Sheet>
            <Copy size={12} muted>
                阅读视图 / 自写样本文稿
            </Copy>
            <Text
                style={{
                    color: ink("text-primary"),
                    fontSize: 29,
                    lineHeight: 41,
                    fontWeight: "600",
                }}
            >
                留一点空间给思考
            </Text>
            <Paragraph>
                今天试着把工具放轻一点。桌面上只留眼前的材料，不急着给每个念头命名。等它们慢慢长出联系，再决定要把什么保留下来。
            </Paragraph>
            <Paragraph>
                阅读是一种进入内容的方式。写作、讨论和绘图也一样。每一种方式，都值得拥有适合自己的空间。
            </Paragraph>
            <Copy muted size={12}>
                样本文字可选择；不读取文件或写入正式知识库
            </Copy>
        </Sheet>
    );
}
const readingQuery = signal(""),
    readingFormat = signal(""),
    readingSection = signal("全部资料"),
    readingLayout = signal("网格"),
    readingOpen = signal(false),
    readingBook = signal("准原生应用的秩序"),
    readingNavigation = signal(false),
    readingBookmarked = signal(false);
const notesQuery = signal(""),
    notesScope = signal("全部想法"),
    notesSelected = signal(""),
    notesNavigation = signal(false),
    notesSelection = signal(""),
    notesCopyStatus = signal(""),
    notesBody = signal<Record<string, string>>({}),
    notesBodyLoading = signal("");
let notesSearchRef: InputRef | null = null;
const readingBooks = [
    {
        title: "准原生应用的秩序",
        author: "研究摘录",
        kind: "PDF",
        progress: "读到第 3 页",
    },
    {
        title: "共同工作区的边界",
        author: "设计札记",
        kind: "Markdown",
        progress: "刚开始",
    },
    {
        title: "插件与宿主之间",
        author: "产品设计",
        kind: "PDF",
        progress: "读到第 8 页",
    },
    {
        title: "工具退到一旁",
        author: "界面研究",
        kind: "文本",
        progress: "已读 24%",
    },
];
type Note = {
    id: string;
    title: string;
    document: string;
    page: number;
    quote: string;
    note: string;
    full?: string;
    date: string;
    scope: string;
    starred: boolean;
    missingOriginal: boolean;
    captureId?: number;
    documentId?: number;
    quoteTruncated?: boolean;
    format?: "pdf" | "epub";
};
const notesFixture: Note[] = [
    {
        id: "native-hierarchy",
        title: "原生界面里的层级",
        document: "准原生应用的秩序",
        page: 3,
        quote: "插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。",
        note: "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
        full: "宿主返回和插件内部返回要写成两个不同的目的地。进入插件后，身份与来源不能一起消失。",
        date: "今天 10:42",
        scope: "当前资料",
        starred: true,
        missingOriginal: false,
    },
    {
        id: "quiet-tools",
        title: "工具应该退到一旁",
        document: "工具退到一旁",
        page: 8,
        quote: "阅读是一种进入内容的方式。写作、讨论和绘图也一样。",
        note: "阅读时不让表单常驻。只有选文后，才把保存和想法动作放到正文附近。",
        full: "阅读时不让表单常驻。只有选文后，才把保存和想法动作放到正文附近。工具应该像书架一样，需要时出现，不需要时退到一旁；但退到一旁不等于消失，用户仍要能从工作区身份和返回路径知道自己在哪。列表用于浏览，详情用于阅读与复制，两者不要同时抢注意力。复制出来的引用必须带上资料标题与页码，否则离开应用之后就无法再核对来源。",
        date: "昨天 21:16",
        scope: "当前资料",
        starred: false,
        missingOriginal: false,
    },
    {
        id: "plugin-boundary",
        title: "知识不属于界面提供者",
        document: "插件与宿主之间",
        page: 5,
        quote: "共同的是工作区，变化的是看待内容的方式。",
        note: "停用插件后，摘录、笔记和来源仍要留在 Core。插件可以被替换，知识身份不能跟着换。",
        full: "停用插件后，摘录、笔记和来源仍要留在 Core。插件可以被替换，知识身份不能跟着换。",
        date: "11 月 7 日",
        scope: "其他资料",
        starred: false,
        missingOriginal: false,
    },
    {
        id: "missing-source",
        title: "原件缺失时的诚实反馈",
        document: "已移走的资料",
        page: 2,
        quote: "来源不可用时就明说，不要把相似文本当成同一条引用。",
        note: "缺少原件、版本不符或权限撤销都应该有可观察的状态。",
        full: "缺少原件、版本不符或权限撤销都应该有可观察的状态。界面不能把“找不到”伪装成“已定位”。",
        date: "11 月 6 日",
        scope: "其他资料",
        starred: false,
        missingOriginal: true,
    },
];
const notesData = signal<Note[]>(notesFixture);
const notesLive = signal(false);
const notesSource = signal("样例数据");
const notesSourceDetail = signal("窗口内样例");
const notesEditingId = signal("");
const notesEditDraft = signal("");
const notesSaving = signal(false);

function firstLine(value: string) {
    return value.split("\n")[0].trim().slice(0, 60);
}

function noteLocationLabel(note: Note | undefined): string {
    if (!note) return "";
    return note.format === "epub" ? `第 ${note.page} 章` : `第 ${note.page} 页`;
}

async function loadLiveNotes() {
    try {
        const raw = await NotesHost.page();
        const result = JSON.parse(raw);
        if (!result.ok || !Array.isArray(result.value?.notes)) return;
        const page = result.value;
        const activeDocId =
            libraryOpenDoc()?.document_id ?? page.current_document_id;
        const mappedPdf: Note[] = page.notes.map((wire: any): Note => ({
            id: `live-${wire.capture_id}-${wire.commit_rev}`,
            title: firstLine(wire.note_preview) || wire.document_title,
            document: wire.document_title,
            page: wire.page_index + 1,
            quote: wire.quote_preview,
            note: wire.note_preview,
            full: wire.note_truncated ? undefined : wire.note_preview,
            date: `提交 ${wire.commit_rev}`,
            scope:
                wire.document_id === activeDocId ? "当前资料" : "其他资料",
            starred: false,
            missingOriginal: false,
            captureId: wire.capture_id,
            documentId: wire.document_id,
            quoteTruncated: wire.quote_truncated === true,
            format: "pdf",
        }));
        let mappedEpub: Note[] = [];
        const epubHost = (NotesHost as any).epubNotesPage;
        if (typeof epubHost === "function") {
            try {
                const epubRaw = await epubHost.call(NotesHost, "", 50);
                const epubRes = JSON.parse(epubRaw);
                if (epubRes?.ok && Array.isArray(epubRes.value?.notes)) {
                    mappedEpub = epubRes.value.notes.map((wire: any): Note => ({
                        id: `live-epub-${wire.capture_id}-${wire.commit_rev}`,
                        title:
                            firstLine(String(wire.note_preview ?? "")) ||
                            String(wire.document_title ?? "EPUB 书籍"),
                        document: String(wire.document_title ?? "EPUB 书籍"),
                        page:
                            Number(
                                wire.chapter_index ?? wire.page_index ?? 0,
                            ) + 1,
                        quote: String(wire.quote_preview ?? ""),
                        note: String(wire.note_preview ?? ""),
                        full: wire.note_truncated
                            ? undefined
                            : String(wire.note_preview ?? ""),
                        date: `提交 ${wire.commit_rev ?? 0}`,
                        scope:
                            wire.document_id === activeDocId
                                ? "当前资料"
                                : "其他资料",
                        starred: false,
                        missingOriginal: false,
                        captureId: Number(wire.capture_id),
                        documentId: Number(wire.document_id),
                        quoteTruncated: wire.quote_truncated === true,
                        format: "epub",
                    }));
                }
            } catch (_) {
                // Keep PDF notes when EPUB notes endpoint is unavailable.
            }
        }
        notesData.set([...mappedPdf, ...mappedEpub]);
        notesLive.set(true);
        notesSource.set("本机 Core");
        notesSourceDetail.set("由 notes 插件读取");
    } catch (_) {
        // Headless tests and the plain development host keep the fixture, but
        // the window states which source is actually shown.
        notesSource.set("样例数据");
        notesSourceDetail.set("窗口内样例");
    }
}

export function refreshNotes() {
    void loadLiveNotes();
}

/// Evidence-only, driven by App when the host was started with
/// QUSOR_WORKBENCH_EVIDENCE. Captures the rendered window through NotesHost so
/// no screen-recording grant or frontmost window is needed.
export async function captureNotesEvidence() {
    let dir = "";
    try {
        const result = JSON.parse(await NotesHost.evidence());
        if (!result.ok || !result.value?.active) return;
        dir = result.value.dir;
    } catch (_) {
        return;
    }
    if (!dir) return;
    for (let attempt = 0; attempt < 30 && !notesLive(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await NotesHost.capture(`${dir}/list.png`);
    const first = visibleNotes()[0];
    if (!first) return;
    selectNote(first);
    await new Promise((resolve) => setTimeout(resolve, 500));
    await NotesHost.capture(`${dir}/detail.png`);
    const truncated = visibleNotes().find((note) => noteTruncated(note));
    if (!truncated) return;
    selectNote(truncated);
    await new Promise((resolve) => setTimeout(resolve, 400));
    await NotesHost.capture(`${dir}/truncated.png`);
    loadNoteBody(truncated);
    for (let attempt = 0; attempt < 30 && !notesBody()[truncated.id]; attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await NotesHost.capture(`${dir}/full-body.png`);
}

function ReadingSidebar(props: { compact: boolean }) {
    return (
        <View
            style={{
                width: props.compact ? "100%" : 214,
                flexShrink: 0,
                borderRightWidth: props.compact ? 0 : 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-toolbar"),
            }}
        >
            <View
                style={{
                    height: 48,
                    paddingHorizontal: 14,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Badge icon="book" size={28} tone="surface-accent" />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy strong>阅读</Copy>
                    <Copy size={11} muted>
                        {notesSource() === "本机 Core"
                            ? "本机 Core"
                            : "工作区样例"}
                    </Copy>
                </View>
                <If when={props.compact}>
                    <Tool
                        label="返回书架内容"
                        icon="back"
                        text="书架"
                        onPress={() => readingNavigation.set(false)}
                    />
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 10, gap: 15 }}>
                    <View style={{ gap: 3 }}>
                        <Each
                            of={["全部资料", "最近阅读", "未读"]}
                        >
                            {(item) => (
                                <Button
                                    accessibilityLabel={
                                        item() === "全部资料"
                                            ? "显示全部资料"
                                            : `显示${item()}`
                                    }
                                    onPress={() => {
                                        readingSection.set(item());
                                        readingNavigation.set(false);
                                    }}
                                    style={{
                                        height: 34,
                                        paddingHorizontal: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        gap: 9,
                                        borderRadius: 7,
                                        backgroundColor: ink(
                                            readingSection() === item()
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Glyph
                                        name={
                                            item() === "最近阅读"
                                                ? "clock"
                                                : item() === "未读"
                                                  ? "book"
                                                  : "grid"
                                        }
                                        size={16}
                                        color={ink(
                                            readingSection() === item()
                                                ? "text-accent"
                                                : "text-secondary",
                                        )}
                                    />
                                    <Copy>{item()}</Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                    <View style={{ gap: 4 }}>
                        <View style={{ paddingHorizontal: 10 }}>
                            <Copy size={11} strong muted>
                                格式
                            </Copy>
                        </View>
                        <Each of={["PDF", "Markdown", "文本"]}>
                            {(format) => (
                                <Button
                                    accessibilityLabel={`筛选${format()}资料`}
                                    onPress={() => {
                                        readingFormat.set(
                                            readingFormat() === format()
                                                ? ""
                                                : format(),
                                        );
                                        readingNavigation.set(false);
                                    }}
                                    style={{
                                        height: 32,
                                        paddingHorizontal: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        borderRadius: 6,
                                        backgroundColor: ink(
                                            readingFormat() === format()
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Copy size={12}>{format()}</Copy>
                                    <View style={{ flex: 1 }} />
                                    <Copy size={11} muted>
                                        {
                                            readingBooks.filter((book) =>
                                                book.kind.includes(format()),
                                            ).length
                                        }
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                </View>
            </Scroll>
            <Rule />
            <View style={{ padding: 13 }}>
                <Copy size={11} muted>
                    窗口内自写资料 · 尚未入库
                </Copy>
            </View>
        </View>
    );
}
function ReadingShelf(props: { compact: boolean }) {
    const visibleBooks = () =>
        readingBooks.filter(
            (book) =>
                (!readingQuery() ||
                    book.title.includes(readingQuery()) ||
                    book.author.includes(readingQuery()) ||
                    book.kind.includes(readingQuery())) &&
                (!readingFormat() ||
                    book.kind.includes(readingFormat())) &&
                (readingSection() !== "未读" ||
                    book.progress === "刚开始"),
        );
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 55,
                    paddingHorizontal: 18,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>{readingSection()}</Copy>
                    <Copy size={11} muted>
                        {visibleBooks().length} 份资料
                    </Copy>
                </View>
                <If when={!props.compact}>
                    <View style={{ width: 220 }}>
                        <Input
                            accessibilityLabel="搜索书库资料"
                            value={readingQuery()}
                            onChange={({ value }) => readingQuery.set(value)}
                            placeholder="搜索资料"
                            style={{
                                height: 32,
                                paddingHorizontal: 10,
                                fontSize: 13,
                                color: ink("text-primary"),
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 7,
                            }}
                            focusStyle={focus()}
                        />
                    </View>
                </If>
                <Tool
                    label={
                        readingLayout() === "网格"
                            ? "切换为列表"
                            : "切换为网格"
                    }
                    icon={readingLayout() === "网格" ? "list" : "grid"}
                    onPress={() =>
                        readingLayout.set(
                            readingLayout() === "网格" ? "列表" : "网格",
                        )
                    }
                />
                <Tool
                    label="导入资料（样例未接入）"
                    icon="plus"
                    text="导入"
                    raised
                    disabled
                />
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: 22,
                        gap: 18,
                        flexDirection:
                            readingLayout() === "网格" ? "row" : "column",
                        flexWrap:
                            readingLayout() === "网格" ? "wrap" : "nowrap",
                        alignItems: "stretch",
                    }}
                >
                    <Each of={visibleBooks()}>
                        {(book) => (
                            <Button
                                accessibilityLabel={`阅读${book().title}`}
                                onPress={() => {
                                    readingBook.set(book().title);
                                    readingBookmarked.set(false);
                                    readingOpen.set(true);
                                }}
                                style={{
                                    width:
                                        readingLayout() === "网格"
                                            ? 218
                                            : "100%",
                                    minHeight:
                                        readingLayout() === "网格" ? 132 : 64,
                                    flexDirection:
                                        readingLayout() === "网格"
                                            ? "column"
                                            : "row",
                                    alignItems: "stretch",
                                    gap: 12,
                                    padding: 13,
                                    borderWidth: 1,
                                    borderColor: ink("border-subtle"),
                                    borderRadius: 9,
                                    borderCurve: "continuous",
                                    backgroundColor: ink("surface-preview"),
                                }}
                                hoverStyle={{
                                    borderColor: ink("border-control"),
                                    backgroundColor: ink("surface-hover"),
                                }}
                                focusStyle={focus()}
                            >
                                <Badge icon="book" size={36} />
                                <View style={{ flex: 1, gap: 4 }}>
                                    <Copy strong>{book().title}</Copy>
                                    <Copy size={11} muted>
                                        {book().author} · {book().kind}
                                    </Copy>
                                    <View style={{ flex: 1 }} />
                                    <Copy size={11} muted>
                                        {book().progress}
                                    </Copy>
                                </View>
                            </Button>
                        )}
                    </Each>
                    <If when={visibleBooks().length === 0}>
                        <View style={{ padding: 28, gap: 8 }}>
                            <Copy strong>没有匹配的资料</Copy>
                            <Copy size={12} muted>
                                清除搜索或选择其他分类后再试。
                            </Copy>
                        </View>
                    </If>
                </View>
            </Scroll>
        </View>
    );
}
function ReadingDocument() {
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 55,
                    paddingHorizontal: 16,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Tool
                    label="返回书架"
                    icon="back"
                    text="书架"
                    onPress={() => readingOpen.set(false)}
                />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy strong>{readingBook()}</Copy>
                    <Copy size={11} muted>
                        插件内阅读视图 · 窗口内样例
                    </Copy>
                </View>
                <Tool
                    label={
                        readingBookmarked()
                            ? "移除窗口内书签"
                            : "添加窗口内书签"
                    }
                    icon="book"
                    selected={readingBookmarked()}
                    onPress={() => readingBookmarked.set(!readingBookmarked())}
                />
                <Tool
                    label="更多阅读工具（样例未接入）"
                    icon="more"
                    disabled
                />
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        width: "100%",
                        maxWidth: 710,
                        alignSelf: "center",
                        paddingHorizontal: 34,
                        paddingTop: 38,
                        paddingBottom: 70,
                        gap: 21,
                    }}
                >
                    <Copy size={12} muted>
                        第一章 · 内容的秩序
                    </Copy>
                    <Text
                        style={{
                            fontSize: 29,
                            lineHeight: 40,
                            fontWeight: "600",
                            color: ink("text-primary"),
                        }}
                    >
                        {readingBook()}
                    </Text>
                    <Paragraph>
                        基座不替阅读决定书架如何排列，也不把阅读工具变成所有工作的入口。进入这份资料后，导航、分类与阅读动作属于阅读插件；返回、权限和恢复仍由宿主守住。
                    </Paragraph>
                    <Paragraph>
                        当窗口变窄，插件优先保留正在阅读的正文。书架导航可以暂时收起，但返回路径、资料身份和当前阅读位置不能被布局变化抹掉。
                    </Paragraph>
                    <View
                        style={{
                            borderLeftWidth: 3,
                            borderColor: ink("text-accent"),
                            paddingLeft: 17,
                            paddingVertical: 3,
                        }}
                    >
                        <Paragraph>
                            插件拥有自己的工作方式，用户始终拥有回到上层工作区的路径。
                        </Paragraph>
                    </View>
                    <Paragraph>
                        本段文字用于检查原生排版与返回行为，没有读取真实文件，也没有把选区写入正式知识库。
                    </Paragraph>
                </View>
            </Scroll>
        </View>
    );
}
function ReadingWorkspace(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="阅读插件工作区"
            style={{
                flex: 1,
                minWidth: 0,
                flexDirection: "row",
                backgroundColor: ink("surface-base"),
            }}
        >
            <If when={!props.compact}>
                <ReadingSidebar compact={props.compact} />
                <If when={readingOpen()}>
                    <ReadingDocument />
                </If>
                <If when={!readingOpen()}>
                    <ReadingShelf compact={props.compact} />
                </If>
            </If>
            <If when={props.compact && readingNavigation()}>
                <ReadingSidebar compact={true} />
            </If>
            <If when={props.compact && !readingNavigation() && !readingOpen()}>
                <View
                    style={{
                        flex: 1,
                        minWidth: 0,
                        flexDirection: "column",
                    }}
                >
                    <View
                        style={{
                            height: 48,
                            paddingHorizontal: 12,
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 9,
                        }}
                    >
                        <Tool
                            label="显示书库导航"
                            icon="list"
                            text="书库导航"
                            onPress={() => readingNavigation.set(true)}
                        />
                        <View style={{ flex: 1, minWidth: 0 }}>
                            <Input
                                accessibilityLabel="搜索书库资料"
                                value={readingQuery()}
                                onChange={({ value }) =>
                                    readingQuery.set(value)
                                }
                                placeholder="搜索资料"
                                style={{
                                    height: 31,
                                    paddingHorizontal: 10,
                                    fontSize: 13,
                                    color: ink("text-primary"),
                                    backgroundColor: ink("surface-inset"),
                                    borderWidth: 1,
                                    borderColor: ink("border-subtle"),
                                    borderRadius: 7,
                                }}
                                focusStyle={focus()}
                            />
                        </View>
                    </View>
                    <Rule />
                    <ReadingShelf compact={true} />
                </View>
            </If>
            <If when={props.compact && readingOpen()}>
                <ReadingDocument />
            </If>
        </View>
    );
}

function noteInScope(note: Note, scope: string) {
    return scope === "全部想法" || note.scope === "当前资料";
}

function noteCount(scope: string) {
    return notesData().filter((note) => noteInScope(note, scope)).length;
}

function visibleNotes() {
    return notesData().filter((note) => {
        const query = notesQuery().trim().toLowerCase();
        const matches =
            !query ||
            `${note.title} ${note.document} ${note.quote} ${note.full ?? ""}`
                .toLowerCase()
                .includes(query);
        return noteInScope(note, notesScope()) && matches;
    });
}

function selectedNote() {
    return notesData().find((note) => note.id === notesSelected());
}

function noteTruncated(note: Note) {
    return note.full !== note.note;
}

function noteBodyText(note: Note) {
    return notesBody()[note.id] ?? note.note;
}

function noteCitation(note: Note) {
    return `> ${note.quote}\n\n— 《${note.document}》${noteLocationLabel(note)}`;
}

function startEditingNote(note: Note) {
    notesEditingId.set(note.id);
    notesEditDraft.set(noteBodyText(note));
    notesCopyStatus.set("");
}

async function saveEditedNote(note: Note) {
    if (notesSaving()) return;
    const text = notesEditDraft().trim();
    notesSaving.set(true);
    try {
        const host = NotesHost as any;
        if (
            notesLive() &&
            note.captureId !== undefined &&
            typeof host?.updateNote === "function"
        ) {
            const raw = await host.updateNote(
                note.captureId,
                note.format ?? "pdf",
                text,
            );
            const result = JSON.parse(raw);
            if (!result?.ok) {
                throw new Error(String(result?.error ?? "修改失败"));
            }
            notesBody.set({ ...notesBody(), [note.id]: text });
            notesData.set(
                notesData().map((item) =>
                    item.id === note.id
                        ? {
                              ...item,
                              title: firstLine(text) || item.document,
                              note: text,
                              full: text,
                          }
                        : item,
                ),
            );
            notesEditingId.set("");
            notesCopyStatus.set("已保存修改到本机 Core。");
            void loadLiveNotes();
            return;
        }
        notesBody.set({ ...notesBody(), [note.id]: text });
        notesData.set(
            notesData().map((item) =>
                item.id === note.id
                    ? {
                          ...item,
                          title: firstLine(text) || item.document,
                          note: text,
                          full: text,
                      }
                    : item,
            ),
        );
        notesEditingId.set("");
        notesCopyStatus.set("已更新当前窗口内的想法。");
    } catch (error) {
        notesCopyStatus.set(
            `修改未保存：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    } finally {
        notesSaving.set(false);
    }
}

async function deleteNoteRecord(note: Note) {
    if (notesSaving()) return;
    notesSaving.set(true);
    try {
        const host = NotesHost as any;
        if (
            notesLive() &&
            note.captureId !== undefined &&
            typeof host?.deleteNote === "function"
        ) {
            const raw = await host.deleteNote(
                note.captureId,
                note.format ?? "pdf",
            );
            const result = JSON.parse(raw);
            if (!result?.ok) {
                throw new Error(String(result?.error ?? "删除失败"));
            }
        }
        notesData.set(notesData().filter((item) => item.id !== note.id));
        notesEditingId.set("");
        notesSelected.set("");
        notesSelection.set("");
        notesCopyStatus.set("已删除该条想法记录。");
        if (notesLive()) void loadLiveNotes();
    } catch (error) {
        notesCopyStatus.set(
            `删除未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    } finally {
        notesSaving.set(false);
    }
}

function noteMenuItems(note: Note): DesktopMenuItem[] {
    return [
        {
            label:
                note.captureId === undefined
                    ? "打开所在页（样例）"
                    : "打开所在页",
            disabled: note.missingOriginal,
            onSelect: () => openNoteSource(note),
        },
        { label: "复制引文", onSelect: () => noteCopy(note.quote, "引文") },
        {
            label: "复制想法",
            onSelect: () => noteCopy(noteBodyText(note), "想法"),
        },
        {
            label: "复制引用",
            onSelect: () => noteCopy(noteCitation(note), "引用"),
        },
        { type: "separator" },
        {
            label: noteTruncated(note) ? "读取完整内容" : "内容已完整",
            disabled: !noteTruncated(note),
            onSelect: () => loadNoteBody(note),
        },
        {
            label: "编辑想法",
            onSelect: () => {
                selectNote(note);
                startEditingNote(note);
            },
        },
        {
            label: "删除记录",
            onSelect: () => void deleteNoteRecord(note),
        },
    ];
}

function noteCopy(text: string, label: string) {
    void Clipboard.writeText(text)
        .then(() => notesCopyStatus.set(`已复制${label}。`))
        .catch(() =>
            notesCopyStatus.set("复制未完成，可选中文本手动复制。"),
        );
}

function openNoteSource(note: Note) {
    if (note.missingOriginal) {
        notesCopyStatus.set("原件缺失，无法定位到这一页。");
        return;
    }
    if (note.documentId !== undefined) {
        const opened = openLibrarySource({
            documentId: note.documentId,
            pageIndex: note.page - 1,
            title: note.document,
            quote: note.quote,
            quoteTruncated: note.quoteTruncated === true,
            format: note.format ?? "pdf",
        });
        if (!opened) {
            notesCopyStatus.set(
                "资料库插件当前不可用；引文与想法仍保留在本机 Core。",
            );
        }
        return;
    }
    if (note.scope !== "当前资料") {
        notesCopyStatus.set(
            `先打开《${note.document}》后可定位到${noteLocationLabel(note)}。`,
        );
        return;
    }
    notesCopyStatus.set(
        `已请求打开《${note.document}》${noteLocationLabel(note)}；真实回源由阅读插件承接。`,
    );
}

function loadNoteBody(note: Note) {
    if (notesBody()[note.id] || notesBodyLoading()) return;
    notesBodyLoading.set(note.id);
    if (notesLive() && note.captureId !== undefined) {
        const captureId = note.captureId;
        void (async () => {
            try {
                let offset = 0;
                let text = "";
                const host = NotesHost as any;
                const isEpub = note.format === "epub";
                if (isEpub && typeof host?.epubBody !== "function") {
                    throw Error("EPUB note body unavailable");
                }
                for (let chunk = 0; chunk < 64; chunk += 1) {
                    const raw = isEpub
                        ? await host.epubBody(captureId, "note", offset, 2048)
                        : await NotesHost.body(
                              captureId,
                              "note",
                              offset,
                              2048,
                          );
                    const result = JSON.parse(raw);
                    if (!result.ok || !result.value) throw Error();
                    text += String(result.value.text ?? "");
                    if (
                        result.value.next_offset === null ||
                        result.value.next_offset === undefined
                    )
                        break;
                    const next = Number(result.value.next_offset);
                    if (!Number.isSafeInteger(next) || next <= offset) break;
                    offset = next;
                }
                notesBody.set({ ...notesBody(), [note.id]: text });
            } catch (_) {
                notesCopyStatus.set(
                    "完整内容暂时读取不到；列表预览仍保留，可重试。",
                );
            } finally {
                notesBodyLoading.set("");
            }
        })();
        return;
    }
    setTimeout(() => {
        notesBody.set({ ...notesBody(), [note.id]: note.full ?? "" });
        notesBodyLoading.set("");
    }, 450);
}

function selectNote(note: Note) {
    notesSelected.set(note.id);
    notesSelection.set("");
    notesCopyStatus.set("");
    notesNavigation.set(false);
}

function moveNoteSelection(delta: number) {
    const list = visibleNotes();
    if (list.length === 0) return;
    const current = list.findIndex((note) => note.id === notesSelected());
    const base = current < 0 ? (delta > 0 ? -1 : 0) : current;
    const next = Math.max(0, Math.min(list.length - 1, base + delta));
    selectNote(list[next]);
}

function copyCurrentNote() {
    if (notesSelection()) {
        noteCopy(notesSelection(), "选中内容");
        return;
    }
    const note = selectedNote();
    if (note) noteCopy(noteBodyText(note), "想法");
}

function NotesSidebar(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="想法插件导航"
            style={{
                width: props.compact ? "100%" : 214,
                flexShrink: 0,
                borderRightWidth: props.compact ? 0 : 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-toolbar"),
            }}
        >
            <View
                style={{
                    height: 48,
                    paddingHorizontal: 14,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Badge icon="pen" size={28} tone="surface-accent" />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy strong>想法</Copy>
                    <Copy size={11} muted>
                        {notesSource() === "本机 Core"
                            ? "本机 Core"
                            : "工作区样例"}
                    </Copy>
                </View>
                <If when={props.compact}>
                    <Tool
                        label="返回想法列表"
                        icon="back"
                        text="想法"
                        size="touch"
                        onPress={() => notesNavigation.set(false)}
                    />
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 10, gap: 15 }}>
                    <View style={{ gap: 3 }}>
                        <Each of={["全部想法", "当前资料"]}>
                            {(scope) => (
                                <Button
                                    accessibilityLabel={`显示${scope()}想法`}
                                    onPress={() => {
                                        notesScope.set(scope());
                                        notesSelected.set("");
                                        notesSelection.set("");
                                        notesCopyStatus.set("");
                                        notesNavigation.set(false);
                                    }}
                                    style={{
                                        height: props.compact ? 44 : 34,
                                        paddingHorizontal: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        gap: 9,
                                        borderRadius: 7,
                                        backgroundColor: ink(
                                            notesScope() === scope()
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Glyph
                                        name={
                                            scope() === "当前资料"
                                                ? "document"
                                                : "grid"
                                        }
                                        size={16}
                                        color={ink(
                                            notesScope() === scope()
                                                ? "text-accent"
                                                : "text-secondary",
                                        )}
                                    />
                                    <Copy>{scope()}</Copy>
                                    <View style={{ flex: 1 }} />
                                    <Copy size={11} muted>
                                        {noteCount(scope())}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                    <View style={{ gap: 4 }}>
                        <View style={{ paddingHorizontal: 10 }}>
                            <Copy size={11} strong muted>
                                资料
                            </Copy>
                        </View>
                        <View
                            style={{
                                paddingHorizontal: 10,
                                paddingVertical: 5,
                                gap: 9,
                            }}
                        >
                            <Copy size={11} muted lines={2}>
                                准原生应用的秩序
                            </Copy>
                            <Copy size={11} muted lines={2}>
                                插件与宿主之间
                            </Copy>
                        </View>
                    </View>
                </View>
            </Scroll>
            <Rule />
            <View style={{ padding: 13 }}>
                <Copy size={11} muted lines={2}>
                    引用与个人观点分开 · {notesSource()}
                </Copy>
            </View>
        </View>
    );
}

function NotesList(props: { compact: boolean }) {
    const list = () => visibleNotes();
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 55,
                    paddingHorizontal: 18,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label="显示想法导航"
                        icon="list"
                        text="导航"
                        size="touch"
                        onPress={() => notesNavigation.set(true)}
                    />
                </If>
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>{notesScope()}</Copy>
                    <Copy size={11} muted>
                        {list().length} 条想法
                    </Copy>
                    <Copy size={10} muted>
                        {notesSource()} · {notesSourceDetail()}
                    </Copy>
                </View>
                <If when={!props.compact}>
                    <View style={{ width: 240 }}>
                        <Input
                            ref={(node) => {
                                notesSearchRef = node;
                            }}
                            accessibilityLabel="搜索想法、引文或资料"
                            value={notesQuery()}
                            onChange={({ value }) => notesQuery.set(value)}
                            placeholder="搜索想法、引文或资料"
                            style={{
                                height: 32,
                                paddingHorizontal: 10,
                                fontSize: 13,
                                color: ink("text-primary"),
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 7,
                            }}
                            focusStyle={focus()}
                        />
                    </View>
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 18, gap: 10 }}>
                    <Each of={list()} key={(note) => note.id}>
                        {(note) => {
                            const selected = () => notesSelected() === note().id;
                            return (

                                        <View
                                            accessibilityLabel={`《${note().document}》${noteLocationLabel(note())}：${note().title}`}
                                            onContextMenu={(event) => {
                                                selectNote(note());
                                                void DesktopMenu.popup(
                                                    noteMenuItems(note()),
                                                    event.pagePosition,
                                                );
                                            }}
                                            onPress={(event) => {
                                                if (event.clickCount === 2) {
                                                    openNoteSource(note());
                                                } else {
                                                    selectNote(note());
                                                }
                                            }}
                                            style={{
                                                padding: 15,
                                                gap: 10,
                                                borderWidth: 1,
                                                borderColor: ink(
                                                    selected()
                                                        ? "text-accent"
                                                        : "border-subtle",
                                                ),
                                                borderLeftWidth: selected() ? 3 : 1,
                                                borderRadius: 9,
                                                borderCurve: "continuous",
                                                backgroundColor: ink(
                                                    selected()
                                                        ? "surface-selected"
                                                        : "surface-preview",
                                                ),
                                                alignItems: "stretch",
                                            }}
                                            hoverStyle={{
                                                borderColor: ink("border-control"),
                                                backgroundColor: ink("surface-hover"),
                                            }}
                                            focusStyle={focus()}
                                        >
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    alignItems: "center",
                                                    gap: 9,
                                                }}
                                            >
                                                <Badge icon="pen" size={28} />
                                                <View
                                                    style={{
                                                        flex: 1,
                                                        minWidth: 0,
                                                        gap: 2,
                                                    }}
                                                >
                                                    <Copy strong>
                                                        {note().title}
                                                    </Copy>
                                                    <Copy size={11} muted>
                                                        {note().document} ·{" "}
                                                        {noteLocationLabel(
                                                            note(),
                                                        )}
                                                    </Copy>
                                                </View>
                                                <If
                                                    when={noteTruncated(note())}
                                                >
                                                    <Copy size={11} muted>
                                                        完整内容
                                                    </Copy>
                                                </If>
                                                <If
                                                    when={
                                                        note().missingOriginal
                                                    }
                                                >
                                                    <Copy size={11} muted>
                                                        原件缺失
                                                    </Copy>
                                                </If>
                                                <If when={note().documentId !== undefined}>
                                                    <Copy size={11} muted>
                                                        可回源
                                                    </Copy>
                                                </If>
                                            </View>
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    gap: 12,
                                                }}
                                            >
                                                <View
                                                    style={{
                                                        width: 3,
                                                        borderRadius: 2,
                                                        backgroundColor: ink(
                                                            "text-accent",
                                                        ),
                                                    }}
                                                />
                                                <View
                                                    style={{
                                                        flex: 1,
                                                        minWidth: 0,
                                                        paddingVertical: 2,
                                                    }}
                                                >
                                                    <Copy lines={2}>
                                                        {note().quote}
                                                    </Copy>
                                                </View>
                                            </View>
                                            <Copy size={12} muted lines={2}>
                                                {note().note}
                                            </Copy>
                                            <View
                                                style={{
                                                    flexDirection: "row",
                                                    alignItems: "center",
                                                }}
                                            >
                                                <Copy size={11} muted>
                                                    {note().date}
                                                </Copy>
                                                <View style={{ flex: 1 }} />
                                                <If when={note().starred}>
                                                    <Glyph
                                                        name="book"
                                                        size={13}
                                                        color={ink(
                                                            "text-accent",
                                                        )}
                                                    />
                                                </If>
                                            </View>
                                        </View>                            );
                        }}
                    </Each>
                    <If when={list().length === 0}>
                        <View style={{ padding: 28, gap: 8 }}>
                            <Copy strong>
                                {notesQuery().trim()
                                    ? "没有匹配的想法"
                                    : notesScope() === "当前资料"
                                      ? "当前资料还没有想法"
                                      : "还没有保存的想法"}
                            </Copy>
                            <Copy size={12} muted>
                                {notesQuery().trim()
                                    ? "清除搜索或切换分类后再试。"
                                    : "在正文中选文并写下想法，保存后会出现在这里。"}
                            </Copy>
                            <If when={!!notesQuery().trim()}>
                                <View style={{ flexDirection: "row" }}>
                                    <TextButton
                                        label="清除搜索"
                                        filled
                                        size={props.compact ? "touch" : "compact"}
                                        onPress={() => notesQuery.set("")}
                                    />
                                </View>
                            </If>
                        </View>
                    </If>
                </View>
            </Scroll>
        </View>
    );
}

function NoteDetail(props: { compact: boolean }) {
    const note = () => selectedNote();
    const canOpen = () =>
        !!note() &&
        !note()!.missingOriginal &&
        (note()!.documentId !== undefined ||
            note()!.scope === "当前资料");
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    minHeight: 55,
                    paddingHorizontal: props.compact ? 12 : 16,
                    paddingVertical: props.compact ? 6 : 0,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 9,
                }}
            >
                <Tool
                    label="返回想法列表"
                    icon="back"
                    text="想法列表"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => {
                        notesSelected.set("");
                        notesSelection.set("");
                        notesCopyStatus.set("");
                    }}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy strong>{note()?.document}</Copy>
                    <Copy size={11} muted>
                        {noteLocationLabel(note())} · 引用与个人观点分开
                    </Copy>
                </View>
                <Tool
                    label={
                        canOpen()
                            ? "打开所在页"
                            : note()?.missingOriginal
                                ? "原件缺失，无法定位"
                                : "来源信息不足"
                    }
                    text="打开所在页"
                    icon="book"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!canOpen()}
                    onPress={() => {
                        if (note()) openNoteSource(note()!);
                    }}
                />
                <Tool
                    label="编辑想法"
                    icon="edit"
                    text="编辑想法"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!note() || notesSaving()}
                    onPress={() => {
                        if (note()) startEditingNote(note()!);
                    }}
                />
                <Tool
                    label="删除记录"
                    icon="trash"
                    text="删除记录"
                    size={props.compact ? "touch" : "regular"}
                    disabled={!note() || notesSaving()}
                    onPress={() => {
                        if (note()) void deleteNoteRecord(note()!);
                    }}
                />
                <Menu
                    trigger={
                        <Tool
                            label="更多想法操作"
                            icon="more"
                            text="更多"
                            size={props.compact ? "touch" : "regular"}
                        />
                    }
                    popupStyle={menuPopup()}
                >
                    <MenuItem
                        style={menuItem()}
                        onSelect={() => note() && noteCopy(note()!.quote, "引文")}
                    >
                        <Copy>复制引文</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        onSelect={() =>
                            note() && noteCopy(noteBodyText(note()!), "想法")
                        }
                    >
                        <Copy>复制想法</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        onSelect={() =>
                            note() && noteCopy(noteCitation(note()!), "引用")
                        }
                    >
                        <Copy>复制引用</Copy>
                    </MenuItem>
                    <MenuSeparator />
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || !noteTruncated(note()!)}
                        onSelect={() => note() && loadNoteBody(note()!)}
                    >
                        <Copy>读取完整内容</Copy>
                    </MenuItem>
                    <MenuSeparator />
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || notesSaving()}
                        onSelect={() => note() && startEditingNote(note()!)}
                    >
                        <Copy>编辑想法</Copy>
                    </MenuItem>
                    <MenuItem
                        style={menuItem()}
                        disabled={!note() || notesSaving()}
                        onSelect={() =>
                            note() && void deleteNoteRecord(note()!)
                        }
                    >
                        <Copy>删除记录</Copy>
                    </MenuItem>
                </Menu>
            </View>
            <Rule />
            <If when={!!notesSelection()}>
                <View
                    style={{
                        paddingHorizontal: 18,
                        paddingVertical: 8,
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                >
                    <Copy size={11} muted>
                        已选取 {notesSelection().length} 个字符
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <TextButton
                        label="复制选中"
                        accessibilityLabel="复制选中文本"
                        filled
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => noteCopy(notesSelection(), "选中内容")}
                    />
                    <TextButton
                        label="复制为引用"
                        accessibilityLabel="把选中文本复制为引用"
                        filled
                        size={props.compact ? "touch" : "compact"}
                        onPress={() =>
                            note() &&
                            noteCopy(
                                `> ${notesSelection()}\n\n— 《${note()!.document}》${noteLocationLabel(note()!)}`,
                                "引用",
                            )
                        }
                    />
                    <TextButton
                        label="取消选择"
                        accessibilityLabel="取消文本选取"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => notesSelection.set("")}
                    />
                </View>
            </If>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        width: "100%",
                        maxWidth: 720,
                        alignSelf: "center",
                        paddingHorizontal: 34,
                        paddingTop: 34,
                        paddingBottom: 70,
                        gap: 18,
                    }}
                >
                    <Copy size={12} muted>
                        {note()?.date}
                    </Copy>
                    <Copy size={27} strong>
                        {note()?.title}
                    </Copy>
                    <View style={{ flexDirection: "row", gap: 14 }}>
                        <View
                            style={{
                                width: 3,
                                borderRadius: 2,
                                backgroundColor: ink("text-accent"),
                            }}
                        />
                        <View
                            style={{
                                flex: 1,
                                minWidth: 0,
                                paddingVertical: 4,
                            }}
                        >
                            <TextView
                                markdown={note()?.quote}
                                selectable
                                onSelectionChange={({ text }) =>
                                    notesSelection.set(text)
                                }
                            />
                        </View>
                    </View>
                    <Rule />
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 8,
                        }}
                    >
                        <Copy size={11} strong muted>
                            我的想法
                        </Copy>
                        <View style={{ flex: 1 }} />
                        <If when={notesEditingId() !== note()?.id}>
                            <TextButton
                                label="编辑想法"
                                size={props.compact ? "touch" : "compact"}
                                filled
                                disabled={!note() || notesSaving()}
                                onPress={() => {
                                    if (note()) startEditingNote(note()!);
                                }}
                            />
                            <TextButton
                                label="删除记录"
                                size={props.compact ? "touch" : "compact"}
                                disabled={!note() || notesSaving()}
                                onPress={() => {
                                    if (note()) void deleteNoteRecord(note()!);
                                }}
                            />
                        </If>
                    </View>
                    <If when={notesEditingId() === note()?.id}>
                        <View style={{ gap: 10 }}>
                            <Input
                                accessibilityLabel="编辑想法内容"
                                multiline
                                value={notesEditDraft()}
                                onChange={({ value }) =>
                                    notesEditDraft.set(value)
                                }
                                placeholder="输入修改后的想法"
                                style={{
                                    minHeight: 96,
                                    padding: 12,
                                    fontSize: 15,
                                    lineHeight: 25,
                                    color: ink("text-primary"),
                                    backgroundColor: ink("surface-inset"),
                                    borderWidth: 1,
                                    borderColor: ink("border-control"),
                                    borderRadius: 8,
                                }}
                                focusStyle={focus()}
                            />
                            <View
                                style={{
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 8,
                                }}
                            >
                                <Tool
                                    label="保存修改"
                                    text={
                                        notesSaving()
                                            ? "正在保存…"
                                            : "保存修改"
                                    }
                                    raised
                                    size={props.compact ? "touch" : "regular"}
                                    disabled={notesSaving()}
                                    onPress={() => {
                                        if (note())
                                            void saveEditedNote(note()!);
                                    }}
                                />
                                <TextButton
                                    label="取消"
                                    size={props.compact ? "touch" : "regular"}
                                    disabled={notesSaving()}
                                    onPress={() => notesEditingId.set("")}
                                />
                            </View>
                        </View>
                    </If>
                    <If when={notesEditingId() !== note()?.id}>
                        <TextView
                            markdown={noteBodyText(note()!)}
                            selectable
                            onSelectionChange={({ text }) =>
                                notesSelection.set(text)
                            }
                        />
                    </If>
                    <If
                        when={
                            !!note() &&
                            noteTruncated(note()!) &&
                            !notesBody()[note()!.id]
                        }
                    >
                        <View
                            style={{
                                padding: 12,
                                gap: 8,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 8,
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                列表只显示开头，完整内容仍保存在本机。
                            </Copy>
                            <View style={{ flexDirection: "row" }}>
                                <TextButton
                                    label={
                                        notesBodyLoading() === note()!.id
                                            ? "正在读取…"
                                            : "读取完整内容"
                                    }
                                    accessibilityLabel="读取完整想法内容"
                                    raised
                                    size={props.compact ? "touch" : "compact"}
                                    disabled={notesBodyLoading() === note()!.id}
                                    onPress={() => loadNoteBody(note()!)}
                                />
                            </View>
                        </View>
                    </If>
                    <If when={!!notesCopyStatus()}>
                        <Copy size={11} muted>
                            {notesCopyStatus()}
                        </Copy>
                    </If>
                    <View
                        style={{
                            marginTop: 8,
                            paddingTop: 16,
                            borderTopWidth: 1,
                            borderColor: ink("border-subtle"),
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <If when={note()?.missingOriginal}>
                            <Copy size={11} muted>
                                原件缺失，无法定位到这一页；引文与想法仍可复制。
                            </Copy>
                        </If>
                        <If
                            when={
                                !note()?.missingOriginal &&
                                note()?.documentId === undefined &&
                                note()?.scope !== "当前资料"
                            }
                        >
                            <Copy size={11} muted>
                                先打开《{note()?.document}》后可定位到这一页。
                            </Copy>
                        </If>
                        <If when={canOpen()}>
                            <Copy size={11} muted>
                                通过资料库插件打开已保存的来源页；不可用时会明确反馈。
                            </Copy>
                        </If>
                    </View>
                </View>
            </Scroll>
        </View>
    );
}

function NotesWorkspace(props: { compact: boolean }) {
    effect(() => {
        if (
            !props.compact ||
            (!notesSelected() && !notesNavigation() && !notesSelection())
        )
            return;
        BackHandler.on("press", () => {
            if (notesSelection()) {
                notesSelection.set("");
                return true;
            }
            if (notesSelected()) {
                notesSelected.set("");
                notesCopyStatus.set("");
                return true;
            }
            if (notesNavigation()) {
                notesNavigation.set(false);
                return true;
            }
            return false;
        });
    });
    keymap.bind({
        up: () => moveNoteSelection(-1),
        down: () => moveNoteSelection(1),
        home: () => {
            const list = visibleNotes();
            if (list.length > 0) selectNote(list[0]);
        },
        end: () => {
            const list = visibleNotes();
            if (list.length > 0) selectNote(list[list.length - 1]);
        },
        enter: () => {
            const note = selectedNote();
            if (note) openNoteSource(note);
        },
        "mod+c": () => copyCurrentNote(),
        "mod+shift+c": () => {
            const note = selectedNote();
            if (note) noteCopy(noteCitation(note), "引用");
        },
        "mod+f": () => notesSearchRef?.focus(),
        "shift+f10": () => {
            const note = selectedNote();
            if (note) {
                selectNote(note);
                void DesktopMenu.popup(noteMenuItems(note));
            }
        },
        escape: () => {
            if (notesSelection()) {
                notesSelection.set("");
                return;
            }
            notesCopyStatus.set("");
            if (notesSelected()) notesSelected.set("");
        },
    });
    return (
        <View
            accessibilityLabel="想法插件工作区"
            style={{
                flex: 1,
                minWidth: 0,
                flexDirection: "row",
                backgroundColor: ink("surface-base"),
            }}
        >
            <If when={!props.compact}>
                <NotesSidebar compact={false} />
                <If when={!!selectedNote()}>
                    <NoteDetail compact={false} />
                </If>
                <If when={!selectedNote()}>
                    <NotesList compact={false} />
                </If>
            </If>
            <If when={props.compact && notesNavigation()}>
                <NotesSidebar compact={true} />
            </If>
            <If when={props.compact && !notesNavigation() && !selectedNote()}>
                <NotesList compact={true} />
            </If>
            <If when={props.compact && !notesNavigation() && !!selectedNote()}>
                <NoteDetail compact={true} />
            </If>
        </View>
    );
}
// ---------------------------------------------------------------------------
// 背单词工作区：词书、学习卡与四档评分。
// The fixture lives in this window; when the host exposes the vocabulary module
// the workspace reads the local Core first and falls back here, saying so.
// ---------------------------------------------------------------------------
type VocabCard = {
    cardId: number;
    term: string;
    meaning: string;
    example: string;
    stage: string;
    pos?: string;
    phonetic?: string;
};
type VocabWordRow = {
    id: number;
    term: string;
    meaning: string;
    example: string;
    pos?: string;
    phonetic?: string;
};
type VocabDeck = {
    id: number;
    title: string;
    total: number;
    due: number;
    new: number;
};
type VocabOperation = {
    cardId: number;
    rating: string;
    id: string;
    attempts: number;
};
const vocabDecksFixture: VocabDeck[] = [
    { id: 1, title: "CET-4 示例", total: 3, due: 0, new: 3 },
    { id: 2, title: "学术写作 示例", total: 2, due: 0, new: 2 },
];
const vocabCardsFixture: Record<number, VocabCard[]> = {
    1: [
        {
            cardId: 1,
            term: "abandon",
            meaning: "放弃；抛弃",
            example: "They had to abandon the plan when the funding was withdrawn.",
            stage: "new",
        },
        {
            cardId: 2,
            term: "benefit",
            meaning: "好处；使受益",
            example: "Daily review benefits long-term memory more than one long session.",
            stage: "new",
        },
        {
            cardId: 3,
            term: "candidate",
            meaning: "候选人；应试者",
            example: "Each candidate read the passage twice before answering.",
            stage: "new",
        },
    ],
    2: [
        {
            cardId: 4,
            term: "coherent",
            meaning: "连贯的；一致的",
            example: "A coherent argument states its claim before the evidence.",
            stage: "new",
        },
        {
            cardId: 5,
            term: "cite",
            meaning: "引用；举例",
            example: "Cite the original page instead of a summary of it.",
            stage: "new",
        },
    ],
};
/// The four grades the vocabulary wire accepts, with the FSRS-5 interval the
/// spec expects for a new card.
const vocabRatings = [
    { rating: "again", label: "不认识", hint: "约 10 分钟" },
    { rating: "hard", label: "模糊", hint: "约 30 分钟" },
    { rating: "good", label: "认识", hint: "约 3 天" },
    { rating: "easy", label: "很熟", hint: "约 16 天" },
];
const vocabStageLabels: Record<string, string> = {
    new: "新词",
    learning: "学习中",
    review: "复习",
};
const vocabDecks = signal<VocabDeck[]>([]);
const vocabDeck = signal(0);
const vocabQueue = signal<VocabCard[]>([]);
const vocabRemainingDue = signal(0);
const vocabRemainingNew = signal(0);
const vocabLoading = signal(true);
const vocabReady = signal(false);
const vocabBusy = signal(false);
const vocabError = signal("");
const vocabNotice = signal("");
const vocabLive = signal(false);
const vocabSource = signal("样例数据");
const vocabSourceDetail = signal("窗口内样例");
const vocabOperation = signal<VocabOperation | null>(null);
const vocabRevealed = signal(false);
/// The accent the sample pronunciation service reads and shows: a window
/// preference, not Core state.
const vocabAccent = signal("us");
/// The text the system voice is reading right now (drives the speaker state).
const vocabSpeaking = signal("");
const vocabMobileView = signal<"decks" | "study" | "words">("decks");
const vocabDailyNewLimit = signal(10);
const vocabAutoSpeak = signal(false);
const vocabSlowSpeak = signal(false);
const vocabWordsDeckId = signal(0);
const vocabWordsList = signal<VocabWordRow[]>([]);
const vocabWordsLoading = signal(false);
const VOCAB_DAILY_LIMIT_KEY = "qusor.vocab.daily-new-limit.v1";
const VOCAB_PRONOUNCE_PREFS_KEY = "qusor.vocab.pronounce-prefs.v1";
const VOCAB_DAILY_LIMIT_OPTIONS = [5, 10, 20, 30] as const;
const MAX_WORDBOOK_IMPORT_BYTES = 2 * 1024 * 1024;

async function restoreVocabPreferences() {
    try {
        const rawLimit = await Storage.getItem(VOCAB_DAILY_LIMIT_KEY);
        if (rawLimit) {
            const parsed = Number(JSON.parse(rawLimit)?.limit);
            if (
                VOCAB_DAILY_LIMIT_OPTIONS.includes(
                    parsed as (typeof VOCAB_DAILY_LIMIT_OPTIONS)[number],
                )
            ) {
                vocabDailyNewLimit.set(parsed);
            }
        }
    } catch (_) {}
    try {
        const rawPrefs = await Storage.getItem(VOCAB_PRONOUNCE_PREFS_KEY);
        if (rawPrefs) {
            const parsed = JSON.parse(rawPrefs);
            if (typeof parsed?.autoSpeak === "boolean") {
                vocabAutoSpeak.set(parsed.autoSpeak);
            }
            if (typeof parsed?.slowSpeak === "boolean") {
                vocabSlowSpeak.set(parsed.slowSpeak);
            }
        }
    } catch (_) {}
}

async function changeVocabDailyNewLimit(limit: number) {
    vocabDailyNewLimit.set(limit);
    try {
        await Storage.setItem(
            VOCAB_DAILY_LIMIT_KEY,
            JSON.stringify({ limit }),
        );
    } catch (_) {}
    if (vocabDeck() > 0 && vocabLive()) {
        void loadVocabQueue(vocabDeck());
    }
}

async function toggleVocabAutoSpeak() {
    const next = !vocabAutoSpeak();
    vocabAutoSpeak.set(next);
    try {
        await Storage.setItem(
            VOCAB_PRONOUNCE_PREFS_KEY,
            JSON.stringify({
                autoSpeak: next,
                slowSpeak: vocabSlowSpeak(),
            }),
        );
    } catch (_) {}
}

async function toggleVocabSlowSpeak() {
    const next = !vocabSlowSpeak();
    vocabSlowSpeak.set(next);
    try {
        await Storage.setItem(
            VOCAB_PRONOUNCE_PREFS_KEY,
            JSON.stringify({
                autoSpeak: vocabAutoSpeak(),
                slowSpeak: next,
            }),
        );
    } catch (_) {}
}

function vocabDeckOf(id: number) {
    return vocabDecks().find((deck) => deck.id === id);
}
function currentVocabCard() {
    return vocabQueue()[0];
}
function vocabRatingLabel(rating: string) {
    const entry = vocabRatings.find((item) => item.rating === rating);
    return entry ? entry.label : rating;
}
function vocabStageLabel(stage: string) {
    return vocabStageLabels[stage] ?? "复习";
}
function vocabAccentLabel(accent: string) {
    return accent === "uk" ? "英音" : "美音";
}
/// Switching the reading never touches Core: it changes the IPA shown and the
/// voice the speaker asks for.
function chooseVocabAccent(accent: string) {
    if (vocabAccent() === accent) return;
    vocabAccent.set(accent);
    vocabSpeaking.set("");
    vocabNotice.set("");
}
function vocabNoticeForMissingVoice() {
    vocabNotice.set("当前宿主没有系统读音；先照音标读一遍再揭晓。");
}
/// The sample host asks a system voice to read a term or an example. A window
/// without that host says so instead of pretending it played something.
async function speakVocabText(text: string, label: string) {
    const spoken = text.trim();
    if (!spoken) return;
    vocabError.set("");
    const host = VocabHost as {
        pronounce?: (text: string, accent: string) => string;
        pronounceWithRate?: (
            text: string,
            accent: string,
            rate: number,
        ) => string;
    };
    const hasPronounceWithRate = typeof host.pronounceWithRate === "function";
    const hasPronounce = typeof host.pronounce === "function";
    if (!hasPronounceWithRate && !hasPronounce) {
        vocabNoticeForMissingVoice();
        return;
    }
    try {
        const accent = vocabAccent();
        const rate = vocabSlowSpeak() ? 0.68 : 1.0;
        const raw = hasPronounceWithRate
            ? await host.pronounceWithRate!(spoken, accent, rate)
            : await host.pronounce!(spoken, accent);
        const value = vocabAnswer(raw) as {
            voice?: string | null;
        };
        const voice = value.voice ? `（${value.voice}）` : "";
        vocabSpeaking.set(spoken);
        vocabNotice.set(`正在朗读${label} · ${vocabAccentLabel(accent)}${voice}`);
        setTimeout(() => {
            if (vocabSpeaking() === spoken) vocabSpeaking.set("");
        }, 1600);
    } catch (error) {
        if (/runtime unavailable/i.test(vocabFailureText(error))) {
            vocabNoticeForMissingVoice();
            return;
        }
        vocabNotice.set(`朗读没有开始：${vocabFailureText(error)}`);
    }
}
function speakVocabWord() {
    const card = currentVocabCard();
    if (card) void speakVocabText(card.term, `「${card.term}」`);
}
function speakVocabExample() {
    const card = currentVocabCard();
    if (card && vocabRevealed()) void speakVocabText(card.example, "例句");
}
function vocabFailureText(error: unknown) {
    if (error && typeof error === "object" && "message" in error)
        return String((error as { message: unknown }).message);
    return String(error ?? "未知错误");
}
/// One user action keeps one operation id: a retry of the same click must not
/// schedule the card twice, so the id is only minted when the click is new.
function vocabOperationId(card: VocabCard, rating: string) {
    const nonce = Math.random().toString(36).slice(2, 10);
    return `vocab-rate-${card.cardId}-${rating}-${Date.now().toString(36)}-${nonce}`;
}
function vocabAnswer(raw: string) {
    const result = JSON.parse(raw);
    if (!result || result.ok !== true)
        throw new Error(String(result?.error ?? "本机 Core 未返回结果"));
    return result.value ?? {};
}
function mapVocabCard(wire: any): VocabCard {
    return {
        cardId: Number(wire.card_id),
        term: String(wire.term ?? ""),
        meaning: String(wire.meaning ?? ""),
        example: String(wire.example ?? ""),
        stage: String(wire.stage ?? "new"),
        pos:
            typeof wire.pos === "string" && wire.pos.trim()
                ? wire.pos.trim()
                : undefined,
        phonetic:
            typeof wire.phonetic === "string" && wire.phonetic.trim()
                ? wire.phonetic.trim()
                : undefined,
    };
}
function mapVocabDeck(wire: any): VocabDeck {
    return {
        id: Number(wire.wordbook_id),
        title: String(wire.title ?? "词书"),
        total: Number(wire.total ?? 0),
        due: Number(wire.due ?? 0),
        new: Number(wire.new ?? 0),
    };
}
/// The sidebar shows the book's own due/new counters, so a committed review
/// asks the Core for them again instead of trusting the window's copy.
async function refreshVocabDecks() {
    if (!vocabLive()) return;
    try {
        const value = vocabAnswer(await VocabHost.decks());
        const decks: VocabDeck[] = Array.isArray(value.decks)
            ? value.decks.map(mapVocabDeck)
            : [];
        if (decks.length > 0) vocabDecks.set(decks);
    } catch (_) {
        // The review already went through; keep the last readable counters.
    }
}
/// The fixture has no Core behind it, so one graded card lowers the same
/// counter the deck row shows. Card stage decides which of due/new moves.
function decrementVocabDeck(card: VocabCard) {
    const id = vocabDeck();
    vocabDecks.set(
        vocabDecks().map((deck) => {
            if (deck.id !== id) return deck;
            if (card.stage === "new")
                return { ...deck, new: Math.max(0, deck.new - 1) };
            return { ...deck, due: Math.max(0, deck.due - 1) };
        }),
    );
}
function revealVocabAnswer() {
    if (currentVocabCard()) setVocabRevealed(true);
}
/// Hiding the answer drops the speaker state too, so the next card never shows
/// the previous term as "reading".
function setVocabRevealed(revealed: boolean) {
    vocabRevealed.set(revealed);
    if (!revealed) vocabSpeaking.set("");
}
function loadVocabFixtureQueue(id: number) {
    // Picking a sample wordbook starts its sample queue again, so the deck
    // row and the remaining counters both come from the fixture cards.
    const deck = vocabDecksFixture.find((item) => item.id === id);
    const cards = vocabCardsFixture[id] ?? [];
    vocabQueue.set(cards.map((card) => ({ ...card })));
    setVocabRevealed(false);
    vocabRemainingDue.set(cards.filter((card) => card.stage !== "new").length);
    vocabRemainingNew.set(cards.filter((card) => card.stage === "new").length);
    if (deck)
        vocabDecks.set(
            vocabDecks().map((item) => (item.id === id ? { ...deck } : item)),
        );
    vocabLoading.set(false);
    vocabBusy.set(false);
    vocabReady.set(true);
}
/// Any failed Core read falls back to the window fixture. The header then says
/// 样例数据 instead of 本机 Core and the reason stays visible.
function useVocabFixture(reason: string) {
    const previous = vocabDeck();
    const sameDeck = vocabDecksFixture.find((deck) => deck.id === previous);
    vocabLive.set(false);
    vocabSource.set("样例数据");
    vocabSourceDetail.set("窗口内样例");
    vocabDecks.set(vocabDecksFixture);
    vocabDeck.set(sameDeck ? sameDeck.id : 0);
    vocabQueue.set([]);
    setVocabRevealed(false);
    vocabRemainingDue.set(0);
    vocabRemainingNew.set(0);
    vocabOperation.set(null);
    vocabError.set(reason);
    vocabNotice.set("");
    vocabLoading.set(false);
    vocabBusy.set(false);
    vocabReady.set(true);
    if (sameDeck) loadVocabFixtureQueue(sameDeck.id);
}
async function loadVocabDecks(retrying = false) {
    vocabLoading.set(true);
    try {
        const value = vocabAnswer(await VocabHost.decks());
        const decks: VocabDeck[] = Array.isArray(value.decks)
            ? value.decks.map(mapVocabDeck)
            : [];
        if (decks.length === 0) throw new Error("词书列表为空");
        vocabDecks.set(decks);
        vocabLive.set(true);
        vocabSource.set("本机 Core");
        vocabSourceDetail.set("由 vocab 插件读取");
        vocabDeck.set(0);
        vocabQueue.set([]);
        setVocabRevealed(false);
        vocabRemainingDue.set(0);
        vocabRemainingNew.set(0);
        vocabError.set("");
    } catch (_) {
        // Headless tests and a plain development host have no native module:
        // the fixture stays and the header names it as a sample.
        vocabDecks.set(vocabDecksFixture);
        vocabLive.set(false);
        vocabSource.set("样例数据");
        vocabSourceDetail.set("窗口内样例");
        vocabDeck.set(0);
        vocabQueue.set([]);
        setVocabRevealed(false);
        vocabRemainingDue.set(0);
        vocabRemainingNew.set(0);
        vocabError.set(
            retrying ? "仍读不到本机 Core，继续使用样例数据。" : "",
        );
    }
    vocabLoading.set(false);
    vocabReady.set(true);
}
function retryVocabDecks() {
    vocabError.set("");
    vocabLoading.set(true);
    void loadVocabDecks(true);
}

async function importVocabWordbook() {
    if (vocabBusy()) return;
    vocabError.set("");
    vocabNotice.set("");
    vocabBusy.set(true);
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "QUSOR 词书", extensions: ["json"] }],
        });
        if (picked.length === 0) {
            vocabBusy.set(false);
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_WORDBOOK_IMPORT_BYTES) {
            throw new Error("词书 JSON 不能超过 2 MiB");
        }
        const wordbookJson = await file.text();
        if (!wordbookJson.trim()) throw new Error("词书 JSON 是空的");
        const value = vocabAnswer(
            await VocabHost.importWordbook(wordbookJson),
        );
        await loadVocabDecks();
        if (!vocabLive()) {
            vocabBusy.set(false);
            vocabNotice.set("词书已写入 Core，但列表读取失败；可重试读取。");
            return;
        }
        const wordbookId = Number(value.wordbook_id);
        vocabBusy.set(false);
        if (Number.isSafeInteger(wordbookId) && wordbookId > 0) {
            selectVocabDeck(wordbookId);
        }
        vocabNotice.set(
            `已导入「${String(value.title ?? "词书")}」 · ${Number(value.total ?? 0)} 张卡`,
        );
    } catch (error) {
        vocabBusy.set(false);
        vocabError.set(`导入词书失败：${vocabFailureText(error)}`);
    }
}

async function loadVocabQueue(id: number) {
    try {
        const host = VocabHost as any;
        const limit = vocabDailyNewLimit();
        const raw =
            typeof host?.queueWithLimit === "function"
                ? await host.queueWithLimit(id, limit)
                : await VocabHost.queue(id, limit);
        const value = vocabAnswer(raw);
        const cards: VocabCard[] = Array.isArray(value.cards)
            ? value.cards.map(mapVocabCard)
            : [];
        vocabQueue.set(cards);
        setVocabRevealed(false);
        vocabRemainingDue.set(Number(value.remaining_due ?? 0));
        vocabRemainingNew.set(Number(value.remaining_new ?? 0));
        vocabError.set("");
        if (vocabAutoSpeak() && cards[0]) {
            void speakVocabText(cards[0].term, `「${cards[0].term}」`);
        }
    } catch (error) {
        useVocabFixture(
            `复习队列读取失败（${vocabFailureText(error)}），已回退样例数据。`,
        );
    }
}
function selectVocabDeck(id: number) {
    vocabDeck.set(id);
    vocabMobileView.set("study");
    vocabQueue.set([]);
    setVocabRevealed(false);
    vocabNotice.set("");
    vocabError.set("");
    vocabOperation.set(null);
    vocabLoading.set(true);
    vocabBusy.set(true);
    if (vocabLive()) {
        void (async () => {
            await loadVocabQueue(id);
            vocabLoading.set(false);
            vocabBusy.set(false);
        })();
        return;
    }
    loadVocabFixtureQueue(id);
    const first = vocabQueue()[0];
    if (vocabAutoSpeak() && first) {
        void speakVocabText(first.term, `「${first.term}」`);
    }
}

async function openVocabWordbookCards(deckId: number) {
    vocabWordsDeckId.set(deckId);
    vocabMobileView.set("words");
    vocabWordsLoading.set(true);
    vocabError.set("");
    try {
        const host = VocabHost as any;
        if (vocabLive() && typeof host?.wordbookCards === "function") {
            const value = vocabAnswer(await host.wordbookCards(deckId, 200, 0));
            const rows: VocabWordRow[] = Array.isArray(value.cards)
                ? value.cards.map((raw: any) => ({
                      id: Number(raw.id ?? raw.card_id ?? 0),
                      term: String(raw.term ?? ""),
                      meaning: String(raw.meaning ?? ""),
                      example: String(raw.example ?? ""),
                      pos:
                          typeof raw.pos === "string" && raw.pos.trim()
                              ? raw.pos.trim()
                              : undefined,
                      phonetic:
                          typeof raw.phonetic === "string" &&
                          raw.phonetic.trim()
                              ? raw.phonetic.trim()
                              : undefined,
                  }))
                : [];
            vocabWordsList.set(rows);
            return;
        }
        const fallback = (vocabCardsFixture[deckId] ?? []).map((card) => ({
            id: card.cardId,
            term: card.term,
            meaning: card.meaning,
            example: card.example,
            pos: card.pos,
            phonetic: card.phonetic,
        }));
        vocabWordsList.set(fallback);
    } catch (error) {
        vocabError.set(`读取词表未完成：${vocabFailureText(error)}`);
        vocabWordsList.set([]);
    } finally {
        vocabWordsLoading.set(false);
    }
}

async function deleteVocabWordbook(deckId: number) {
    if (vocabBusy()) return;
    const targetDeck = vocabDeckOf(deckId);
    vocabBusy.set(true);
    vocabError.set("");
    vocabNotice.set("");
    try {
        const host = VocabHost as any;
        if (vocabLive() && typeof host?.deleteWordbook === "function") {
            vocabAnswer(await host.deleteWordbook(deckId));
            await loadVocabDecks();
        } else {
            vocabDecks.set(
                vocabDecks().filter((item) => item.id !== deckId),
            );
        }
        if (vocabDeck() === deckId) {
            vocabDeck.set(0);
            vocabQueue.set([]);
        }
        if (vocabWordsDeckId() === deckId) {
            vocabWordsDeckId.set(0);
            vocabWordsList.set([]);
        }
        vocabMobileView.set("decks");
        vocabNotice.set(`已删除词书「${targetDeck?.title ?? deckId}」`);
    } catch (error) {
        vocabError.set(`删除词书未完成：${vocabFailureText(error)}`);
    } finally {
        vocabBusy.set(false);
    }
}
function vocabScheduledText(minutes: number) {
    if (!Number.isFinite(minutes) || minutes <= 0) return "稍后";
    if (minutes < 60) return `${Math.round(minutes)} 分钟后`;
    if (minutes < 1440) return `${Math.round(minutes / 60)} 小时后`;
    return `${(minutes / 1440).toFixed(1)} 天后`;
}
async function gradeVocabCard(rating: string) {
    const card = vocabQueue()[0];
    // A grade is a self-assessment of a shown answer: the answer has to be
    // visible first, so a stray digit cannot rate a card the user never saw.
    if (!card || vocabBusy() || !vocabRevealed()) return;
    if (!vocabLive()) {
        vocabQueue.set(vocabQueue().filter((item) => item.cardId !== card.cardId));
        if (card.stage === "new")
            vocabRemainingNew.set(Math.max(0, vocabRemainingNew() - 1));
        else vocabRemainingDue.set(Math.max(0, vocabRemainingDue() - 1));
        decrementVocabDeck(card);
        setVocabRevealed(false);
        vocabOperation.set(null);
        vocabError.set("");
        vocabNotice.set(
            `已记录：${vocabRatingLabel(rating)} · 样例队列不写入 Core`,
        );
        return;
    }
    const previous = vocabOperation();
    const reuse =
        !!previous &&
        previous.cardId === card.cardId &&
        previous.rating === rating;
    const operation: VocabOperation =
        reuse && previous
            ? { ...previous, attempts: previous.attempts + 1 }
            : {
                  cardId: card.cardId,
                  rating,
                  id: vocabOperationId(card, rating),
                  attempts: 1,
              };
    vocabOperation.set(operation);
    vocabBusy.set(true);
    vocabError.set("");
    vocabNotice.set("");
    let failure = "";
    try {
        const value = vocabAnswer(
            await VocabHost.review(card.cardId, rating, operation.id),
        );
        vocabOperation.set(null);
        vocabRemainingDue.set(Number(value.remaining_due ?? vocabRemainingDue()));
        vocabRemainingNew.set(Number(value.remaining_new ?? vocabRemainingNew()));
        vocabQueue.set(vocabQueue().filter((item) => item.cardId !== card.cardId));
        setVocabRevealed(false);
        vocabNotice.set(
            `已记录：${vocabRatingLabel(rating)} · 下次 ${vocabScheduledText(
                Number(value.scheduled_minutes ?? 0),
            )}`,
        );
        await refreshVocabDecks();
        await loadVocabQueue(vocabDeck());
    } catch (error) {
        failure = vocabFailureText(error);
    } finally {
        vocabBusy.set(false);
    }
    if (!failure) return;
    if (operation.attempts >= 2) {
        useVocabFixture(
            `评分未能写入本机 Core（${failure}），已回退样例数据。`,
        );
        return;
    }
    vocabError.set(`评分未保存：${failure}`);
    vocabNotice.set("重试会复用同一次评分的操作编号，不会重复安排这张卡。");
}
function retryVocabReview() {
    const operation = vocabOperation();
    if (!operation) {
        vocabError.set("");
        return;
    }
    void gradeVocabCard(operation.rating);
}
let vocabBooted = false;
function bootVocabWorkspace() {
    if (vocabBooted) return;
    vocabBooted = true;
    void restoreVocabPreferences();
    setTimeout(() => void loadVocabDecks(), 0);
}
async function settleVocab() {
    for (let attempt = 0; attempt < 60 && vocabBusy(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
}
/// Evidence-only, driven by App when the host was started with
/// QUSOR_WORKBENCH_EVIDENCE and the launcher asked for the vocabulary target.
export async function captureVocabEvidence() {
    let dir = "";
    try {
        const result = JSON.parse(await VocabHost.evidence());
        if (!result.ok || !result.value?.active) return;
        if (String(result.value.target ?? "") !== "vocab") return;
        dir = result.value.dir;
    } catch (_) {
        return;
    }
    if (!dir) return;
    for (let attempt = 0; attempt < 40 && !vocabReady(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/decks.png`);
    const deck = vocabDecks()[0];
    if (!deck) return;
    selectVocabDeck(deck.id);
    for (let attempt = 0; attempt < 40 && !currentVocabCard(); attempt += 1)
        await new Promise((resolve) => setTimeout(resolve, 100));
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/study.png`);
    if (!currentVocabCard()) return;
    speakVocabWord();
    await new Promise((resolve) => setTimeout(resolve, 400));
    await VocabHost.capture(`${dir}/spoken.png`);
    chooseVocabAccent("uk");
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/accent.png`);
    chooseVocabAccent("us");
    revealVocabAnswer();
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/revealed.png`);
    await gradeVocabCard("good");
    await settleVocab();
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/reviewed.png`);
    for (let round = 0; round < 40 && currentVocabCard(); round += 1) {
        revealVocabAnswer();
        await gradeVocabCard("good");
        await settleVocab();
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
    await VocabHost.capture(`${dir}/done.png`);
}

function VocabDeckRow(props: { deck: VocabDeck; compact: boolean }) {
    const selected = () => vocabDeck() === props.deck.id;
    return (
        <View
            style={{
                borderRadius: 9,
                borderCurve: "continuous",
                borderWidth: props.compact ? 1 : 0,
                borderColor: ink(
                    selected() ? "border-control" : "border-subtle",
                ),
                backgroundColor: ink(
                    selected()
                        ? "surface-selected"
                        : props.compact
                          ? "surface-preview"
                          : "transparent",
                ),
                padding: props.compact ? 12 : 4,
                gap: props.compact ? 10 : 4,
            }}
        >
            <Button
                accessibilityLabel={`选择词书 ${props.deck.title}`}
                onPress={() => selectVocabDeck(props.deck.id)}
                style={{
                    minHeight: 44,
                    paddingHorizontal: 8,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    borderRadius: 7,
                    borderCurve: "continuous",
                }}
                hoverStyle={{ backgroundColor: ink("surface-hover") }}
                activeStyle={{ backgroundColor: ink("surface-pressed") }}
                focusStyle={focus()}
            >
                <Glyph
                    name="book"
                    size={16}
                    color={ink(selected() ? "text-accent" : "text-secondary")}
                />
                <View style={{ flex: 1, gap: 2, minWidth: 0 }}>
                    <Copy size={props.compact ? 15 : 13} strong={props.compact}>
                        {props.deck.title}
                    </Copy>
                    <Copy size={11} muted>
                        {`共 ${props.deck.total} · 待复习 ${props.deck.due} · 新词 ${props.deck.new}`}
                    </Copy>
                </View>
            </Button>
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 6,
                    paddingHorizontal: 8,
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label={`开始学习 ${props.deck.title}`}
                        text="开始学习"
                        raised
                        size="touch"
                        onPress={() => selectVocabDeck(props.deck.id)}
                    />
                </If>
                <TextButton
                    label="词表"
                    accessibilityLabel={`查看 ${props.deck.title} 词表`}
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openVocabWordbookCards(props.deck.id)}
                />
                <TextButton
                    label="删除"
                    accessibilityLabel={`删除词书 ${props.deck.title}`}
                    size={props.compact ? "touch" : "compact"}
                    disabled={vocabBusy()}
                    onPress={() => void deleteVocabWordbook(props.deck.id)}
                />
            </View>
        </View>
    );
}

function VocabDeckList(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="背单词词书导航"
            style={{
                width: props.compact ? "100%" : 228,
                flex: props.compact ? 1 : 0,
                flexShrink: 0,
                borderRightWidth: props.compact ? 0 : 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink(
                    props.compact ? "surface-base" : "surface-toolbar",
                ),
            }}
        >
            <View
                style={{
                    minHeight: 52,
                    paddingHorizontal: props.compact ? 16 : 14,
                    paddingVertical: 8,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 9,
                }}
            >
                <Badge icon="book" size={28} tone="surface-accent" />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy size={props.compact ? 16 : 13} strong>
                        词书与设置
                    </Copy>
                    <Copy size={11} muted>
                        {`${vocabDecks().length} 本词书 · ${vocabSourceDetail()}`}
                    </Copy>
                </View>
                <Tool
                    label="导入词书 JSON"
                    icon="plus"
                    text={props.compact ? "导入词书 JSON" : "导入"}
                    raised={props.compact}
                    size={props.compact ? "touch" : "compact"}
                    disabled={vocabBusy()}
                    onPress={() => void importVocabWordbook()}
                />
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 16 : 10,
                        gap: props.compact ? 14 : 8,
                    }}
                >
                    <View
                        style={{
                            padding: 12,
                            gap: 10,
                            borderRadius: 9,
                            borderCurve: "continuous",
                            borderWidth: 1,
                            borderColor: ink("border-subtle"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <View
                            style={{
                                flexDirection: "row",
                                alignItems: "center",
                                flexWrap: "wrap",
                                gap: 6,
                            }}
                        >
                            <Copy size={12} strong>
                                每日新词上限
                            </Copy>
                            <View style={{ flex: 1 }} />
                            <Each of={[...VOCAB_DAILY_LIMIT_OPTIONS]}>
                                {(limit) => (
                                    <TextButton
                                        label={`${limit()}`}
                                        accessibilityLabel={`每日新词上限 ${limit()}`}
                                        size={props.compact ? "touch" : "compact"}
                                        raised={
                                            vocabDailyNewLimit() === limit()
                                        }
                                        filled={
                                            vocabDailyNewLimit() !== limit()
                                        }
                                        onPress={() =>
                                            void changeVocabDailyNewLimit(
                                                limit(),
                                            )
                                        }
                                    />
                                )}
                            </Each>
                        </View>
                        <View
                            style={{
                                flexDirection: "row",
                                alignItems: "center",
                                flexWrap: "wrap",
                                gap: 8,
                            }}
                        >
                            <Copy size={12} muted>
                                读音偏好
                            </Copy>
                            <View style={{ flex: 1 }} />
                            <TextButton
                                label={
                                    vocabAutoSpeak()
                                        ? "自动朗读：开"
                                        : "自动朗读：关"
                                }
                                size={props.compact ? "touch" : "compact"}
                                raised={vocabAutoSpeak()}
                                filled={!vocabAutoSpeak()}
                                onPress={() => void toggleVocabAutoSpeak()}
                            />
                            <TextButton
                                label={
                                    vocabSlowSpeak() ? "慢速：开" : "慢速：关"
                                }
                                size={props.compact ? "touch" : "compact"}
                                raised={vocabSlowSpeak()}
                                filled={!vocabSlowSpeak()}
                                onPress={() => void toggleVocabSlowSpeak()}
                            />
                        </View>
                    </View>
                    <If when={!!vocabNotice()}>
                        <Copy size={12} muted>
                            {vocabNotice()}
                        </Copy>
                    </If>
                    <If when={!!vocabError()}>
                        <Copy size={12} muted>
                            {vocabError()}
                        </Copy>
                    </If>
                    <If when={vocabDecks().length === 0}>
                        <View style={{ paddingVertical: 18 }}>
                            <Copy size={12} muted>
                                正在读取词书…
                            </Copy>
                        </View>
                    </If>
                    <View style={{ gap: props.compact ? 10 : 4 }}>
                        <Each
                            of={vocabDecks()}
                            key={(deck) => `vocab-deck-${deck.id}`}
                        >
                            {(deck) => (
                                <VocabDeckRow
                                    deck={deck()}
                                    compact={props.compact}
                                />
                            )}
                        </Each>
                    </View>
                </View>
            </Scroll>
            <If when={!props.compact}>
                <Rule />
                <View style={{ padding: 13, gap: 4 }}>
                    <Copy size={11} muted lines={2}>
                        评分由 FSRS-5 安排下次复习 · 卸载插件后学习数据仍在 Core
                    </Copy>
                    <Copy size={11} muted lines={2}>
                        音标来自词书或样例资源 · 读音由系统语音朗读
                    </Copy>
                </View>
            </If>
        </View>
    );
}

function VocabDeckPlaceholder() {
    return (
        <View
            style={{
                flexGrow: 1,
                alignItems: "center",
                justifyContent: "center",
                gap: 10,
                paddingVertical: 40,
            }}
        >
            <Badge icon="book" size={52} tone="surface-accent" />
            <Copy size={20} strong>
                选择一本词书
            </Copy>
            <Copy muted>
                {`共 ${vocabDecks().length} 本词书 · 来源：${vocabSource()}`}
            </Copy>
            <Copy size={11} muted>
                先清到期复习，再按今日新词预算学习
            </Copy>
        </View>
    );
}

function VocabDoneState() {
    return (
        <View
            style={{
                flexGrow: 1,
                alignItems: "center",
                justifyContent: "center",
                gap: 10,
                paddingVertical: 40,
            }}
        >
            <Badge icon="clock" size={52} tone="surface-accent" />
            <Copy size={22} strong>
                今天完成
            </Copy>
            <Copy muted>这本词书的队列已经清空，明天再来；不会自动扩大新词预算。</Copy>
            <Copy size={11} muted>
                {`来源：${vocabSource()}`}
            </Copy>
        </View>
    );
}

/// One accent of the sample reading. The chip only changes which IPA shows and
/// which system voice the speaker asks for; Core keeps none of it.
function VocabAccentChip(props: {
    accent: string;
    label: string;
    compact?: boolean;
}) {
    const selected = () => vocabAccent() === props.accent;
    return (
        <Button
            accessibilityLabel={props.label}
            onPress={() => chooseVocabAccent(props.accent)}
            style={{
                height: props.compact ? 44 : 24,
                paddingHorizontal: props.compact ? 14 : 8,
                alignItems: "center",
                justifyContent: "center",
                borderRadius: props.compact ? 10 : 6,
                borderCurve: "continuous",
                backgroundColor: ink(
                    selected() ? "surface-selected" : "transparent",
                ),
                borderWidth: 1,
                borderColor: ink(
                    selected() ? "border-control" : "border-subtle",
                ),
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <Copy size={props.compact ? 13 : 11} muted={!selected()} strong={selected()}>
                {props.label}
            </Copy>
        </Button>
    );
}

/// The part of speech and the phonetic of the selected accent. A term the
/// sample books do not cover says so instead of showing an invented phonetic.
function VocabPhoneticRow(props: { compact: boolean }) {
    const card = () => currentVocabCard();
    const entry = () => pronunciationOf(card()?.term ?? "");
    const posText = () => card()?.pos || entry()?.pos || "";
    const phoneticText = () =>
        card()?.phonetic ||
        (entry() ? phoneticOf(card()?.term ?? "", vocabAccent()) : "");
    return (
        <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            <If when={!!posText()}>
                <Copy size={props.compact ? 12 : 13} muted>
                    {posText()}
                </Copy>
            </If>
            <If when={!!phoneticText()}>
                <Copy size={props.compact ? 13 : 14} muted>
                    {phoneticText()}
                </Copy>
            </If>
            <If when={!posText() && !phoneticText() && !props.compact}>
                <Copy size={11} muted>
                    音标未收录 · 由词书资源提供
                </Copy>
            </If>
            <View style={{ flex: 1 }} />
            <VocabAccentChip accent="us" label="美音" compact={props.compact} />
            <VocabAccentChip accent="uk" label="英音" compact={props.compact} />
        </View>
    );
}

function VocabCardView(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel={`学习卡片 ${currentVocabCard()!.term}`}
            style={{
                alignSelf: "center",
                width: "100%",
                maxWidth: 720,
                padding: props.compact ? 20 : 26,
                gap: 14,
                borderWidth: 1,
                borderColor: ink("border-subtle"),
                borderRadius: 12,
                borderCurve: "continuous",
                backgroundColor: ink("surface-preview"),
            }}
        >
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 8,
                }}
            >
                <Badge icon="book" size={26} />
                <Copy size={11} muted strong>
                    {vocabStageLabel(currentVocabCard()!.stage)}
                </Copy>
                <View style={{ flex: 1 }} />
                <Copy size={11} muted>
                    {`队列 ${vocabQueue().length} 张`}
                </Copy>
            </View>
            <View
                style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                }}
            >
                <Copy size={props.compact ? 32 : 40} strong>
                    {currentVocabCard()!.term}
                </Copy>
                <Button
                    accessibilityLabel={`朗读 ${currentVocabCard()!.term}`}
                    onPress={speakVocabWord}
                    style={{
                        width: props.compact ? 44 : 36,
                        height: props.compact ? 44 : 36,
                        alignItems: "center",
                        justifyContent: "center",
                        borderRadius: 9,
                        borderCurve: "continuous",
                        backgroundColor: ink("surface-control"),
                        borderWidth: 1,
                        borderColor: ink(
                            vocabSpeaking()
                                ? "border-control"
                                : "border-subtle",
                        ),
                    }}
                    hoverStyle={{
                        backgroundColor: ink("surface-hover"),
                        borderColor: ink("border-control"),
                    }}
                    activeStyle={{ backgroundColor: ink("surface-pressed") }}
                    focusStyle={focus()}
                >
                    <Glyph
                        name="speaker"
                        size={18}
                        color={ink(
                            vocabSpeaking()
                                ? "text-accent"
                                : "text-secondary",
                        )}
                    />
                </Button>
            </View>
            <VocabPhoneticRow compact={props.compact} />
            <If when={!vocabRevealed()}>
                <Copy size={13} muted>
                    先回想这个词的意思，再显示答案。
                </Copy>
                <Copy size={11} muted>
                    释义、例句与四档评分在显示答案后出现。
                </Copy>
            </If>
            <If when={vocabRevealed()}>
                <Copy size={17}>{currentVocabCard()!.meaning}</Copy>
                <View style={{ flexDirection: "row", gap: 10 }}>
                    <View
                        style={{
                            width: 3,
                            borderRadius: 2,
                            backgroundColor: ink("border-control"),
                        }}
                    />
                    <View style={{ flex: 1, gap: 6 }}>
                        <Copy size={14} muted lines={3}>
                            {currentVocabCard()!.example}
                        </Copy>
                        <View style={{ flexDirection: "row" }}>
                            <TextButton
                                label="朗读例句"
                                size={props.compact ? "touch" : "compact"}
                                filled
                                onPress={speakVocabExample}
                            />
                        </View>
                    </View>
                </View>
            </If>
        </View>
    );
}

function VocabActionBar(props: { compact: boolean }) {
    return (
        <View style={{ flexShrink: 0 }}>
            <Rule />
            <If when={!vocabRevealed()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 16 : 22,
                        paddingVertical: 12,
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                    }}
                >
                    <Button
                        accessibilityLabel="显示答案"
                        onPress={revealVocabAnswer}
                        style={{
                            height: props.compact ? 46 : 44,
                            alignItems: "center",
                            justifyContent: "center",
                            borderRadius: 8,
                            borderCurve: "continuous",
                            backgroundColor: ink("surface-accent"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                        }}
                        hoverStyle={{ backgroundColor: ink("surface-hover") }}
                        activeStyle={{ backgroundColor: ink("surface-pressed") }}
                        focusStyle={focus()}
                    >
                        <Copy size={14} strong>
                            显示答案
                        </Copy>
                    </Button>
                    <If when={!props.compact}>
                        <Copy size={11} muted>
                            空格或回车显示答案 · R 朗读单词；只看词条，先自己回想
                        </Copy>
                    </If>
                </View>
            </If>
            <If when={vocabRevealed()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 16 : 22,
                        paddingVertical: 12,
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                    }}
                >
                    <View style={{ flexDirection: "row", gap: 8 }}>
                        <Each of={vocabRatings}>
                            {(rating) => (
                                <Button
                                    accessibilityLabel={rating().label}
                                    disabled={vocabBusy()}
                                    onPress={() =>
                                        void gradeVocabCard(rating().rating)
                                    }
                                    style={{
                                        flex: 1,
                                        height: props.compact ? 46 : 44,
                                        justifyContent: "center",
                                        alignItems: "center",
                                        gap: 1,
                                        borderRadius: 8,
                                        borderCurve: "continuous",
                                        backgroundColor: ink("surface-control"),
                                        borderWidth: 1,
                                        borderColor: ink("border-subtle"),
                                        opacity: vocabBusy() ? 0.4 : 1,
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                        borderColor: ink("border-control"),
                                    }}
                                    activeStyle={{
                                        backgroundColor: ink("surface-pressed"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <Copy size={13} strong>
                                        {rating().label}
                                    </Copy>
                                    <Copy size={10} muted>
                                        {rating().hint}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                    <If when={!props.compact}>
                        <Copy size={11} muted>
                            {vocabLive()
                                ? "评分写入本机 Core · 数字键 1–4 · R 朗读"
                                : "样例数据不写入 Core · 数字键 1–4 · R 朗读"}
                        </Copy>
                    </If>
                </View>
            </If>
        </View>
    );
}

function VocabWordsView(props: { compact: boolean }) {
    const deck = () => vocabDeckOf(vocabWordsDeckId());
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: 54,
                    paddingHorizontal: props.compact ? 14 : 22,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <Tool
                    label="返回词书列表"
                    icon="back"
                    text="词书列表"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() =>
                        vocabMobileView.set(props.compact ? "decks" : "study")
                    }
                />
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>{deck()?.title ?? "词表"} · 全部词条</Copy>
                    <Copy size={11} muted>
                        {`共 ${vocabWordsList().length} 个词条`}
                    </Copy>
                </View>
                <If when={!!deck()}>
                    <Tool
                        label="开始学习此词书"
                        text="开始学习"
                        raised
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => selectVocabDeck(vocabWordsDeckId())}
                    />
                </If>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: props.compact ? 16 : 22, gap: 10 }}>
                    <If when={vocabWordsLoading()}>
                        <Copy muted>正在读取词表…</Copy>
                    </If>
                    <Each
                        of={vocabWordsList()}
                        key={(item) => `word-row-${item.id}-${item.term}`}
                    >
                        {(item) => {
                            const sample = pronunciationOf(item().term);
                            const pos = item().pos || sample?.pos || "";
                            const phonetic =
                                item().phonetic ||
                                (sample
                                    ? phoneticOf(item().term, vocabAccent())
                                    : "");
                            return (
                                <View
                                    style={{
                                        padding: 14,
                                        gap: 6,
                                        borderRadius: 10,
                                        borderCurve: "continuous",
                                        borderWidth: 1,
                                        borderColor: ink("border-subtle"),
                                        backgroundColor: ink("surface-preview"),
                                    }}
                                >
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 8,
                                        }}
                                    >
                                        <Copy size={16} strong>
                                            {item().term}
                                        </Copy>
                                        <If when={!!pos}>
                                            <Copy size={12} muted>
                                                {pos}
                                            </Copy>
                                        </If>
                                        <If when={!!phonetic}>
                                            <Copy size={12} muted>
                                                {phonetic}
                                            </Copy>
                                        </If>
                                        <View style={{ flex: 1 }} />
                                        <Tool
                                            label={`朗读 ${item().term}`}
                                            icon="speaker"
                                            size={props.compact ? "touch" : "compact"}
                                            onPress={() =>
                                                void speakVocabText(
                                                    item().term,
                                                    `「${item().term}」`,
                                                )
                                            }
                                        />
                                    </View>
                                    <Copy size={14}>{item().meaning}</Copy>
                                    <If when={!!item().example}>
                                        <Copy size={12} muted lines={2}>
                                            {item().example}
                                        </Copy>
                                    </If>
                                </View>
                            );
                        }}
                    </Each>
                </View>
            </Scroll>
        </View>
    );
}

function VocabStudy(props: { compact: boolean }) {
    return (
        <View style={{ flex: 1, minWidth: 0 }}>
            <View
                style={{
                    height: props.compact ? 54 : 58,
                    paddingHorizontal: props.compact ? 14 : 22,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 10,
                    backgroundColor: ink("surface-base"),
                }}
            >
                <If when={props.compact}>
                    <Tool
                        label="返回词书列表"
                        icon="back"
                        text="词书列表"
                        size="touch"
                        onPress={() => vocabMobileView.set("decks")}
                    />
                </If>
                <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                    <Copy strong>
                        {vocabDeckOf(vocabDeck())?.title ?? "背单词"}
                    </Copy>
                    <Copy size={11} muted>
                        {`${vocabSource()} · ${vocabSourceDetail()}`}
                    </Copy>
                </View>
                <View style={{ alignItems: "flex-end", gap: 1 }}>
                    <Copy size={11} muted>
                        {`待复习 ${vocabRemainingDue()}`}
                    </Copy>
                    <Copy size={11} muted>
                        {`今日新词 ${vocabRemainingNew()}`}
                    </Copy>
                </View>
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }} contentStyle={{ flexGrow: 1 }}>
                <View
                    style={{
                        flexGrow: 1,
                        justifyContent: "center",
                        padding: props.compact ? 18 : 26,
                        gap: 14,
                    }}
                >
                    <If when={!!vocabError()}>
                        <View
                            style={{
                                alignSelf: "center",
                                width: "100%",
                                maxWidth: 720,
                                padding: 12,
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 9,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 9,
                                borderCurve: "continuous",
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Glyph
                                name="info"
                                size={16}
                                color={ink("text-accent")}
                            />
                            <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                                <Copy size={12} lines={2}>
                                    {vocabError()}
                                </Copy>
                                <If when={!!vocabOperation()}>
                                    <Copy size={11} muted>
                                        重试复用同一次评分的操作编号。
                                    </Copy>
                                </If>
                            </View>
                            <If when={!!vocabOperation()}>
                                <TextButton
                                    label="重试评分"
                                    size={props.compact ? "touch" : "compact"}
                                    filled
                                    onPress={retryVocabReview}
                                />
                            </If>
                            <If when={!vocabOperation()}>
                                <TextButton
                                    label="重试读取"
                                    size={props.compact ? "touch" : "compact"}
                                    filled
                                    onPress={retryVocabDecks}
                                />
                            </If>
                        </View>
                    </If>
                    <If when={vocabLoading()}>
                        <View
                            style={{
                                flexGrow: 1,
                                alignItems: "center",
                                justifyContent: "center",
                                gap: 8,
                                paddingVertical: 40,
                            }}
                        >
                            <Copy muted>正在读取学习队列…</Copy>
                        </View>
                    </If>
                    <If when={!vocabLoading() && !vocabDeckOf(vocabDeck())}>
                        <VocabDeckPlaceholder />
                    </If>
                    <If
                        when={
                            !vocabLoading() &&
                            !!vocabDeckOf(vocabDeck()) &&
                            !currentVocabCard()
                        }
                    >
                        <VocabDoneState />
                    </If>
                    <If
                        when={
                            !vocabLoading() &&
                            !!vocabDeckOf(vocabDeck()) &&
                            !!currentVocabCard()
                        }
                    >
                        <VocabCardView compact={props.compact} />
                    </If>
                    <If when={!!vocabNotice()}>
                        <View
                            style={{
                                alignSelf: "center",
                                width: "100%",
                                maxWidth: 720,
                            }}
                        >
                            <Copy size={11} muted>
                                {vocabNotice()}
                            </Copy>
                        </View>
                    </If>
                </View>
            </Scroll>
            <If when={!vocabLoading() && !!currentVocabCard()}>
                <VocabActionBar compact={props.compact} />
            </If>
        </View>
    );
}

function VocabWorkspace(props: { compact: boolean }) {
    bootVocabWorkspace();
    effect(() => {
        if (!props.compact || vocabMobileView() === "decks") return;
        BackHandler.on("press", () => {
            if (vocabMobileView() !== "decks") {
                vocabMobileView.set("decks");
                return true;
            }
            return false;
        });
    });
    keymap.bind({
        space: () => revealVocabAnswer(),
        enter: () => revealVocabAnswer(),
        r: () => speakVocabWord(),
        "1": () => void gradeVocabCard("again"),
        "2": () => void gradeVocabCard("hard"),
        "3": () => void gradeVocabCard("good"),
        "4": () => void gradeVocabCard("easy"),
    });
    return (
        <View
            accessibilityLabel="背单词插件工作区"
            style={{
                flex: 1,
                minWidth: 0,
                flexDirection: props.compact ? "column" : "row",
                backgroundColor: ink("surface-base"),
            }}
        >
            <If when={!props.compact}>
                <VocabDeckList compact={false} />
                <If when={vocabMobileView() === "words"}>
                    <VocabWordsView compact={false} />
                </If>
                <If when={vocabMobileView() !== "words"}>
                    <VocabStudy compact={false} />
                </If>
            </If>
            <If when={props.compact && vocabMobileView() === "decks"}>
                <VocabDeckList compact={true} />
            </If>
            <If when={props.compact && vocabMobileView() === "words"}>
                <VocabWordsView compact={true} />
            </If>
            <If when={props.compact && vocabMobileView() === "study"}>
                <VocabStudy compact={true} />
            </If>
        </View>
    );
}

type LibraryRow = {
    document_id: number;
    title: string;
    page_count: number;
    original_bytes: number;
    commit_rev: number;
};
type EpubRow = {
    document_id: number;
    title: string;
    author: string;
    chapter_count: number;
    original_bytes: number;
    commit_rev: number;
};
type TocEntry = {
    title: string;
    index: number;
    depth: number;
};
type LibrarySourceTarget = {
    serial: number;
    documentId: number;
    pageIndex: number;
    title: string;
    quote: string;
    quoteTruncated: boolean;
    format?: "pdf" | "epub";
};

const libraryRows = signal<LibraryRow[]>([]);
const libraryEpubRows = signal<EpubRow[]>([]);
const libraryPositions = signal<Record<number, ReadingPosition>>({});
const libraryEpubPositions = signal<Record<number, EpubPosition>>({});
const libraryStatus = signal("正在读取本机资料库…");
const libraryNext = signal("");
const libraryImporting = signal(false);
const libraryOpenDoc = signal<LibraryRow | null>(null);
const libraryOpenEpub = signal<EpubRow | null>(null);
const libraryViewMode = signal<"image" | "text">("image");
const libraryImmersive = signal(false);
const libraryTocOpen = signal(false);
const libraryTocEntries = signal<TocEntry[]>([]);
const libraryPageJumpInput = signal("");
const libraryNoteSheetOpen = signal(false);
const libraryPageIndex = signal(0);
const libraryPageText = signal("");
const libraryPageImage = signal<{
    documentId: number;
    pageIndex: number;
    url: string;
    width: number;
    height: number;
} | null>(null);
const libraryPageImageState = signal<"idle" | "loading" | "ready" | "failed">(
    "idle",
);
const libraryPageGeometry = signal<PageGeometry | null>(null);
const libraryPageStatus = signal("");
const libraryResumeNotice = signal("");
const librarySelection = signal("");
const librarySelectionStart = signal(0);
const librarySelectionScalarRange = signal<{
    start: number;
    end: number;
} | null>(null);
const librarySelectionBoxes = signal<
    [number, number, number, number][]
>([]);
const librarySavedSelections = signal<SavedSelectionRange[]>([]);
const librarySelectionNote = signal("");
const libraryZoom = signal(1);
const libraryReaderWidth = signal(0);
const librarySourceTarget = signal<LibrarySourceTarget | null>(null);
const librarySourceQuote = signal<{
    text: string;
    truncated: boolean;
} | null>(null);
const epubChapterIndex = signal(0);
const epubChapterTitle = signal("");
const epubChapterText = signal("");
const epubChapterStatus = signal("");
const epubSavedRanges = signal<SavedSelectionRange[]>([]);
const epubSelection = signal("");
const epubSelectionScalarStart = signal(0);
const epubSelectionNote = signal("");
const epubTocOpen = signal(false);
const epubTocEntries = signal<TocEntry[]>([]);
const epubTypographyOpen = signal(false);
const epubNoteSheetOpen = signal(false);
const epubFontSize = signal(DEFAULT_EPUB_FONT_SIZE);
const epubLineHeight = signal(DEFAULT_EPUB_LINE_HEIGHT);
const EPUB_TYPOGRAPHY_KEY = "qusor.library.epub-typography.v1";
const EPUB_POS_PREFIX = "qusor.library.epub-position.v1.";
let libraryBooted = false;
let nextLibrarySourceTarget = 0;
let handledLibrarySourceTarget = 0;
let libraryImageRequest = 0;
let librarySelectionAnchor: number | null = null;
let librarySelectionPointer: number | null = null;
let libraryDragMode: "new" | "start" | "end" = "new";
let libraryPageDisplay = { width: 0, height: 0 };
let librarySavedSelectionsRequest = 0;
let libraryZoomSaveRequest = 0;
const MAX_PDF_IMPORT_BYTES = 16 * 1024 * 1024;
const MAX_EPUB_IMPORT_BYTES = 16 * 1024 * 1024;

async function restoreEpubTypography() {
    try {
        const raw = await Storage.getItem(EPUB_TYPOGRAPHY_KEY);
        if (!raw) return;
        const parsed = JSON.parse(raw);
        epubFontSize.set(normalizeEpubFontSize(parsed?.fontSize));
        epubLineHeight.set(normalizeEpubLineHeight(parsed?.lineHeight));
    } catch (_) {}
}

async function saveEpubTypography(fontSize: number, lineHeight: number) {
    const nextFont = normalizeEpubFontSize(fontSize);
    const nextLine = normalizeEpubLineHeight(lineHeight);
    epubFontSize.set(nextFont);
    epubLineHeight.set(nextLine);
    try {
        await Storage.setItem(
            EPUB_TYPOGRAPHY_KEY,
            JSON.stringify({ fontSize: nextFont, lineHeight: nextLine }),
        );
    } catch (_) {}
}

async function loadEpubBookPosition(
    documentId: number,
    chapterCount: number,
): Promise<EpubPosition | null> {
    try {
        const raw = await Storage.getItem(`${EPUB_POS_PREFIX}${documentId}`);
        return parseEpubPosition(raw, chapterCount);
    } catch (_) {
        return null;
    }
}

async function saveEpubBookPosition(
    documentId: number,
    chapterIndex: number,
    scrollRatio = 0,
): Promise<EpubPosition> {
    const pos: EpubPosition = { chapterIndex, scrollRatio };
    try {
        await Storage.setItem(
            `${EPUB_POS_PREFIX}${documentId}`,
            formatEpubPosition(pos),
        );
    } catch (_) {}
    libraryEpubPositions.set({
        ...libraryEpubPositions(),
        [documentId]: pos,
    });
    return pos;
}

async function collectSelectionToVocab(
    rawText: string,
    contextText: string,
    sourceTitle: string,
    onStatus: (msg: string) => void,
) {
    const term = rawText.trim().split(/\s+/)[0]?.slice(0, 64) ?? "";
    if (!term) {
        onStatus("请先选择要收录的词条。");
        return;
    }
    const snippet = rawText.trim().slice(0, 180) || contextText.slice(0, 180);
    const payload = JSON.stringify({
        title: "阅读生词本",
        cards: [
            {
                term,
                meaning: `来自《${sourceTitle}》摘录`,
                example: snippet,
            },
        ],
    });
    try {
        vocabAnswer(await VocabHost.importWordbook(payload));
        void loadVocabDecks();
        onStatus(`已将「${term}」收录到「阅读生词本」词书。`);
    } catch (error) {
        onStatus(`收词未完成：${vocabFailureText(error)}`);
    }
}

async function restoreLibraryZoom() {
    const request = libraryZoomSaveRequest;
    const zoom = await loadReadingZoom();
    if (request === libraryZoomSaveRequest) libraryZoom.set(zoom);
}

async function changeLibraryZoom(zoom: number) {
    const normalized = normalizeReadingZoom(zoom);
    libraryZoom.set(normalized);
    const request = ++libraryZoomSaveRequest;
    const saved = await saveReadingZoom(normalized);
    if (request === libraryZoomSaveRequest) libraryZoom.set(saved);
}

function libraryRow(raw: any): LibraryRow | null {
    if (
        typeof raw?.document_id !== "number" ||
        typeof raw?.title !== "string" ||
        typeof raw?.page_count !== "number" ||
        typeof raw?.original_bytes !== "number" ||
        typeof raw?.commit_rev !== "number"
    ) {
        return null;
    }
    return raw;
}

function epubRow(raw: any): EpubRow | null {
    if (
        typeof raw?.document_id !== "number" ||
        typeof raw?.title !== "string" ||
        typeof raw?.chapter_count !== "number"
    ) {
        return null;
    }
    return {
        document_id: raw.document_id,
        title: raw.title,
        author: typeof raw.author === "string" ? raw.author : "",
        chapter_count: raw.chapter_count,
        original_bytes:
            typeof raw.original_bytes === "number" ? raw.original_bytes : 0,
        commit_rev: typeof raw.commit_rev === "number" ? raw.commit_rev : 0,
    };
}

async function refreshLibraryPositions(rows: LibraryRow[]) {
    const positions: Record<number, ReadingPosition> = {};
    await Promise.all(
        rows.map(async (row) => {
            const position = await loadReadingPosition(
                row.document_id,
                row.page_count,
            );
            if (position) positions[row.document_id] = position;
        }),
    );
    libraryPositions.set(positions);
}

async function refreshEpubPositions(rows: EpubRow[]) {
    const positions: Record<number, EpubPosition> = {};
    await Promise.all(
        rows.map(async (row) => {
            const pos = await loadEpubBookPosition(
                row.document_id,
                row.chapter_count,
            );
            if (pos) positions[row.document_id] = pos;
        }),
    );
    libraryEpubPositions.set(positions);
}

function libraryPositionOf(row: LibraryRow): ReadingPosition | null {
    return libraryPositions()[row.document_id] ?? null;
}

function libraryPositionLabel(row: LibraryRow): string {
    const position = libraryPositionOf(row);
    return position ? ` · 上次读到第 ${position.page + 1} 页` : "";
}

function epubPositionOf(row: EpubRow): EpubPosition | null {
    return libraryEpubPositions()[row.document_id] ?? null;
}

function epubPositionLabel(row: EpubRow): string {
    const pos = epubPositionOf(row);
    return pos ? ` · 上次读到第 ${pos.chapterIndex + 1} 章` : "";
}

function openLibrarySource(
    target: Omit<LibrarySourceTarget, "serial">,
): boolean {
    const previous = librarySourceTarget();
    librarySourceTarget.set({
        ...target,
        serial: ++nextLibrarySourceTarget,
    });
    if (pluginWorkspaceOpener?.("library_workspace")) return true;
    librarySourceTarget.set(previous);
    return false;
}

async function loadLibraryPage(cursor = "") {
    libraryStatus.set("正在读取本机资料库…");
    let pdfLoaded = false;
    let epubLoaded = false;
    try {
        const response = JSON.parse(await LibraryHost.page(cursor, 6));
        if (response.ok && Array.isArray(response.value?.documents)) {
            const rows: LibraryRow[] = [];
            for (const raw of response.value.documents) {
                const row = libraryRow(raw);
                if (row) rows.push(row);
            }
            libraryRows.set(rows);
            await refreshLibraryPositions(rows);
            libraryNext.set(
                typeof response.value.next_cursor === "string"
                    ? response.value.next_cursor
                    : "",
            );
            pdfLoaded = true;
        }
    } catch (_) {
        libraryRows.set([]);
        libraryNext.set("");
    }
    try {
        const host = LibraryHost as any;
        if (typeof host?.epubPage === "function") {
            const epubRaw = await host.epubPage("", 20);
            const epubRes = JSON.parse(epubRaw);
            if (epubRes?.ok && Array.isArray(epubRes.value?.items)) {
                const books: EpubRow[] = [];
                for (const raw of epubRes.value.items) {
                    const row = epubRow(raw);
                    if (row) books.push(row);
                }
                libraryEpubRows.set(books);
                await refreshEpubPositions(books);
                epubLoaded = true;
            }
        }
    } catch (_) {
        libraryEpubRows.set([]);
    }
    const totalCount = libraryRows().length + libraryEpubRows().length;
    if (pdfLoaded || epubLoaded) {
        libraryStatus.set(
            totalCount > 0
                ? "本机 Core · 只显示资料元数据"
                : "本机 Core 还没有导入 PDF 或 EPUB 资料",
        );
    } else {
        libraryStatus.set("本机资料库暂不可用；已收录内容没有被删除");
    }
}

async function deleteLibraryDocument(
    documentId: number,
    format: "pdf" | "epub",
    title: string,
) {
    const host = LibraryHost as any;
    if (typeof host?.deleteDocument !== "function") {
        libraryStatus.set("当前宿主尚未启用资料删除接口。");
        return;
    }
    try {
        const raw = await host.deleteDocument(documentId, format);
        const res = JSON.parse(raw);
        if (!res?.ok) {
            throw new Error(String(res?.error ?? "删除失败"));
        }
        if (libraryOpenDoc()?.document_id === documentId && format === "pdf") {
            clearLibraryPageImage();
            libraryOpenDoc.set(null);
        }
        if (
            libraryOpenEpub()?.document_id === documentId &&
            format === "epub"
        ) {
            libraryOpenEpub.set(null);
        }
        await loadLibraryPage();
        libraryStatus.set(`已从本机资料库移除《${title}》。`);
    } catch (error) {
        libraryStatus.set(
            `删除《${title}》未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

function clearLibraryPageImage() {
    ++libraryImageRequest;
    const previous = libraryPageImage();
    libraryPageImage.set(null);
    libraryPageImageState.set("idle");
    libraryPageGeometry.set(null);
    libraryPageDisplay = { width: 0, height: 0 };
    librarySavedSelections.set([]);
    ++librarySavedSelectionsRequest;
    clearLibrarySelection();
    if (previous?.url) URL.revokeObjectURL(previous.url);
}

function clearLibrarySelection() {
    librarySelection.set("");
    librarySelectionStart.set(0);
    librarySelectionScalarRange.set(null);
    librarySelectionBoxes.set([]);
    librarySelectionAnchor = null;
    librarySelectionPointer = null;
    libraryDragMode = "new";
    libraryNoteSheetOpen.set(false);
}

function librarySelectionGeometry(
    displayX: number,
    displayY: number,
): { geometry: PageGeometry; glyph: number } | null {
    const geometry = libraryPageGeometry();
    if (!geometry) return null;
    const glyph = glyphAt(geometry, libraryPageDisplay, displayX, displayY);
    return glyph === null ? null : { geometry, glyph };
}

function hitLibrarySelectionHandle(
    displayX: number,
    displayY: number,
): "start" | "end" | null {
    const geometry = libraryPageGeometry();
    const range = librarySelectionScalarRange();
    if (
        !geometry ||
        !range ||
        libraryPageDisplay.width <= 0 ||
        libraryPageDisplay.height <= 0
    ) {
        return null;
    }
    const handles = selectionHandles(geometry, range.start, range.end);
    if (!handles) return null;
    const scaleX = libraryPageDisplay.width / geometry.width;
    const scaleY = libraryPageDisplay.height / geometry.height;
    const startX = handles.startBox[0] * scaleX;
    const startY = handles.startBox[1] * scaleY;
    const endX = (handles.endBox[0] + handles.endBox[2]) * scaleX;
    const endY = (handles.endBox[1] + handles.endBox[3]) * scaleY;
    const distStart = Math.hypot(displayX - startX, displayY - startY);
    const distEnd = Math.hypot(displayX - endX, displayY - endY);
    const threshold = 28;
    if (distStart <= threshold && distStart <= distEnd) return "start";
    if (distEnd <= threshold) return "end";
    return null;
}

function applyScalarRangeSelection(
    geometry: PageGeometry,
    start: number,
    end: number,
): boolean {
    const scalars = Array.from(libraryPageText());
    if (start < 0 || end <= start || end > scalars.length) return false;
    const text = scalars.slice(start, end).join("");
    if (!text.trim()) return false;
    librarySelection.set(text);
    librarySelectionStart.set(start);
    librarySelectionScalarRange.set({ start, end });
    librarySelectionBoxes.set(boxesForRange(geometry, start, end));
    return true;
}

function beginLibrarySelection(
    pointer: number,
    displayX: number,
    displayY: number,
): boolean {
    const handleHit = hitLibrarySelectionHandle(displayX, displayY);
    if (handleHit) {
        librarySelectionPointer = pointer;
        libraryDragMode = handleHit;
        return true;
    }
    const hit = librarySelectionGeometry(displayX, displayY);
    if (!hit) return false;
    librarySelectionPointer = pointer;
    librarySelectionAnchor = hit.glyph;
    libraryDragMode = "new";
    return selectLibraryGlyphs(hit.geometry, hit.glyph, hit.glyph);
}

function moveLibrarySelection(
    pointer: number,
    displayX: number,
    displayY: number,
) {
    if (librarySelectionPointer !== pointer) return;
    const geometry = libraryPageGeometry();
    const range = librarySelectionScalarRange();
    if (libraryDragMode === "start" && geometry && range) {
        const next = adjustSelectionStart(
            geometry,
            libraryPageDisplay,
            displayX,
            displayY,
            range.start,
            range.end,
        );
        applyScalarRangeSelection(geometry, next.start, next.end);
        return;
    }
    if (libraryDragMode === "end" && geometry && range) {
        const next = adjustSelectionEnd(
            geometry,
            libraryPageDisplay,
            displayX,
            displayY,
            range.start,
            range.end,
        );
        applyScalarRangeSelection(geometry, next.start, next.end);
        return;
    }
    if (librarySelectionAnchor === null) return;
    const hit = librarySelectionGeometry(displayX, displayY);
    if (hit) {
        selectLibraryGlyphs(
            hit.geometry,
            librarySelectionAnchor,
            hit.glyph,
        );
    }
}

function selectLibraryGlyphs(
    geometry: PageGeometry,
    first: number,
    last: number,
): boolean {
    const selection = selectionBetween(
        geometry,
        libraryPageText(),
        first,
        last,
    );
    if (!selection) return false;
    const end = selection.start + Array.from(selection.text).length;
    librarySelection.set(selection.text);
    librarySelectionStart.set(selection.start);
    librarySelectionScalarRange.set({ start: selection.start, end });
    librarySelectionBoxes.set(selection.boxes);
    return true;
}

function endLibrarySelection(pointer: number) {
    if (librarySelectionPointer === pointer) {
        librarySelectionPointer = null;
        librarySelectionAnchor = null;
        libraryDragMode = "new";
    }
}

async function openPdfToc(row: LibraryRow) {
    libraryPageJumpInput.set(String(libraryPageIndex() + 1));
    libraryTocOpen.set(true);
    const host = LibraryHost as any;
    if (typeof host?.pdfToc !== "function") {
        libraryTocEntries.set([]);
        return;
    }
    try {
        const raw = await host.pdfToc(row.document_id);
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.toc)) {
            libraryTocEntries.set(
                res.value.toc.map((item: any) => ({
                    title: String(item.title ?? ""),
                    index: Number(item.page_index ?? 0),
                    depth: Number(item.depth ?? 0),
                })),
            );
        } else {
            libraryTocEntries.set([]);
        }
    } catch (_) {
        libraryTocEntries.set([]);
    }
}

async function loadLibraryPageImage(row: LibraryRow, pageIndex: number) {
    const request = ++libraryImageRequest;
    const previous = libraryPageImage();
    if (previous?.url) URL.revokeObjectURL(previous.url);
    libraryPageImage.set(null);
    libraryPageImageState.set("loading");
    libraryPageGeometry.set(null);
    try {
        const frame = await LibraryHost.pageImage(row.document_id, pageIndex);
        if (request !== libraryImageRequest) return;
        const info = JSON.parse(frame.info);
        if (
            !info.ok ||
            !frame.file ||
            info.document_id !== row.document_id ||
            info.page_index !== pageIndex ||
            typeof info.width !== "number" ||
            typeof info.height !== "number"
        ) {
            throw Error(info.error ?? "PDF page rendering failed");
        }
        libraryPageImage.set({
            documentId: row.document_id,
            pageIndex,
            url: URL.createObjectURL(frame.file),
            width: info.width,
            height: info.height,
        });
        libraryPageGeometry.set(
            parsePageGeometry(
                frame.geometry,
                info.width,
                info.height,
            ),
        );
        libraryPageImageState.set("ready");
        libraryPageStatus.set(
            libraryPageText()
                ? "本机 Core · 原生页图 + 文字层"
                : "页图已加载；正在读取文字层…",
        );
        if (pageIndex + 1 < row.page_count) {
            void LibraryHost.prefetchPage(
                row.document_id,
                pageIndex + 1,
            ).catch(() => {});
        }
    } catch (error) {
        if (request === libraryImageRequest) {
            libraryPageImageState.set("failed");
            libraryPageStatus.set(
                `页图暂时无法渲染：${
                    error instanceof Error ? error.message : "未知错误"
                }。已可切换重排文字阅读。`,
            );
        }
    }
}

async function loadLibrarySavedSelections(
    row: LibraryRow,
    pageIndex: number,
) {
    const request = ++librarySavedSelectionsRequest;
    try {
        const response = JSON.parse(
            await LibraryHost.savedSelections(row.document_id, pageIndex),
        );
        if (
            request !== librarySavedSelectionsRequest ||
            !response.ok ||
            response.value?.document_id !== row.document_id ||
            response.value?.page_index !== pageIndex ||
            !Array.isArray(response.value?.ranges)
        ) {
            return;
        }
        librarySavedSelections.set(
            response.value.ranges.filter(
                (range: any) =>
                    Number.isSafeInteger(range?.capture_id) &&
                    Number.isSafeInteger(range?.start) &&
                    Number.isSafeInteger(range?.end) &&
                    range.start >= 0 &&
                    range.start < range.end,
            ),
        );
    } catch (_) {
        if (request === librarySavedSelectionsRequest) {
            librarySavedSelections.set([]);
        }
    }
}

async function loadLibraryText(row: LibraryRow, pageIndex: number) {
    libraryOpenEpub.set(null);
    libraryOpenDoc.set(row);
    libraryPageIndex.set(pageIndex);
    libraryPageText.set("");
    clearLibraryPageImage();
    clearLibrarySelection();
    librarySelectionNote.set("");
    librarySourceQuote.set(null);
    libraryResumeNotice.set("");
    libraryPageStatus.set("正在读取本机页图与文字…");
    void loadLibraryPageImage(row, pageIndex);
    void loadLibrarySavedSelections(row, pageIndex);
    let offset = 0;
    let text = "";
    try {
        for (let chunk = 0; chunk < 64; chunk += 1) {
            const response = JSON.parse(
                await LibraryHost.pageText(row.document_id, pageIndex, offset, 2048),
            );
            if (!response.ok || !response.value)
                throw Error(response.error ?? "page text failed");
            if (response.value.document_id !== row.document_id)
                throw Error("stale library document");
            text += String(response.value.text ?? "");
            if (response.value.next_offset == null) break;
            const next = Number(response.value.next_offset);
            if (!Number.isFinite(next) || next <= offset)
                throw Error("invalid page offset");
            offset = next;
        }
        libraryPageText.set(text);
        const imageState = libraryPageImageState();
        libraryPageStatus.set(
            text
                ? imageState === "ready"
                    ? "本机 Core · 原生页图 + 文字层"
                    : imageState === "failed"
                      ? "本机 Core · 文字层可用；页图暂未渲染"
                      : "本机 Core · 正在加载页图，文字层已可用"
                : "这一页没有可提取的文字层。",
        );
        const saved = await saveReadingPosition(
            row.document_id,
            pageIndex,
            row.page_count,
        );
        if (saved) {
            libraryPositions.set({
                ...libraryPositions(),
                [row.document_id]: saved,
            });
        }
    } catch (_) {
        libraryPageStatus.set("这一页的文字暂时读不到；页图或资料仍在本机。");
    }
}

async function importLibraryPdf() {
    if (libraryImporting()) return;
    libraryImporting.set(true);
    libraryStatus.set("请选择本地 PDF；原文件只写入本机 Core。");
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
        });
        if (picked.length === 0) {
            libraryStatus.set("已取消导入；当前资料库没有变化。");
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_PDF_IMPORT_BYTES) {
            throw new Error("PDF 不能超过 16 MiB");
        }
        const bytes = new Uint8Array(await file.arrayBuffer());
        const response = JSON.parse(
            await LibraryHost.importPdf(file.name, bytes),
        );
        if (!response.ok || !response.value) {
            throw new Error(response.error ?? "PDF import failed");
        }
        await loadLibraryPage();
        const documentId = Number(response.value.document_id);
        const imported =
            libraryRows().find((row) => row.document_id === documentId) ??
            ({
                document_id: documentId,
                title: String(response.value.title ?? file.name),
                page_count: Number(response.value.page_count),
                original_bytes: file.size,
                commit_rev: 0,
            } satisfies LibraryRow);
        if (
            Number.isSafeInteger(imported.document_id) &&
            imported.page_count > 0
        ) {
            await loadLibraryText(imported, 0);
            libraryResumeNotice.set("PDF 已写入本机 Core，并打开第一页。");
        } else {
            libraryStatus.set("PDF 已写入本机 Core；刷新列表后可阅读。");
        }
    } catch (error) {
        libraryStatus.set(
            `PDF 未导入：${
                error instanceof Error ? error.message : "未知错误"
            }。原资料库没有被删除。`,
        );
    } finally {
        libraryImporting.set(false);
    }
}

async function importLibraryEpub() {
    if (libraryImporting()) return;
    const host = LibraryHost as any;
    if (typeof host?.importEpub !== "function") {
        libraryStatus.set("当前宿主尚未启用 EPUB 导入接口。");
        return;
    }
    libraryImporting.set(true);
    libraryStatus.set("请选择本地 EPUB 书籍；原文件只写入本机 Core。");
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "EPUB 书籍", extensions: ["epub"] }],
        });
        if (picked.length === 0) {
            libraryStatus.set("已取消导入；当前资料库没有变化。");
            return;
        }
        const file = picked[0].file;
        if (file.size > MAX_EPUB_IMPORT_BYTES) {
            throw new Error("EPUB 不能超过 16 MiB");
        }
        const bytes = new Uint8Array(await file.arrayBuffer());
        const raw = await host.importEpub(file.name, bytes);
        const response = JSON.parse(raw);
        if (!response?.ok || !response.value) {
            throw new Error(String(response?.error ?? "EPUB 导入失败"));
        }
        await loadLibraryPage();
        const documentId = Number(response.value.document_id);
        const imported =
            libraryEpubRows().find(
                (item) => item.document_id === documentId,
            ) ??
            ({
                document_id: documentId,
                title: String(response.value.title ?? file.name),
                author: String(response.value.author ?? ""),
                chapter_count: Number(response.value.chapter_count ?? 1),
                original_bytes: file.size,
                commit_rev: Number(response.value.commit_rev ?? 0),
            } satisfies EpubRow);
        if (
            Number.isSafeInteger(imported.document_id) &&
            imported.chapter_count > 0
        ) {
            await loadEpubChapterContent(imported, 0);
            libraryResumeNotice.set("EPUB 已写入本机 Core，并打开第一章。");
        } else {
            libraryStatus.set("EPUB 已写入本机 Core；刷新列表后可阅读。");
        }
    } catch (error) {
        libraryStatus.set(
            `EPUB 未导入：${
                error instanceof Error ? error.message : String(error)
            }。原资料库没有被删除。`,
        );
    } finally {
        libraryImporting.set(false);
    }
}

async function loadEpubSavedHighlights(book: EpubRow, chapterIndex: number) {
    const host = LibraryHost as any;
    if (typeof host?.epubSavedSelections !== "function") {
        epubSavedRanges.set([]);
        return;
    }
    try {
        const raw = await host.epubSavedSelections(
            book.document_id,
            chapterIndex,
        );
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.ranges)) {
            epubSavedRanges.set(
                res.value.ranges.filter(
                    (r: any) =>
                        Number.isSafeInteger(r?.capture_id) &&
                        Number.isSafeInteger(r?.start) &&
                        Number.isSafeInteger(r?.end) &&
                        r.start >= 0 &&
                        r.start < r.end,
                ),
            );
        } else {
            epubSavedRanges.set([]);
        }
    } catch (_) {
        epubSavedRanges.set([]);
    }
}

async function loadEpubChapterContent(book: EpubRow, chapterIndex: number) {
    clearLibraryPageImage();
    libraryOpenDoc.set(null);
    libraryOpenEpub.set(book);
    epubChapterIndex.set(chapterIndex);
    epubChapterTitle.set(`第 ${chapterIndex + 1} 章`);
    epubChapterText.set("");
    epubSelection.set("");
    epubSelectionScalarStart.set(0);
    epubSelectionNote.set("");
    epubNoteSheetOpen.set(false);
    librarySourceQuote.set(null);
    libraryResumeNotice.set("");
    epubChapterStatus.set("正在读取本机 EPUB 章节…");
    void loadEpubSavedHighlights(book, chapterIndex);
    const host = LibraryHost as any;
    if (typeof host?.epubChapter !== "function") {
        epubChapterStatus.set("当前宿主尚未启用 EPUB 章节读取接口。");
        return;
    }
    try {
        const raw = await host.epubChapter(book.document_id, chapterIndex);
        const res = JSON.parse(raw);
        if (!res?.ok || !res.value) {
            throw new Error(String(res?.error ?? "章节读取失败"));
        }
        const title = String(res.value.title ?? `第 ${chapterIndex + 1} 章`);
        const text = String(res.value.text ?? "");
        epubChapterTitle.set(title);
        epubChapterText.set(text);
        epubChapterStatus.set(
            `本机 Core · 原生重排排版 · 第 ${chapterIndex + 1}/${book.chapter_count} 章`,
        );
        await saveEpubBookPosition(book.document_id, chapterIndex, 0);
    } catch (error) {
        epubChapterStatus.set(
            `章节读取未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

async function openEpubDocument(book: EpubRow) {
    const saved = await loadEpubBookPosition(
        book.document_id,
        book.chapter_count,
    );
    const chapter = saved ? saved.chapterIndex : 0;
    await loadEpubChapterContent(book, chapter);
    if (saved) {
        libraryResumeNotice.set(
            `已继续上次阅读 · 第 ${saved.chapterIndex + 1} 章`,
        );
    }
}

async function openEpubToc(book: EpubRow) {
    epubTocOpen.set(true);
    const host = LibraryHost as any;
    if (typeof host?.epubToc !== "function") {
        epubTocEntries.set([]);
        return;
    }
    try {
        const raw = await host.epubToc(book.document_id);
        const res = JSON.parse(raw);
        if (res?.ok && Array.isArray(res.value?.toc)) {
            epubTocEntries.set(
                res.value.toc.map((item: any) => ({
                    title: String(item.title ?? ""),
                    index: Number(item.chapter_index ?? 0),
                    depth: Number(item.depth ?? 0),
                })),
            );
        } else {
            epubTocEntries.set([]);
        }
    } catch (_) {
        epubTocEntries.set([]);
    }
}

async function captureEpubExcerpt(book: EpubRow, chapterIndex: number) {
    const selected = epubSelection().trim();
    if (!selected) return;
    const host = LibraryHost as any;
    if (typeof host?.captureEpubSelection !== "function") {
        epubChapterStatus.set("当前宿主尚未启用 EPUB 摘录保存接口。");
        return;
    }
    const start = epubSelectionScalarStart();
    const end = start + Array.from(selected).length;
    const note = epubSelectionNote();
    const operationId = `epub-${book.document_id}-${chapterIndex}-${Date.now().toString(36)}`;
    try {
        const raw = await host.captureEpubSelection(
            book.document_id,
            chapterIndex,
            start,
            end,
            note,
            operationId,
        );
        const res = JSON.parse(raw);
        if (!res?.ok) {
            throw new Error(String(res?.error ?? "保存失败"));
        }
        epubChapterStatus.set("EPUB 选文与想法已保存到本机 Core。");
        epubSelection.set("");
        epubSelectionNote.set("");
        epubNoteSheetOpen.set(false);
        void loadLiveNotes();
        void loadEpubSavedHighlights(book, chapterIndex);
    } catch (error) {
        epubChapterStatus.set(
            `选文暂未保存：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

async function openLibraryDocument(row: LibraryRow) {
    libraryOpenEpub.set(null);
    libraryOpenDoc.set(row);
    libraryViewMode.set("image");
    libraryImmersive.set(false);
    libraryTocOpen.set(false);
    libraryPageIndex.set(0);
    libraryPageText.set("");
    clearLibraryPageImage();
    clearLibrarySelection();
    librarySelectionNote.set("");
    librarySourceQuote.set(null);
    libraryPageStatus.set("正在恢复上次阅读位置…");
    const position = await loadReadingPosition(
        row.document_id,
        row.page_count,
    );
    const page = position ? position.page : 0;
    await loadLibraryText(row, page);
    if (position) {
        libraryResumeNotice.set(
            `已继续上次阅读 · 第 ${position.page + 1} 页`,
        );
    }
}

async function openLibrarySourceTarget(target: LibrarySourceTarget) {
    libraryStatus.set(`正在定位《${target.title}》…`);
    if (target.format === "epub") {
        try {
            const host = LibraryHost as any;
            if (typeof host?.epubPage === "function") {
                const raw = await host.epubPage("", 50);
                const res = JSON.parse(raw);
                if (res?.ok && Array.isArray(res.value?.items)) {
                    const book = res.value.items
                        .map(epubRow)
                        .find(
                            (candidate: EpubRow | null) =>
                                candidate?.document_id === target.documentId,
                        );
                    if (book) {
                        const chapterIndex =
                            target.pageIndex < book.chapter_count
                                ? Math.max(0, target.pageIndex)
                                : 0;
                        await loadEpubChapterContent(book, chapterIndex);
                        librarySourceQuote.set({
                            text: target.quote,
                            truncated: target.quoteTruncated,
                        });
                        epubChapterStatus.set(
                            `已从想法定位到《${target.title}》第 ${chapterIndex + 1} 章。`,
                        );
                        return;
                    }
                }
            }
        } catch (_) {}
    }
    let cursor = "";
    const seen = new Set<string>();
    try {
        for (;;) {
            if (seen.has(cursor)) throw Error("library cursor cycle");
            seen.add(cursor);
            const response = JSON.parse(await LibraryHost.page(cursor, 6));
            if (!response.ok || !Array.isArray(response.value?.documents))
                throw Error(response.error ?? "library page failed");
            const row = response.value.documents
                .map(libraryRow)
                .find(
                    (candidate: LibraryRow | null) =>
                        candidate?.document_id === target.documentId,
                );
            if (row) {
                libraryViewMode.set("image");
                libraryImmersive.set(false);
                libraryTocOpen.set(false);
                if (target.pageIndex >= row.page_count) {
                    await loadLibraryText(row, 0);
                    libraryPageStatus.set(
                        `想法记录在第 ${target.pageIndex + 1} 页，但当前资料只有 ${row.page_count} 页；未定位到相似内容。`,
                    );
                    return;
                }
                await loadLibraryText(row, target.pageIndex);
                librarySourceQuote.set({
                    text: target.quote,
                    truncated: target.quoteTruncated,
                });
                libraryPageStatus.set(
                    `已从想法定位到《${target.title}》第 ${target.pageIndex + 1} 页。`,
                );
                return;
            }
            const next = response.value.next_cursor;
            if (typeof next !== "string" || !next) break;
            cursor = next;
        }
    } catch (_) {
        // Fall through to the normal list with an explicit locator failure.
    }
    await loadLibraryPage();
    libraryStatus.set(
        `没有找到《${target.title}》当前可读的原件；没有用相似内容代替。`,
    );
}

function utf16ToScalar(text: string, offset: number) {
    let units = 0;
    let scalars = 0;
    for (const character of text) {
        if (units >= offset) return scalars;
        units += character.length;
        scalars += 1;
    }
    return scalars;
}

async function captureLibrarySelection(row: LibraryRow, pageIndex: number) {
    const selected = librarySelection();
    if (!selected) return;
    const note = librarySelectionNote();
    const scalarRange = librarySelectionScalarRange();
    const start = scalarRange
        ? scalarRange.start
        : utf16ToScalar(libraryPageText(), librarySelectionStart());
    const end = scalarRange
        ? scalarRange.end
        : start + Array.from(selected).length;
    const operation = `library-${row.document_id}-${pageIndex}-${Date.now().toString(36)}`;
    try {
        const response = JSON.parse(
            await LibraryHost.captureSelection(
                row.document_id,
                pageIndex,
                start,
                end,
                note,
                operation,
            ),
        );
        if (!response.ok) throw Error(response.error ?? "capture failed");
        libraryPageStatus.set("选文已保存到本机 Core。");
        void loadLiveNotes();
        void loadLibrarySavedSelections(row, pageIndex);
        clearLibrarySelection();
        librarySelectionNote.set("");
    } catch (_) {
        libraryPageStatus.set("选文暂未保存；原选文仍在当前页面。");
    }
}

function LibraryReader(props: { compact: boolean }) {
    const row = libraryOpenDoc();
    effect(() => {
        if (!row) return;
        BackHandler.on("press", () => {
            if (libraryNoteSheetOpen()) {
                libraryNoteSheetOpen.set(false);
                return true;
            }
            if (libraryTocOpen()) {
                libraryTocOpen.set(false);
                return true;
            }
            if (librarySelection()) {
                clearLibrarySelection();
                return true;
            }
            clearLibraryPageImage();
            libraryOpenDoc.set(null);
            return true;
        });
    });
    if (!row) return <View />;
    const pages = Math.max(1, row.page_count);
    const pageWidth = () =>
        Math.max(
            240,
            (libraryReaderWidth() || (props.compact ? 330 : 640)) *
                libraryZoom(),
        );
    const showTextMode = () =>
        libraryViewMode() === "text" || libraryPageImageState() === "failed";
    return (
        <View
            accessibilityLabel="资料库阅读视图"
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
            <View
                style={{
                    minHeight: props.compact ? 48 : 52,
                    paddingHorizontal: props.compact ? 10 : 18,
                    paddingVertical: 6,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 6,
                    backgroundColor: ink("surface-toolbar"),
                    borderBottomWidth: 1,
                    borderColor: ink("border-subtle"),
                }}
            >
                <Tool
                    label="返回资料库列表"
                    icon="back"
                    text="资料库"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => {
                        clearLibraryPageImage();
                        libraryOpenDoc.set(null);
                    }}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy size={props.compact ? 13 : 15} strong>
                        {row.title}
                    </Copy>
                </View>
                <Tool
                    label="上一页"
                    icon="back"
                    size={props.compact ? "touch" : "compact"}
                    disabled={libraryPageIndex() === 0}
                    onPress={() =>
                        void loadLibraryText(row, libraryPageIndex() - 1)
                    }
                />
                <TextButton
                    label={`${libraryPageIndex() + 1} / ${pages}`}
                    accessibilityLabel="打开目录与页码跳转"
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openPdfToc(row)}
                />
                <Tool
                    label="下一页"
                    icon="forward"
                    size={props.compact ? "touch" : "compact"}
                    disabled={libraryPageIndex() + 1 >= pages}
                    onPress={() =>
                        void loadLibraryText(row, libraryPageIndex() + 1)
                    }
                />
                <Tool
                    label="目录"
                    icon="list"
                    text="目录"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => void openPdfToc(row)}
                />
                <TextButton
                    label={
                        libraryViewMode() === "image" ? "重排文字" : "页图视图"
                    }
                    accessibilityLabel="切换页图与重排文字视图"
                    size={props.compact ? "touch" : "compact"}
                    raised
                    onPress={() =>
                        libraryViewMode.set(
                            libraryViewMode() === "image" ? "text" : "image",
                        )
                    }
                />
                <TextButton
                    label={libraryImmersive() ? "退出沉浸" : "沉浸"}
                    size={props.compact ? "touch" : "compact"}
                    filled={!libraryImmersive()}
                    raised={libraryImmersive()}
                    onPress={() => libraryImmersive.set(!libraryImmersive())}
                />
            </View>
            <If when={!libraryImmersive()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 18,
                        paddingVertical: 6,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-inset"),
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                >
                    <If when={!showTextMode()}>
                        <Tool
                            label="缩小资料页图"
                            text="−"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() <= 1}
                            onPress={() =>
                                void changeLibraryZoom(
                                    nextReadingZoom(libraryZoom(), -1),
                                )
                            }
                        />
                        <Copy size={11} muted>
                            {readingZoomLabel(libraryZoom())}
                        </Copy>
                        <Tool
                            label="放大资料页图"
                            text="＋"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() >= 2}
                            onPress={() =>
                                void changeLibraryZoom(
                                    nextReadingZoom(libraryZoom(), 1),
                                )
                            }
                        />
                        <Tool
                            label="资料页图适合宽度"
                            text="适合宽度"
                            size={props.compact ? "touch" : "compact"}
                            disabled={libraryZoom() <= 1}
                            onPress={() => void changeLibraryZoom(1)}
                        />
                    </If>
                    <View style={{ flex: 1, minWidth: 120, gap: 2 }}>
                        <Copy size={11} muted>
                            {libraryPageStatus()}
                        </Copy>
                        <If when={!!libraryResumeNotice()}>
                            <Copy size={11} muted>
                                {libraryResumeNotice()}
                            </Copy>
                        </If>
                    </View>
                </View>
            </If>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 12 : 28,
                        maxWidth: 900,
                        width: "100%",
                        alignSelf: "center",
                        gap: 14,
                    }}
                >
                    <If when={!!librarySourceQuote()}>
                        <View
                            style={{
                                gap: 4,
                                padding: 12,
                                borderRadius: 8,
                                borderLeftWidth: 3,
                                borderColor: ink("text-accent"),
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                从想法回到此页 ·{" "}
                                {librarySourceQuote()?.truncated
                                    ? "引文预览"
                                    : "保存的引文"}
                            </Copy>
                            <Copy size={13} lines={3}>
                                {librarySourceQuote()?.text}
                            </Copy>
                        </View>
                    </If>
                    <If when={!showTextMode() && !!libraryPageImage()}>
                        <View
                            style={{
                                width: "100%",
                                maxWidth: 780,
                                alignSelf: "center",
                                padding: props.compact ? 4 : 10,
                                borderRadius: 8,
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                            }}
                            onLayout={({ bounds }) => {
                                const pad = props.compact ? 8 : 20;
                                const width = Math.max(
                                    0,
                                    bounds.size.width - pad,
                                );
                                if (
                                    width > 0 &&
                                    Math.abs(width - libraryReaderWidth()) > 0.5
                                ) {
                                    libraryReaderWidth.set(width);
                                }
                            }}
                        >
                            <Scroll
                                focusable={false}
                                horizontal
                                onScroll={() => {
                                    if (librarySelectionPointer !== null) {
                                        clearLibrarySelection();
                                    }
                                }}
                                style={{ width: "100%" }}
                                contentStyle={{
                                    minWidth: "100%",
                                    justifyContent: "center",
                                }}
                            >
                                <View
                                    style={{
                                        width: pageWidth(),
                                        aspectRatio:
                                            libraryPageImage()!.width /
                                            libraryPageImage()!.height,
                                        backgroundColor: ink("surface-base"),
                                        position: "relative",
                                    }}
                                >
                                    <Image
                                        accessibilityLabel={`《${row.title}》第 ${
                                            libraryPageIndex() + 1
                                        } 页原生页图`}
                                        src={libraryPageImage()!.url}
                                        fadeDuration={0}
                                        style={{
                                            width: "100%",
                                            height: "100%",
                                            objectFit: "contain",
                                        }}
                                    />
                                    <Canvas
                                        accessibilityLabel="页图选文层"
                                        focusable={false}
                                        onStartShouldSetResponder={(event) =>
                                            hitLibrarySelectionHandle(
                                                event.locationX,
                                                event.locationY,
                                            ) !== null ||
                                            librarySelectionGeometry(
                                                event.locationX,
                                                event.locationY,
                                            ) !== null
                                        }
                                        onResponderGrant={(event) => {
                                            librarySelectionNote.set("");
                                            beginLibrarySelection(
                                                event.identifier,
                                                event.locationX,
                                                event.locationY,
                                            );
                                        }}
                                        onResponderMove={(event) =>
                                            moveLibrarySelection(
                                                event.identifier,
                                                event.locationX,
                                                event.locationY,
                                            )
                                        }
                                        onResponderRelease={(event) =>
                                            endLibrarySelection(
                                                event.identifier,
                                            )
                                        }
                                        onResponderTerminate={(event) =>
                                            endLibrarySelection(
                                                event.identifier,
                                            )
                                        }
                                        onResponderTerminationRequest={() =>
                                            true
                                        }
                                        draw={(context, size) => {
                                            const geometry =
                                                libraryPageGeometry();
                                            if (!geometry) return;
                                            libraryPageDisplay = {
                                                width: size.width,
                                                height: size.height,
                                            };
                                            const scaleX =
                                                size.width / geometry.width;
                                            const scaleY =
                                                size.height / geometry.height;
                                            context.fillStyle = ink(
                                                "pdf-saved-selection",
                                            );
                                            for (const [x, y, width, height] of
                                                boxesForSavedRanges(
                                                    geometry,
                                                    librarySavedSelections(),
                                                )) {
                                                context.fillRect(
                                                    x * scaleX,
                                                    y * scaleY,
                                                    width * scaleX,
                                                    height * scaleY,
                                                );
                                            }
                                            context.fillStyle =
                                                ink("pdf-selection");
                                            for (const [x, y, width, height] of
                                                librarySelectionBoxes()) {
                                                context.fillRect(
                                                    x * scaleX,
                                                    y * scaleY,
                                                    width * scaleX,
                                                    height * scaleY,
                                                );
                                            }
                                            const range =
                                                librarySelectionScalarRange();
                                            if (range) {
                                                const handles =
                                                    selectionHandles(
                                                        geometry,
                                                        range.start,
                                                        range.end,
                                                    );
                                                if (handles) {
                                                    const [sx, sy, , sh] =
                                                        handles.startBox;
                                                    const [ex, ey, ew, eh] =
                                                        handles.endBox;
                                                    const hx1 = sx * scaleX;
                                                    const hy1 = sy * scaleY;
                                                    const hx2 =
                                                        (ex + ew) * scaleX;
                                                    const hy2 =
                                                        (ey + eh) * scaleY;
                                                    context.fillStyle =
                                                        ink("text-accent");
                                                    context.fillRect(
                                                        hx1 - 1,
                                                        hy1,
                                                        2,
                                                        sh * scaleY,
                                                    );
                                                    context.beginPath();
                                                    context.arc(
                                                        hx1,
                                                        hy1,
                                                        6,
                                                        0,
                                                        Math.PI * 2,
                                                    );
                                                    context.fill();
                                                    context.fillRect(
                                                        hx2 - 1,
                                                        ey * scaleY,
                                                        2,
                                                        eh * scaleY,
                                                    );
                                                    context.beginPath();
                                                    context.arc(
                                                        hx2,
                                                        hy2,
                                                        6,
                                                        0,
                                                        Math.PI * 2,
                                                    );
                                                    context.fill();
                                                }
                                            }
                                        }}
                                        style={{
                                            position: "absolute",
                                            left: 0,
                                            top: 0,
                                            width: "100%",
                                            height: "100%",
                                        }}
                                    />
                                </View>
                            </Scroll>
                        </View>
                    </If>
                    <If when={!showTextMode() && !libraryPageImage()}>
                        <View
                            style={{
                                padding: 28,
                                alignItems: "center",
                                gap: 10,
                            }}
                        >
                            <Copy muted>{libraryPageStatus()}</Copy>
                            <TextButton
                                label="切换到重排文字阅读"
                                size={props.compact ? "touch" : "regular"}
                                raised
                                onPress={() => libraryViewMode.set("text")}
                            />
                        </View>
                    </If>
                    <If when={showTextMode()}>
                        <View
                            selectable
                            accessibilityLabel="可选择的本页文字"
                            onSelectionChange={({ text, start }) => {
                                if (!text) return;
                                const startOffset = start ?? 0;
                                const scalarStart = utf16ToScalar(
                                    libraryPageText(),
                                    startOffset,
                                );
                                librarySelection.set(text);
                                librarySelectionStart.set(startOffset);
                                librarySelectionScalarRange.set({
                                    start: scalarStart,
                                    end:
                                        scalarStart +
                                        Array.from(text).length,
                                });
                            }}
                            style={{
                                padding: props.compact ? 16 : 24,
                                borderRadius: 10,
                                borderCurve: "continuous",
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                backgroundColor: ink("surface-preview"),
                            }}
                        >
                            <Text
                                style={{
                                    fontSize: 16,
                                    lineHeight: 29,
                                    color: ink("text-primary"),
                                    userSelect: "text",
                                }}
                            >
                                {libraryPageText() ||
                                    "本页暂无可提取的文字层。"}
                            </Text>
                        </View>
                    </If>
                </View>
            </Scroll>
            <If when={!!librarySelection()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 20,
                        paddingVertical: 10,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-floating"),
                        borderTopWidth: 1,
                        borderColor: ink("border-control"),
                    }}
                >
                    <Copy size={12} strong>
                        {`已选 ${Array.from(librarySelection()).length} 字`}
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="复制当前选文"
                        text="复制"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void Clipboard.writeText(librarySelection())
                        }
                    />
                    <Tool
                        label="保存当前选文和想法到 Core"
                        text="保存摘录"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void captureLibrarySelection(
                                row,
                                libraryPageIndex(),
                            )
                        }
                    />
                    <Tool
                        label="写想法并保存"
                        icon="pen"
                        text="写想法并保存"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() => libraryNoteSheetOpen.set(true)}
                    />
                    <Tool
                        label="收词到词书"
                        icon="book"
                        text="收词到词书"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void collectSelectionToVocab(
                                librarySelection(),
                                libraryPageText(),
                                row.title,
                                (msg) => libraryPageStatus.set(msg),
                            )
                        }
                    />
                    <TextButton
                        label="取消"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => {
                            clearLibrarySelection();
                            librarySelectionNote.set("");
                        }}
                    />
                </View>
            </If>
            <BottomSheet
                open={libraryNoteSheetOpen()}
                onClose={() => libraryNoteSheetOpen.set(false)}
                title="写想法并保存到 Core"
                subtitle={`《${row.title}》第 ${libraryPageIndex() + 1} 页 · 已选 ${
                    Array.from(librarySelection()).length
                } 字`}
            >
                <View style={{ gap: 12 }}>
                    <View
                        style={{
                            padding: 10,
                            borderRadius: 8,
                            borderLeftWidth: 3,
                            borderColor: ink("text-accent"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <Copy size={12} lines={3}>
                            {librarySelection()}
                        </Copy>
                    </View>
                    <Input
                        accessibilityLabel="为摘录写下想法"
                        multiline
                        value={librarySelectionNote()}
                        onChange={({ value }) =>
                            librarySelectionNote.set(value)
                        }
                        placeholder="写下自己的想法（可选）"
                        style={{
                            minHeight: 88,
                            padding: 12,
                            fontSize: 15,
                            lineHeight: 24,
                            color: ink("text-primary"),
                            backgroundColor: ink("surface-base"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                            borderRadius: 8,
                        }}
                        focusStyle={focus()}
                    />
                    <View
                        style={{
                            flexDirection: "row",
                            justifyContent: "flex-end",
                            gap: 10,
                        }}
                    >
                        <TextButton
                            label="取消"
                            size="touch"
                            onPress={() => libraryNoteSheetOpen.set(false)}
                        />
                        <Tool
                            label="保存摘录与想法"
                            text="保存到 Core"
                            raised
                            size="touch"
                            onPress={() =>
                                void captureLibrarySelection(
                                    row,
                                    libraryPageIndex(),
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
            <BottomSheet
                open={libraryTocOpen()}
                onClose={() => libraryTocOpen.set(false)}
                title="目录与页码跳转"
                subtitle={`《${row.title}》 · 共 ${pages} 页`}
            >
                <View style={{ gap: 12, maxHeight: 420 }}>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 8,
                        }}
                    >
                        <Input
                            accessibilityLabel="输入跳转页码"
                            value={libraryPageJumpInput()}
                            onChange={({ value }) =>
                                libraryPageJumpInput.set(value)
                            }
                            placeholder={`页码 (1-${pages})`}
                            style={{
                                flex: 1,
                                height: props.compact ? 44 : 38,
                                paddingHorizontal: 10,
                                fontSize: 14,
                                color: ink("text-primary"),
                                backgroundColor: ink("surface-inset"),
                                borderWidth: 1,
                                borderColor: ink("border-control"),
                                borderRadius: 8,
                            }}
                            focusStyle={focus()}
                        />
                        <Tool
                            label="跳转到指定页码"
                            text="跳转"
                            raised
                            size={props.compact ? "touch" : "regular"}
                            onPress={() => {
                                const targetPage =
                                    Number.parseInt(
                                        libraryPageJumpInput(),
                                        10,
                                    ) - 1;
                                if (
                                    Number.isSafeInteger(targetPage) &&
                                    targetPage >= 0 &&
                                    targetPage < pages
                                ) {
                                    libraryTocOpen.set(false);
                                    void loadLibraryText(row, targetPage);
                                }
                            }}
                        />
                    </View>
                    <Scroll style={{ maxHeight: 320 }}>
                        <View style={{ gap: 6 }}>
                            <If when={libraryTocEntries().length === 0}>
                                <Copy size={12} muted>
                                    此 PDF 未内嵌书签目录，可使用上方页码跳转。
                                </Copy>
                            </If>
                            <Each
                                of={libraryTocEntries()}
                                key={(item) =>
                                    `pdf-toc-${item.index}-${item.title}`
                                }
                            >
                                {(item) => (
                                    <Button
                                        accessibilityLabel={`跳转到 ${item().title}`}
                                        onPress={() => {
                                            libraryTocOpen.set(false);
                                            void loadLibraryText(
                                                row,
                                                Math.min(
                                                    pages - 1,
                                                    Math.max(0, item().index),
                                                ),
                                            );
                                        }}
                                        style={{
                                            minHeight: props.compact ? 44 : 40,
                                            paddingLeft:
                                                10 + item().depth * 14,
                                            paddingRight: 10,
                                            flexDirection: "row",
                                            alignItems: "center",
                                            borderRadius: 7,
                                            backgroundColor: ink(
                                                libraryPageIndex() ===
                                                    item().index
                                                    ? "surface-selected"
                                                    : "transparent",
                                            ),
                                        }}
                                        hoverStyle={{
                                            backgroundColor:
                                                ink("surface-hover"),
                                        }}
                                        focusStyle={focus()}
                                    >
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy>{item().title}</Copy>
                                        </View>
                                        <Copy size={11} muted>
                                            {`第 ${item().index + 1} 页`}
                                        </Copy>
                                    </Button>
                                )}
                            </Each>
                        </View>
                    </Scroll>
                </View>
            </BottomSheet>
        </View>
    );
}

type EpubParagraph = {
    index: number;
    text: string;
    scalarStart: number;
    scalarEnd: number;
};

function splitEpubParagraphs(text: string): EpubParagraph[] {
    const lines = text.split("\n");
    const result: EpubParagraph[] = [];
    let cursor = 0;
    for (let idx = 0; idx < lines.length; idx += 1) {
        const line = lines[idx];
        const len = Array.from(line).length;
        const trimmed = line.trim();
        if (trimmed) {
            result.push({
                index: result.length,
                text: trimmed,
                scalarStart: cursor,
                scalarEnd: cursor + len,
            });
        }
        cursor += len + 1;
    }
    return result;
}

function EpubReader(props: { compact: boolean }) {
    const book = libraryOpenEpub();
    effect(() => {
        if (!book) return;
        BackHandler.on("press", () => {
            if (epubNoteSheetOpen()) {
                epubNoteSheetOpen.set(false);
                return true;
            }
            if (epubTocOpen()) {
                epubTocOpen.set(false);
                return true;
            }
            if (epubTypographyOpen()) {
                epubTypographyOpen.set(false);
                return true;
            }
            if (epubSelection()) {
                epubSelection.set("");
                return true;
            }
            libraryOpenEpub.set(null);
            return true;
        });
    });
    if (!book) return <View />;
    const chapters = Math.max(1, book.chapter_count);
    const paragraphs = () => splitEpubParagraphs(epubChapterText());
    return (
        <View
            accessibilityLabel="EPUB 阅读视图"
            style={{
                flex: 1,
                minWidth: 0,
                backgroundColor: ink("surface-base"),
            }}
        >
            <View
                style={{
                    minHeight: props.compact ? 48 : 52,
                    paddingHorizontal: props.compact ? 10 : 18,
                    paddingVertical: 6,
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 6,
                    backgroundColor: ink("surface-toolbar"),
                    borderBottomWidth: 1,
                    borderColor: ink("border-subtle"),
                }}
            >
                <Tool
                    label="返回资料库列表"
                    icon="back"
                    text="资料库"
                    size={props.compact ? "touch" : "regular"}
                    onPress={() => libraryOpenEpub.set(null)}
                />
                <View style={{ flex: 1, minWidth: 80 }}>
                    <Copy size={props.compact ? 13 : 15} strong>
                        {book.title}
                    </Copy>
                    <Copy size={11} muted>
                        {epubChapterTitle()}
                    </Copy>
                </View>
                <Tool
                    label="上一章"
                    icon="back"
                    size={props.compact ? "touch" : "compact"}
                    disabled={epubChapterIndex() === 0}
                    onPress={() =>
                        void loadEpubChapterContent(
                            book,
                            epubChapterIndex() - 1,
                        )
                    }
                />
                <TextButton
                    label={`第 ${epubChapterIndex() + 1}/${chapters} 章`}
                    accessibilityLabel="打开章节目录"
                    size={props.compact ? "touch" : "compact"}
                    filled
                    onPress={() => void openEpubToc(book)}
                />
                <Tool
                    label="下一章"
                    icon="forward"
                    size={props.compact ? "touch" : "compact"}
                    disabled={epubChapterIndex() + 1 >= chapters}
                    onPress={() =>
                        void loadEpubChapterContent(
                            book,
                            epubChapterIndex() + 1,
                        )
                    }
                />
                <Tool
                    label="目录"
                    icon="list"
                    text="目录"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => void openEpubToc(book)}
                />
                <Tool
                    label="排版设置"
                    icon="sliders"
                    text="排版"
                    size={props.compact ? "touch" : "compact"}
                    onPress={() => epubTypographyOpen.set(true)}
                />
            </View>
            <Scroll style={{ flex: 1 }}>
                <View
                    style={{
                        padding: props.compact ? 16 : 36,
                        maxWidth: 740,
                        width: "100%",
                        alignSelf: "center",
                        gap: 16,
                    }}
                >
                    <Copy size={11} muted>
                        {epubChapterStatus()}
                    </Copy>
                    <If when={!!libraryResumeNotice()}>
                        <Copy size={11} muted>
                            {libraryResumeNotice()}
                        </Copy>
                    </If>
                    <If when={!!librarySourceQuote()}>
                        <View
                            style={{
                                gap: 4,
                                padding: 12,
                                borderRadius: 8,
                                borderLeftWidth: 3,
                                borderColor: ink("text-accent"),
                                backgroundColor: ink("surface-inset"),
                            }}
                        >
                            <Copy size={11} muted>
                                从想法定位到此章 · 保存的引文
                            </Copy>
                            <Copy size={13} lines={3}>
                                {librarySourceQuote()?.text}
                            </Copy>
                        </View>
                    </If>
                    <Text
                        style={{
                            fontSize: epubFontSize() + 5,
                            lineHeight: Math.round(
                                (epubFontSize() + 5) * epubLineHeight(),
                            ),
                            fontWeight: "600",
                            color: ink("text-primary"),
                        }}
                    >
                        {epubChapterTitle()}
                    </Text>
                    <Each
                        of={paragraphs()}
                        key={(para) =>
                            `epub-para-${epubChapterIndex()}-${para.index}`
                        }
                    >
                        {(para) => {
                            const quoteSnippet =
                                librarySourceQuote()?.text
                                    .trim()
                                    .slice(0, 24) ?? "";
                            const highlighted = () =>
                                epubSavedRanges().some(
                                    (r) =>
                                        r.start < para().scalarEnd &&
                                        r.end > para().scalarStart,
                                ) ||
                                (quoteSnippet.length > 0 &&
                                    para().text.includes(quoteSnippet));
                            const selectedPara = () =>
                                epubSelection() === para().text;
                            return (
                                <View
                                    selectable
                                    onPress={() => {
                                        epubSelection.set(para().text);
                                        epubSelectionScalarStart.set(
                                            para().scalarStart,
                                        );
                                    }}
                                    onSelectionChange={({ text, start }) => {
                                        if (!text) return;
                                        const offset = utf16ToScalar(
                                            para().text,
                                            start ?? 0,
                                        );
                                        epubSelection.set(text);
                                        epubSelectionScalarStart.set(
                                            para().scalarStart + offset,
                                        );
                                    }}
                                    style={{
                                        paddingVertical: 6,
                                        paddingHorizontal:
                                            highlighted() || selectedPara()
                                                ? 12
                                                : 2,
                                        borderLeftWidth:
                                            highlighted() || selectedPara()
                                                ? 3
                                                : 0,
                                        borderColor: ink("text-accent"),
                                        borderRadius: 6,
                                        backgroundColor: ink(
                                            selectedPara()
                                                ? "surface-selected"
                                                : highlighted()
                                                  ? "surface-inset"
                                                  : "transparent",
                                        ),
                                    }}
                                >
                                    <Text
                                        style={{
                                            fontSize: epubFontSize(),
                                            lineHeight: Math.round(
                                                epubFontSize() *
                                                    epubLineHeight(),
                                            ),
                                            color: ink("text-primary"),
                                            userSelect: "text",
                                        }}
                                    >
                                        {para().text}
                                    </Text>
                                </View>
                            );
                        }}
                    </Each>
                </View>
            </Scroll>
            <If when={!!epubSelection()}>
                <View
                    style={{
                        paddingHorizontal: props.compact ? 12 : 20,
                        paddingVertical: 10,
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                        backgroundColor: ink("surface-floating"),
                        borderTopWidth: 1,
                        borderColor: ink("border-control"),
                    }}
                >
                    <Copy size={12} strong>
                        {`已选 ${Array.from(epubSelection()).length} 字`}
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="复制当前选文"
                        text="复制"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void Clipboard.writeText(epubSelection())
                        }
                    />
                    <Tool
                        label="保存当前选文到 Core"
                        text="保存摘录"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void captureEpubExcerpt(book, epubChapterIndex())
                        }
                    />
                    <Tool
                        label="写想法并保存"
                        icon="pen"
                        text="写想法并保存"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() => epubNoteSheetOpen.set(true)}
                    />
                    <Tool
                        label="收词到词书"
                        icon="book"
                        text="收词到词书"
                        size={props.compact ? "touch" : "compact"}
                        raised
                        onPress={() =>
                            void collectSelectionToVocab(
                                epubSelection(),
                                epubChapterText(),
                                book.title,
                                (msg) => epubChapterStatus.set(msg),
                            )
                        }
                    />
                    <TextButton
                        label="取消"
                        size={props.compact ? "touch" : "compact"}
                        onPress={() => {
                            epubSelection.set("");
                            epubSelectionNote.set("");
                        }}
                    />
                </View>
            </If>
            <BottomSheet
                open={epubNoteSheetOpen()}
                onClose={() => epubNoteSheetOpen.set(false)}
                title="写想法并保存到 Core"
                subtitle={`《${book.title}》 · ${epubChapterTitle()}`}
            >
                <View style={{ gap: 12 }}>
                    <View
                        style={{
                            padding: 10,
                            borderRadius: 8,
                            borderLeftWidth: 3,
                            borderColor: ink("text-accent"),
                            backgroundColor: ink("surface-inset"),
                        }}
                    >
                        <Copy size={12} lines={3}>
                            {epubSelection()}
                        </Copy>
                    </View>
                    <Input
                        accessibilityLabel="为 EPUB 摘录写下想法"
                        multiline
                        value={epubSelectionNote()}
                        onChange={({ value }) => epubSelectionNote.set(value)}
                        placeholder="写下自己的想法（可选）"
                        style={{
                            minHeight: 88,
                            padding: 12,
                            fontSize: 15,
                            lineHeight: 24,
                            color: ink("text-primary"),
                            backgroundColor: ink("surface-base"),
                            borderWidth: 1,
                            borderColor: ink("border-control"),
                            borderRadius: 8,
                        }}
                        focusStyle={focus()}
                    />
                    <View
                        style={{
                            flexDirection: "row",
                            justifyContent: "flex-end",
                            gap: 10,
                        }}
                    >
                        <TextButton
                            label="取消"
                            size="touch"
                            onPress={() => epubNoteSheetOpen.set(false)}
                        />
                        <Tool
                            label="保存 EPUB 摘录与想法"
                            text="保存到 Core"
                            raised
                            size="touch"
                            onPress={() =>
                                void captureEpubExcerpt(
                                    book,
                                    epubChapterIndex(),
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
            <BottomSheet
                open={epubTocOpen()}
                onClose={() => epubTocOpen.set(false)}
                title="章节目录"
                subtitle={`《${book.title}》 · 共 ${chapters} 章`}
            >
                <Scroll style={{ maxHeight: 360 }}>
                    <View style={{ gap: 6 }}>
                        <Each
                            of={
                                epubTocEntries().length > 0
                                    ? epubTocEntries()
                                    : Array.from(
                                          { length: chapters },
                                          (_, index) => ({
                                              title: `第 ${index + 1} 章`,
                                              index,
                                              depth: 0,
                                          }),
                                      )
                            }
                            key={(item) =>
                                `epub-toc-${item.index}-${item.title}`
                            }
                        >
                            {(item) => (
                                <Button
                                    accessibilityLabel={`打开章节 ${item().title}`}
                                    onPress={() => {
                                        epubTocOpen.set(false);
                                        void loadEpubChapterContent(
                                            book,
                                            item().index,
                                        );
                                    }}
                                    style={{
                                        minHeight: props.compact ? 44 : 42,
                                        paddingLeft: 10 + item().depth * 14,
                                        paddingRight: 10,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        borderRadius: 7,
                                        backgroundColor: ink(
                                            epubChapterIndex() === item().index
                                                ? "surface-selected"
                                                : "transparent",
                                        ),
                                    }}
                                    hoverStyle={{
                                        backgroundColor: ink("surface-hover"),
                                    }}
                                    focusStyle={focus()}
                                >
                                    <View style={{ flex: 1, minWidth: 0 }}>
                                        <Copy>{item().title}</Copy>
                                    </View>
                                    <Copy size={11} muted>
                                        {`第 ${item().index + 1} 章`}
                                    </Copy>
                                </Button>
                            )}
                        </Each>
                    </View>
                </Scroll>
            </BottomSheet>
            <BottomSheet
                open={epubTypographyOpen()}
                onClose={() => epubTypographyOpen.set(false)}
                title="阅读排版设置"
                subtitle="字号与行高自动保存在本机"
            >
                <View style={{ gap: 14 }}>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <Copy strong>字号</Copy>
                        <View style={{ flex: 1 }} />
                        <Tool
                            label="减小字号"
                            text="A−"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={epubFontSize() <= MIN_EPUB_FONT_SIZE}
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize() - 1,
                                    epubLineHeight(),
                                )
                            }
                        />
                        <Copy strong>{`${epubFontSize()} px`}</Copy>
                        <Tool
                            label="增大字号"
                            text="A＋"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={epubFontSize() >= MAX_EPUB_FONT_SIZE}
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize() + 1,
                                    epubLineHeight(),
                                )
                            }
                        />
                    </View>
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 10,
                        }}
                    >
                        <Copy strong>行高</Copy>
                        <View style={{ flex: 1 }} />
                        <Tool
                            label="减小行高"
                            text="紧凑"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={
                                epubLineHeight() <= MIN_EPUB_LINE_HEIGHT
                            }
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize(),
                                    epubLineHeight() - 0.1,
                                )
                            }
                        />
                        <Copy strong>{`${epubLineHeight().toFixed(1)}x`}</Copy>
                        <Tool
                            label="增大行高"
                            text="舒展"
                            size={props.compact ? "touch" : "regular"}
                            raised
                            disabled={
                                epubLineHeight() >= MAX_EPUB_LINE_HEIGHT
                            }
                            onPress={() =>
                                void saveEpubTypography(
                                    epubFontSize(),
                                    epubLineHeight() + 0.1,
                                )
                            }
                        />
                    </View>
                </View>
            </BottomSheet>
        </View>
    );
}

export function LibraryWorkspace(props: { compact: boolean }) {
    const target = librarySourceTarget();
    if (!libraryBooted) {
        libraryBooted = true;
        void restoreLibraryZoom();
        void restoreEpubTypography();
        if (!target) void loadLibraryPage();
    }
    if (target && target.serial !== handledLibrarySourceTarget) {
        handledLibrarySourceTarget = target.serial;
        void openLibrarySourceTarget(target);
    }
    return (
        <View style={{ flex: 1 }}>
            <If when={!!libraryOpenDoc()}>
                <LibraryReader compact={props.compact} />
            </If>
            <If when={!libraryOpenDoc() && !!libraryOpenEpub()}>
                <EpubReader compact={props.compact} />
            </If>
            <If when={!libraryOpenDoc() && !libraryOpenEpub()}>
                <LibraryList compact={props.compact} />
            </If>
        </View>
    );
}

function LibraryList(props: { compact: boolean }) {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                accessibilityLabel="资料库插件工作区"
                style={{
                    padding: props.compact ? 18 : 40,
                    maxWidth: 900,
                    width: "100%",
                    alignSelf: "center",
                    gap: 18,
                }}
            >
                <View
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 10,
                    }}
                >
                    <Copy size={25} strong>
                        资料库
                    </Copy>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="导入本地 PDF"
                        icon="plus"
                        text={
                            libraryImporting() ? "正在导入…" : "导入 PDF"
                        }
                        raised
                        size={props.compact ? "touch" : "regular"}
                        disabled={libraryImporting()}
                        onPress={() => void importLibraryPdf()}
                    />
                    <Tool
                        label="导入本地 EPUB"
                        icon="book"
                        text={
                            libraryImporting() ? "正在导入…" : "导入 EPUB"
                        }
                        raised
                        size={props.compact ? "touch" : "regular"}
                        disabled={libraryImporting()}
                        onPress={() => void importLibraryEpub()}
                    />
                </View>
                <Copy size={12} muted>
                    {libraryStatus()}
                </Copy>
                <If
                    when={
                        libraryRows().length + libraryEpubRows().length > 0
                    }
                >
                    <Copy size={11} muted>
                        {`共 ${libraryRows().length} 份 PDF · ${libraryEpubRows().length} 本 EPUB`}
                    </Copy>
                </If>
                <Each
                    of={libraryRows()}
                    key={(row) => `pdf-doc-${row.document_id}`}
                >
                    {(row) => (
                        <View
                            style={{
                                paddingVertical: 12,
                                paddingHorizontal: 10,
                                borderBottomWidth: 1,
                                borderColor: ink("border-subtle"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 12,
                                borderRadius: 8,
                            }}
                        >
                            <Button
                                accessibilityLabel={`${
                                    libraryPositionOf(row())
                                        ? "继续阅读"
                                        : "阅读"
                                }${row().title}`}
                                onPress={() =>
                                    void openLibraryDocument(row())
                                }
                                style={{
                                    flex: 1,
                                    minWidth: 0,
                                    minHeight: props.compact ? 44 : undefined,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 12,
                                }}
                                hoverStyle={{
                                    backgroundColor: ink("surface-hover"),
                                }}
                                activeStyle={{
                                    backgroundColor: ink("surface-pressed"),
                                }}
                                focusStyle={focus()}
                            >
                                <Badge icon="document" size={38} />
                                <View style={{ flex: 1, gap: 4, minWidth: 0 }}>
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 6,
                                        }}
                                    >
                                        <View
                                            style={{
                                                paddingHorizontal: 6,
                                                paddingVertical: 1,
                                                borderRadius: 4,
                                                backgroundColor:
                                                    ink("surface-inset"),
                                            }}
                                        >
                                            <Copy size={10} strong>
                                                PDF
                                            </Copy>
                                        </View>
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy size={15} strong>
                                                {row().title}
                                            </Copy>
                                        </View>
                                    </View>
                                    <Copy size={11} muted>
                                        {row().page_count} 页 ·{" "}
                                        {Math.max(
                                            1,
                                            Math.round(
                                                row().original_bytes / 1024,
                                            ),
                                        )}{" "}
                                        KiB · 提交 {row().commit_rev}
                                        {libraryPositionLabel(row())}
                                    </Copy>
                                </View>
                                <Copy size={12} muted>
                                    {libraryPositionOf(row())
                                        ? "继续"
                                        : "阅读"}
                                </Copy>
                            </Button>
                            <Tool
                                label={`删除 ${row().title}`}
                                icon="trash"
                                size={props.compact ? "touch" : "compact"}
                                onPress={() =>
                                    void deleteLibraryDocument(
                                        row().document_id,
                                        "pdf",
                                        row().title,
                                    )
                                }
                            />
                        </View>
                    )}
                </Each>
                <Each
                    of={libraryEpubRows()}
                    key={(row) => `epub-doc-${row.document_id}`}
                >
                    {(row) => (
                        <View
                            style={{
                                paddingVertical: 12,
                                paddingHorizontal: 10,
                                borderBottomWidth: 1,
                                borderColor: ink("border-subtle"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 12,
                                borderRadius: 8,
                            }}
                        >
                            <Button
                                accessibilityLabel={`${
                                    epubPositionOf(row())
                                        ? "继续阅读"
                                        : "阅读"
                                }${row().title}`}
                                onPress={() => void openEpubDocument(row())}
                                style={{
                                    flex: 1,
                                    minWidth: 0,
                                    minHeight: props.compact ? 44 : undefined,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 12,
                                }}
                                hoverStyle={{
                                    backgroundColor: ink("surface-hover"),
                                }}
                                activeStyle={{
                                    backgroundColor: ink("surface-pressed"),
                                }}
                                focusStyle={focus()}
                            >
                                <Badge
                                    icon="book"
                                    size={38}
                                    tone="surface-accent"
                                />
                                <View style={{ flex: 1, gap: 4, minWidth: 0 }}>
                                    <View
                                        style={{
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 6,
                                        }}
                                    >
                                        <View
                                            style={{
                                                paddingHorizontal: 6,
                                                paddingVertical: 1,
                                                borderRadius: 4,
                                                backgroundColor:
                                                    ink("surface-selected"),
                                            }}
                                        >
                                            <Copy size={10} strong>
                                                EPUB
                                            </Copy>
                                        </View>
                                        <View style={{ flex: 1, minWidth: 0 }}>
                                            <Copy size={15} strong>
                                                {row().title}
                                            </Copy>
                                        </View>
                                    </View>
                                    <Copy size={11} muted>
                                        {row().author
                                            ? `${row().author} · `
                                            : ""}
                                        {row().chapter_count} 章 ·{" "}
                                        {Math.max(
                                            1,
                                            Math.round(
                                                row().original_bytes / 1024,
                                            ),
                                        )}{" "}
                                        KiB
                                        {epubPositionLabel(row())}
                                    </Copy>
                                </View>
                                <Copy size={12} muted>
                                    {epubPositionOf(row()) ? "继续" : "阅读"}
                                </Copy>
                            </Button>
                            <Tool
                                label={`删除 ${row().title}`}
                                icon="trash"
                                size={props.compact ? "touch" : "compact"}
                                onPress={() =>
                                    void deleteLibraryDocument(
                                        row().document_id,
                                        "epub",
                                        row().title,
                                    )
                                }
                            />
                        </View>
                    )}
                </Each>
                <If
                    when={
                        !libraryRows().length &&
                        !libraryEpubRows().length &&
                        libraryStatus().includes("没有")
                    }
                >
                    <Copy muted>
                        导入 PDF 或 EPUB 后，资料会出现在这里；当前不会上传原文件。
                    </Copy>
                </If>
                <If when={!!libraryNext()}>
                    <Tool
                        label="加载更多资料"
                        text="加载更多"
                        size={props.compact ? "touch" : "regular"}
                        onPress={() => void loadLibraryPage(libraryNext())}
                    />
                </If>
            </View>
        </Scroll>
    );
}

export const entries: Entry[] = [
    {
        id: "note",
        title: "工作区设计备忘",
        kind: "文稿",
        provider: "文稿",
        icon: "document",
        description: "关于内容、工具与工作区的几条想法",
        render: NoteView,
    },
    {
        id: "canvas",
        title: "从想法到成果",
        kind: "画布",
        provider: "画布",
        icon: "flow",
        description: "收集、整理与形成结果",
        render: CanvasView,
    },
    {
        id: "session",
        title: "下一步的讨论",
        kind: "会话",
        provider: "会话",
        icon: "chat",
        description: "尚未连接 Agent",
        render: SessionView,
    },
    {
        id: "reader",
        title: "留一点空间给思考",
        kind: "阅读",
        provider: "阅读",
        icon: "book",
        description: "自写样本文稿",
        render: ReaderView,
    },
];
type WorkspaceRenderer = {
    kind: string;
    icon: string;
    description: string;
    render: PluginWorkspace["render"];
};

const workspaceRenderers: Record<string, WorkspaceRenderer> = {
    notes_workspace: {
        kind: "想法",
        icon: "pen",
        description: "引用、个人观点和按资料浏览",
        render: (props) => <NotesWorkspace compact={props.compact} />,
    },
    vocab_workspace: {
        kind: "词汇",
        icon: "book",
        description: "词书、复习安排与四档评分",
        render: (props) => <VocabWorkspace compact={props.compact} />,
    },
    reader_workspace: {
        kind: "阅读",
        icon: "book",
        description: "原生阅读界面资格；窗口内样例不写入 Core",
        render: (props) => <ReadingWorkspace compact={props.compact} />,
    },
    library_workspace: {
        kind: "资料库",
        icon: "book",
        description: "本机 Core 的 PDF 元数据列表",
        render: (props) => <LibraryWorkspace compact={props.compact} />,
    },
};

const installedWorkspaces = signal<PluginWorkspace[]>([]);
const installedPackages = signal<PluginPackage[]>([]);
let workspacesBooted = false;

function applyWorkspaceCatalog(value: any) {
    if (!Array.isArray(value?.workspaces)) return false;
    const workspaces: PluginWorkspace[] = [];
    for (const raw of value.workspaces) {
        if (
            typeof raw?.id !== "string" ||
            typeof raw?.package !== "string" ||
            typeof raw?.title !== "string" ||
            typeof raw?.kind !== "string"
        ) {
            continue;
        }
        const renderer = workspaceRenderers[raw.kind];
        if (!renderer) continue;
        workspaces.push({
            id: raw.id,
            package: raw.package,
            title: raw.title,
            kind: renderer.kind,
            role: raw.kind,
            icon: renderer.icon,
            description: renderer.description,
            render: renderer.render,
        });
    }
    installedWorkspaces.set(workspaces);
    return true;
}

export function refreshWorkspaceCatalog(): boolean {
    try {
        const response = JSON.parse(CompositionHost.workspaces());
        return response.ok === true && applyWorkspaceCatalog(response.value);
    } catch (_) {
        // A headless shell has no native contribution catalog.
        return false;
    }
}

function refreshPackageCatalog(): boolean {
    try {
        const response = JSON.parse(CompositionHost.packages());
        if (!response.ok || !Array.isArray(response.value?.packages))
            return false;
        installedPackages.set(
            response.value.packages.filter(
                (raw: any) =>
                    typeof raw?.id === "string" &&
                    typeof raw?.title === "string" &&
                    typeof raw?.version === "string" &&
                    typeof raw?.enabled === "boolean",
            ),
        );
        return true;
    } catch (_) {
        return false;
    }
}

export function setWorkspacePackageEnabled(
    packageId: string,
    enabled: boolean,
): boolean {
    try {
        const response = JSON.parse(
            CompositionHost.setPackageEnabled(packageId, enabled),
        );
        if (!response.ok || !applyWorkspaceCatalog(response.value)) return false;
        refreshPackageCatalog();
        return true;
    } catch (_) {
        return false;
    }
}

export function bootWorkspaceCatalog() {
    if (workspacesBooted) return;
    workspacesBooted = true;
    refreshWorkspaceCatalog();
    refreshPackageCatalog();
}

export function pluginWorkspaces(): PluginWorkspace[] {
    return installedWorkspaces();
}

export function pluginPackages(): PluginPackage[] {
    return installedPackages();
}
