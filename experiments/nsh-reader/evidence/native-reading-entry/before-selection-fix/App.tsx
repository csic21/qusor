import { mount, Scroll, setColorScheme, signal, Text, View } from "nsh";
import { Button, Input, Menu, MenuItem, MenuSeparator } from "nsh/base";
import { currentWindow, titleBar } from "nsh/desktop";
import { ReadingWorkspace, readingEntry } from "./ReadingWorkspace";
import { readerMode } from "./reading-state";
import { Control, Glyph, ink, Label, Mark, Rule, Segments, fonts, compactUI, touchUI } from "./ui";

// Isolated native preview: reading is a separate contribution fixture.
// User-picked text stays in memory; no Agent, network or Core writes.
const mobileNavigation = signal(false);
const workspace = signal(readingEntry.id);
const scene = signal("research"), tab = signal("document"), inspector = signal(false);
const library = signal(false), dark = signal(false), sidebar = signal(true);
const researchDraft = signal("工作台需要保持安静，扩展能力在需要时出现。");
const writingDraft = signal("一款好用的软件，需要给复杂的工作留出清晰的秩序。");
const note = signal(""), search = signal("");
const scenes = [
  { id: "research", title: "原生软件体验研究", kind: "研究", icon: "book", tint: "#6584A2" },
  { id: "writing", title: "把想法整理成文章", kind: "写作", icon: "pen", tint: "#A18053" },
  { id: "process", title: "安排一次工作流程", kind: "流程", icon: "flow", tint: "#768E77" },
  { id: "development", title: "制作一个小工具", kind: "开发", icon: "code", tint: "#8B7FA3" },
];
const current = () => scenes.find(s => s.id === scene())!;
function choose(id: string) { mobileNavigation.set(false); workspace.set("tasks"); library.set(false); scene.set(id); tab.set("document"); note.set(""); }

function MenuRow(props: { text: string; icon: string; onSelect: () => void }) {
  return <MenuItem onSelect={props.onSelect} style={{ flexDirection: "row", gap: 10, alignItems: "center", paddingHorizontal: 10, paddingVertical: 8, borderRadius: 6 }} hoverStyle={{ backgroundColor: ink("surface-selected") }} focusStyle={{ backgroundColor: ink("surface-selected") }}><Glyph name={props.icon} size={16} /><Label>{props.text}</Label></MenuItem>;
}

function readingHome() { mobileNavigation.set(false); workspace.set(readingEntry.id); readerMode.set("shelf"); sidebar.set(true); }

function showLibrary() { mobileNavigation.set(false); workspace.set("tasks"); inspector.set(false); library.set(true); }

function AgentMenu() {
  return <Menu offset={8} margin={10} popupStyle={{ padding: 5, minWidth: 242, borderRadius: 11, borderCurve: "continuous", backgroundColor: ink("surface-floating"), backdropFilter: "blur(18px)", borderWidth: 1, borderColor: ink("border-control"), boxShadow: "0 8px 30px rgba(0,0,0,0.17)" }} trigger={<Control label="选择 Agent 接入方式" text="选择 Agent" icon="plus" raised />}>
    <View style={{ padding: 10 }}><Text style={{ fontSize: 12, color: ink("text-secondary") }}>接入方式 · 设计样例</Text></View>
    <MenuRow text="结构化 Agent 接入" icon="stack" onSelect={() => note.set("结构化接入：待配置提供者，当前未连接。")} />
    <MenuRow text="本机命令行接入" icon="code" onSelect={() => note.set("命令行接入：待配置本机程序，当前未运行命令。")} />
    <MenuRow text="远程 Agent 接入" icon="plug" onSelect={() => note.set("远程接入：待配置服务，当前没有发送数据。")} />
    <MenuSeparator style={{ height: 1, marginVertical: 5, backgroundColor: ink("border-subtle") }} />
    <View style={{ padding: 10 }}><Text style={{ fontSize: 12, lineHeight: 18, color: ink("text-secondary"), maxWidth: 230 }}>当前窗口只演示界面，不连接服务。</Text></View>
  </Menu>;
}

function Sidebar() {
  const win = currentWindow();
  return <View style={{ width: 230, height: "100%", flexShrink: 0, borderRightWidth: 1, borderColor: ink("border-subtle"), backgroundColor: ink("surface-navigation") }}>
    <View desktop={{ windowDrag: true }} style={{ height: 52, paddingLeft: titleBar().left + 8, paddingRight: 10, flexDirection: "row", alignItems: "center", justifyContent: "flex-end" }}><Control icon="panel" label="收起侧栏" onPress={() => { sidebar.set(false); mobileNavigation.set(false); }} /></View>
    <View style={{ flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 20, paddingTop: 12, paddingBottom: 23 }}><Mark /><View style={{ gap: 1 }}><Text style={{ fontSize: 14, letterSpacing: 1.1, fontWeight: "650", color: ink("text-primary") }}>QUSOR</Text><Text style={{ fontSize: 12, color: ink("text-secondary") }}>个人空间</Text></View></View>
    <Button accessibilityLabel="打开书架与文本" onPress={readingHome} style={{ marginHorizontal: 9, marginBottom: 17, height: touchUI() ? 48 : 37, paddingHorizontal: 12, flexDirection: "row", alignItems: "center", gap: 10, borderRadius: 6, backgroundColor: ink(workspace() === readingEntry.id ? "surface-selected" : "transparent") }} hoverStyle={{ backgroundColor: ink("surface-hover") }}><Glyph name={readingEntry.icon} size={18} color="#6584A2" /><Label weight="500">{readingEntry.title}</Label></Button>
    <View style={{ marginHorizontal: 13, marginBottom: 23, height: 31, paddingHorizontal: 9, borderRadius: 7, borderWidth: 1, borderColor: ink("border-control"), backgroundColor: ink("surface-search"), flexDirection: "row", alignItems: "center", gap: 7, boxShadow: "inset 0 1px 1px rgba(0,0,0,0.03)" }}><Glyph name="search" size={15} /><Input accessibilityLabel="查找设计样例" value={search()} onChange={({ value }) => search.set(value)} placeholder="查找任务" style={{ flex: 1, fontSize: 13, color: ink("text-primary"), backgroundColor: "transparent", borderWidth: 0, padding: 0, height: 25 }} /></View>
    <View style={{ paddingHorizontal: 21, marginBottom: 8, flexDirection: "row", alignItems: "center" }}><Text style={{ fontSize: 12, fontWeight: "600", color: ink("text-secondary") }}>工作中的任务</Text><View style={{ flex: 1 }} /><Text style={{ fontSize: 12, color: ink("text-secondary") }}>样例</Text></View>
    <View style={{ paddingHorizontal: 9, gap: 3 }}><Each of={scenes.filter(s => !search() || s.title.includes(search()) || s.kind.includes(search()))}>{s => <Button accessibilityLabel={s().title} onPress={() => choose(s().id)} style={{ paddingHorizontal: 11, paddingVertical: 11, gap: 11, flexDirection: "row", alignItems: "center", borderRadius: 7, backgroundColor: ink((workspace() === "tasks" && scene() === s().id) ? "surface-selected" : "transparent") }} hoverStyle={{ backgroundColor: ink("surface-hover") }} focusStyle={{ outlineWidth: 2, outlineColor: ink("focus-ring") }}><Glyph name={s().icon} size={18} color={s().tint} /><View style={{ flex: 1, gap: 3 }}><Text style={{ fontSize: 13, fontWeight: (workspace() === "tasks" && scene() === s().id) ? "600" : "400", color: ink("text-primary"), numberOfLines: 1 }}>{s().title}</Text><Text style={{ fontSize: 12, color: ink("text-secondary") }}>{s().kind}</Text></View></Button>}</Each></View>
    <View style={{ paddingHorizontal: 20, paddingTop: 25, gap: 10 }}><Text style={{ fontSize: 12, fontWeight: "600", color: ink("text-secondary") }}>固定空间</Text><View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}><Glyph name="stack" size={17} color="#A88B58" /><Label small>个人工作室</Label></View></View>
    <View style={{ flex: 1 }} />
    <View style={{ marginHorizontal: 13 }}><Rule /></View>
    <View style={{ padding: 12, gap: 6 }}><Button accessibilityLabel="打开扩展样例" onPress={showLibrary} style={{ height: 34, paddingHorizontal: 8, flexDirection: "row", alignItems: "center", gap: 10, borderRadius: 6 }} hoverStyle={{ backgroundColor: ink("surface-hover") }}><Glyph name="plug" size={17} /><Label small>扩展与能力</Label><View style={{ flex: 1 }} /><Text style={{ fontSize: 12, color: ink("text-secondary") }}>{scenes.length + 1}</Text></Button><View style={{ flexDirection: "row", alignItems: "center", paddingLeft: 8, paddingTop: 6 }}><View style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: ink("text-secondary"), marginRight: 7 }} /><Text style={{ flex: 1, fontSize: 12, color: ink("text-secondary") }}>原生交互预览</Text><Control icon={dark() ? "sun" : "moon"} label="切换明暗外观" onPress={() => { dark.set(!dark()); setColorScheme(dark() ? "dark" : "light"); win.set({ colorScheme: dark() ? "dark" : "light" }); }} /></View></View>
  </View>;
}

function Caption(props: { children: any }) { return <Text style={{ fontSize: 12, lineHeight: 18, color: ink("text-secondary") }}>{props.children}</Text>; }
function Paragraph(props: { children: any }) { return <Text style={{ fontSize: fonts.body, lineHeight: 28, color: ink("text-primary") }}>{props.children}</Text>; }
function Heading(props: { children: any }) { return <Text style={{ fontSize: 17, lineHeight: 26, fontWeight: "600", color: ink("text-primary"), marginTop: 12 }}>{props.children}</Text>; }

function Research() {
  return <View style={{ gap: 18 }}>
    <View style={{ flexDirection: "row", alignItems: "center", gap: 7 }}><View style={{ width: 5, height: 5, borderRadius: 3, backgroundColor: "#AD8A51" }} /><Caption>研究札记</Caption><View style={{ flex: 1 }} /><Caption>2026.11.09</Caption></View>
    <View style={{ gap: 8 }}><Text style={{ fontSize: 26, lineHeight: 36, letterSpacing: -0.5, fontWeight: "600", color: ink("text-primary") }}>原生软件的秩序与手感</Text><Caption>QUSOR 设计研究 · 自拟材料</Caption></View>
    <Paragraph>好的工具让人专注于眼前的内容。窗口、文字与操作保持稳定，复杂能力在需要的时候出现。</Paragraph>
    <View style={{ flexDirection: "row", alignItems: "center", gap: 14, paddingVertical: 12, borderTopWidth: 1, borderBottomWidth: 1, borderColor: ink("border-subtle") }}><Glyph name="book" size={16} /><Caption>参考作品</Caption><Text style={{ fontSize: 13, color: ink("text-primary") }}>Things  ·  Reeder  ·  Kino</Text></View>
    <Heading>01　让内容建立层级</Heading>
    <Paragraph>用字阶、行距和对齐组织一张清晰的工作台。文稿像文稿，画布像画布；工具不需要用一排卡片争夺注意力。</Paragraph>
    <Heading>02　让扩展自然进入工作</Heading>
    <Paragraph>进入研究、写作或开发任务时，更换的是主内容与适用工具。侧栏保留任务的位置，检查器按需展开，Agent 从当前任务接手。</Paragraph>
    <View style={{ marginTop: 4, paddingLeft: 15, borderLeftWidth: 2, borderColor: "#B59A6C", gap: 8 }}><Text style={{ fontSize: 14, fontWeight: "600", color: ink("text-primary") }}>随手记</Text><Input accessibilityLabel="研究札记草稿" value={researchDraft()} onChange={({ value }) => researchDraft.set(value)} multiline style={{ minHeight: 72, padding: 0, borderWidth: 0, backgroundColor: ink("surface-paper"), color: ink("text-primary"), fontSize: 16, lineHeight: 27 }} focusStyle={{ outlineWidth: 1, outlineOffset: 5, outlineColor: ink("focus-ring") }} /><Caption>草稿只保留在当前窗口</Caption></View>
  </View>;
}

function Writing() {
  return <View style={{ gap: 20 }}><Caption>文稿 · 设计样例</Caption><Text style={{ fontSize: 26, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>把想法整理成文章</Text><Paragraph>主内容可以直接成为写作空间。资料、想法与 Agent 建议都围绕这篇文稿出现。</Paragraph><Heading>我的草稿</Heading><Input accessibilityLabel="写作草稿" value={writingDraft()} onChange={({ value }) => writingDraft.set(value)} multiline style={{ minHeight: 320, padding: 0, borderWidth: 0, backgroundColor: ink("surface-paper"), color: ink("text-primary"), fontSize: 17, lineHeight: 30 }} focusStyle={{ outlineWidth: 1, outlineOffset: 8, outlineColor: ink("focus-ring") }} /><Caption>本任务的草稿只保留在当前窗口，切换标签不会清空输入。</Caption></View>;
}

function Process() {
  return <View style={{ gap: 22 }}><Caption>工作流程 · 设计样例</Caption><Text style={{ fontSize: 26, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>安排一次工作流程</Text><Paragraph>把过程铺开，逐步检查输入与结果。Agent 连接后才可以执行；当前步骤均未开始。</Paragraph><Each of={["准备资料与目标", "选择适用的 Agent 与工具", "检查计划并允许执行", "审阅结果，保留需要的内容"]}>{(step, index) => <View style={{ flexDirection: "row", gap: 15, paddingVertical: 15, borderBottomWidth: 1, borderColor: ink("border-subtle"), alignItems: "center" }}><View style={{ width: 26, height: 26, borderRadius: 13, borderWidth: 1, borderColor: ink("border-control"), alignItems: "center", justifyContent: "center" }}><Caption>{index() + 1}</Caption></View><View style={{ flex: 1 }}><Label>{step()}</Label></View><Caption>未开始</Caption></View>}</Each></View>;
}

function Development() {
  return <View style={{ gap: 22 }}><Caption>开发 · 可选的任务界面</Caption><Text style={{ fontSize: 26, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>制作一个小工具</Text><Paragraph>代码是这个任务的内容。需要时再打开文件、预览与执行输出，其他任务继续使用各自适合的界面。</Paragraph><View style={{ flexDirection: "row", paddingVertical: 12, borderTopWidth: 1, borderBottomWidth: 1, borderColor: ink("border-subtle"), alignItems: "center", gap: 8 }}><Glyph name="code" size={16} /><Label small>示例代码</Label><View style={{ flex: 1 }} /><Caption>只读 · 未执行</Caption></View><Text style={{ userSelect: "text", fontFamily: "monospace", fontSize: 14, lineHeight: 27, color: ink("text-primary"), padding: 15, backgroundColor: ink("surface-inset"), borderRadius: 5 }}>{'// A task can choose its own tools.\nconst task = {\n  title: "A small useful tool",\n  agent: null,\n  views: ["code", "preview"]\n};'}</Text><Caption>上面的代码仅说明设计思路，并非已发布的 SDK。</Caption></View>;
}

function Steps() {
  return <View style={{ gap: 22 }}><Caption>任务过程 · 设计样例</Caption><Text style={{ fontSize: 26, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>从目标到结果</Text><Paragraph>过程与成果可以分别查看，切换标签保留正在编辑的内容。</Paragraph><Each of={[{ title: "明确当前目标", text: "制作原生软件的视觉方向与交互样例" }, { title: "准备上下文", text: "参考作品、当前草稿与用户的设计要求" }, { title: "选择 Agent", text: "尚未连接，没有执行记录" }]}>{s => <View style={{ gap: 8, paddingVertical: 15, borderBottomWidth: 1, borderColor: ink("border-subtle") }}><Label weight="600">{s().title}</Label><Caption>{s().text}</Caption></View>}</Each></View>;
}

function Inspector() {
  return <View style={{ width: compactUI() ? "100%" : 260, flexShrink: 0, backgroundColor: ink("surface-inspector"), borderLeftWidth: 1, borderColor: ink("border-subtle") }}><View style={{ height: 52, paddingHorizontal: 16, alignItems: "center", flexDirection: "row", gap: 7 }}><Label weight="600">任务信息</Label><View style={{ flex: 1 }} /><Control label="关闭任务信息" icon="close" onPress={() => inspector.set(false)} /></View><Rule /><View style={{ padding: 20, gap: 16 }}><Caption>当前任务</Caption><Label weight="600">{current().title}</Label><Caption>设计样例 · 当前窗口</Caption><Rule /><Caption>使用的界面</Caption><View style={{ flexDirection: "row", gap: 10, alignItems: "center" }}><Glyph name={current().icon} /><Label>{current().kind}工作台</Label></View><Caption>切换任务时，主内容随场景变化。</Caption><Rule /><Caption>Agent</Caption><Label>尚未连接</Label><Caption>没有运行中的任务或外部操作。</Caption><Rule /><Caption>内容状态</Caption><Label>未保存的会话草稿</Label><Caption>正式保存与恢复尚未接入本样例。</Caption></View></View>;
}

function CapabilityLibrary() {
  return <View style={{ flex: 1, backgroundColor: ink("surface-paper"), padding: 0, gap: 18 }}><Text style={{ fontSize: 21, fontWeight: "600", color: ink("text-primary") }}>选择适合的工作界面</Text><Caption>以下是界面样例，尚未安装真实插件包。</Caption><View style={{ flexDirection: "row", alignItems: "center", gap: 14, paddingVertical: 13, borderBottomWidth: 1, borderColor: ink("border-subtle") }}><Glyph name="book" size={22} color="#6584A2" /><View style={{ flex: 1, gap: 5 }}><Label weight="500">书架与阅读</Label><Caption>选择书籍、打开文本、阅读与选文</Caption></View><Control label="预览阅读工作台" text="预览" raised onPress={readingHome} /></View><Each of={scenes}>{s => <View style={{ flexDirection: "row", gap: 14, alignItems: "center", paddingVertical: 13, borderBottomWidth: 1, borderColor: ink("border-subtle") }}><Glyph name={s().icon} size={22} color={s().tint} /><View style={{ flex: 1, gap: 5 }}><Label weight="500">{s().kind}工作台</Label><Caption>{s().title}</Caption></View><Control label={`预览${s().kind}工作台`} text="预览" raised onPress={() => { choose(s().id); library.set(false); }} /></View>}</Each><Caption>所有场景共用任务导航、标签与外观规则。</Caption></View>;
}

function App() {
  return <View onLayout={({ bounds }) => compactUI.set(bounds.size.width < 700)} style={{ flex: 1, flexDirection: "row", paddingTop: "env(safe-area-inset-top)", paddingBottom: "env(safe-area-inset-bottom)", paddingLeft: "env(safe-area-inset-left)", paddingRight: "env(safe-area-inset-right)" }}>
    <If when={sidebar() && !compactUI()}><Sidebar /></If>
    <If when={workspace() === readingEntry.id}><ReadingWorkspace sidebarVisible={sidebar() && !compactUI()} toggleSidebar={() => compactUI() ? mobileNavigation.set(!mobileNavigation()) : sidebar.set(!sidebar())} enterReading={() => sidebar.set(false)} returnShelf={() => sidebar.set(true)} /></If>
    <If when={workspace() === "tasks"}>
    <View style={{ flex: 1, minWidth: 0, backgroundColor: ink("surface-stage") }}>
      <View desktop={{ windowDrag: true }} style={{ height: 52, flexShrink: 0, paddingLeft: sidebar() && !compactUI() ? 18 : titleBar().left + 12, paddingRight: titleBar().right + 16, flexDirection: "row", alignItems: "center", gap: 9, backgroundColor: ink("surface-toolbar"), borderBottomWidth: 1, borderColor: ink("border-subtle") }}>
        <If when={!sidebar() || compactUI()}><Control label="展开侧栏" icon="panel" onPress={() => compactUI() ? mobileNavigation.set(true) : sidebar.set(true)} /></If>
        <If when={library()}><Control label="返回当前任务" icon="back" onPress={() => library.set(false)} /></If><Glyph name={library() ? "plug" : current().icon} size={17} /><View style={{ flex: 1, minWidth: 0 }}><Label weight="600">{library() ? "扩展与能力" : current().title}</Label></View><View style={{ flex: 1 }} /><If when={!library() && !compactUI()}><Segments value={tab()} onChange={v => tab.set(v)} items={[{ id: "document", label: "工作台" }, { id: "steps", label: "任务过程" }]} /></If><View style={{ width: 7 }} /><Control label="任务信息" icon="panel" selected={inspector()} onPress={() => inspector.set(!inspector())} /><Control label="打开扩展样例" icon="plug" onPress={showLibrary} />
      </View>
      <If when={!library() && compactUI()}><View style={{ paddingHorizontal: 12, paddingVertical: 7 }}><Segments value={tab()} onChange={v => tab.set(v)} items={[{id:"document",label:"工作台"},{id:"steps",label:"任务过程"}]} /></View></If>
      <View style={{ flex: 1, flexDirection: "row", minHeight: 0 }}>
        <If when={!compactUI() || !inspector()}><Scroll style={{ flex: 1 }}><View style={{ padding: compactUI() ? 14 : 30, alignItems: "center" }}><View style={{ width: "100%", maxWidth: 790, padding: compactUI() ? 20 : 38, minHeight: 650, backgroundColor: ink("surface-paper"), borderRadius: 3, borderWidth: 1, borderColor: ink("border-paper"), boxShadow: "0 2px 8px rgba(0,0,0,0.045), 0 0.5px 1px rgba(0,0,0,0.025)" }}>
          <If when={library()}><CapabilityLibrary /></If>
          <If when={!library() && tab() === "steps"}><Steps /></If>
          <If when={!library() && tab() === "document" && scene() === "research"}><Research /></If>
          <If when={!library() && tab() === "document" && scene() === "writing"}><Writing /></If>
          <If when={!library() && tab() === "document" && scene() === "process"}><Process /></If>
          <If when={!library() && tab() === "document" && scene() === "development"}><Development /></If>
        </View></View></Scroll></If>
        <If when={inspector()}><Inspector /></If>
      </View>
      <View style={{ minHeight: 61, flexShrink: 0, paddingHorizontal: 23, paddingVertical: 11, flexDirection: "row", alignItems: "center", gap: 12, borderTopWidth: 1, borderColor: ink("border-subtle"), backgroundColor: ink("surface-toolbar") }}><View style={{ width: 30, height: 30, borderRadius: 15, backgroundColor: ink("surface-track"), alignItems: "center", justifyContent: "center" }}><Glyph name="plug" size={16} /></View><View style={{ flex: 1, gap: 3 }}><Text style={{ fontSize: 13, fontWeight: "500", color: ink("text-primary") }}>Agent 尚未连接</Text><Text style={{ fontSize: 12, color: ink("text-secondary"), numberOfLines: 1 }}>{note() || "当前任务的材料与草稿，在需要时交给 Agent。"}</Text></View><AgentMenu /></View>
    </View>
    </If>
    <If when={compactUI() && mobileNavigation()}><View style={{ position: "absolute", top: 0, bottom: 0, left: 0, right: 0, zIndex: 200 }}><View accessibilityLabel="关闭全局导航" onPress={() => mobileNavigation.set(false)} style={{ position: "absolute", top: 0, bottom: 0, left: 0, right: 0, backgroundColor: "rgba(0,0,0,0.28)" }} /><Sidebar /></View></If>
    {/* Keep the platform's traffic lights and real native window dragging. */}
  </View>;
}
setColorScheme("light");
mount(App);
