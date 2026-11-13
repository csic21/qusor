import {
    Clipboard,
    effect,
    mount,
    Picker,
    Scroll,
    setColorScheme,
    signal,
    Text,
    View,
    windowSize,
} from "nsh";
import { AppInfo, NotesHost, VocabHost } from "qusor/pdf-surface";
import { Button, Command, Dialog, Menu, MenuItem, Tab, Tabs } from "nsh/base";
import { titleBar } from "nsh/desktop";
import { BackHandler } from "nsh/mobile";
import {
    captureNotesEvidence,
    captureVocabEvidence,
    bootWorkspaceCatalog,
    entries,
    pluginPackages,
    pluginWorkspaces,
    refreshNotes,
    setPluginWorkspaceOpener,
    setWorkspacePackageEnabled,
    Diagram,
    type Entry,
} from "./contributions";
import {
    Badge,
    Copy,
    focus,
    Glyph,
    ink,
    menuItem,
    menuPopup,
    ProviderIdentity,
    Rule,
    Tool,
} from "./ui";
import {
    appVersion,
    openUpdatePanel,
    runUpdateEvidence,
    UpdateDialog,
    updateOpen,
} from "./update";

// The shell branches by generic contribution role; identities remain fixtures.
const active = signal("overview"),
    opened = signal(["overview", "note", "canvas"]),
    activePlugin = signal("");
const sidebar = signal(windowSize().width >= 1120),
    inspector = signal(false),
    library = signal(false),
    designPreview = signal(false),
    dark = signal(false);
const searchOpen = signal(false),
    query = signal("");
const navigationScope = signal("space");
const pluginOrigin = signal<{
    active: string;
    library: boolean;
    sidebar: boolean;
    inspector: boolean;
    navigationScope: string;
} | null>(null);
function showSearch() {
    query.set("");
    searchOpen.set(true);
}
function narrow() {
    return windowSize().width < 1120;
}
function frameWidth() {
    return windowSize().width;
}
const showSidebar = () =>
    !activePlugin() &&
    sidebar() &&
    !(narrow() && inspector() && !library());
const showMobileHome = () =>
    narrow() &&
    !activePlugin() &&
    !library() &&
    !designPreview() &&
    active() === "overview";
function revealSidebar() {
    if (narrow()) inspector.set(false);
    sidebar.set(true);
}
function toggleNavigation() {
    if (showSidebar()) sidebar.set(false);
    else revealSidebar();
}
function openEntry(id: string) {
    if (!opened().includes(id)) opened.set([...opened(), id]);
    activePlugin.set("");
    pluginOrigin.set(null);
    active.set(id);
    library.set(false);
    designPreview.set(true);
    navigationScope.set("workspace");
    searchOpen.set(false);
    if (narrow()) sidebar.set(false);
}
function openMobileHome() {
    if (activePlugin()) {
        activePlugin.set("");
        pluginOrigin.set(null);
    }
    library.set(false);
    inspector.set(false);
    designPreview.set(false);
    active.set("overview");
    navigationScope.set("space");
    searchOpen.set(false);
    if (narrow()) sidebar.set(false);
}
function openLibrary() {
    if (activePlugin()) closePluginWorkspace();
    library.set(true);
    inspector.set(false);
    navigationScope.set("space");
    searchOpen.set(false);
    if (narrow()) sidebar.set(false);
}
function currentPlugin() {
    return allWorkspaces().find((workspace) => workspace.id === activePlugin());
}
function openPluginWorkspace(id: string) {
    const workspace = allWorkspaces().find((candidate) => candidate.id === id);
    if (!workspace) return;
    if (narrow()) sidebar.set(false);
    if (!activePlugin()) {
        pluginOrigin.set({
            active: active(),
            library: library(),
            sidebar: sidebar(),
            inspector: inspector(),
            navigationScope: navigationScope(),
        });
    }
    activePlugin.set(id);
    library.set(false);
    inspector.set(false);
    searchOpen.set(false);
    if (workspace.role === "notes_workspace") refreshNotes();
}
function openWorkspaceOrLibrary(role: string) {
    if (!openPluginWorkspaceByRole(role)) openLibrary();
}
function allWorkspaces() {
    return pluginWorkspaces();
}
function togglePluginPackage(packageId: string, enabled: boolean) {
    const activePackage = currentPlugin()?.package;
    if (!setWorkspacePackageEnabled(packageId, enabled)) return;
    if (!enabled && activePackage === packageId) closePluginWorkspace();
}
function openPluginWorkspaceByRole(role: string) {
    const workspace = pluginWorkspaces().find(
        (candidate) => candidate.role === role,
    );
    if (!workspace) return false;
    openPluginWorkspace(workspace.id);
    return true;
}
setPluginWorkspaceOpener(openPluginWorkspaceByRole);
function closePluginWorkspace() {
    activePlugin.set("");
    const origin = pluginOrigin();
    if (origin) {
        active.set(origin.active);
        library.set(origin.library);
        sidebar.set(origin.sidebar);
        inspector.set(origin.inspector);
        navigationScope.set(origin.navigationScope);
    }
    pluginOrigin.set(null);
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
            style={menuItem()}
            hoverStyle={{ backgroundColor: ink("surface-selected") }}
            focusStyle={{ backgroundColor: ink("surface-selected") }}
        >
            <Glyph name={props.icon} size={16} />
            <Copy>{props.title}</Copy>
        </MenuItem>
    );
}
function NewMenu() {
    return (
        <Menu
            offset={8}
            margin={10}
            popupStyle={menuPopup()}
            trigger={<Tool label="新建内容…" icon="plus" />}
        >
            <View style={{ padding: 9 }}>
                <Copy size={11} muted>
                    打开内容样例
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
                height: narrow() ? 44 : 34,
                flexDirection: "row",
                gap: 8,
                alignItems: "center",
                paddingHorizontal: 10,
                borderRadius: 8,
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
                flex: 1,
                backgroundColor: ink("transparent"),
            }}
        >
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
                    <Copy size={15} strong>
                        个人空间
                    </Copy>
                </View>
                <Button
                    accessibilityLabel="搜索工作区…"
                    onPress={showSearch}
                    style={{
                        height: 32,
                        borderRadius: 7,
                        paddingHorizontal: 10,
                        gap: 8,
                        flexDirection: "row",
                        alignItems: "center",
                        backgroundColor: ink("surface-search"),
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                    }}
                    focusStyle={focus()}
                >
                    <Glyph name="search" size={15} />
                    <View style={{ flex: 1 }}>
                        <Copy size={12} muted>
                            搜索
                        </Copy>
                    </View>
                </Button>
                <View style={{ gap: 3 }}>
                    <If when={narrow()}>
                        <SidebarRow
                            title="工作台首页"
                            icon="grid"
                            selected={showMobileHome()}
                            onPress={openMobileHome}
                        />
                    </If>
                    <SidebarRow
                        title="最近内容"
                        icon="clock"
                        onPress={() => openEntry("overview")}
                    />
                    <SidebarRow
                        title={narrow() ? "数据与功能" : "功能"}
                        icon="sliders"
                        selected={library()}
                        onPress={openLibrary}
                    />
                </View>
                <View style={{ gap: 5 }}>
                    <View style={{ paddingHorizontal: 10, paddingBottom: 3 }}>
                        <Copy size={11} muted strong>
                            扩展工作区
                        </Copy>
                    </View>
                    <Each of={pluginWorkspaces()}>
                        {(workspace) => (
                            <SidebarRow
                                title={workspace().title}
                                icon={workspace().icon}
                                onPress={() =>
                                    openPluginWorkspace(workspace().id)
                                }
                                trailing="插件"
                            />
                        )}
                    </Each>
                </View>
                <View style={{ gap: 5 }}>
                    <View style={{ paddingHorizontal: 10, paddingBottom: 3 }}>
                        <Copy size={11} muted strong>
                            工作区
                        </Copy>
                    </View>
                    <SidebarRow
                        title={narrow() ? "设计样例" : "产品设计"}
                        icon="folder"
                        selected={!library() && !showMobileHome()}
                        trailing="4"
                        onPress={() => openEntry("overview")}
                    />
                </View>

                <View style={{ flex: 1 }} />
                <View style={{ paddingBottom: 14, gap: 10 }}>
                    <Rule />
                    <SidebarRow
                        title="软件更新"
                        icon="arrow"
                        trailing={appVersion()}
                        onPress={openUpdatePanel}
                    />
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
                flex: 1,
                minHeight: 0,
                overflow: "hidden",
            }}
        >
            <View
                style={{
                    height: 43,
                    flexShrink: 0,
                    paddingHorizontal: 19,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 7,
                }}
            >
                <View style={{ flex: 1, minWidth: 0 }}>
                    <Copy size={12} muted strong>
                        工作区内容
                    </Copy>
                </View>
                <Copy size={11} muted>
                    4 项
                </Copy>
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
function NavigationPane(props: { compact: boolean }) {
    return (
        <View
            accessibilityLabel="工作区导航"
            accessibilityElementsHidden={searchOpen()}
            importantForAccessibility={
                searchOpen() ? "no-hide-descendants" : "auto"
            }
            style={{
                width: props.compact ? "100%" : 248,
                flex: props.compact ? 1 : 0,
                flexShrink: 0,
                paddingLeft: props.compact ? "env(safe-area-inset-left)" : 0,
                paddingRight: props.compact ? "env(safe-area-inset-right)" : 0,
            }}
        >
            <Tabs
                style={{
                    marginHorizontal: 12,
                    marginTop: 5,
                    marginBottom: 10,
                    height: props.compact ? 44 : 32,
                    flexShrink: 0,
                    flexDirection: "row",
                    padding: 2,
                    borderRadius: 7,
                    backgroundColor: ink("surface-inset"),
                }}
                indicatorStyle={{
                    top: 2,
                    bottom: 2,
                    borderRadius: 5,
                    backgroundColor: ink("surface-active-tab"),
                }}
            >
                <Each
                    of={[
                        { id: "space", title: "空间" },
                        {
                            id: "workspace",
                            title: props.compact ? "设计样例" : "产品设计",
                        },
                    ]}
                >
                    {(scope) => (
                        <Tab
                            accessibilityLabel={`${scope().title}导航`}
                            selected={navigationScope() === scope().id}
                            onPress={() => navigationScope.set(scope().id)}
                            style={{
                                flex: 1,
                                alignItems: "center",
                                justifyContent: "center",
                                borderRadius: 5,
                            }}
                            focusStyle={focus()}
                        >
                            <Copy
                                size={12}
                                strong={navigationScope() === scope().id}
                                muted={navigationScope() !== scope().id}
                            >
                                {scope().title}
                            </Copy>
                        </Tab>
                    )}
                </Each>
            </Tabs>
            <If when={navigationScope() === "space"}>
                <Sidebar />
            </If>
            <If when={navigationScope() === "workspace"}>
                <CollectionPane />
            </If>
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
function MobileHomeCard(props: {
    title: string;
    subtitle: string;
    icon: string;
    tone?: string;
    onPress: () => void;
}) {
    return (
        <Button
            accessibilityLabel={props.title}
            onPress={props.onPress}
            style={{
                minHeight: 72,
                paddingHorizontal: 14,
                paddingVertical: 12,
                borderRadius: 12,
                borderCurve: "continuous",
                borderWidth: 1,
                borderColor: ink("border-subtle"),
                backgroundColor: ink("surface-control"),
                flexDirection: "row",
                alignItems: "center",
                gap: 13,
            }}
            hoverStyle={{ backgroundColor: ink("surface-hover") }}
            activeStyle={{ backgroundColor: ink("surface-pressed") }}
            focusStyle={focus()}
        >
            <Badge
                icon={props.icon}
                size={42}
                tone={props.tone ?? "surface-accent"}
            />
            <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
                <Copy size={15} strong>
                    {props.title}
                </Copy>
                <Copy size={12} muted lines={2}>
                    {props.subtitle}
                </Copy>
            </View>
            <Glyph name="arrow" size={16} />
        </Button>
    );
}
function MobileHome() {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    paddingHorizontal: 16,
                    paddingTop: 18,
                    paddingBottom: 28,
                    gap: 18,
                }}
            >
                <View style={{ gap: 4, paddingHorizontal: 2 }}>
                    <Copy size={22} strong>
                        QUSOR 工作台
                    </Copy>
                    <Copy size={12} muted>
                        本地优先 · 阅读、记忆与想法空间
                    </Copy>
                </View>
                <View style={{ gap: 11 }}>
                    <MobileHomeCard
                        title="资料库（PDF · EPUB）"
                        subtitle="导入与阅读书籍文档，支持选文摘录"
                        icon="book"
                        onPress={() =>
                            openWorkspaceOrLibrary("library_workspace")
                        }
                    />
                    <MobileHomeCard
                        title="背单词"
                        subtitle="FSRS-5 间隔记忆与词汇复习"
                        icon="clock"
                        onPress={() =>
                            openWorkspaceOrLibrary("vocab_workspace")
                        }
                    />
                    <MobileHomeCard
                        title="想法"
                        subtitle="随时记录灵感与整理摘录片段"
                        icon="pen"
                        onPress={() =>
                            openWorkspaceOrLibrary("notes_workspace")
                        }
                    />
                    <MobileHomeCard
                        title="数据备份与管理"
                        subtitle="扩展管理、功能列表与软件更新"
                        icon="sliders"
                        tone="surface-inset"
                        onPress={openLibrary}
                    />
                </View>
                <Rule />
                <View
                    style={{
                        flexDirection: "row",
                        flexWrap: "wrap",
                        gap: 10,
                    }}
                >
                    <Tool
                        label="搜索内容"
                        icon="search"
                        text="搜索内容"
                        raised
                        size="touch"
                        onPress={showSearch}
                    />
                    <Tool
                        label="打开设计样例"
                        icon="folder"
                        text="设计样例"
                        raised
                        size="touch"
                        onPress={() => openEntry("overview")}
                    />
                    <Tool
                        label="软件更新"
                        icon="arrow"
                        text={`更新 ${appVersion()}`}
                        raised
                        size="touch"
                        onPress={openUpdatePanel}
                    />
                </View>
            </View>
        </Scroll>
    );
}
function Overview() {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    paddingHorizontal: narrow() ? 16 : 40,
                    paddingTop: narrow() ? 18 : 30,
                    paddingBottom: narrow() ? 28 : 40,
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
                    <View
                        style={{
                            flexDirection: narrow() ? "column" : "row",
                            gap: 18,
                        }}
                    >
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
const backupBusy = signal(false);
const backupStatus = signal(
    "可将本机 Core 中的词书、复习记录、PDF/EPUB 元数据与想法摘录导出为 JSON 备份，或从备份恢复。",
);
const backupPath = signal("");
const backupJson = signal("");
const backupBytes = signal(0);

async function exportCoreBackup() {
    if (backupBusy()) return;
    const host = AppInfo as any;
    if (typeof host?.exportBackup !== "function") {
        backupStatus.set("当前宿主尚未启用备份导出接口。");
        return;
    }
    backupBusy.set(true);
    backupStatus.set("正在生成本机 Core 完整备份…");
    try {
        const raw = await host.exportBackup();
        const result = JSON.parse(raw);
        if (!result?.ok || !result.value) {
            throw new Error(String(result?.error ?? "导出失败"));
        }
        const path = String(result.value.path ?? "");
        const json = String(result.value.json ?? "");
        const bytes = Number(result.value.bytes ?? json.length);
        backupPath.set(path);
        backupJson.set(json);
        backupBytes.set(bytes);
        backupStatus.set(
            `备份已生成（${Math.max(1, Math.round(bytes / 1024))} KiB）${
                path ? ` · ${path}` : ""
            }`,
        );
    } catch (error) {
        backupStatus.set(
            `备份导出未完成：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    } finally {
        backupBusy.set(false);
    }
}

async function shareCoreBackup() {
    const path = backupPath();
    if (!path || backupBusy()) return;
    const host = AppInfo as any;
    if (typeof host?.shareBackup !== "function") {
        backupStatus.set("当前宿主不支持系统分享，可直接复制备份 JSON。");
        return;
    }
    try {
        const raw = await host.shareBackup(path);
        const result = JSON.parse(raw);
        if (!result?.ok) {
            throw new Error(String(result?.error ?? "分享未完成"));
        }
        backupStatus.set(`已打开系统分享/导出：${path}`);
    } catch (error) {
        backupStatus.set(
            `未能调起系统分享：${
                error instanceof Error ? error.message : String(error)
            }`,
        );
    }
}

async function copyCoreBackupJson() {
    const json = backupJson();
    if (!json) return;
    try {
        await Clipboard.writeText(json);
        backupStatus.set("已将完整备份 JSON 复制到剪贴板。");
    } catch (_) {
        backupStatus.set("复制未完成，请重试或使用文件导出。");
    }
}

async function restoreCoreBackup() {
    if (backupBusy()) return;
    const host = AppInfo as any;
    if (typeof host?.restoreBackup !== "function") {
        backupStatus.set("当前宿主尚未启用备份恢复接口。");
        return;
    }
    backupBusy.set(true);
    try {
        const picked = await Picker.pickFiles({
            multiple: false,
            filters: [{ name: "QUSOR 备份 JSON", extensions: ["json"] }],
        });
        if (picked.length === 0) {
            backupStatus.set("已取消备份恢复；本机数据未更改。");
            return;
        }
        const text = await picked[0].file.text();
        if (!text.trim()) throw new Error("备份文件为空");
        const raw = await host.restoreBackup(text);
        const result = JSON.parse(raw);
        if (!result?.ok) {
            throw new Error(String(result?.error ?? "恢复失败"));
        }
        refreshNotes();
        backupStatus.set("已从备份文件恢复本机 Core 数据。");
    } catch (error) {
        backupStatus.set(
            `备份恢复失败：${
                error instanceof Error ? error.message : String(error)
            }。原有数据未损坏。`,
        );
    } finally {
        backupBusy.set(false);
    }
}

function Library() {
    return (
        <Scroll style={{ flex: 1 }}>
            <View
                style={{
                    padding: narrow() ? 18 : 40,
                    maxWidth: 870,
                    width: "100%",
                    alignSelf: "center",
                    gap: 24,
                }}
            >
                <Copy size={25} strong>
                    {narrow() ? "数据备份与管理" : "功能与数据管理"}
                </Copy>
                <View
                    style={{
                        padding: 16,
                        gap: 12,
                        borderRadius: 12,
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
                            gap: 10,
                        }}
                    >
                        <Badge icon="download" size={36} tone="surface-accent" />
                        <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
                            <Copy size={16} strong>
                                数据备份与恢复（本机 Core）
                            </Copy>
                            <Copy size={12} muted lines={2}>
                                导出完整 JSON 备份或从本地文件恢复；凭据不进入导出文件。
                            </Copy>
                        </View>
                    </View>
                    <Copy size={12} muted lines={3}>
                        {backupStatus()}
                    </Copy>
                    <View
                        style={{
                            flexDirection: "row",
                            flexWrap: "wrap",
                            gap: 10,
                        }}
                    >
                        <Tool
                            label="导出完整备份"
                            icon="download"
                            text={backupBusy() ? "正在处理…" : "导出完整备份"}
                            raised
                            size={narrow() ? "touch" : "regular"}
                            disabled={backupBusy()}
                            onPress={() => void exportCoreBackup()}
                        />
                        <Tool
                            label="从备份文件恢复"
                            icon="upload"
                            text="从备份文件恢复"
                            raised
                            size={narrow() ? "touch" : "regular"}
                            disabled={backupBusy()}
                            onPress={() => void restoreCoreBackup()}
                        />
                        <If when={!!backupPath()}>
                            <Tool
                                label="分享或导出备份文件"
                                icon="arrow"
                                text="分享/导出文件"
                                raised
                                size={narrow() ? "touch" : "regular"}
                                disabled={backupBusy()}
                                onPress={() => void shareCoreBackup()}
                            />
                        </If>
                        <If when={!!backupJson()}>
                            <Tool
                                label="复制备份 JSON"
                                icon="document"
                                text="复制备份 JSON"
                                size={narrow() ? "touch" : "regular"}
                                disabled={backupBusy()}
                                onPress={() => void copyCoreBackupJson()}
                            />
                        </If>
                    </View>
                </View>
                <Copy muted>选择一种适合当前工作的视图</Copy>
                <Copy size={12} muted>
                    扩展工作区来自已验签的普通插件包；阅读预览包仍使用窗口内样例。
                </Copy>
                <Each of={pluginWorkspaces()}>
                    {(workspace) => (
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
                            <Badge icon={workspace().icon} size={42} />
                            <View style={{ flex: 1, gap: 4 }}>
                                <View
                                    style={{
                                        flexDirection: "row",
                                        alignItems: "center",
                                        gap: 8,
                                    }}
                                >
                                    <Copy size={15} strong>
                                        {workspace().title}
                                    </Copy>
                                    <Copy size={11} muted>
                                        {workspace().kind}
                                    </Copy>
                                </View>
                                <Copy size={12} muted>
                                    {workspace().description}
                                </Copy>
                            </View>
                            <Tool
                                label={`打开${workspace().title}工作区`}
                                text="打开工作区"
                                raised
                                size={narrow() ? "touch" : "regular"}
                                onPress={() =>
                                    openPluginWorkspace(workspace().id)
                                }
                            />
                        </View>
                    )}
                </Each>
                <View style={{ paddingTop: 10, gap: 8 }}>
                    <Copy size={18} strong>
                        扩展管理
                    </Copy>
                    <Copy size={12} muted>
                        启停随应用包；停用会移除工作区入口和 Host
                        会话，已收录内容仍保留。
                    </Copy>
                    <Each of={pluginPackages()}>
                        {(plugin) => (
                            <View
                                style={{
                                    paddingVertical: 13,
                                    borderBottomWidth: 1,
                                    borderColor: ink("border-subtle"),
                                    flexDirection: "row",
                                    alignItems: "center",
                                    gap: 12,
                                }}
                            >
                                <Badge
                                    icon={
                                        plugin().id.includes("vocab")
                                            ? "book"
                                            : plugin().id.includes("notes")
                                              ? "pen"
                                              : "plug"
                                    }
                                    size={34}
                                />
                                <View style={{ flex: 1, gap: 3 }}>
                                    <Copy size={14} strong>
                                        {plugin().title}
                                    </Copy>
                                    <Copy size={11} muted>
                                        {plugin().id} · v{plugin().version} ·{" "}
                                        {plugin().enabled ? "已启用" : "已停用"}
                                    </Copy>
                                </View>
                                <Tool
                                    label={`${plugin().enabled ? "停用" : "启用"}${plugin().title}插件`}
                                    text={plugin().enabled ? "停用" : "启用"}
                                    size={narrow() ? "touch" : "regular"}
                                    onPress={() =>
                                        togglePluginPackage(
                                            plugin().id,
                                            !plugin().enabled,
                                        )
                                    }
                                />
                            </View>
                        )}
                    </Each>
                </View>
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
                                size={narrow() ? "touch" : "regular"}
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
                <If when={current()}>
                    <Copy size={11} muted>
                        界面提供者
                    </Copy>
                    <Copy>{current()?.provider}插件 · 窗口内样例</Copy>
                </If>
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
    const dialogWidth = () =>
        Math.min(520, Math.max(280, frameWidth() - 24));
    return (
        <Dialog
            open={searchOpen()}
            closeOnEscape={false}
            onOpenChange={({ open }) => searchOpen.set(open)}
            animation="none"
            backdropStyle={{ backgroundColor: ink("surface-scrim") }}
            style={{
                position: "absolute",
                top: narrow() ? 56 : 96,
                marginTop: narrow() ? "env(safe-area-inset-top)" : 0,
                left: Math.max(12, (frameWidth() - dialogWidth()) / 2),
                width: dialogWidth(),
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
                            size={narrow() ? "touch" : "regular"}
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
                    listStyle={{ maxHeight: narrow() ? 220 : 320, padding: 5 }}
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
                        <If when={!narrow()}>
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
                        </If>
                    }
                />
            </If>
        </Dialog>
    );
}
function MoreMenu(props: { touch?: boolean }) {
    return (
        <Menu
            offset={8}
            margin={10}
            popupStyle={menuPopup()}
            trigger={
                <Tool
                    label="更多工作区操作…"
                    icon="more"
                    size={props.touch ? "touch" : "regular"}
                />
            }
        >
            <MenuRow
                title={dark() ? "浅色外观" : "深色外观"}
                icon={dark() ? "sun" : "moon"}
                onSelect={toggleDark}
            />
            <MenuRow
                title={narrow() ? "数据与功能" : "功能样例"}
                icon="grid"
                onSelect={openLibrary}
            />
            <MenuRow
                title="软件更新"
                icon="arrow"
                onSelect={openUpdatePanel}
            />
            <If when={!currentPlugin() && (!narrow() || designPreview())}>
                <MenuRow
                    title="关闭当前标签"
                    icon="close"
                    onSelect={closeActive}
                />
            </If>
        </Menu>
    );
}
function HostToolbar() {
    return (
        <View
            desktop={{ windowDrag: true }}
            accessibilityElementsHidden={searchOpen()}
            importantForAccessibility={searchOpen() ? "no-hide-descendants" : "auto"}
            style={{
                flexShrink: 0,
                paddingTop: narrow() ? "env(safe-area-inset-top)" : 0,
                backgroundColor: ink("surface-frame"),
            }}
        >
            <View
                style={{
                    height: 52,
                    paddingLeft: narrow()
                        ? `calc(env(safe-area-inset-left) + ${titleBar().left + 12}px)`
                        : titleBar().left + 12,
                    paddingRight: narrow()
                        ? `calc(env(safe-area-inset-right) + ${titleBar().right + 12}px)`
                        : titleBar().right + 17,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: narrow() ? 8 : 10,
                }}
            >
                <If when={!narrow()}>
                    <Copy size={12} muted strong>
                        QUSOR
                    </Copy>
                    <If when={!activePlugin()}>
                        <Tool
                            label={showSidebar() ? "隐藏导航" : "显示导航"}
                            icon="panel"
                            text="导航"
                            selected={showSidebar()}
                            onPress={toggleNavigation}
                        />
                    </If>
                    <If when={currentPlugin()}>
                        <Tool
                            label={
                                pluginOrigin()?.library
                                    ? "返回功能列表"
                                    : `返回产品设计 · ${current()?.title ?? "概览"}`
                            }
                            icon="back"
                            text={
                                pluginOrigin()?.library
                                    ? "返回功能"
                                    : "返回产品设计"
                            }
                            onPress={closePluginWorkspace}
                        />
                        <Glyph name="forward" size={13} />
                        <ProviderIdentity
                            title={currentPlugin()!.title}
                            icon={currentPlugin()!.icon}
                        />
                    </If>
                    <If when={!activePlugin() && library()}>
                        <Glyph name="forward" size={13} />
                        <Copy strong>功能</Copy>
                    </If>
                    <If when={!activePlugin() && !library()}>
                        <If when={active() === "overview"}>
                            <Copy strong>产品设计</Copy>
                        </If>
                        <If when={active() !== "overview"}>
                            <Tool
                                label="返回产品设计概览"
                                text="产品设计"
                                icon="folder"
                                onPress={() => openEntry("overview")}
                            />
                            <Glyph name="forward" size={13} />
                            <ProviderIdentity
                                title={current()?.provider ?? ""}
                                icon={current()?.icon ?? "plug"}
                            />
                        </If>
                    </If>
                    <View style={{ flex: 1, minWidth: 0 }} />
                    <If when={!activePlugin()}>
                        <Tool
                            label="搜索内容…"
                            icon="search"
                            onPress={showSearch}
                        />
                        <NewMenu />
                        <Tool
                            label={inspector() ? "关闭信息" : "显示信息"}
                            icon="info"
                            selected={inspector()}
                            onPress={() => inspector.set(!inspector())}
                        />
                    </If>
                    <MoreMenu />
                </If>
                <If when={narrow()}>
                    <If when={currentPlugin()}>
                        <Tool
                            label="返回"
                            icon="back"
                            text="返回"
                            size="touch"
                            onPress={closePluginWorkspace}
                        />
                        <View
                            style={{
                                flex: 1,
                                minWidth: 0,
                                flexDirection: "row",
                                alignItems: "center",
                                gap: 6,
                            }}
                        >
                            <Glyph name={currentPlugin()!.icon} size={16} />
                            <Copy size={15} strong>
                                {currentPlugin()!.title}
                            </Copy>
                        </View>
                    </If>
                    <If when={!currentPlugin()}>
                        <Tool
                            label={showSidebar() ? "隐藏导航" : "显示导航"}
                            icon="panel"
                            selected={showSidebar()}
                            size="touch"
                            onPress={toggleNavigation}
                        />
                        <If when={library()}>
                            <View style={{ flex: 1, minWidth: 0 }}>
                                <Copy size={15} strong>
                                    数据与功能
                                </Copy>
                            </View>
                        </If>
                        <If when={!library() && showMobileHome()}>
                            <View style={{ flex: 1, minWidth: 0 }}>
                                <Copy size={15} strong>
                                    QUSOR
                                </Copy>
                            </View>
                        </If>
                        <If when={!library() && !showMobileHome()}>
                            <Tool
                                label="返回工作台"
                                icon="back"
                                text="返回"
                                size="touch"
                                onPress={openMobileHome}
                            />
                            <View style={{ flex: 1, minWidth: 0 }}>
                                <Copy size={15} strong>
                                    {current()?.title ?? "设计样例"}
                                </Copy>
                            </View>
                        </If>
                        <Tool
                            label="搜索内容…"
                            icon="search"
                            size="touch"
                            onPress={showSearch}
                        />
                    </If>
                    <MoreMenu touch />
                </If>
            </View>
        </View>
    );
}
function App() {
    effect(() => {
        BackHandler.on("press", () => {
            if (updateOpen()) {
                updateOpen.set(false);
                return true;
            }
            if (searchOpen()) {
                searchOpen.set(false);
                return true;
            }
            if (activePlugin()) {
                closePluginWorkspace();
                return true;
            }
            if (narrow() && sidebar()) {
                sidebar.set(false);
                return true;
            }
            if (library()) {
                if (narrow()) openMobileHome();
                else openEntry("overview");
                return true;
            }
            if (active() !== "overview") {
                openEntry("overview");
                return true;
            }
            if (narrow() && designPreview()) {
                openMobileHome();
                return true;
            }
            return false;
        });
    });
    return (
        <View
            style={{
                flex: 1,
                backgroundColor: ink("surface-frame"),
                paddingBottom: narrow() ? "env(safe-area-inset-bottom)" : 0,
            }}
        >
            <HostToolbar />
            <View style={{ flex: 1, flexDirection: "row", minHeight: 0 }}>
                <If when={showSidebar()}>
                    <NavigationPane compact={narrow()} />
                </If>
                <View
                    accessibilityElementsHidden={searchOpen()}
                    importantForAccessibility={searchOpen() ? "no-hide-descendants" : "auto"}
                    style={{
                        flex: 1,
                        minWidth: 0,
                        flexDirection: "column",
                        marginLeft: narrow()
                            ? "calc(env(safe-area-inset-left) + 8px)"
                            : showSidebar()
                              ? 0
                              : 8,
                        marginRight: narrow()
                            ? "calc(env(safe-area-inset-right) + 8px)"
                            : 8,
                        marginBottom: 8,
                        borderRadius: 12,
                        borderCurve: "continuous",
                        borderWidth: 1,
                        borderColor: ink("border-subtle"),
                        overflow: "hidden",
                        backgroundColor: ink("surface-base"),
                        display: narrow() && showSidebar() ? "none" : "flex",
                    }}
                >
                <If when={!library() && !activePlugin() && !showMobileHome()}>
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
                        <If when={!activePlugin() && library()}>
                            <Library />
                        </If>
                        <If when={showMobileHome()}>
                            <MobileHome />
                        </If>
                        <If
                            when={
                                !activePlugin() &&
                                !library() &&
                                !showMobileHome() &&
                                active() === "overview"
                            }
                        >
                            <Overview />
                        </If>
                        <Each of={allWorkspaces()}>
                            {(workspace) => (
                                <If
                                    when={
                                        activePlugin() === workspace().id
                                    }
                                >
                                    {workspace().render({
                                        compact: narrow(),
                                    })}
                                </If>
                            )}
                        </Each>
                        <Each of={entries}>
                            {(entry) => (
                                <If
                                    when={
                                        !activePlugin() &&
                                        !library() &&
                                        active() === entry().id
                                    }
                                >
                                    {entry().render()}
                                </If>
                            )}
                        </Each>
                    </View>
                    <If when={!activePlugin() && inspector() && !library()}>
                        <Inspector />
                    </If>
                </View>
            </View>

            </View>
            <Search />
            <UpdateDialog frameWidth={frameWidth()} />
        </View>
    );
}
bootWorkspaceCatalog();
setColorScheme("light");
mount(App);

// Evidence-only entry: the launcher names the workspace to capture. The notes
// host is the fallback for hosts that only know the notes bridge, so a plain
// headless test (no native module) stays in the normal shell.
void (async () => {
    try {
        await runUpdateEvidence();
        let target = "";
        try {
            const vocab = JSON.parse(await VocabHost.evidence());
            if (vocab.ok && vocab.value?.active)
                target = String(vocab.value.target ?? "notes");
        } catch (_) {
            // Older hosts expose the evidence switch only through NotesHost.
        }
        if (!target) {
            const notes = JSON.parse(await NotesHost.evidence());
            if (!notes.ok || !notes.value?.active) return;
            target = String(notes.value.target ?? "notes");
        }
        setTimeout(() => {
            if (target === "vocab") {
                if (!openPluginWorkspaceByRole("vocab_workspace")) return;
                void (async () => {
                    await new Promise((resolve) => setTimeout(resolve, 700));
                    await captureVocabEvidence();
                })();
                return;
            }
            if (!openPluginWorkspaceByRole("notes_workspace")) return;
            void (async () => {
                await new Promise((resolve) => setTimeout(resolve, 700));
                await captureNotesEvidence();
            })();
        }, 500);
    } catch (_) {
        // No native host (headless tests) or no evidence mode.
    }
})();
