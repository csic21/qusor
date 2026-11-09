import { Canvas, Path2D, Scroll, signal, Text, View, type JSX } from "nsh";
import { Button, Input } from "nsh/base";
import { Badge, Copy, focus, Glyph, ink, Rule, Tool } from "./ui";

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
                        工作区样例
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
        id: "fixture.reading",
        title: "阅读",
        kind: "插件工作区样例",
        icon: "book",
        description: "书库、阅读和工具由插件组织",
        render: (props) => <ReadingWorkspace compact={props.compact} />,
    },
];
