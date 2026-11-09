import { Clipboard, Picker, Scroll, Text, View } from "nsh";
import { Button, Input, Menu, MenuItem } from "nsh/base";
import { titleBar } from "nsh/desktop";
import { Control, Glyph, ink, Label, Rule, Segments, compactUI } from "./ui";
import { activeDocument, addTextDocument, chapterIndex, chapterPositions, currentChapter, currentDocument, documents, goToChapter, inputText, inputTitle, openDocument, pickedPassage, picking, readerMessage, readerMode, shelfQuery, sourceFilter, textSize, tocOpen, type ReadingDocument } from "./reading-state";

// UI contribution fixture, not an installed package or a published SDK shape.
export const readingEntry = { id: "reading", title: "书架与文本", icon: "book" };
type Props = { sidebarVisible: boolean; toggleSidebar: () => void; enterReading: () => void; returnShelf: () => void };

function Caption(props: { children: any }) { return <Text style={{ fontSize: compactUI() ? 13 : 12, lineHeight: compactUI() ? 21 : 19, color: ink("text-secondary") }}>{props.children}</Text>; }
function read(id: string, props: Props) { openDocument(id); props.enterReading(); }
function shelf(props: Props) { pickedPassage.set(null); readerMode.set("shelf"); props.returnShelf(); }

async function chooseTextFile(props: Props) {
  if (picking()) return;
  picking.set(true); readerMessage.set("");
  try {
    const [picked] = await Picker.pickFiles({ multiple: false, filters: [{ name: "UTF-8 文本", extensions: ["txt", "md", "markdown"] }] });
    if (!picked) { readerMessage.set("未选择文件，原来的阅读内容保持。"); return; }
    const file = picked.file;
    if (!/\.(txt|md|markdown)$/i.test(file.name)) throw new Error("当前可打开 TXT 和 Markdown；PDF、EPUB 尚未接入这个窗口。");
    if (file.size > 1_048_576) throw new Error("这份文件超过 1 MiB；本阅读样例暂不打开更大的文本。");
    const bytes = new Uint8Array(await file.arrayBuffer());
    let text: string;
    try { text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
    catch { throw new Error("这份文件不是可识别的 UTF-8 文本，请转换编码后重试。"); }
    addTextDocument(file.name, text, "file", /\.txt$/i.test(file.name) ? "TXT" : "Markdown 文本");
    props.enterReading();
  } catch (error) {
    readerMessage.set(error instanceof Error ? error.message : "暂时无法打开文本，原来的内容保持。");
  } finally { picking.set(false); }
}

function Cover(props: { document: ReadingDocument }) {
  return <View style={{ width: compactUI() ? 44 : 52, height: compactUI() ? 64 : 72, flexShrink: 0, borderRadius: 2, backgroundColor: props.document.tint, borderLeftWidth: 4, borderLeftColor: "rgba(0,0,0,0.15)", boxShadow: "1px 2px 3px rgba(0,0,0,0.14)", paddingHorizontal: 7, paddingTop: compactUI() ? 9 : 12, gap: 4, overflow: "hidden" }}><View style={{ width: 16, height: 1, backgroundColor: "rgba(255,255,255,0.65)" }} /><Text style={{ fontSize: compactUI() ? 10 : 11, lineHeight: compactUI() ? 14 : 16, color: "#FFFFFF", fontWeight: "500", numberOfLines: 2 }}>{props.document.title.slice(0, 6)}</Text><View style={{ flex: 1 }} /><Text style={{ fontSize: 7, lineHeight: 9, numberOfLines: 1, letterSpacing: 0.7, color: "rgba(255,255,255,0.8)", marginBottom: 6 }}>QUSOR</Text></View>;
}

function Shelf(props: Props) {
  const visible = () => documents().filter(d => (!shelfQuery() || d.title.includes(shelfQuery())) && (sourceFilter() === "all" || (sourceFilter() === "sample" ? d.source === "sample" : d.source !== "sample")));
  return <Scroll style={{ flex: 1 }}><View style={{ paddingHorizontal: compactUI() ? 16 : 46, paddingTop: compactUI() ? 23 : 34, paddingBottom: 38, gap: 23 }}>
    <View style={{ gap: 8 }}><Text style={{ fontSize: compactUI() ? 23 : 26, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>选择接下来要读的内容</Text><Caption>示例书籍可以直接打开。自己的文本可从本机选择，或在这里输入。</Caption></View>
    <View style={{ flexDirection: compactUI() ? "column" : "row", alignItems: compactUI() ? "stretch" : "center", gap: compactUI() ? 12 : 18 }}><Segments value={sourceFilter()} onChange={v => sourceFilter.set(v)} items={[{ id: "all", label: "全部" }, { id: "sample", label: "示例书籍" }, { id: "local", label: "本次打开" }]} /><View style={{ flex: 1 }} /><View style={{ width: compactUI() ? "100%" : 200, height: compactUI() ? 44 : 31, flexDirection: "row", alignItems: "center", gap: 7, paddingHorizontal: 9, borderRadius: 7, borderWidth: 1, borderColor: ink("border-control"), backgroundColor: ink("surface-search") }}><Glyph name="search" size={15} /><Input accessibilityLabel="查找书籍或文本" value={shelfQuery()} onChange={({ value }) => shelfQuery.set(value)} placeholder="查找书籍或文本" style={{ flex: 1, height: 27, fontSize: 13, padding: 0, borderWidth: 0, backgroundColor: "transparent", color: ink("text-primary") }} /></View></View>
    <View style={{ flexDirection: "row", paddingHorizontal: 4 }}><Text style={{ flex: 1, fontSize: 12, color: ink("text-secondary") }}>书籍与文本</Text><Caption>{visible().length} 项</Caption></View>
    <View><Each of={visible()}>{doc => <Button accessibilityLabel={`阅读：${doc().title}`} onPress={() => read(doc().id, props)} style={{ flexDirection: "row", alignItems: "center", gap: compactUI() ? 13 : 21, paddingHorizontal: compactUI() ? 4 : 12, paddingVertical: 18, borderBottomWidth: 1, borderColor: ink("border-subtle"), borderRadius: 4 }} hoverStyle={{ backgroundColor: ink("surface-hover") }} focusStyle={{ outlineWidth: 2, outlineColor: ink("focus-ring") }}><Cover document={doc()} /><View style={{ flex: 1, gap: 7 }}><Text style={{ fontSize: 17, lineHeight: 25, fontWeight: "500", color: ink("text-primary") }}>{doc().title}</Text><Caption>{doc().byline}</Caption><Caption>{doc().format} · {doc().chapters.length} {doc().source === "sample" ? "章" : "段"}{chapterPositions()[doc().id] !== undefined ? ` · 本窗口停在第 ${chapterPositions()[doc().id] + 1} ${doc().source === "sample" ? "章" : "段"}` : ""}</Caption></View><If when={!compactUI()}><Label small>打开阅读</Label></If><Glyph name="arrow" size={16} /></Button>}</Each>
      <If when={visible().length === 0}><View style={{ paddingVertical: 35, gap: 12 }}><Label>这里还没有匹配的内容</Label><Caption>可以更换筛选、清空查找条件，或打开一份文本文件。</Caption></View></If>
    </View>
    <View style={{ gap: 7 }}><Caption>本窗口可打开 UTF-8 TXT、Markdown（按纯文本显示）；PDF、EPUB 暂未接入。</Caption><Caption>“本次打开”只保留在当前窗口，不代表正式导入书库；退出后需要重新选择。</Caption></View>
  </View></Scroll>;
}

function Compose(props: Props) {
  return <Scroll style={{ flex: 1 }}><View style={{ padding: compactUI() ? 18 : 40, maxWidth: 900, gap: 18 }}><Text style={{ fontSize: 25, fontWeight: "600", color: ink("text-primary") }}>把一段文本放进阅读空间</Text><Caption>可以粘贴文章或直接输入，只保留在这个窗口。</Caption><Input accessibilityLabel="文本标题" value={inputTitle()} onChange={({ value }) => inputTitle.set(value)} placeholder="标题（可选）" style={{ height: 40, fontSize: 17, padding: 8, borderWidth: 1, borderRadius: 5, borderColor: ink("border-control"), color: ink("text-primary"), backgroundColor: ink("surface-paper") }} /><Input accessibilityLabel="输入要阅读的文本" value={inputText()} onChange={({ value }) => inputText.set(value)} multiline placeholder="在这里粘贴或输入文本…" style={{ minHeight: 350, padding: 15, fontSize: 17, lineHeight: 29, borderWidth: 1, borderRadius: 5, borderColor: ink("border-control"), color: ink("text-primary"), backgroundColor: ink("surface-paper") }} focusStyle={{ outlineColor: ink("focus-ring"), outlineWidth: 1 }} /><View style={{ flexDirection: "row", gap: 12 }}><Control label="开始阅读输入的文本" text="开始阅读" icon="book" primary disabled={!inputText().trim()} onPress={() => { try { addTextDocument(inputTitle(), inputText(), "paste", "输入文本"); props.enterReading(); } catch (e) { readerMessage.set(e instanceof Error ? e.message : "暂时无法打开这段文本。"); } }} /><Control label="返回书架，保留输入" text="返回书架" onPress={() => shelf(props)} /></View></View></Scroll>;
}

function TableOfContents() {
  return <Scroll contentStyle={{ padding: 15, gap: 12 }} style={{ width: compactUI() ? "100%" : 215, flexShrink: 0, backgroundColor: ink("surface-inspector"), borderRightWidth: 1, borderColor: ink("border-subtle") }}><View style={{ flexDirection:"row", alignItems:"center" }}><Label weight="600">目录</Label><View style={{flex:1}} /><If when={compactUI()}><Control label="返回正文" text="正文" icon="back" onPress={() => tocOpen.set(false)} /></If></View><Each of={currentDocument()?.chapters ?? []}>{(chapter, index) => <Button accessibilityLabel={`转到${chapter().title}`} onPress={() => { goToChapter(index()); if (compactUI()) tocOpen.set(false); }} style={{ minHeight: compactUI() ? 48 : 36, padding: 10, borderRadius: 5, backgroundColor: ink(index() === chapterIndex() ? "surface-selected" : "transparent") }} hoverStyle={{ backgroundColor: ink("surface-hover") }}><Text style={{ fontSize: 13, lineHeight: 22, color: ink("text-primary") }}>{chapter().title}</Text></Button>}</Each></Scroll>;
}

function SelectableChapter(props: { identity: string; title: string; chapter: string; text: string }) {
  // A new source/chapter receives a new native selection container. Late events
  // from the retired container cannot select text in its successor.
  const identity = props.identity, title = props.title, chapter = props.chapter;
  return <View selectable accessibilityLabel="可选择的阅读正文" onSelectionChange={({ text }) => {
    if (`${currentDocument()?.id}:${chapterIndex()}` !== identity) return;
    if (text) pickedPassage.set({ text, title, chapter, live: true });
    else if (pickedPassage()) pickedPassage.set({ ...pickedPassage()!, live: false });
  }}><Text style={{ userSelect: "text", fontSize: textSize(), lineHeight: Math.round(textSize() * 1.85), color: ink("text-primary") }}>{props.text}</Text></View>;
}

function ReadingPage() {
  return <View style={{ flex: 1, flexDirection: "row", minHeight: 0 }}><If when={tocOpen()}><TableOfContents /></If><If when={!compactUI() || !tocOpen()}><Scroll style={{ flex: 1 }}><View style={{ alignItems: "center", paddingHorizontal: compactUI() ? 20 : 34, paddingVertical: compactUI() ? 24 : 38 }}><View style={{ width: "100%", maxWidth: 710, gap: 21 }}><Caption>{currentDocument()?.title} · {currentDocument()?.byline}</Caption><Text style={{ fontSize: 25, lineHeight: 36, fontWeight: "600", color: ink("text-primary") }}>{currentChapter()?.title}</Text><Rule /><Each of={currentDocument() && currentChapter() ? [{ identity: `${currentDocument()!.id}:${chapterIndex()}`, title: currentDocument()!.title, chapter: currentChapter()!.title, text: currentChapter()!.text }] : []} key={part => part.identity}>{part => <SelectableChapter identity={part().identity} title={part().title} chapter={part().chapter} text={part().text} />}</Each><View style={{ paddingVertical: 22 }}><Caption>在正文上按住并拖动即可选中文字。选文可复制；正式摘录与笔记保存尚未接入本样例。</Caption></View></View></View></Scroll></If></View>;
}

function ReadingMenu(props: Props) {
  const item = { minHeight: 48, paddingHorizontal: 15, paddingVertical: 12, borderRadius: 5 };
  return <Menu offset={7} margin={8} popupStyle={{ padding: 5, minWidth: 220, borderRadius: 10, backgroundColor: ink("surface-floating"), boxShadow: "0 4px 20px rgba(0,0,0,0.18)" }} trigger={<Control label={readerMode() === "shelf" ? "添加阅读内容" : "阅读工具"} icon={readerMode() === "shelf" ? "plus" : "more"} />}>
    <If when={readerMode() === "shelf"}><MenuItem style={item} onSelect={() => { void chooseTextFile(props); }}><Label>打开文本文件…</Label></MenuItem><MenuItem style={item} onSelect={() => { readerMessage.set(""); readerMode.set("compose"); }}><Label>输入或粘贴文本</Label></MenuItem></If>
    <If when={readerMode() === "read"}><MenuItem style={item} onSelect={() => tocOpen.set(!tocOpen())}><Label>{tocOpen() ? "返回正文" : "打开目录"}</Label></MenuItem><MenuItem style={item} onSelect={() => { pickedPassage.set(null); textSize.set(Math.min(26,textSize()+1)); }}><Label>增大字号</Label></MenuItem><MenuItem style={item} onSelect={() => { pickedPassage.set(null); textSize.set(Math.max(16,textSize()-1)); }}><Label>减小字号</Label></MenuItem><MenuItem style={item} onSelect={() => shelf(props)}><Label>返回书架</Label></MenuItem></If>
  </Menu>;
}

export function ReadingWorkspace(props: Props) {
  return <View style={{ flex: 1, minWidth: 0, backgroundColor: ink(readerMode() === "read" ? "surface-paper" : "surface-stage") }}>
    <View desktop={{ windowDrag: true }} style={{ height: 52, flexShrink: 0, flexDirection: "row", alignItems: "center", gap: 9, paddingLeft: props.sidebarVisible ? 18 : titleBar().left + (compactUI() ? 6 : 12), paddingRight: titleBar().right + 17, borderBottomWidth: 1, borderColor: ink("border-subtle"), backgroundColor: ink("surface-toolbar") }}>
      <If when={!props.sidebarVisible && (!compactUI() || readerMode() === "shelf")}><Control label="展开全局导航" icon="panel" onPress={props.toggleSidebar} /></If>
      <If when={readerMode() !== "shelf"}><Control label="返回书架" icon="back" text={compactUI() ? undefined : "书架"} onPress={() => shelf(props)} /></If><Label weight="600">{readerMode() === "read" ? (compactUI() ? "阅读" : currentDocument()?.title) : readerMode() === "compose" ? "输入文本" : (compactUI() ? "书架" : "书架与文本")}</Label><View style={{ flex: 1 }} />
      <If when={readerMode() === "read" && !compactUI()}><Control label="打开或收起目录" icon="book" text="目录" selected={tocOpen()} onPress={() => tocOpen.set(!tocOpen())} /><Control label="减小阅读字号" text="A−" onPress={() => { pickedPassage.set(null); textSize.set(Math.max(16, textSize() - 1)); }} /><Control label="增大阅读字号" text="A＋" onPress={() => { pickedPassage.set(null); textSize.set(Math.min(26, textSize() + 1)); }} /></If>
      <If when={readerMode() === "shelf" && !compactUI()}><Control label="输入或粘贴文本" icon="pen" text="输入文本" onPress={() => { readerMessage.set(""); readerMode.set("compose"); }} /><Control label="打开本机文本文件" icon="plus" text={picking() ? "正在选择…" : "打开文本文件…"} disabled={picking()} raised onPress={() => { void chooseTextFile(props); }} /></If>
      <If when={compactUI() && readerMode() !== "compose"}><ReadingMenu {...props} /></If>
    </View>
    <If when={readerMode() === "shelf"}><Shelf {...props} /></If><If when={readerMode() === "compose"}><Compose {...props} /></If><If when={readerMode() === "read"}><ReadingPage /></If>
    <If when={readerMessage()}><View accessibilityLabel="阅读状态提示" style={{ paddingHorizontal: 24, paddingVertical: 8, backgroundColor: ink("surface-toolbar"), borderTopWidth: 1, borderColor: ink("border-subtle") }}><Caption>{readerMessage()}</Caption></View></If>
    <If when={readerMode() === "read"}><View style={{ minHeight: 49, paddingHorizontal: compactUI() ? 12 : 22, paddingVertical: 9, flexDirection: "row", alignItems: "center", gap: 10, backgroundColor: ink("surface-toolbar"), borderTopWidth: 1, borderColor: ink("border-subtle") }}><If when={pickedPassage()}><Text style={{ flex: 1, fontSize: 12, color: ink("text-secondary"), numberOfLines: 1 }}>{pickedPassage()?.live ? "已选" : "上次选文"} {Array.from(pickedPassage()?.text ?? "").length} 字 · {pickedPassage()?.text}</Text><Control label="复制选文" icon="document" text={compactUI() ? "复制" : "复制选文"} raised onPress={() => { const p = pickedPassage(); if (p) Clipboard.writeText(p.text).then(() => readerMessage.set("选文已复制。"), () => readerMessage.set("暂时无法复制，请重试。")); }} /></If><If when={!pickedPassage()}><Caption>文字阅读 · 当前窗口</Caption><View style={{ flex: 1 }} /></If><Control label="上一章或上一段" icon="back" disabled={chapterIndex() === 0} onPress={() => goToChapter(chapterIndex() - 1)} /><Caption>{chapterIndex() + 1} / {currentDocument()?.chapters.length}</Caption><Control label="下一章或下一段" icon="arrow" disabled={chapterIndex() + 1 >= (currentDocument()?.chapters.length ?? 0)} onPress={() => goToChapter(chapterIndex() + 1)} /></View></If>
  </View>;
}
