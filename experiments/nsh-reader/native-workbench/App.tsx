import { mount, Scroll, setColorScheme, signal, Text, View } from "nsh";
import { Button, Command, Dialog, Menu, MenuItem, Tab, Tabs } from "nsh/base";
import { titleBar } from "nsh/desktop";
import { entries, Diagram, type Entry } from "./contributions";
import { Badge, Copy, focus, Glyph, ink, Rule, Tool } from "./ui";

// No view-specific route branches in the shell. All contribution identities are fixtures.
const active = signal("overview"),
    opened = signal(["overview", "note", "canvas"]);
const sidebar = signal(true),
    inspector = signal(false),
    library = signal(false),
    dark = signal(false);
const searchOpen = signal(false),
    query = signal("");
const narrow = signal(false),
    frameWidth = signal(1280),
    listOpen = signal(true);
function showSearch() {
    query.set("");
    searchOpen.set(true);
}
const showSidebar = () => sidebar() && !(narrow() && inspector() && !library());
const showLeading = () =>
    listOpen() &&
    !library() &&
    !inspector() &&
    active() !== "overview" &&
    frameWidth() - (showSidebar() ? 200 : 0) >= 900;
function toggleLeading() {
    if (showLeading()) {
        listOpen.set(false);
        return;
    }
    inspector.set(false);
    listOpen.set(true);
    if (frameWidth() < 1100) sidebar.set(false);
}
function revealSidebar() {
    if (narrow()) inspector.set(false);
    sidebar.set(true);
}
function openEntry(id: string) {
    if (!opened().includes(id)) opened.set([...opened(), id]);
    active.set(id);
    library.set(false);
    searchOpen.set(false);
}
function current() {
    return entries.find((e) => e.id === active());
}
function closeActive() {
    const index = opened().indexOf(active());
    if (active() === "overview") return;
    const next = opened().filter((id) => id !== active());
    opened.set(next);
    active.set(next[Math.max(0, index - 1)] ?? "overview");
}
function toggleDark() {
    dark.set(!dark());
    setColorScheme(dark() ? "dark" : "light");
}
function MenuRow(props: { title: string; icon: string; onSelect: () => void }) {
    return (
        <MenuItem
            onSelect={props.onSelect}
            style={{
                paddingHorizontal: 10,
                paddingVertical: 8,
                flexDirection: "row",
                alignItems: "center",
                gap: 10,
                borderRadius: 5,
            }}
            hoverStyle={{ backgroundColor: ink("surface-selected") }}
            focusStyle={{ backgroundColor: ink("surface-selected") }}
        >
            <Glyph name={props.icon} size={16} />
            <Copy>{props.title}</Copy>
        </MenuItem>
    );
}
const popupStyle = () => ({
    padding: 5,
    minWidth: 208,
    borderRadius: 9,
    borderCurve: "continuous" as const,
    backgroundColor: ink("surface-floating"),
    borderWidth: 1,
    borderColor: ink("border-control"),
    boxShadow: "0 7px 24px rgba(0,0,0,0.16)",
});
function NewMenu() {
    return (
        <Menu
            offset={8}
            margin={10}
            popupStyle={popupStyle()}
            trigger={<Tool label="新建内容…" icon="plus" />}
        >
            <View style={{ padding: 9 }}>
                <Copy size={11} muted>
                    打开界面样例
                </Copy>
            </View>
            <Each of={entries}>
                {(entry) => (
                    <MenuRow
                        title={entry().kind}
                        icon={entry().icon}
                        onSelect={() => openEntry(entry().id)}
                    />
                )}
            </Each>
        </Menu>
    );
}
function SidebarRow(props: {
    title: string;
    icon: string;
    selected?: boolean;
    onPress: () => void;
    trailing?: string;
}) {
    return (
        <Button
            accessibilityLabel={props.title}
            onPress={props.onPress}
            style={{
                height: 33,
                flexDirection: "row",
                gap: 10,
                alignItems: "center",
                paddingHorizontal: 10,
                borderRadius: 7,
                borderCurve: "continuous",
                backgroundColor: ink(
                    props.selected ? "surface-selected" : "transparent",
                ),
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <Glyph
                name={props.icon}
                size={17}
                color={ink(props.selected ? "text-accent" : "text-secondary")}
            />
            <View style={{ flex: 1, minWidth: 0 }}>
                <Copy strong={props.selected}>{props.title}</Copy>
            </View>
            <If when={props.trailing}>
                <Copy size={11} muted>
                    {props.trailing}
                </Copy>
            </If>
        </Button>
    );
}
function Sidebar() {
    return (
        <View
            accessibilityElementsHidden={searchOpen()}
            importantForAccessibility={
                searchOpen() ? "no-hide-descendants" : "auto"
            }
            style={{
                width: 200,
                flexShrink: 0,
                backgroundColor: ink("transparent"),
            }}
        >
            <View
                desktop={{ windowDrag: true }}
                style={{
                    height: 52,
                    flexShrink: 0,
                    paddingLeft: titleBar().left + 12,
                    paddingRight: 8,
                    flexDirection: "row",
                    alignItems: "center",
                }}
            >
                <View style={{ flex: 1 }} />
                <Tool
                    label="隐藏侧栏"
                    icon="panel"
                    onPress={() => sidebar.set(false)}
                />
            </View>
            <View
                style={{
                    paddingHorizontal: 12,
                    paddingTop: 7,
                    gap: 17,
                    flex: 1,
                }}
            >
                <View
                    style={{ paddingHorizontal: 10, paddingBottom: 4, gap: 3 }}
                >
                    <Copy size={17} strong>
                        QUSOR
                    </Copy>
                    <Copy size={12} muted>
                        个人空间
                    </Copy>
                </View>
                <Button
                    accessibilityLabel="搜索工作区…"
                    onPress={showSearch}
                    style={{
                        height: 31,
                        borderRadius: 7,
                        paddingHorizontal: 10,
                        gap: 7,
                        flexDirection: "row",
                        alignItems: "center",
                        backgroundColor: ink("surface-search"),
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                    focusStyle={focus()}
                >
                    <Glyph name="search" size={14} />
                    <View style={{ flex: 1 }}>
                        <Copy size={12} muted>
                            搜索
                        </Copy>
                    </View>
                </Button>
                <View style={{ gap: 3 }}>
                    <SidebarRow
                        title="最近内容"
                        icon="clock"
                        onPress={() => openEntry("overview")}
                    />
                    <SidebarRow
                        title="功能"
                        icon="grid"
                        selected={library()}
                        onPress={() => library.set(true)}
                    />
                </View>
                <View style={{ gap: 5 }}>
                    <View style={{ paddingHorizontal: 10, paddingBottom: 3 }}>
                        <Copy size={11} muted strong>
                            工作区
                        </Copy>
                    </View>
                    <SidebarRow
                        title="产品设计"
                        icon="folder"
                        selected={!library()}
                        trailing="4"
                        onPress={() => openEntry("overview")}
                    />
                </View>

                <View style={{ flex: 1 }} />
                <View style={{ paddingBottom: 14, gap: 10 }}>
                    <Rule />
                    <View
                        style={{
                            flexDirection: "row",
                            alignItems: "center",
                            paddingLeft: 9,
                        }}
                    >
                        <Copy size={11} muted>
                            原生界面研究
                        </Copy>
                        <View style={{ flex: 1 }} />
                        <Tool
                            label={dark() ? "切换浅色" : "切换深色"}
                            icon={dark() ? "sun" : "moon"}
                            onPress={toggleDark}
                        />
                    </View>
                </View>
            </View>
        </View>
    );
}
function CollectionPane() {
    return (
        <View
            accessibilityElementsHidden={searchOpen()}
            importantForAccessibility={
                searchOpen() ? "no-hide-descendants" : "auto"
            }
            style={{
                width: 288,
                flexShrink: 0,
                marginTop: 8,
                marginRight: 8,
                marginBottom: 8,
                borderRadius: 12,
                borderCurve: "continuous",
                borderWidth: 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-base"),
                overflow: "hidden",
            }}
        >
            <View
                desktop={{ windowDrag: true }}
                style={{
                    height: 51,
                    flexShrink: 0,
                    paddingLeft: showSidebar() ? 17 : titleBar().left + 12,
                    paddingRight: 9,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 7,
                }}
            >
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy size={16} strong>
                        产品设计
                    </Copy>
                </View>
                <Tool
                    label="收起内容列表"
                    icon="panel"
                    onPress={() => listOpen.set(false)}
                />
            </View>
            <Rule />
            <Scroll style={{ flex: 1 }}>
                <View style={{ padding: 9, gap: 14 }}>
                    <Each
                        of={[
                            { title: "固定", items: entries.slice(0, 2) },
                            { title: "其他内容", items: entries.slice(2) },
                        ]}
                    >
                        {(section) => (
                            <View style={{ gap: 4 }}>
                                <View
                                    style={{
                                        paddingHorizontal: 10,
                                        height: 29,
                                        flexDirection: "row",
                                        alignItems: "center",
                                        gap: 7,
                                        borderRadius: 6,
                                        backgroundColor: ink("surface-inset"),
                                    }}
                                >
                                    <Copy size={12} strong>
                                        {section().title}
                                    </Copy>
                                    <Copy size={11} muted>
                                        {section().items.length}
                                    </Copy>
                                </View>
                                <Each of={section().items}>
                                    {(entry) => (
                                        <Button
                                            accessibilityLabel={`浏览${entry().title}`}
                                            onPress={() =>
                                                openEntry(entry().id)
                                            }
                                            style={{
                                                paddingHorizontal: 10,
                                                height: 78,
                                                borderRadius: 7,
                                                flexDirection: "row",
                                                alignItems: "center",
                                                gap: 10,
                                                backgroundColor: ink(
                                                    active() === entry().id
                                                        ? "surface-selected"
                                                        : "transparent",
                                                ),
                                            }}
                                            hoverStyle={{
                                                backgroundColor:
                                                    ink("surface-hover"),
                                            }}
                                            activeStyle={{
                                                backgroundColor:
                                                    ink("surface-pressed"),
                                            }}
                                            focusStyle={focus()}
                                        >
                                            <Glyph
                                                name={entry().icon}
                                                size={16}
                                                color={ink(
                                                    active() === entry().id
                                                        ? "text-accent"
                                                        : "text-secondary",
                                                )}
                                            />
                                            <View
                                                style={{
                                                    flex: 1,
                                                    minWidth: 0,
                                                    gap: 4,
                                                }}
                                            >
                                                <View
                                                    style={{
                                                        flexDirection: "row",
                                                        gap: 6,
                                                        alignItems: "center",
                                                    }}
                                                >
                                                    <View
                                                        style={{
                                                            flex: 1,
                                                            minWidth: 0,
                                                        }}
                                                    >
                                                        <Copy size={13} strong>
                                                            {entry().title}
                                                        </Copy>
                                                    </View>
                                                    <Copy size={11} muted>
                                                        {entry().kind}
                                                    </Copy>
                                                </View>
                                                <Copy size={12} muted>
                                                    {entry().description}
                                                </Copy>
                                            </View>
                                        </Button>
                                    )}
                                </Each>
                            </View>
                        )}
                    </Each>
                </View>
            </Scroll>
            <View style={{ padding: 13 }}>
                <Copy size={11} muted>
                    4 个内容 · 当前工作区
                </Copy>
            </View>
        </View>
    );
}
function ContentRow(props: { entry: Entry }) {
    return (
        <Button
            accessibilityLabel={`打开${props.entry.title}`}
            onPress={() => openEntry(props.entry.id)}
            style={{
                flexDirection: "row",
                alignItems: "center",
                gap: 13,
                height: 68,
                paddingVertical: 0,
                paddingHorizontal: 12,
                borderRadius: 7,
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <Badge icon={props.entry.icon} size={35} />
            <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
                <Copy size={13} strong>
                    {props.entry.title}
                </Copy>
                <Copy size={12} muted>
                    {props.entry.description}
                </Copy>
            </View>
            <View style={{ width: 52 }}>
                <Copy size={12} muted>
                    {props.entry.kind}
                </Copy>
            </View>
            <Glyph name="arrow" size={15} />
        </Button>
    );
}
function Overview() {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    paddingHorizontal: 40,
                    paddingTop: 30,
                    paddingBottom: 40,
                    width: "100%",
                    maxWidth: 1120,
                    alignSelf: "center",
                    gap: 27,
                }}
            >
                <View
                    style={{
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 14,
                    }}
                >
                    <Badge icon="folder" size={43} tone="surface-accent" />
                    <View style={{ gap: 4, flex: 1 }}>
                        <Copy size={25} strong>
                            产品设计
                        </Copy>
                        <Copy size={12} muted>
                            4 个内容 · 个人空间
                        </Copy>
                    </View>
                    <Copy size={12} muted>
                        工作区
                    </Copy>
                </View>
                <View style={{ gap: 12 }}>
                    <Copy size={12} muted strong>
                        固定
                    </Copy>
                    <View style={{ flexDirection: "row", gap: 18 }}>
                        <Button
                            accessibilityLabel="打开固定的工作区设计备忘"
                            onPress={() => openEntry("note")}
                            style={{
                                flex: 1,
                                minWidth: 0,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 9,
                                borderCurve: "continuous",
                                overflow: "hidden",
                                backgroundColor: ink("surface-preview"),
                            }}
                            hoverStyle={{ borderColor: ink("border-control") }}
                            focusStyle={focus()}
                        >
                            <View style={{ padding: 20, gap: 9, height: 132 }}>
                                <Copy size={16} strong>
                                    工作区设计备忘
                                </Copy>
                                <Copy size={11} muted>
                                    共同的是工作区，变化的是看待内容的方式。
                                </Copy>
                                <View
                                    style={{
                                        height: 1,
                                        backgroundColor: ink("border-subtle"),
                                        marginTop: 2,
                                        width: "82%",
                                    }}
                                />
                                <Copy size={11} muted>
                                    先把内容放在手边
                                </Copy>
                            </View>
                            <Rule />
                            <View
                                style={{
                                    paddingHorizontal: 14,
                                    height: 37,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 8,
                                    backgroundColor: ink("surface-base"),
                                }}
                            >
                                <Glyph name="document" size={14} />
                                <Copy size={12}>设计备忘</Copy>
                            </View>
                        </Button>
                        <Button
                            accessibilityLabel="打开固定的从想法到成果"
                            onPress={() => openEntry("canvas")}
                            style={{
                                flex: 1,
                                minWidth: 0,
                                borderWidth: 1,
                                borderColor: ink("border-subtle"),
                                borderRadius: 9,
                                borderCurve: "continuous",
                                overflow: "hidden",
                                backgroundColor: ink("surface-canvas"),
                            }}
                            hoverStyle={{ borderColor: ink("border-control") }}
                            focusStyle={focus()}
                        >
                            <Diagram small />
                            <Rule />
                            <View
                                style={{
                                    paddingHorizontal: 14,
                                    height: 37,
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 8,
                                    backgroundColor: ink("surface-base"),
                                }}
                            >
                                <Glyph name="flow" size={14} />
                                <Copy size={12}>从想法到成果</Copy>
                            </View>
                        </Button>
                    </View>
                </View>
                <View style={{ gap: 9 }}>
                    <View
                        style={{
                            paddingBottom: 2,
                            flexDirection: "row",
                            alignItems: "center",
                        }}
                    >
                        <Copy size={12} muted strong>
                            工作区内容
                        </Copy>
                        <View style={{ flex: 1 }} />
                        <Copy size={11} muted>
                            4 项
                        </Copy>
                    </View>
                    <Rule />
                    <Each of={entries}>
                        {(entry) => (
                            <View>
                                <ContentRow entry={entry()} />
                                <Rule />
                            </View>
                        )}
                    </Each>
                </View>
            </View>
        </Scroll>
    );
}
function Library() {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    padding: 40,
                    maxWidth: 870,
                    width: "100%",
                    alignSelf: "center",
                    gap: 24,
                }}
            >
                <Copy size={25} strong>
                    功能
                </Copy>
                <Copy muted>选择一种适合当前工作的视图</Copy>
                <Copy size={12} muted>
                    以下为界面样例，尚未安装真实插件包
                </Copy>
                <Each of={entries}>
                    {(entry) => (
                        <View
                            style={{
                                paddingVertical: 18,
                                borderBottomWidth: 1,
                                borderColor: ink("border-subtle"),
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 15,
                            }}
                        >
                            <Badge icon={entry().icon} size={42} />
                            <View style={{ flex: 1, gap: 4 }}>
                                <Copy size={15} strong>
                                    {entry().kind}
                                </Copy>
                                <Copy size={12} muted>
                                    {entry().description}
                                </Copy>
                            </View>
                            <Tool
                                label={`打开${entry().kind}样例`}
                                text="打开样例"
                                raised
                                onPress={() => openEntry(entry().id)}
                            />
                        </View>
                    )}
                </Each>
            </View>
        </Scroll>
    );
}
function Inspector() {
    return (
        <View
            style={{
                width: 256,
                flexShrink: 0,
                borderLeftWidth: 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-inspector"),
            }}
        >
            <View
                style={{
                    height: 43,
                    paddingHorizontal: 17,
                    flexDirection: "row",
                    alignItems: "center",
                }}
            >
                <Copy strong>信息</Copy>
                <View style={{ flex: 1 }} />
                <Tool
                    label="关闭信息"
                    icon="close"
                    onPress={() => inspector.set(false)}
                />
            </View>
            <Rule />
            <View style={{ padding: 23, gap: 19 }}>
                <Badge icon={current()?.icon ?? "folder"} size={52} />
                <Copy size={17} strong>
                    {current()?.title ?? "产品设计"}
                </Copy>
                <Copy size={12} muted>
                    {current()?.kind ?? "工作区"}
                </Copy>
                <Rule />
                <Copy size={11} muted>
                    所在位置
                </Copy>
                <Copy>个人空间 / 产品设计</Copy>
                <Copy size={11} muted>
                    内容状态
                </Copy>
                <Copy>窗口内样例</Copy>
                <Rule />
                <Copy size={12} muted lines={4}>
                    此窗口用于检查布局与交互，内容尚未接入正式保存与恢复。
                </Copy>
            </View>
        </View>
    );
}
function Search() {
    return (
        <Dialog
            accessibilityLabel="搜索当前工作区"
            open={searchOpen()}
            closeOnEscape={false}
            onOpenChange={({ open }) => searchOpen.set(open)}
            animation="none"
            backdropStyle={{ backgroundColor: ink("surface-scrim") }}
            style={{
                position: "absolute",
                top: 96,
                left: Math.max(16, (frameWidth() - 520) / 2),
                width: 520,
                padding: 8,
                borderRadius: 14,
                borderCurve: "continuous",
                backgroundColor: ink("surface-floating"),
                borderWidth: 1,
                borderColor: ink("border-control"),
                boxShadow: "0 16px 60px rgba(0,0,0,0.22)",
            }}
        >
            <If when={searchOpen()}>
                <Command
                    autoFocus
                    accessibilityLabel="工作区内容"
                    searchAccessibilityLabel="搜索内容"
                    placeholder="搜索当前工作区"
                    query={query()}
                    onQueryChange={({ query: value }) => query.set(value)}
                    items={entries.map((e) => ({
                        id: e.id,
                        label: e.title,
                        keywords: [e.kind, e.description],
                    }))}
                    onSelect={({ id }) => openEntry(id)}
                    onCancel={() => searchOpen.set(false)}
                    searchPrefix={<Glyph name="search" size={18} />}
                    searchSuffix={
                        <Tool
                            label="关闭搜索"
                            icon="close"
                            onPress={() => searchOpen.set(false)}
                        />
                    }
                    searchBarStyle={{
                        height: 50,
                        flexDirection: "row",
                        gap: 9,
                        alignItems: "center",
                        paddingHorizontal: 9,
                        borderBottomWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                    searchStyle={{
                        flex: 1,
                        height: 42,
                        fontSize: 16,
                        color: ink("text-primary"),
                        backgroundColor: ink("transparent"),
                        borderWidth: 0,
                    }}
                    listStyle={{ maxHeight: 320, padding: 5 }}
                    itemClass="rounded-md focus:bg-accent hover:bg-muted"
                    itemStyle={{
                        height: 57,
                        paddingHorizontal: 10,
                        color: ink("text-primary"),
                        fontSize: 13,
                    }}
                    renderItem={(entry) => (
                        <View
                            style={{
                                height: 57,
                                flexDirection: "row",
                                gap: 12,
                                alignItems: "center",
                            }}
                        >
                            <Badge
                                icon={
                                    entries.find((e) => e.id === entry.id)
                                        ?.icon ?? "document"
                                }
                                size={30}
                            />
                            <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                                <Copy strong>{entry.label}</Copy>
                                <Copy size={11} muted>
                                    {
                                        entries.find((e) => e.id === entry.id)
                                            ?.kind
                                    }
                                </Copy>
                            </View>
                            <Glyph name="arrow" size={14} />
                        </View>
                    )}
                    empty={
                        <View style={{ padding: 22 }}>
                            <Copy muted>没有匹配的内容</Copy>
                        </View>
                    }
                    footer={
                        <View
                            style={{
                                paddingHorizontal: 14,
                                paddingVertical: 7,
                            }}
                        >
                            <Copy size={11} muted>
                                ↑ ↓ 选择　↵ 打开　Esc 清空 / 返回
                            </Copy>
                        </View>
                    }
                />
            </If>
        </Dialog>
    );
}
function App() {
    return (
        <View
            onLayout={({ bounds }) => {
                narrow.set(bounds.size.width < 1120);
                frameWidth.set(bounds.size.width);
            }}
            style={{
                flex: 1,
                flexDirection: "row",
                backgroundColor: ink("surface-frame"),
            }}
        >
            <If when={showSidebar()}>
                <Sidebar />
            </If>
            <If when={showLeading()}>
                <CollectionPane />
            </If>
            <View
                accessibilityElementsHidden={searchOpen()}
                importantForAccessibility={
                    searchOpen() ? "no-hide-descendants" : "auto"
                }
                style={{
                    flex: 1,
                    minWidth: 0,
                    marginTop: 8,
                    marginRight: 8,
                    marginBottom: 8,
                    borderRadius: 12,
                    borderCurve: "continuous",
                    borderWidth: 1,
                    borderColor: ink("border-subtle"),
                    overflow: "hidden",
                    backgroundColor: ink("surface-base"),
                }}
            >
                <View
                    desktop={{ windowDrag: true }}
                    style={{
                        height: 52,
                        flexShrink: 0,
                        paddingLeft:
                            showSidebar() || showLeading()
                                ? 14
                                : titleBar().left + 12,
                        paddingRight: titleBar().right + 13,
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 8,
                        backgroundColor: ink("surface-toolbar"),
                    }}
                >
                    <If when={!showSidebar()}>
                        <Tool
                            label={
                                narrow() && inspector()
                                    ? "关闭信息并显示侧栏"
                                    : "显示侧栏"
                            }
                            icon="panel"
                            onPress={revealSidebar}
                        />
                    </If>
                    <Tool
                        label="返回工作区"
                        icon="back"
                        disabled={active() === "overview" && !library()}
                        onPress={() => openEntry("overview")}
                    />
                    <If when={active() !== "overview" && !library()}>
                        <Tool
                            label={
                                showLeading() ? "隐藏内容列表" : "显示内容列表"
                            }
                            icon="panel"
                            selected={showLeading()}
                            onPress={toggleLeading}
                        />
                    </If>
                    <View style={{ flex: 1, minWidth: 0 }}>
                        <Copy strong>
                            {library()
                                ? "功能"
                                : (current()?.title ?? "产品设计")}
                        </Copy>
                    </View>
                    <View style={{ flex: 1 }} />
                    <Tool
                        label="搜索内容…"
                        icon="search"
                        onPress={showSearch}
                    />
                    <NewMenu />
                    <Tool
                        label="显示信息"
                        icon="info"
                        selected={inspector()}
                        onPress={() => inspector.set(!inspector())}
                    />
                    <Menu
                        offset={8}
                        margin={10}
                        popupStyle={popupStyle()}
                        trigger={<Tool label="更多工作区操作…" icon="more" />}
                    >
                        <MenuRow
                            title={dark() ? "浅色外观" : "深色外观"}
                            icon={dark() ? "sun" : "moon"}
                            onSelect={toggleDark}
                        />
                        <MenuRow
                            title="功能样例"
                            icon="grid"
                            onSelect={() => library.set(true)}
                        />
                        <MenuRow
                            title="关闭当前标签"
                            icon="close"
                            onSelect={closeActive}
                        />
                    </Menu>
                </View>
                <Rule />
                <If when={!library()}>
                    <View
                        style={{
                            height: 40,
                            flexDirection: "row",
                            alignItems: "center",
                            paddingHorizontal: 10,
                            backgroundColor: ink("surface-tabs"),
                            gap: 3,
                        }}
                    >
                        <Tabs
                            style={{
                                flex: 1,
                                height: 32,
                                flexDirection: "row",
                                gap: 3,
                            }}
                            indicatorStyle={{
                                top: 1,
                                bottom: 1,
                                borderRadius: 6,
                                backgroundColor: ink("surface-active-tab"),
                            }}
                        >
                            <Each of={opened()}>
                                {(id) => (
                                    <Tab
                                        accessibilityLabel={
                                            id() === "overview"
                                                ? "工作区概览"
                                                : entries.find(
                                                      (e) => e.id === id(),
                                                  )!.title
                                        }
                                        selected={active() === id()}
                                        onPress={() => active.set(id())}
                                        style={{
                                            paddingHorizontal: 14,
                                            minWidth: 86,
                                            flexDirection: "row",
                                            alignItems: "center",
                                            gap: 7,
                                            borderRadius: 6,
                                        }}
                                        focusStyle={focus()}
                                    >
                                        <Glyph
                                            name={
                                                id() === "overview"
                                                    ? "grid"
                                                    : entries.find(
                                                          (e) => e.id === id(),
                                                      )!.icon
                                            }
                                            size={14}
                                            color={ink(
                                                active() === id()
                                                    ? "text-primary"
                                                    : "text-secondary",
                                            )}
                                        />
                                        <Copy
                                            size={12}
                                            muted={active() !== id()}
                                        >
                                            {id() === "overview"
                                                ? "概览"
                                                : entries.find(
                                                      (e) => e.id === id(),
                                                  )!.title}
                                        </Copy>
                                    </Tab>
                                )}
                            </Each>
                        </Tabs>
                        <If when={active() !== "overview"}>
                            <Tool
                                label="关闭当前标签"
                                icon="close"
                                onPress={closeActive}
                            />
                        </If>
                    </View>
                    <Rule />
                </If>
                <View style={{ flex: 1, flexDirection: "row", minHeight: 0 }}>
                    <View style={{ flex: 1, minWidth: 0 }}>
                        <If when={library()}>
                            <Library />
                        </If>
                        <If when={!library() && active() === "overview"}>
                            <Overview />
                        </If>
                        <Each of={entries}>
                            {(entry) => (
                                <If
                                    when={!library() && active() === entry().id}
                                >
                                    {entry().render()}
                                </If>
                            )}
                        </Each>
                    </View>
                    <If when={inspector() && !library()}>
                        <Inspector />
                    </If>
                </View>
            </View>
            <Search />
        </View>
    );
}
setColorScheme("light");
mount(App);
