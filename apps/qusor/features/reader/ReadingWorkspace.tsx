import { Scroll, signal, Text, View } from "nsh";
import { Button, Input } from "nsh/base";
import { Badge, Copy, focus, Glyph, ink, Rule, Tool } from "../../ui";
import { Paragraph, Sheet } from "../../shell/editorial";
import { notesSource } from "../notes/NotesWorkspace";

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
export function ReadingWorkspace(props: { compact: boolean }) {
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
