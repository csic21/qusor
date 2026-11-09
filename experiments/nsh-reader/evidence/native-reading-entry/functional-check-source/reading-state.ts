import { signal } from "nsh";

export type Chapter = { title: string; text: string };
export type ReadingDocument = {
  id: string; title: string; byline: string; format: string;
  source: "sample" | "file" | "paste"; tint: string; chapters: Chapter[];
};
export const documents = signal<ReadingDocument[]>([
  { id: "sample-book", title: "求索：阅读与思考", byline: "QUSOR · 自拟示例文集", format: "文本书籍", source: "sample", tint: "#56736C", chapters: [
    { title: "第一章　从一本书开始", text: "阅读从选择一本书开始。找到书架，打开内容，选中一段文字，再回到资料列表，这些动作应该连贯而自然。\n\n一本书不只是封面与标题。真正的阅读空间，需要让正文拥有足够的宽度，让文字的大小和行距适合长时间观看。目录与翻章控制应当容易找到，其他工具则在需要的时候出现。\n\n你可以在这段文字上按住并拖动，选择其中的一部分。底部会显示选文，并提供复制入口。选中的文字来自当前正文，没有预先编好的固定选段。\n\n如果要阅读自己的文本，可以返回书架，选择“打开文本文件”，或使用“输入文本”。本次打开的内容只保留在当前窗口，不会上传，也不会显示成已经收录的知识。\n\n读到这里，可以打开目录，或者使用下方的下一章按钮。返回书架再打开这本示例，会继续显示本窗口中选择的章节。" },
    { title: "第二章　给思考留一点空白", text: "读书的时候，我们常常同时做两件事：理解作者在说什么，以及辨认自己的想法。两者有关联，却值得分开保留。\n\n原文是原文，个人理解是个人理解。一段准确的引用应当保留来源；一条想法可以提出疑问，也可以联系另一段经验。界面需要帮助我们核对这些关系。\n\n在完整的阅读流程里，选择文字后可以保存摘录、写想法或主动向 Agent 提问。这个窗口目前只接通文字阅读、选择和复制；正式摘录、笔记保存与来源定位仍走既有 Core 和插件链路。\n\n工具在需要时出现，阅读便能保持连续。你可以调大字号，重新阅读前一段，也可以切回书架选择另一份文本。" },
    { title: "第三章　让工具适应内容", text: "同一个基座可以承载不同的工作。读书时，主内容是正文；写作时，主内容是文稿；安排工作时，主内容可能是一段可检查的过程。\n\n阅读入口是阅读场景提供的能力，不要求所有插件都长成书架。任务导航、窗口和公共控件保持稳定，内容与适用工具则随场景变化。\n\n本次示例中的书籍由项目自拟，用来检查选择资料、阅读、选文与返回的路径。它不代表已经存在一个正式资料库，也不代表本地文件已经持久导入。\n\n合上这一章之前，可以回到目录看看整本示例的结构。好的界面应当告诉你身在何处，也让你知道怎样继续。" },
  ] },
  { id: "sample-text", title: "一份普通的文本", byline: "QUSOR · 自拟短文", format: "Markdown 文本", source: "sample", tint: "#A08055", chapters: [
    { title: "阅读短文", text: "文本不一定来自一本完整的书。它也可能是一篇文章、一段摘记，或者临时复制过来的材料。\n\n选择书架中的资料，或者打开本机的 UTF-8 TXT / Markdown 文件，都应该进入同一个安静的阅读空间。\n\nMarkdown 在这个样例里按纯文本显示，不执行其中的 HTML、脚本或链接。原文件不被修改，正式资料收录也没有在后台发生。" },
  ] },
]);
export const readerMode = signal<"shelf" | "read" | "compose">("shelf");
export const activeDocument = signal<string | null>(null);
export const chapterPositions = signal<Record<string, number>>({});
export const shelfQuery = signal("");
export const sourceFilter = signal("all");
export const textSize = signal(19);
export const tocOpen = signal(false);
export const readerMessage = signal("");
export const picking = signal(false);
export const inputTitle = signal("");
export const inputText = signal("");
export const pickedPassage = signal<{ text: string; title: string; chapter: string; live: boolean } | null>(null);
export const currentDocument = () => documents().find(d => d.id === activeDocument()) ?? null;
export const chapterIndex = () => chapterPositions()[activeDocument() ?? ""] ?? 0;
export const currentChapter = () => currentDocument()?.chapters[chapterIndex()] ?? null;
let documentSequence = 0;

export function openDocument(id: string) {
  if (!documents().some(d => d.id === id)) return;
  pickedPassage.set(null); readerMessage.set(""); tocOpen.set(false);
  if (chapterPositions()[id] === undefined) chapterPositions.set({ ...chapterPositions(), [id]: 0 });
  activeDocument.set(id); readerMode.set("read");
}
export function goToChapter(index: number) {
  const doc = currentDocument();
  if (!doc || index < 0 || index >= doc.chapters.length) return;
  pickedPassage.set(null);
  chapterPositions.set({ ...chapterPositions(), [doc.id]: index });
  readerMessage.set("");
}

export function splitText(text: string): Chapter[] {
  // Bounded display chunks, not permanent source anchors or EPUB chapters.
  const normalized = text.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  if (!normalized.trim()) throw new Error("文件没有可阅读的文字。");
  if (normalized.includes("\0")) throw new Error("文件含有非文本内容，请选择 UTF-8 TXT 或 Markdown。");
  const chunks: Chapter[] = [];
  const chars = Array.from(normalized);
  for (let i = 0; i < chars.length; i += 3000) {
    chunks.push({ title: `第 ${chunks.length + 1} 段`, text: chars.slice(i, i + 3000).join("") });
  }
  return chunks;
}

export function addTextDocument(title: string, text: string, source: "file" | "paste", format: string) {
  if (text.length > 1_048_576) throw new Error("这份文本过大；本阅读样例暂限 1 MiB UTF-8 文本。");
  const chapters = splitText(text);
  const id = `local-text-${++documentSequence}`;
  const doc: ReadingDocument = { id, title: title.trim() || "未命名文本", byline: source === "file" ? "本机文件 · 当前窗口" : "输入文本 · 当前窗口", format, source, tint: "#7387A1", chapters };
  documents.set([...documents(), doc]); openDocument(id);
}
