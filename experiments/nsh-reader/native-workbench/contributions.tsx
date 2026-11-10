import { Canvas, Clipboard, keymap, Path2D, Scroll, signal, Text, View, type InputRef, type JSX } from "nsh";
import { Button, Input, Menu, MenuItem, MenuSeparator, TextView } from "nsh/base";
import { Menu as DesktopMenu, type MenuItem as DesktopMenuItem } from "nsh/desktop";
import { NotesHost } from "qusor/pdf-surface";
import {
    Badge,
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

// Contribution fixtures, not an installed SDK or a security boundary.
// This module owns content, geometry and local draft state; the shell owns none of them.
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
    title: string;
    kind: string;
    icon: string;
    description: string;
    render: (props: { compact: boolean }) => JSX.Element;
};
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
    sourceUnavailable?: "reader";
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

function firstLine(value: string) {
    return value.split("\n")[0].trim().slice(0, 60);
}

async function loadLiveNotes() {
    try {
        const raw = await NotesHost.page();
        const result = JSON.parse(raw);
        if (!result.ok || !Array.isArray(result.value?.notes)) return;
        const page = result.value;
        const mapped = page.notes.map((wire: any): Note => ({
            id: `live-${wire.capture_id}-${wire.commit_rev}`,
            title: firstLine(wire.note_preview) || wire.document_title,
            document: wire.document_title,
            page: wire.page_index + 1,
            quote: wire.quote_preview,
            note: wire.note_preview,
            full: wire.note_truncated ? undefined : wire.note_preview,
            date: `提交 ${wire.commit_rev}`,
            scope:
                wire.document_id === page.current_document_id
                    ? "当前资料"
                    : "其他资料",
            starred: false,
            missingOriginal: false,
            captureId: wire.capture_id,
            sourceUnavailable: "reader",
        }));
        notesData.set(mapped);
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

void loadLiveNotes();

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
    return `> ${note.quote}\n\n— 《${note.document}》第 ${note.page} 页`;
}

function noteMenuItems(note: Note): DesktopMenuItem[] {
    return [
        {
            label:
                note.sourceUnavailable === "reader"
                    ? "打开所在页（阅读窗口未接入）"
                    : "打开所在页",
            disabled:
                note.missingOriginal || note.sourceUnavailable === "reader",
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
        { label: "编辑想法（下一增量）", disabled: true },
        { label: "删除想法（需要历史与撤销）", disabled: true },
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
    if (note.sourceUnavailable === "reader") {
        notesCopyStatus.set("阅读窗口尚未接入，无法回源；引文与想法仍可复制。");
        return;
    }
    if (note.missingOriginal) {
        notesCopyStatus.set("原件缺失，无法定位到这一页。");
        return;
    }
    if (note.scope !== "当前资料") {
        notesCopyStatus.set(`先打开《${note.document}》后可定位到这一页。`);
        return;
    }
    notesCopyStatus.set(
        `已请求打开《${note.document}》第 ${note.page} 页；真实回源由阅读插件承接。`,
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
                for (;;) {
                    const raw = await NotesHost.body(
                        captureId,
                        "note",
                        offset,
                        2048,
                    );
                    const result = JSON.parse(raw);
                    if (!result.ok) throw Error();
                    text += result.value.text;
                    if (
                        result.value.next_offset === null ||
                        result.value.next_offset === undefined
                    )
                        break;
                    offset = result.value.next_offset;
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
        notesBody.set({ ...notesBody(), [note.id]: note.full });
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
                                        height: 34,
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
                                            accessibilityLabel={`《${note().document}》第 ${note().page} 页：${note().title}`}
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
                                                        {note().document} · 第{" "}
                                                        {note().page} 页
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
                                                <If
                                                    when={
                                                        note()
                                                            .sourceUnavailable ===
                                                        "reader"
                                                    }
                                                >
                                                    <Copy size={11} muted>
                                                        阅读未接入
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
                                        size="compact"
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

function NoteDetail() {
    const note = () => selectedNote();
    const canOpen = () =>
        !!note() &&
        !note()!.missingOriginal &&
        !note()!.sourceUnavailable &&
        note()!.scope === "当前资料";
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
                    label="返回想法列表"
                    icon="back"
                    text="想法列表"
                    onPress={() => {
                        notesSelected.set("");
                        notesSelection.set("");
                        notesCopyStatus.set("");
                    }}
                />
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy strong>{note()?.document}</Copy>
                    <Copy size={11} muted>
                        第 {note()?.page} 页 · 引用与个人观点分开
                    </Copy>
                </View>
                <Tool
                    label={
                        canOpen()
                            ? "打开所在页"
                            : note()?.sourceUnavailable === "reader"
                              ? "阅读窗口未接入，无法回源"
                              : note()?.missingOriginal
                                ? "原件缺失，无法定位"
                                : "先打开来源资料后可定位"
                    }
                    text="打开所在页"
                    icon="book"
                    disabled={!canOpen()}
                    onPress={() => {
                        if (note()) openNoteSource(note()!);
                    }}
                />
                <Menu
                    trigger={
                        <Tool
                            label="更多想法操作"
                            icon="more"
                            text="更多"
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
                        size="compact"
                        onPress={() => noteCopy(notesSelection(), "选中内容")}
                    />
                    <TextButton
                        label="复制为引用"
                        accessibilityLabel="把选中文本复制为引用"
                        filled
                        size="compact"
                        onPress={() =>
                            note() &&
                            noteCopy(
                                `> ${notesSelection()}\n\n— 《${note()!.document}》第 ${note()!.page} 页`,
                                "引用",
                            )
                        }
                    />
                    <TextButton
                        label="取消选择"
                        accessibilityLabel="取消文本选取"
                        size="compact"
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
                    <Copy size={11} strong muted>
                        我的想法
                    </Copy>
                    <TextView
                        markdown={noteBodyText(note()!)}
                        selectable
                        onSelectionChange={({ text }) =>
                            notesSelection.set(text)
                        }
                    />
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
                                    size="compact"
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
                        <If when={note()?.sourceUnavailable === "reader"}>
                            <Copy size={11} muted>
                                阅读窗口尚未接入，无法回源；引文与想法仍可复制。
                            </Copy>
                        </If>
                        <If
                            when={
                                !note()?.missingOriginal &&
                                !note()?.sourceUnavailable &&
                                note()?.scope !== "当前资料"
                            }
                        >
                            <Copy size={11} muted>
                                先打开《{note()?.document}》后可定位到这一页。
                            </Copy>
                        </If>
                        <If when={canOpen()}>
                            <Copy size={11} muted>
                                真实回源由 Core 与阅读插件承接；上方“打开所在页”执行。
                            </Copy>
                        </If>
                    </View>
                </View>
            </Scroll>
        </View>
    );
}

function NotesWorkspace(props: { compact: boolean }) {
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
                    <NoteDetail />
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
                <NoteDetail />
            </If>
        </View>
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
export const pluginWorkspaces: PluginWorkspace[] = [
    {
        id: "fixture.notes",
        title: "想法",
        kind: "插件工作区样例",
        icon: "pen",
        description: "引用、个人观点和按资料浏览",
        render: (props) => <NotesWorkspace compact={props.compact} />,
    },
    {
        id: "fixture.reading",
        title: "阅读",
        kind: "插件工作区样例",
        icon: "book",
        description: "书库、阅读和工具由插件组织",
        render: (props) => <ReadingWorkspace compact={props.compact} />,
    },
];
