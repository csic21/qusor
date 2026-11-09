import { append, bindProp, component, cond, each, element, insert, prop, setup, text } from "nsh:rt";
import { Canvas, Clipboard, Path2D, Picker, mount, platform, setColorScheme, signal, token } from "nsh";
import { Button, Menu, MenuItem, MenuSeparator, Tab, Tabs } from "nsh/base";
import { currentWindow, titleBar } from "nsh/desktop";

//#region ui.tsx
const compactUI = signal(false);
const touchUI = () => compactUI() || !platform.desktop;
const ink = (role) => token(role);
const fonts = {
	caption: 12,
	small: 13,
	ui: 14,
	heading: 17,
	body: 16,
	title: 26
};
const line = {
	caption: 18,
	ui: 21,
	body: 28,
	title: 36
};
const paths = {
	search: "M10.5 3.5a7 7 0 1 0 0 14a7 7 0 1 0 0-14 M16 16l4.5 4.5",
	plus: "M12 5v14 M5 12h14",
	close: "M6 6l12 12 M18 6L6 18",
	chevron: "M7 10l5 5l5-5",
	back: "M14 6l-6 6l6 6",
	document: "M5 3h10l4 4v14H5z M14 3v5h5 M8 12h8 M8 16h8",
	book: "M12 5v15 M3 4q5-2 9 1q4-3 9-1v15q-5-2-9 1q-4-3-9-1z",
	pen: "M5 16L16 5l3 3L8 19l-4 1z M14 7l3 3",
	plug: "M8 3v5 M16 3v5 M6 8h12v4a6 6 0 0 1-12 0z M12 18v4",
	code: "M8 6l-6 6l6 6 M16 6l6 6l-6 6 M14 3l-4 18",
	panel: "M3 4h18v16H3z M15 4v16",
	flow: "M3 3h7v6H3z M14 15h7v6h-7z M7 9v9h7 M7 13h11V7 M15 7h6",
	sun: "M12 7a5 5 0 1 0 0 10a5 5 0 1 0 0-10 M12 1v2 M12 21v2 M1 12h2 M21 12h2 M4 4l2 2 M18 18l2 2 M4 20l2-2 M18 6l2-2",
	moon: "M19 16A9 9 0 1 1 8 4A8 8 0 0 0 19 16z",
	more: "M5 11v2 M12 11v2 M19 11v2",
	arrow: "M5 12h14 M13 6l6 6l-6 6",
	check: "M5 12l5 5L20 7",
	stack: "M3 8l9-5l9 5l-9 5z M3 12l9 5l9-5 M3 16l9 5l9-5",
	circle: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18"
};
function Glyph(props) {
	return component(Canvas, {
		get style() {
			return {
				width: props.size ?? 18,
				height: props.size ?? 18,
				pointerEvents: "none"
			};
		},
		draw: (cx, { width }) => {
			cx.scale(width / 24, width / 24);
			cx.strokeStyle = props.color ?? ink("text-secondary");
			cx.lineWidth = 1.7;
			cx.lineCap = "round";
			cx.lineJoin = "round";
			cx.stroke(new Path2D(paths[props.name] ?? paths.circle));
		}
	});
}
function Mark(props) {
	return component(Canvas, {
		get style() {
			return {
				width: props.size ?? 25,
				height: props.size ?? 25
			};
		},
		draw: (cx, { width }) => {
			cx.scale(width / 28, width / 28);
			cx.fillStyle = ink("text-primary");
			cx.beginPath();
			cx.roundRect(3, 3, 22, 22, 6);
			cx.fill();
			cx.strokeStyle = ink("surface-paper");
			cx.lineWidth = 1.5;
			cx.lineJoin = "round";
			cx.stroke(new Path2D("M8 9h12v10H8z M11 6v16 M8 13h12"));
		}
	});
}
function Label(props) {
	return (() => {
		const __nsh_hv0 = element("text");
		bindProp(__nsh_hv0, "style", () => ({
			color: ink(props.muted ? "text-secondary" : "text-primary"),
			fontSize: touchUI() ? 15 : props.small ? fonts.small : fonts.ui,
			lineHeight: line.ui,
			fontWeight: props.weight ?? "400",
			numberOfLines: 1
		}));
		insert(__nsh_hv0, () => props.children);
		return __nsh_hv0;
	})();
}
function Rule() {
	return (() => {
		const __nsh_hv1 = element("view");
		bindProp(__nsh_hv1, "style", () => ({
			height: 1,
			flexShrink: 0,
			backgroundColor: ink("border-subtle")
		}));
		return __nsh_hv1;
	})();
}
function Control(props) {
	return component(Button, {
		get accessibilityLabel() {
			return props.label;
		},
		get disabled() {
			return props.disabled;
		},
		get onPress() {
			return props.onPress;
		},
		get style() {
			return {
				opacity: props.disabled ? .35 : 1,
				height: touchUI() ? 44 : 30,
				minWidth: touchUI() ? 44 : 30,
				paddingHorizontal: props.text ? 10 : 6,
				flexDirection: "row",
				alignItems: "center",
				justifyContent: "center",
				gap: 6,
				borderRadius: 7,
				borderCurve: "continuous",
				borderWidth: props.raised || props.primary ? 1 : 0,
				borderColor: ink(props.primary ? "control-primary" : "border-control"),
				backgroundColor: ink(props.primary ? "control-primary" : props.selected ? "surface-selected" : props.raised ? "surface-control" : "transparent"),
				boxShadow: props.raised ? "0 1px 1px rgba(0,0,0,0.06), inset 0 1px 0 rgba(255,255,255,0.12)" : "none"
			};
		},
		get hoverStyle() {
			return { backgroundColor: ink(props.primary ? "control-primary-hover" : "surface-hover") };
		},
		get activeStyle() {
			return {
				backgroundColor: ink(props.primary ? "control-primary-pressed" : "surface-pressed"),
				boxShadow: "inset 0 1px 2px rgba(0,0,0,0.14)"
			};
		},
		get focusStyle() {
			return {
				outlineWidth: 2,
				outlineOffset: 2,
				outlineColor: ink("focus-ring")
			};
		},
		get children() {
			return [
				cond(() => props.icon, () => component(Glyph, {
					get name() {
						return props.icon;
					},
					size: 16,
					get color() {
						return ink(props.primary ? "control-primary-text" : props.selected ? "text-accent" : "text-secondary");
					}
				})),
				cond(() => props.text, () => (() => {
					const __nsh_hv2 = element("text");
					bindProp(__nsh_hv2, "style", () => ({
						fontSize: touchUI() ? 15 : fonts.small,
						fontWeight: "500",
						color: ink(props.primary ? "control-primary-text" : "text-primary")
					}));
					insert(__nsh_hv2, () => props.text);
					return __nsh_hv2;
				})()),
				() => props.children
			];
		}
	});
}
function Segments(props) {
	return component(Tabs, {
		get style() {
			return {
				flexDirection: "row",
				height: touchUI() ? 44 : 31,
				padding: 2,
				borderRadius: 7,
				borderCurve: "continuous",
				borderWidth: 1,
				borderColor: ink("border-control"),
				backgroundColor: ink("surface-track"),
				boxShadow: "inset 0 1px 1px rgba(0,0,0,0.035)"
			};
		},
		get indicatorStyle() {
			return {
				top: 2,
				bottom: 2,
				borderRadius: 5,
				backgroundColor: ink("surface-control"),
				boxShadow: "0 1px 3px rgba(0,0,0,0.12)"
			};
		},
		get children() {
			return each(() => props.items, (item) => component(Tab, {
				get accessibilityLabel() {
					return item().label;
				},
				get selected() {
					return props.value === item().id;
				},
				onPress: () => props.onChange(item().id),
				get style() {
					return {
						flex: touchUI() ? 1 : void 0,
						paddingHorizontal: touchUI() ? 10 : 17,
						justifyContent: "center",
						alignItems: "center",
						borderRadius: 5
					};
				},
				get focusStyle() {
					return {
						outlineWidth: 2,
						outlineColor: ink("focus-ring")
					};
				},
				get children() {
					return (() => {
						const __nsh_hv3 = element("text");
						bindProp(__nsh_hv3, "style", () => ({
							fontSize: touchUI() ? 15 : fonts.small,
							fontWeight: props.value === item().id ? "600" : "400",
							color: ink(props.value === item().id ? "text-primary" : "text-secondary")
						}));
						insert(__nsh_hv3, () => item().label);
						return __nsh_hv3;
					})();
				}
			}));
		}
	});
}

//#endregion
//#region reading-state.ts
const documents = signal([{
	id: "sample-book",
	title: "求索：阅读与思考",
	byline: "QUSOR · 自拟示例文集",
	format: "文本书籍",
	source: "sample",
	tint: "#56736C",
	chapters: [
		{
			title: "第一章　从一本书开始",
			text: "阅读从选择一本书开始。找到书架，打开内容，选中一段文字，再回到资料列表，这些动作应该连贯而自然。\n\n一本书不只是封面与标题。真正的阅读空间，需要让正文拥有足够的宽度，让文字的大小和行距适合长时间观看。目录与翻章控制应当容易找到，其他工具则在需要的时候出现。\n\n你可以在这段文字上按住并拖动，选择其中的一部分。底部会显示选文，并提供复制入口。选中的文字来自当前正文，没有预先编好的固定选段。\n\n如果要阅读自己的文本，可以返回书架，选择“打开文本文件”，或使用“输入文本”。本次打开的内容只保留在当前窗口，不会上传，也不会显示成已经收录的知识。\n\n读到这里，可以打开目录，或者使用下方的下一章按钮。返回书架再打开这本示例，会继续显示本窗口中选择的章节。"
		},
		{
			title: "第二章　给思考留一点空白",
			text: "读书的时候，我们常常同时做两件事：理解作者在说什么，以及辨认自己的想法。两者有关联，却值得分开保留。\n\n原文是原文，个人理解是个人理解。一段准确的引用应当保留来源；一条想法可以提出疑问，也可以联系另一段经验。界面需要帮助我们核对这些关系。\n\n在完整的阅读流程里，选择文字后可以保存摘录、写想法或主动向 Agent 提问。这个窗口目前只接通文字阅读、选择和复制；正式摘录、笔记保存与来源定位仍走既有 Core 和插件链路。\n\n工具在需要时出现，阅读便能保持连续。你可以调大字号，重新阅读前一段，也可以切回书架选择另一份文本。"
		},
		{
			title: "第三章　让工具适应内容",
			text: "同一个基座可以承载不同的工作。读书时，主内容是正文；写作时，主内容是文稿；安排工作时，主内容可能是一段可检查的过程。\n\n阅读入口是阅读场景提供的能力，不要求所有插件都长成书架。任务导航、窗口和公共控件保持稳定，内容与适用工具则随场景变化。\n\n本次示例中的书籍由项目自拟，用来检查选择资料、阅读、选文与返回的路径。它不代表已经存在一个正式资料库，也不代表本地文件已经持久导入。\n\n合上这一章之前，可以回到目录看看整本示例的结构。好的界面应当告诉你身在何处，也让你知道怎样继续。"
		}
	]
}, {
	id: "sample-text",
	title: "一份普通的文本",
	byline: "QUSOR · 自拟短文",
	format: "Markdown 文本",
	source: "sample",
	tint: "#A08055",
	chapters: [{
		title: "阅读短文",
		text: "文本不一定来自一本完整的书。它也可能是一篇文章、一段摘记，或者临时复制过来的材料。\n\n选择书架中的资料，或者打开本机的 UTF-8 TXT / Markdown 文件，都应该进入同一个安静的阅读空间。\n\nMarkdown 在这个样例里按纯文本显示，不执行其中的 HTML、脚本或链接。原文件不被修改，正式资料收录也没有在后台发生。"
	}]
}]);
const readerMode = signal("shelf");
const activeDocument = signal(null);
const chapterPositions = signal({});
const shelfQuery = signal("");
const sourceFilter = signal("all");
const textSize = signal(19);
const tocOpen = signal(false);
const readerMessage = signal("");
const picking = signal(false);
const inputTitle = signal("");
const inputText = signal("");
const pickedPassage = signal(null);
const currentDocument = () => documents().find((d) => d.id === activeDocument()) ?? null;
const chapterIndex = () => chapterPositions()[activeDocument() ?? ""] ?? 0;
const currentChapter = () => currentDocument()?.chapters[chapterIndex()] ?? null;
let documentSequence = 0;
function openDocument(id) {
	if (!documents().some((d) => d.id === id)) return;
	pickedPassage.set(null);
	readerMessage.set("");
	tocOpen.set(false);
	if (chapterPositions()[id] === void 0) chapterPositions.set({
		...chapterPositions(),
		[id]: 0
	});
	activeDocument.set(id);
	readerMode.set("read");
}
function goToChapter(index) {
	const doc = currentDocument();
	if (!doc || index < 0 || index >= doc.chapters.length) return;
	pickedPassage.set(null);
	chapterPositions.set({
		...chapterPositions(),
		[doc.id]: index
	});
	readerMessage.set("");
}
function splitText(text) {
	const normalized = text.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
	if (!normalized.trim()) throw new Error("文件没有可阅读的文字。");
	if (normalized.includes("\0")) throw new Error("文件含有非文本内容，请选择 UTF-8 TXT 或 Markdown。");
	const chunks = [];
	const chars = Array.from(normalized);
	for (let i = 0; i < chars.length; i += 3e3) chunks.push({
		title: `第 ${chunks.length + 1} 段`,
		text: chars.slice(i, i + 3e3).join("")
	});
	return chunks;
}
function addTextDocument(title, text, source, format) {
	if (text.length > 1048576) throw new Error("这份文本过大；本阅读样例暂限 1 MiB UTF-8 文本。");
	const chapters = splitText(text);
	const id = `local-text-${++documentSequence}`;
	const doc = {
		id,
		title: title.trim() || "未命名文本",
		byline: source === "file" ? "本机文件 · 当前窗口" : "输入文本 · 当前窗口",
		format,
		source,
		tint: "#7387A1",
		chapters
	};
	documents.set([...documents(), doc]);
	openDocument(id);
}

//#endregion
//#region ReadingWorkspace.tsx
const readingEntry = {
	id: "reading",
	title: "书架与文本",
	icon: "book"
};
function Caption$1(props) {
	return (() => {
		const __nsh_hv0 = element("text");
		bindProp(__nsh_hv0, "style", () => ({
			fontSize: compactUI() ? 13 : 12,
			lineHeight: compactUI() ? 21 : 19,
			color: ink("text-secondary")
		}));
		insert(__nsh_hv0, () => props.children);
		return __nsh_hv0;
	})();
}
function read(id, props) {
	openDocument(id);
	props.enterReading();
}
function shelf(props) {
	pickedPassage.set(null);
	readerMode.set("shelf");
	props.returnShelf();
}
async function chooseTextFile(props) {
	if (picking()) return;
	picking.set(true);
	readerMessage.set("");
	try {
		const [picked] = await Picker.pickFiles({
			multiple: false,
			filters: [{
				name: "UTF-8 文本",
				extensions: [
					"txt",
					"md",
					"markdown"
				]
			}]
		});
		if (!picked) {
			readerMessage.set("未选择文件，原来的阅读内容保持。");
			return;
		}
		const file = picked.file;
		if (!/\.(txt|md|markdown)$/i.test(file.name)) throw new Error("当前可打开 TXT 和 Markdown；PDF、EPUB 尚未接入这个窗口。");
		if (file.size > 1048576) throw new Error("这份文件超过 1 MiB；本阅读样例暂不打开更大的文本。");
		const bytes = new Uint8Array(await file.arrayBuffer());
		let text;
		try {
			text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
		} catch {
			throw new Error("这份文件不是可识别的 UTF-8 文本，请转换编码后重试。");
		}
		addTextDocument(file.name, text, "file", /\.txt$/i.test(file.name) ? "TXT" : "Markdown 文本");
		props.enterReading();
	} catch (error) {
		readerMessage.set(error instanceof Error ? error.message : "暂时无法打开文本，原来的内容保持。");
	} finally {
		picking.set(false);
	}
}
function Cover(props) {
	return (() => {
		const __nsh_hv1 = element("view");
		bindProp(__nsh_hv1, "style", () => ({
			width: compactUI() ? 44 : 52,
			height: compactUI() ? 64 : 72,
			flexShrink: 0,
			borderRadius: 2,
			backgroundColor: props.document.tint,
			borderLeftWidth: 4,
			borderLeftColor: "rgba(0,0,0,0.15)",
			boxShadow: "1px 2px 3px rgba(0,0,0,0.14)",
			paddingHorizontal: 7,
			paddingTop: compactUI() ? 9 : 12,
			gap: 4,
			overflow: "hidden"
		}));
		const __nsh_hv2 = element("view");
		prop(__nsh_hv2, "style", {
			width: 16,
			height: 1,
			backgroundColor: "rgba(255,255,255,0.65)"
		});
		const __nsh_hv3 = element("text");
		bindProp(__nsh_hv3, "style", () => ({
			fontSize: compactUI() ? 10 : 11,
			lineHeight: compactUI() ? 14 : 16,
			color: "#FFFFFF",
			fontWeight: "500",
			numberOfLines: 2
		}));
		insert(__nsh_hv3, () => props.document.title.slice(0, 6));
		const __nsh_hv4 = element("view");
		prop(__nsh_hv4, "style", { flex: 1 });
		const __nsh_hv5 = element("text");
		prop(__nsh_hv5, "style", {
			fontSize: 7,
			lineHeight: 9,
			numberOfLines: 1,
			letterSpacing: .7,
			color: "rgba(255,255,255,0.8)",
			marginBottom: 6
		});
		const __nsh_hv6 = text("QUSOR");
		append(__nsh_hv5, __nsh_hv6);
		append(__nsh_hv1, __nsh_hv2);
		append(__nsh_hv1, __nsh_hv3);
		append(__nsh_hv1, __nsh_hv4);
		append(__nsh_hv1, __nsh_hv5);
		return __nsh_hv1;
	})();
}
function Shelf(props) {
	const visible = () => documents().filter((d) => (!shelfQuery() || d.title.includes(shelfQuery())) && (sourceFilter() === "all" || (sourceFilter() === "sample" ? d.source === "sample" : d.source !== "sample")));
	return (() => {
		const __nsh_hv7 = element("scroll");
		prop(__nsh_hv7, "style", { flex: 1 });
		const __nsh_hv8 = element("view");
		bindProp(__nsh_hv8, "style", () => ({
			paddingHorizontal: compactUI() ? 16 : 46,
			paddingTop: compactUI() ? 23 : 34,
			paddingBottom: 38,
			gap: 23
		}));
		const __nsh_hv9 = element("view");
		prop(__nsh_hv9, "style", { gap: 8 });
		const __nsh_hv10 = element("text");
		bindProp(__nsh_hv10, "style", () => ({
			fontSize: compactUI() ? 23 : 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv11 = text("选择接下来要读的内容");
		append(__nsh_hv10, __nsh_hv11);
		append(__nsh_hv9, __nsh_hv10);
		insert(__nsh_hv9, component(Caption$1, { get children() {
			return "示例书籍可以直接打开。自己的文本可从本机选择，或在这里输入。";
		} }));
		const __nsh_hv12 = element("view");
		bindProp(__nsh_hv12, "style", () => ({
			flexDirection: compactUI() ? "column" : "row",
			alignItems: compactUI() ? "stretch" : "center",
			gap: compactUI() ? 12 : 18
		}));
		const __nsh_hv13 = element("view");
		prop(__nsh_hv13, "style", { flex: 1 });
		const __nsh_hv14 = element("view");
		bindProp(__nsh_hv14, "style", () => ({
			width: compactUI() ? "100%" : 200,
			height: compactUI() ? 44 : 31,
			flexDirection: "row",
			alignItems: "center",
			gap: 7,
			paddingHorizontal: 9,
			borderRadius: 7,
			borderWidth: 1,
			borderColor: ink("border-control"),
			backgroundColor: ink("surface-search")
		}));
		const __nsh_hv15 = element("input");
		prop(__nsh_hv15, "accessibilityLabel", "查找书籍或文本");
		bindProp(__nsh_hv15, "value", () => shelfQuery());
		prop(__nsh_hv15, "onChange", ({ value }) => shelfQuery.set(value));
		prop(__nsh_hv15, "placeholder", "查找书籍或文本");
		bindProp(__nsh_hv15, "style", () => ({
			flex: 1,
			height: 27,
			fontSize: 13,
			padding: 0,
			borderWidth: 0,
			backgroundColor: "transparent",
			color: ink("text-primary")
		}));
		append(__nsh_hv14, __nsh_hv15);
		insert(__nsh_hv14, component(Glyph, {
			name: "search",
			size: 15
		}), __nsh_hv15);
		append(__nsh_hv12, __nsh_hv13);
		append(__nsh_hv12, __nsh_hv14);
		insert(__nsh_hv12, component(Segments, {
			get value() {
				return sourceFilter();
			},
			onChange: (v) => sourceFilter.set(v),
			items: [
				{
					id: "all",
					label: "全部"
				},
				{
					id: "sample",
					label: "示例书籍"
				},
				{
					id: "local",
					label: "本次打开"
				}
			]
		}), __nsh_hv13);
		const __nsh_hv16 = element("view");
		prop(__nsh_hv16, "style", {
			flexDirection: "row",
			paddingHorizontal: 4
		});
		const __nsh_hv17 = element("text");
		bindProp(__nsh_hv17, "style", () => ({
			flex: 1,
			fontSize: 12,
			color: ink("text-secondary")
		}));
		const __nsh_hv18 = text("书籍与文本");
		append(__nsh_hv17, __nsh_hv18);
		append(__nsh_hv16, __nsh_hv17);
		insert(__nsh_hv16, component(Caption$1, { get children() {
			return [() => visible().length, " 项"];
		} }));
		const __nsh_hv19 = element("view");
		insert(__nsh_hv19, [each(() => visible(), (doc) => component(Button, {
			get accessibilityLabel() {
				return `阅读：${doc().title}`;
			},
			onPress: () => read(doc().id, props),
			get style() {
				return {
					flexDirection: "row",
					alignItems: "center",
					gap: compactUI() ? 13 : 21,
					paddingHorizontal: compactUI() ? 4 : 12,
					paddingVertical: 18,
					borderBottomWidth: 1,
					borderColor: ink("border-subtle"),
					borderRadius: 4
				};
			},
			get hoverStyle() {
				return { backgroundColor: ink("surface-hover") };
			},
			get focusStyle() {
				return {
					outlineWidth: 2,
					outlineColor: ink("focus-ring")
				};
			},
			get children() {
				return [
					component(Cover, { get document() {
						return doc();
					} }),
					(() => {
						const __nsh_hv20 = element("view");
						prop(__nsh_hv20, "style", {
							flex: 1,
							gap: 7
						});
						const __nsh_hv21 = element("text");
						bindProp(__nsh_hv21, "style", () => ({
							fontSize: 17,
							lineHeight: 25,
							fontWeight: "500",
							color: ink("text-primary")
						}));
						insert(__nsh_hv21, () => doc().title);
						append(__nsh_hv20, __nsh_hv21);
						insert(__nsh_hv20, [component(Caption$1, { get children() {
							return doc().byline;
						} }), component(Caption$1, { get children() {
							return [
								() => doc().format,
								" · ",
								() => doc().chapters.length,
								" ",
								() => doc().source === "sample" ? "章" : "段",
								() => chapterPositions()[doc().id] !== void 0 ? ` · 本窗口停在第 ${chapterPositions()[doc().id] + 1} ${doc().source === "sample" ? "章" : "段"}` : ""
							];
						} })]);
						return __nsh_hv20;
					})(),
					cond(() => !compactUI(), () => component(Label, {
						small: true,
						get children() {
							return "打开阅读";
						}
					})),
					component(Glyph, {
						name: "arrow",
						size: 16
					})
				];
			}
		})), cond(() => visible().length === 0, () => (() => {
			const __nsh_hv22 = element("view");
			prop(__nsh_hv22, "style", {
				paddingVertical: 35,
				gap: 12
			});
			insert(__nsh_hv22, [component(Label, { get children() {
				return "这里还没有匹配的内容";
			} }), component(Caption$1, { get children() {
				return "可以更换筛选、清空查找条件，或打开一份文本文件。";
			} })]);
			return __nsh_hv22;
		})())]);
		const __nsh_hv23 = element("view");
		prop(__nsh_hv23, "style", { gap: 7 });
		insert(__nsh_hv23, [component(Caption$1, { get children() {
			return "本窗口可打开 UTF-8 TXT、Markdown（按纯文本显示）；PDF、EPUB 暂未接入。";
		} }), component(Caption$1, { get children() {
			return "“本次打开”只保留在当前窗口，不代表正式导入书库；退出后需要重新选择。";
		} })]);
		append(__nsh_hv8, __nsh_hv9);
		append(__nsh_hv8, __nsh_hv12);
		append(__nsh_hv8, __nsh_hv16);
		append(__nsh_hv8, __nsh_hv19);
		append(__nsh_hv8, __nsh_hv23);
		append(__nsh_hv7, __nsh_hv8);
		return __nsh_hv7;
	})();
}
function Compose(props) {
	return (() => {
		const __nsh_hv24 = element("scroll");
		prop(__nsh_hv24, "style", { flex: 1 });
		const __nsh_hv25 = element("view");
		bindProp(__nsh_hv25, "style", () => ({
			padding: compactUI() ? 18 : 40,
			maxWidth: 900,
			gap: 18
		}));
		const __nsh_hv26 = element("text");
		bindProp(__nsh_hv26, "style", () => ({
			fontSize: 25,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv27 = text("把一段文本放进阅读空间");
		append(__nsh_hv26, __nsh_hv27);
		const __nsh_hv28 = element("input");
		prop(__nsh_hv28, "accessibilityLabel", "文本标题");
		bindProp(__nsh_hv28, "value", () => inputTitle());
		prop(__nsh_hv28, "onChange", ({ value }) => inputTitle.set(value));
		prop(__nsh_hv28, "placeholder", "标题（可选）");
		bindProp(__nsh_hv28, "style", () => ({
			height: 40,
			fontSize: 17,
			padding: 8,
			borderWidth: 1,
			borderRadius: 5,
			borderColor: ink("border-control"),
			color: ink("text-primary"),
			backgroundColor: ink("surface-paper")
		}));
		const __nsh_hv29 = element("input");
		prop(__nsh_hv29, "accessibilityLabel", "输入要阅读的文本");
		bindProp(__nsh_hv29, "value", () => inputText());
		prop(__nsh_hv29, "onChange", ({ value }) => inputText.set(value));
		prop(__nsh_hv29, "multiline", true);
		prop(__nsh_hv29, "placeholder", "在这里粘贴或输入文本…");
		bindProp(__nsh_hv29, "style", () => ({
			minHeight: 350,
			padding: 15,
			fontSize: 17,
			lineHeight: 29,
			borderWidth: 1,
			borderRadius: 5,
			borderColor: ink("border-control"),
			color: ink("text-primary"),
			backgroundColor: ink("surface-paper")
		}));
		bindProp(__nsh_hv29, "focusStyle", () => ({
			outlineColor: ink("focus-ring"),
			outlineWidth: 1
		}));
		const __nsh_hv30 = element("view");
		prop(__nsh_hv30, "style", {
			flexDirection: "row",
			gap: 12
		});
		insert(__nsh_hv30, [component(Control, {
			label: "开始阅读输入的文本",
			text: "开始阅读",
			icon: "book",
			primary: true,
			get disabled() {
				return !inputText().trim();
			},
			onPress: () => {
				try {
					addTextDocument(inputTitle(), inputText(), "paste", "输入文本");
					props.enterReading();
				} catch (e) {
					readerMessage.set(e instanceof Error ? e.message : "暂时无法打开这段文本。");
				}
			}
		}), component(Control, {
			label: "返回书架，保留输入",
			text: "返回书架",
			onPress: () => shelf(props)
		})]);
		append(__nsh_hv25, __nsh_hv26);
		append(__nsh_hv25, __nsh_hv28);
		append(__nsh_hv25, __nsh_hv29);
		append(__nsh_hv25, __nsh_hv30);
		insert(__nsh_hv25, component(Caption$1, { get children() {
			return "可以粘贴文章或直接输入，只保留在这个窗口。";
		} }), __nsh_hv28);
		append(__nsh_hv24, __nsh_hv25);
		return __nsh_hv24;
	})();
}
function TableOfContents() {
	return (() => {
		const __nsh_hv31 = element("scroll");
		prop(__nsh_hv31, "contentStyle", {
			padding: 15,
			gap: 12
		});
		bindProp(__nsh_hv31, "style", () => ({
			width: compactUI() ? "100%" : 215,
			flexShrink: 0,
			backgroundColor: ink("surface-inspector"),
			borderRightWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv32 = element("view");
		prop(__nsh_hv32, "style", {
			flexDirection: "row",
			alignItems: "center"
		});
		const __nsh_hv33 = element("view");
		prop(__nsh_hv33, "style", { flex: 1 });
		append(__nsh_hv32, __nsh_hv33);
		insert(__nsh_hv32, component(Label, {
			weight: "600",
			get children() {
				return "目录";
			}
		}), __nsh_hv33);
		insert(__nsh_hv32, cond(() => compactUI(), () => component(Control, {
			label: "返回正文",
			text: "正文",
			icon: "back",
			onPress: () => tocOpen.set(false)
		})));
		append(__nsh_hv31, __nsh_hv32);
		insert(__nsh_hv31, each(() => currentDocument()?.chapters ?? [], (chapter, index) => component(Button, {
			get accessibilityLabel() {
				return `转到${chapter().title}`;
			},
			onPress: () => {
				goToChapter(index());
				if (compactUI()) tocOpen.set(false);
			},
			get style() {
				return {
					minHeight: compactUI() ? 48 : 36,
					padding: 10,
					borderRadius: 5,
					backgroundColor: ink(index() === chapterIndex() ? "surface-selected" : "transparent")
				};
			},
			get hoverStyle() {
				return { backgroundColor: ink("surface-hover") };
			},
			get children() {
				return (() => {
					const __nsh_hv34 = element("text");
					bindProp(__nsh_hv34, "style", () => ({
						fontSize: 13,
						lineHeight: 22,
						color: ink("text-primary")
					}));
					insert(__nsh_hv34, () => chapter().title);
					return __nsh_hv34;
				})();
			}
		})));
		return __nsh_hv31;
	})();
}
function ReadingPage() {
	return (() => {
		const __nsh_hv35 = element("view");
		prop(__nsh_hv35, "style", {
			flex: 1,
			flexDirection: "row",
			minHeight: 0
		});
		insert(__nsh_hv35, [cond(() => tocOpen(), () => component(TableOfContents, {})), cond(() => !compactUI() || !tocOpen(), () => (() => {
			const __nsh_hv36 = element("scroll");
			prop(__nsh_hv36, "style", { flex: 1 });
			const __nsh_hv37 = element("view");
			bindProp(__nsh_hv37, "style", () => ({
				alignItems: "center",
				paddingHorizontal: compactUI() ? 20 : 34,
				paddingVertical: compactUI() ? 24 : 38
			}));
			const __nsh_hv38 = element("view");
			prop(__nsh_hv38, "style", {
				width: "100%",
				maxWidth: 710,
				gap: 21
			});
			const __nsh_hv39 = element("text");
			bindProp(__nsh_hv39, "style", () => ({
				fontSize: 25,
				lineHeight: 36,
				fontWeight: "600",
				color: ink("text-primary")
			}));
			insert(__nsh_hv39, () => currentChapter()?.title);
			const __nsh_hv40 = element("view");
			prop(__nsh_hv40, "selectable", true);
			prop(__nsh_hv40, "accessibilityLabel", "可选择的阅读正文");
			prop(__nsh_hv40, "onSelectionChange", ({ text }) => {
				if (text) pickedPassage.set({
					text,
					title: currentDocument()?.title ?? "",
					chapter: currentChapter()?.title ?? "",
					live: true
				});
				else if (pickedPassage()) pickedPassage.set({
					...pickedPassage(),
					live: false
				});
			});
			const __nsh_hv41 = element("text");
			bindProp(__nsh_hv41, "style", () => ({
				userSelect: "text",
				fontSize: textSize(),
				lineHeight: Math.round(textSize() * 1.85),
				color: ink("text-primary")
			}));
			insert(__nsh_hv41, () => currentChapter()?.text);
			append(__nsh_hv40, __nsh_hv41);
			const __nsh_hv42 = element("view");
			prop(__nsh_hv42, "style", { paddingVertical: 22 });
			insert(__nsh_hv42, component(Caption$1, { get children() {
				return "在正文上按住并拖动即可选中文字。选文可复制；正式摘录与笔记保存尚未接入本样例。";
			} }));
			append(__nsh_hv38, __nsh_hv39);
			append(__nsh_hv38, __nsh_hv40);
			append(__nsh_hv38, __nsh_hv42);
			insert(__nsh_hv38, component(Caption$1, { get children() {
				return [
					() => currentDocument()?.title,
					" · ",
					() => currentDocument()?.byline
				];
			} }), __nsh_hv39);
			insert(__nsh_hv38, component(Rule, {}), __nsh_hv40);
			append(__nsh_hv37, __nsh_hv38);
			append(__nsh_hv36, __nsh_hv37);
			return __nsh_hv36;
		})())]);
		return __nsh_hv35;
	})();
}
function ReadingMenu(props) {
	const item = {
		minHeight: 48,
		paddingHorizontal: 15,
		paddingVertical: 12,
		borderRadius: 5
	};
	return component(Menu, {
		offset: 7,
		margin: 8,
		get popupStyle() {
			return {
				padding: 5,
				minWidth: 220,
				borderRadius: 10,
				backgroundColor: ink("surface-floating"),
				boxShadow: "0 4px 20px rgba(0,0,0,0.18)"
			};
		},
		get trigger() {
			return component(Control, {
				get label() {
					return readerMode() === "shelf" ? "添加阅读内容" : "阅读工具";
				},
				get icon() {
					return readerMode() === "shelf" ? "plus" : "more";
				}
			});
		},
		get children() {
			return [cond(() => readerMode() === "shelf", () => [component(MenuItem, {
				style: item,
				onSelect: () => {
					chooseTextFile(props);
				},
				get children() {
					return component(Label, { get children() {
						return "打开文本文件…";
					} });
				}
			}), component(MenuItem, {
				style: item,
				onSelect: () => {
					readerMessage.set("");
					readerMode.set("compose");
				},
				get children() {
					return component(Label, { get children() {
						return "输入或粘贴文本";
					} });
				}
			})]), cond(() => readerMode() === "read", () => [
				component(MenuItem, {
					style: item,
					onSelect: () => tocOpen.set(!tocOpen()),
					get children() {
						return component(Label, { get children() {
							return tocOpen() ? "返回正文" : "打开目录";
						} });
					}
				}),
				component(MenuItem, {
					style: item,
					onSelect: () => {
						pickedPassage.set(null);
						textSize.set(Math.min(26, textSize() + 1));
					},
					get children() {
						return component(Label, { get children() {
							return "增大字号";
						} });
					}
				}),
				component(MenuItem, {
					style: item,
					onSelect: () => {
						pickedPassage.set(null);
						textSize.set(Math.max(16, textSize() - 1));
					},
					get children() {
						return component(Label, { get children() {
							return "减小字号";
						} });
					}
				}),
				component(MenuItem, {
					style: item,
					onSelect: () => shelf(props),
					get children() {
						return component(Label, { get children() {
							return "返回书架";
						} });
					}
				})
			])];
		}
	});
}
function ReadingWorkspace(props) {
	return (() => {
		const __nsh_hv43 = element("view");
		bindProp(__nsh_hv43, "style", () => ({
			flex: 1,
			minWidth: 0,
			backgroundColor: ink(readerMode() === "read" ? "surface-paper" : "surface-stage")
		}));
		const __nsh_hv44 = element("view");
		prop(__nsh_hv44, "desktop", { windowDrag: true });
		bindProp(__nsh_hv44, "style", () => ({
			height: 52,
			flexShrink: 0,
			flexDirection: "row",
			alignItems: "center",
			gap: 9,
			paddingLeft: props.sidebarVisible ? 18 : titleBar().left + (compactUI() ? 6 : 12),
			paddingRight: titleBar().right + 17,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle"),
			backgroundColor: ink("surface-toolbar")
		}));
		const __nsh_hv45 = element("view");
		prop(__nsh_hv45, "style", { flex: 1 });
		append(__nsh_hv44, __nsh_hv45);
		insert(__nsh_hv44, [
			cond(() => !props.sidebarVisible && (!compactUI() || readerMode() === "shelf"), () => component(Control, {
				label: "展开全局导航",
				icon: "panel",
				get onPress() {
					return props.toggleSidebar;
				}
			})),
			cond(() => readerMode() !== "shelf", () => component(Control, {
				label: "返回书架",
				icon: "back",
				get text() {
					return compactUI() ? void 0 : "书架";
				},
				onPress: () => shelf(props)
			})),
			component(Label, {
				weight: "600",
				get children() {
					return readerMode() === "read" ? compactUI() ? "阅读" : currentDocument()?.title : readerMode() === "compose" ? "输入文本" : compactUI() ? "书架" : "书架与文本";
				}
			})
		], __nsh_hv45);
		insert(__nsh_hv44, [
			cond(() => readerMode() === "read" && !compactUI(), () => [
				component(Control, {
					label: "打开或收起目录",
					icon: "book",
					text: "目录",
					get selected() {
						return tocOpen();
					},
					onPress: () => tocOpen.set(!tocOpen())
				}),
				component(Control, {
					label: "减小阅读字号",
					text: "A−",
					onPress: () => {
						pickedPassage.set(null);
						textSize.set(Math.max(16, textSize() - 1));
					}
				}),
				component(Control, {
					label: "增大阅读字号",
					text: "A＋",
					onPress: () => {
						pickedPassage.set(null);
						textSize.set(Math.min(26, textSize() + 1));
					}
				})
			]),
			cond(() => readerMode() === "shelf" && !compactUI(), () => [component(Control, {
				label: "输入或粘贴文本",
				icon: "pen",
				text: "输入文本",
				onPress: () => {
					readerMessage.set("");
					readerMode.set("compose");
				}
			}), component(Control, {
				label: "打开本机文本文件",
				icon: "plus",
				get text() {
					return picking() ? "正在选择…" : "打开文本文件…";
				},
				get disabled() {
					return picking();
				},
				raised: true,
				onPress: () => {
					chooseTextFile(props);
				}
			})]),
			cond(() => compactUI() && readerMode() !== "compose", () => component(ReadingMenu, props))
		]);
		append(__nsh_hv43, __nsh_hv44);
		insert(__nsh_hv43, [
			cond(() => readerMode() === "shelf", () => component(Shelf, props)),
			cond(() => readerMode() === "compose", () => component(Compose, props)),
			cond(() => readerMode() === "read", () => component(ReadingPage, {})),
			cond(() => readerMessage(), () => (() => {
				const __nsh_hv46 = element("view");
				prop(__nsh_hv46, "accessibilityLabel", "阅读状态提示");
				bindProp(__nsh_hv46, "style", () => ({
					paddingHorizontal: 24,
					paddingVertical: 8,
					backgroundColor: ink("surface-toolbar"),
					borderTopWidth: 1,
					borderColor: ink("border-subtle")
				}));
				insert(__nsh_hv46, component(Caption$1, { get children() {
					return readerMessage();
				} }));
				return __nsh_hv46;
			})()),
			cond(() => readerMode() === "read", () => (() => {
				const __nsh_hv47 = element("view");
				bindProp(__nsh_hv47, "style", () => ({
					minHeight: 49,
					paddingHorizontal: compactUI() ? 12 : 22,
					paddingVertical: 9,
					flexDirection: "row",
					alignItems: "center",
					gap: 10,
					backgroundColor: ink("surface-toolbar"),
					borderTopWidth: 1,
					borderColor: ink("border-subtle")
				}));
				insert(__nsh_hv47, [
					cond(() => pickedPassage(), () => [(() => {
						const __nsh_hv48 = element("text");
						bindProp(__nsh_hv48, "style", () => ({
							flex: 1,
							fontSize: 12,
							color: ink("text-secondary"),
							numberOfLines: 1
						}));
						const __nsh_hv49 = text(" ");
						const __nsh_hv50 = text(" 字 · ");
						append(__nsh_hv48, __nsh_hv49);
						append(__nsh_hv48, __nsh_hv50);
						insert(__nsh_hv48, () => pickedPassage()?.live ? "已选" : "上次选文", __nsh_hv49);
						insert(__nsh_hv48, () => Array.from(pickedPassage()?.text ?? "").length, __nsh_hv50);
						insert(__nsh_hv48, () => pickedPassage()?.text);
						return __nsh_hv48;
					})(), component(Control, {
						label: "复制选文",
						icon: "document",
						get text() {
							return compactUI() ? "复制" : "复制选文";
						},
						raised: true,
						onPress: () => {
							const p = pickedPassage();
							if (p) Clipboard.writeText(p.text).then(() => readerMessage.set("选文已复制。"), () => readerMessage.set("暂时无法复制，请重试。"));
						}
					})]),
					cond(() => !pickedPassage(), () => [component(Caption$1, { get children() {
						return "文字阅读 · 当前窗口";
					} }), (() => {
						const __nsh_hv51 = element("view");
						prop(__nsh_hv51, "style", { flex: 1 });
						return __nsh_hv51;
					})()]),
					component(Control, {
						label: "上一章或上一段",
						icon: "back",
						get disabled() {
							return chapterIndex() === 0;
						},
						onPress: () => goToChapter(chapterIndex() - 1)
					}),
					component(Caption$1, { get children() {
						return [
							() => chapterIndex() + 1,
							" / ",
							() => currentDocument()?.chapters.length
						];
					} }),
					component(Control, {
						label: "下一章或下一段",
						icon: "arrow",
						get disabled() {
							return chapterIndex() + 1 >= (currentDocument()?.chapters.length ?? 0);
						},
						onPress: () => goToChapter(chapterIndex() + 1)
					})
				]);
				return __nsh_hv47;
			})())
		]);
		return __nsh_hv43;
	})();
}

//#endregion
//#region App.tsx
setup(null, "{\n  \"name\": \"QUSOR Graphite & Paper — native study\",\n  \"cssVars\": {\n    \"theme\": {\n      \"radius\": \"7px\"\n    },\n    \"light\": {\n      \"surface-navigation\": \"rgba(231,231,226,0.82)\",\n      \"surface-stage\": \"#EFEEEA\",\n      \"surface-toolbar\": \"#F5F5F1\",\n      \"surface-paper\": \"#FFFEFC\",\n      \"surface-inspector\": \"#F6F5F1\",\n      \"surface-search\": \"rgba(255,255,255,0.46)\",\n      \"surface-control\": \"#FAFAF7\",\n      \"surface-floating\": \"rgba(250,250,247,0.96)\",\n      \"surface-track\": \"#E8E8E3\",\n      \"surface-inset\": \"#F3F3EE\",\n      \"surface-selected\": \"#D8E1E7\",\n      \"surface-hover\": \"#DFE2DF\",\n      \"surface-pressed\": \"#D2D7D5\",\n      \"border-subtle\": \"#D8D8D0\",\n      \"border-control\": \"#D2D3CC\",\n      \"border-paper\": \"#E3E2DB\",\n      \"text-primary\": \"#2D302D\",\n      \"text-secondary\": \"#6B7069\",\n      \"text-accent\": \"#386080\",\n      \"control-primary\": \"#3C4542\",\n      \"control-primary-hover\": \"#303B37\",\n      \"control-primary-pressed\": \"#25302C\",\n      \"control-primary-text\": \"#FFFFFF\",\n      \"focus-ring\": \"#467BBD\",\n      \"transparent\": \"#00000000\",\n      \"background\": \"#FFFEFC\",\n      \"foreground\": \"#2D302D\",\n      \"primary\": \"#3C4542\",\n      \"primary-foreground\": \"#FFFFFF\",\n      \"muted\": \"#E8E8E3\",\n      \"muted-foreground\": \"#6B7069\",\n      \"accent\": \"#D8E1E7\",\n      \"accent-foreground\": \"#2D302D\",\n      \"border\": \"#D8D8D0\",\n      \"input\": \"#D2D3CC\",\n      \"ring\": \"#467BBD\",\n      \"popover\": \"rgba(250,250,247,0.96)\",\n      \"popover-foreground\": \"#2D302D\"\n    },\n    \"dark\": {\n      \"surface-navigation\": \"rgba(35,38,36,0.88)\",\n      \"surface-stage\": \"#1C1F1D\",\n      \"surface-toolbar\": \"#272B28\",\n      \"surface-paper\": \"#2D312D\",\n      \"surface-inspector\": \"#272C28\",\n      \"surface-search\": \"rgba(0,0,0,0.16)\",\n      \"surface-control\": \"#3D433D\",\n      \"surface-floating\": \"rgba(47,53,48,0.97)\",\n      \"surface-track\": \"#202722\",\n      \"surface-inset\": \"#242A25\",\n      \"surface-selected\": \"#384B51\",\n      \"surface-hover\": \"#39433D\",\n      \"surface-pressed\": \"#42534A\",\n      \"border-subtle\": \"#3C443D\",\n      \"border-control\": \"#4B554B\",\n      \"border-paper\": \"#3A443A\",\n      \"text-primary\": \"#ECEEE6\",\n      \"text-secondary\": \"#B1B9AC\",\n      \"text-accent\": \"#ADCFEA\",\n      \"control-primary\": \"#B4C7B7\",\n      \"control-primary-hover\": \"#C6D6C7\",\n      \"control-primary-pressed\": \"#9AB39E\",\n      \"control-primary-text\": \"#15221B\",\n      \"focus-ring\": \"#93B7ED\",\n      \"transparent\": \"#00000000\",\n      \"background\": \"#2D312D\",\n      \"foreground\": \"#ECEEE6\",\n      \"primary\": \"#B4C7B7\",\n      \"primary-foreground\": \"#15221B\",\n      \"muted\": \"#202722\",\n      \"muted-foreground\": \"#B1B9AC\",\n      \"accent\": \"#384B51\",\n      \"accent-foreground\": \"#ECEEE6\",\n      \"border\": \"#3C443D\",\n      \"input\": \"#4B554B\",\n      \"ring\": \"#93B7ED\",\n      \"popover\": \"rgba(47,53,48,0.97)\",\n      \"popover-foreground\": \"#ECEEE6\"\n    }\n  }\n}\n", null, null);
const mobileNavigation = signal(false);
const workspace = signal(readingEntry.id);
const scene = signal("research");
const tab = signal("document");
const inspector = signal(false);
const library = signal(false);
const dark = signal(false);
const sidebar = signal(true);
const researchDraft = signal("工作台需要保持安静，扩展能力在需要时出现。");
const writingDraft = signal("一款好用的软件，需要给复杂的工作留出清晰的秩序。");
const note = signal("");
const search = signal("");
const scenes = [
	{
		id: "research",
		title: "原生软件体验研究",
		kind: "研究",
		icon: "book",
		tint: "#6584A2"
	},
	{
		id: "writing",
		title: "把想法整理成文章",
		kind: "写作",
		icon: "pen",
		tint: "#A18053"
	},
	{
		id: "process",
		title: "安排一次工作流程",
		kind: "流程",
		icon: "flow",
		tint: "#768E77"
	},
	{
		id: "development",
		title: "制作一个小工具",
		kind: "开发",
		icon: "code",
		tint: "#8B7FA3"
	}
];
const current = () => scenes.find((s) => s.id === scene());
function choose(id) {
	mobileNavigation.set(false);
	workspace.set("tasks");
	library.set(false);
	scene.set(id);
	tab.set("document");
	note.set("");
}
function MenuRow(props) {
	return component(MenuItem, {
		get onSelect() {
			return props.onSelect;
		},
		style: {
			flexDirection: "row",
			gap: 10,
			alignItems: "center",
			paddingHorizontal: 10,
			paddingVertical: 8,
			borderRadius: 6
		},
		get hoverStyle() {
			return { backgroundColor: ink("surface-selected") };
		},
		get focusStyle() {
			return { backgroundColor: ink("surface-selected") };
		},
		get children() {
			return [component(Glyph, {
				get name() {
					return props.icon;
				},
				size: 16
			}), component(Label, { get children() {
				return props.text;
			} })];
		}
	});
}
function readingHome() {
	mobileNavigation.set(false);
	workspace.set(readingEntry.id);
	readerMode.set("shelf");
	sidebar.set(true);
}
function showLibrary() {
	mobileNavigation.set(false);
	workspace.set("tasks");
	inspector.set(false);
	library.set(true);
}
function AgentMenu() {
	return component(Menu, {
		offset: 8,
		margin: 10,
		get popupStyle() {
			return {
				padding: 5,
				minWidth: 242,
				borderRadius: 11,
				borderCurve: "continuous",
				backgroundColor: ink("surface-floating"),
				backdropFilter: "blur(18px)",
				borderWidth: 1,
				borderColor: ink("border-control"),
				boxShadow: "0 8px 30px rgba(0,0,0,0.17)"
			};
		},
		get trigger() {
			return component(Control, {
				label: "选择 Agent 接入方式",
				text: "选择 Agent",
				icon: "plus",
				raised: true
			});
		},
		get children() {
			return [
				(() => {
					const __nsh_hv0 = element("view");
					prop(__nsh_hv0, "style", { padding: 10 });
					const __nsh_hv1 = element("text");
					bindProp(__nsh_hv1, "style", () => ({
						fontSize: 12,
						color: ink("text-secondary")
					}));
					const __nsh_hv2 = text("接入方式 · 设计样例");
					append(__nsh_hv1, __nsh_hv2);
					append(__nsh_hv0, __nsh_hv1);
					return __nsh_hv0;
				})(),
				component(MenuRow, {
					text: "结构化 Agent 接入",
					icon: "stack",
					onSelect: () => note.set("结构化接入：待配置提供者，当前未连接。")
				}),
				component(MenuRow, {
					text: "本机命令行接入",
					icon: "code",
					onSelect: () => note.set("命令行接入：待配置本机程序，当前未运行命令。")
				}),
				component(MenuRow, {
					text: "远程 Agent 接入",
					icon: "plug",
					onSelect: () => note.set("远程接入：待配置服务，当前没有发送数据。")
				}),
				component(MenuSeparator, { get style() {
					return {
						height: 1,
						marginVertical: 5,
						backgroundColor: ink("border-subtle")
					};
				} }),
				(() => {
					const __nsh_hv3 = element("view");
					prop(__nsh_hv3, "style", { padding: 10 });
					const __nsh_hv4 = element("text");
					bindProp(__nsh_hv4, "style", () => ({
						fontSize: 12,
						lineHeight: 18,
						color: ink("text-secondary"),
						maxWidth: 230
					}));
					const __nsh_hv5 = text("当前窗口只演示界面，不连接服务。");
					append(__nsh_hv4, __nsh_hv5);
					append(__nsh_hv3, __nsh_hv4);
					return __nsh_hv3;
				})()
			];
		}
	});
}
function Sidebar() {
	const win = currentWindow();
	return (() => {
		const __nsh_hv6 = element("view");
		bindProp(__nsh_hv6, "style", () => ({
			width: 230,
			height: "100%",
			flexShrink: 0,
			borderRightWidth: 1,
			borderColor: ink("border-subtle"),
			backgroundColor: ink("surface-navigation")
		}));
		const __nsh_hv7 = element("view");
		prop(__nsh_hv7, "desktop", { windowDrag: true });
		bindProp(__nsh_hv7, "style", () => ({
			height: 52,
			paddingLeft: titleBar().left + 8,
			paddingRight: 10,
			flexDirection: "row",
			alignItems: "center",
			justifyContent: "flex-end"
		}));
		insert(__nsh_hv7, component(Control, {
			icon: "panel",
			label: "收起侧栏",
			onPress: () => {
				sidebar.set(false);
				mobileNavigation.set(false);
			}
		}));
		const __nsh_hv8 = element("view");
		prop(__nsh_hv8, "style", {
			flexDirection: "row",
			alignItems: "center",
			gap: 10,
			paddingHorizontal: 20,
			paddingTop: 12,
			paddingBottom: 23
		});
		const __nsh_hv9 = element("view");
		prop(__nsh_hv9, "style", { gap: 1 });
		const __nsh_hv10 = element("text");
		bindProp(__nsh_hv10, "style", () => ({
			fontSize: 14,
			letterSpacing: 1.1,
			fontWeight: "650",
			color: ink("text-primary")
		}));
		const __nsh_hv11 = text("QUSOR");
		append(__nsh_hv10, __nsh_hv11);
		const __nsh_hv12 = element("text");
		bindProp(__nsh_hv12, "style", () => ({
			fontSize: 12,
			color: ink("text-secondary")
		}));
		const __nsh_hv13 = text("个人空间");
		append(__nsh_hv12, __nsh_hv13);
		append(__nsh_hv9, __nsh_hv10);
		append(__nsh_hv9, __nsh_hv12);
		append(__nsh_hv8, __nsh_hv9);
		insert(__nsh_hv8, component(Mark, {}), __nsh_hv9);
		const __nsh_hv14 = element("view");
		bindProp(__nsh_hv14, "style", () => ({
			marginHorizontal: 13,
			marginBottom: 23,
			height: 31,
			paddingHorizontal: 9,
			borderRadius: 7,
			borderWidth: 1,
			borderColor: ink("border-control"),
			backgroundColor: ink("surface-search"),
			flexDirection: "row",
			alignItems: "center",
			gap: 7,
			boxShadow: "inset 0 1px 1px rgba(0,0,0,0.03)"
		}));
		const __nsh_hv15 = element("input");
		prop(__nsh_hv15, "accessibilityLabel", "查找设计样例");
		bindProp(__nsh_hv15, "value", () => search());
		prop(__nsh_hv15, "onChange", ({ value }) => search.set(value));
		prop(__nsh_hv15, "placeholder", "查找任务");
		bindProp(__nsh_hv15, "style", () => ({
			flex: 1,
			fontSize: 13,
			color: ink("text-primary"),
			backgroundColor: "transparent",
			borderWidth: 0,
			padding: 0,
			height: 25
		}));
		append(__nsh_hv14, __nsh_hv15);
		insert(__nsh_hv14, component(Glyph, {
			name: "search",
			size: 15
		}), __nsh_hv15);
		const __nsh_hv16 = element("view");
		prop(__nsh_hv16, "style", {
			paddingHorizontal: 21,
			marginBottom: 8,
			flexDirection: "row",
			alignItems: "center"
		});
		const __nsh_hv17 = element("text");
		bindProp(__nsh_hv17, "style", () => ({
			fontSize: 12,
			fontWeight: "600",
			color: ink("text-secondary")
		}));
		const __nsh_hv18 = text("工作中的任务");
		append(__nsh_hv17, __nsh_hv18);
		const __nsh_hv19 = element("view");
		prop(__nsh_hv19, "style", { flex: 1 });
		const __nsh_hv20 = element("text");
		bindProp(__nsh_hv20, "style", () => ({
			fontSize: 12,
			color: ink("text-secondary")
		}));
		const __nsh_hv21 = text("样例");
		append(__nsh_hv20, __nsh_hv21);
		append(__nsh_hv16, __nsh_hv17);
		append(__nsh_hv16, __nsh_hv19);
		append(__nsh_hv16, __nsh_hv20);
		const __nsh_hv22 = element("view");
		prop(__nsh_hv22, "style", {
			paddingHorizontal: 9,
			gap: 3
		});
		insert(__nsh_hv22, each(() => scenes.filter((s) => !search() || s.title.includes(search()) || s.kind.includes(search())), (s) => component(Button, {
			get accessibilityLabel() {
				return s().title;
			},
			onPress: () => choose(s().id),
			get style() {
				return {
					paddingHorizontal: 11,
					paddingVertical: 11,
					gap: 11,
					flexDirection: "row",
					alignItems: "center",
					borderRadius: 7,
					backgroundColor: ink(workspace() === "tasks" && scene() === s().id ? "surface-selected" : "transparent")
				};
			},
			get hoverStyle() {
				return { backgroundColor: ink("surface-hover") };
			},
			get focusStyle() {
				return {
					outlineWidth: 2,
					outlineColor: ink("focus-ring")
				};
			},
			get children() {
				return [component(Glyph, {
					get name() {
						return s().icon;
					},
					size: 18,
					get color() {
						return s().tint;
					}
				}), (() => {
					const __nsh_hv23 = element("view");
					prop(__nsh_hv23, "style", {
						flex: 1,
						gap: 3
					});
					const __nsh_hv24 = element("text");
					bindProp(__nsh_hv24, "style", () => ({
						fontSize: 13,
						fontWeight: workspace() === "tasks" && scene() === s().id ? "600" : "400",
						color: ink("text-primary"),
						numberOfLines: 1
					}));
					insert(__nsh_hv24, () => s().title);
					const __nsh_hv25 = element("text");
					bindProp(__nsh_hv25, "style", () => ({
						fontSize: 12,
						color: ink("text-secondary")
					}));
					insert(__nsh_hv25, () => s().kind);
					append(__nsh_hv23, __nsh_hv24);
					append(__nsh_hv23, __nsh_hv25);
					return __nsh_hv23;
				})()];
			}
		})));
		const __nsh_hv26 = element("view");
		prop(__nsh_hv26, "style", {
			paddingHorizontal: 20,
			paddingTop: 25,
			gap: 10
		});
		const __nsh_hv27 = element("text");
		bindProp(__nsh_hv27, "style", () => ({
			fontSize: 12,
			fontWeight: "600",
			color: ink("text-secondary")
		}));
		const __nsh_hv28 = text("固定空间");
		append(__nsh_hv27, __nsh_hv28);
		const __nsh_hv29 = element("view");
		prop(__nsh_hv29, "style", {
			flexDirection: "row",
			alignItems: "center",
			gap: 10
		});
		insert(__nsh_hv29, [component(Glyph, {
			name: "stack",
			size: 17,
			color: "#A88B58"
		}), component(Label, {
			small: true,
			get children() {
				return "个人工作室";
			}
		})]);
		append(__nsh_hv26, __nsh_hv27);
		append(__nsh_hv26, __nsh_hv29);
		const __nsh_hv30 = element("view");
		prop(__nsh_hv30, "style", { flex: 1 });
		const __nsh_hv31 = element("view");
		prop(__nsh_hv31, "style", { marginHorizontal: 13 });
		insert(__nsh_hv31, component(Rule, {}));
		const __nsh_hv32 = element("view");
		prop(__nsh_hv32, "style", {
			padding: 12,
			gap: 6
		});
		const __nsh_hv35 = element("view");
		prop(__nsh_hv35, "style", {
			flexDirection: "row",
			alignItems: "center",
			paddingLeft: 8,
			paddingTop: 6
		});
		const __nsh_hv36 = element("view");
		bindProp(__nsh_hv36, "style", () => ({
			width: 6,
			height: 6,
			borderRadius: 3,
			backgroundColor: ink("text-secondary"),
			marginRight: 7
		}));
		const __nsh_hv37 = element("text");
		bindProp(__nsh_hv37, "style", () => ({
			flex: 1,
			fontSize: 12,
			color: ink("text-secondary")
		}));
		const __nsh_hv38 = text("原生交互预览");
		append(__nsh_hv37, __nsh_hv38);
		append(__nsh_hv35, __nsh_hv36);
		append(__nsh_hv35, __nsh_hv37);
		insert(__nsh_hv35, component(Control, {
			get icon() {
				return dark() ? "sun" : "moon";
			},
			label: "切换明暗外观",
			onPress: () => {
				dark.set(!dark());
				setColorScheme(dark() ? "dark" : "light");
				win.set({ colorScheme: dark() ? "dark" : "light" });
			}
		}));
		append(__nsh_hv32, __nsh_hv35);
		insert(__nsh_hv32, component(Button, {
			accessibilityLabel: "打开扩展样例",
			onPress: showLibrary,
			style: {
				height: 34,
				paddingHorizontal: 8,
				flexDirection: "row",
				alignItems: "center",
				gap: 10,
				borderRadius: 6
			},
			get hoverStyle() {
				return { backgroundColor: ink("surface-hover") };
			},
			get children() {
				return [
					component(Glyph, {
						name: "plug",
						size: 17
					}),
					component(Label, {
						small: true,
						get children() {
							return "扩展与能力";
						}
					}),
					(() => {
						const __nsh_hv33 = element("view");
						prop(__nsh_hv33, "style", { flex: 1 });
						return __nsh_hv33;
					})(),
					(() => {
						const __nsh_hv34 = element("text");
						bindProp(__nsh_hv34, "style", () => ({
							fontSize: 12,
							color: ink("text-secondary")
						}));
						insert(__nsh_hv34, () => scenes.length + 1);
						return __nsh_hv34;
					})()
				];
			}
		}), __nsh_hv35);
		append(__nsh_hv6, __nsh_hv7);
		append(__nsh_hv6, __nsh_hv8);
		append(__nsh_hv6, __nsh_hv14);
		append(__nsh_hv6, __nsh_hv16);
		append(__nsh_hv6, __nsh_hv22);
		append(__nsh_hv6, __nsh_hv26);
		append(__nsh_hv6, __nsh_hv30);
		append(__nsh_hv6, __nsh_hv31);
		append(__nsh_hv6, __nsh_hv32);
		insert(__nsh_hv6, component(Button, {
			accessibilityLabel: "打开书架与文本",
			onPress: readingHome,
			get style() {
				return {
					marginHorizontal: 9,
					marginBottom: 17,
					height: touchUI() ? 48 : 37,
					paddingHorizontal: 12,
					flexDirection: "row",
					alignItems: "center",
					gap: 10,
					borderRadius: 6,
					backgroundColor: ink(workspace() === readingEntry.id ? "surface-selected" : "transparent")
				};
			},
			get hoverStyle() {
				return { backgroundColor: ink("surface-hover") };
			},
			get children() {
				return [component(Glyph, {
					get name() {
						return readingEntry.icon;
					},
					size: 18,
					color: "#6584A2"
				}), component(Label, {
					weight: "500",
					get children() {
						return readingEntry.title;
					}
				})];
			}
		}), __nsh_hv14);
		return __nsh_hv6;
	})();
}
function Caption(props) {
	return (() => {
		const __nsh_hv39 = element("text");
		bindProp(__nsh_hv39, "style", () => ({
			fontSize: 12,
			lineHeight: 18,
			color: ink("text-secondary")
		}));
		insert(__nsh_hv39, () => props.children);
		return __nsh_hv39;
	})();
}
function Paragraph(props) {
	return (() => {
		const __nsh_hv40 = element("text");
		bindProp(__nsh_hv40, "style", () => ({
			fontSize: fonts.body,
			lineHeight: 28,
			color: ink("text-primary")
		}));
		insert(__nsh_hv40, () => props.children);
		return __nsh_hv40;
	})();
}
function Heading(props) {
	return (() => {
		const __nsh_hv41 = element("text");
		bindProp(__nsh_hv41, "style", () => ({
			fontSize: 17,
			lineHeight: 26,
			fontWeight: "600",
			color: ink("text-primary"),
			marginTop: 12
		}));
		insert(__nsh_hv41, () => props.children);
		return __nsh_hv41;
	})();
}
function Research() {
	return (() => {
		const __nsh_hv42 = element("view");
		prop(__nsh_hv42, "style", { gap: 18 });
		const __nsh_hv43 = element("view");
		prop(__nsh_hv43, "style", {
			flexDirection: "row",
			alignItems: "center",
			gap: 7
		});
		const __nsh_hv44 = element("view");
		prop(__nsh_hv44, "style", {
			width: 5,
			height: 5,
			borderRadius: 3,
			backgroundColor: "#AD8A51"
		});
		const __nsh_hv45 = element("view");
		prop(__nsh_hv45, "style", { flex: 1 });
		append(__nsh_hv43, __nsh_hv44);
		append(__nsh_hv43, __nsh_hv45);
		insert(__nsh_hv43, component(Caption, { get children() {
			return "研究札记";
		} }), __nsh_hv45);
		insert(__nsh_hv43, component(Caption, { get children() {
			return "2026.11.09";
		} }));
		const __nsh_hv46 = element("view");
		prop(__nsh_hv46, "style", { gap: 8 });
		const __nsh_hv47 = element("text");
		bindProp(__nsh_hv47, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			letterSpacing: -.5,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv48 = text("原生软件的秩序与手感");
		append(__nsh_hv47, __nsh_hv48);
		append(__nsh_hv46, __nsh_hv47);
		insert(__nsh_hv46, component(Caption, { get children() {
			return "QUSOR 设计研究 · 自拟材料";
		} }));
		const __nsh_hv49 = element("view");
		bindProp(__nsh_hv49, "style", () => ({
			flexDirection: "row",
			alignItems: "center",
			gap: 14,
			paddingVertical: 12,
			borderTopWidth: 1,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv50 = element("text");
		bindProp(__nsh_hv50, "style", () => ({
			fontSize: 13,
			color: ink("text-primary")
		}));
		const __nsh_hv51 = text("Things  ·  Reeder  ·  Kino");
		append(__nsh_hv50, __nsh_hv51);
		append(__nsh_hv49, __nsh_hv50);
		insert(__nsh_hv49, [component(Glyph, {
			name: "book",
			size: 16
		}), component(Caption, { get children() {
			return "参考作品";
		} })], __nsh_hv50);
		const __nsh_hv52 = element("view");
		prop(__nsh_hv52, "style", {
			marginTop: 4,
			paddingLeft: 15,
			borderLeftWidth: 2,
			borderColor: "#B59A6C",
			gap: 8
		});
		const __nsh_hv53 = element("text");
		bindProp(__nsh_hv53, "style", () => ({
			fontSize: 14,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv54 = text("随手记");
		append(__nsh_hv53, __nsh_hv54);
		const __nsh_hv55 = element("input");
		prop(__nsh_hv55, "accessibilityLabel", "研究札记草稿");
		bindProp(__nsh_hv55, "value", () => researchDraft());
		prop(__nsh_hv55, "onChange", ({ value }) => researchDraft.set(value));
		prop(__nsh_hv55, "multiline", true);
		bindProp(__nsh_hv55, "style", () => ({
			minHeight: 72,
			padding: 0,
			borderWidth: 0,
			backgroundColor: ink("surface-paper"),
			color: ink("text-primary"),
			fontSize: 16,
			lineHeight: 27
		}));
		bindProp(__nsh_hv55, "focusStyle", () => ({
			outlineWidth: 1,
			outlineOffset: 5,
			outlineColor: ink("focus-ring")
		}));
		append(__nsh_hv52, __nsh_hv53);
		append(__nsh_hv52, __nsh_hv55);
		insert(__nsh_hv52, component(Caption, { get children() {
			return "草稿只保留在当前窗口";
		} }));
		append(__nsh_hv42, __nsh_hv43);
		append(__nsh_hv42, __nsh_hv46);
		append(__nsh_hv42, __nsh_hv49);
		append(__nsh_hv42, __nsh_hv52);
		insert(__nsh_hv42, component(Paragraph, { get children() {
			return "好的工具让人专注于眼前的内容。窗口、文字与操作保持稳定，复杂能力在需要的时候出现。";
		} }), __nsh_hv49);
		insert(__nsh_hv42, [
			component(Heading, { get children() {
				return "01　让内容建立层级";
			} }),
			component(Paragraph, { get children() {
				return "用字阶、行距和对齐组织一张清晰的工作台。文稿像文稿，画布像画布；工具不需要用一排卡片争夺注意力。";
			} }),
			component(Heading, { get children() {
				return "02　让扩展自然进入工作";
			} }),
			component(Paragraph, { get children() {
				return "进入研究、写作或开发任务时，更换的是主内容与适用工具。侧栏保留任务的位置，检查器按需展开，Agent 从当前任务接手。";
			} })
		], __nsh_hv52);
		return __nsh_hv42;
	})();
}
function Writing() {
	return (() => {
		const __nsh_hv56 = element("view");
		prop(__nsh_hv56, "style", { gap: 20 });
		const __nsh_hv57 = element("text");
		bindProp(__nsh_hv57, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv58 = text("把想法整理成文章");
		append(__nsh_hv57, __nsh_hv58);
		const __nsh_hv59 = element("input");
		prop(__nsh_hv59, "accessibilityLabel", "写作草稿");
		bindProp(__nsh_hv59, "value", () => writingDraft());
		prop(__nsh_hv59, "onChange", ({ value }) => writingDraft.set(value));
		prop(__nsh_hv59, "multiline", true);
		bindProp(__nsh_hv59, "style", () => ({
			minHeight: 320,
			padding: 0,
			borderWidth: 0,
			backgroundColor: ink("surface-paper"),
			color: ink("text-primary"),
			fontSize: 17,
			lineHeight: 30
		}));
		bindProp(__nsh_hv59, "focusStyle", () => ({
			outlineWidth: 1,
			outlineOffset: 8,
			outlineColor: ink("focus-ring")
		}));
		append(__nsh_hv56, __nsh_hv57);
		append(__nsh_hv56, __nsh_hv59);
		insert(__nsh_hv56, component(Caption, { get children() {
			return "文稿 · 设计样例";
		} }), __nsh_hv57);
		insert(__nsh_hv56, [component(Paragraph, { get children() {
			return "主内容可以直接成为写作空间。资料、想法与 Agent 建议都围绕这篇文稿出现。";
		} }), component(Heading, { get children() {
			return "我的草稿";
		} })], __nsh_hv59);
		insert(__nsh_hv56, component(Caption, { get children() {
			return "本任务的草稿只保留在当前窗口，切换标签不会清空输入。";
		} }));
		return __nsh_hv56;
	})();
}
function Process() {
	return (() => {
		const __nsh_hv60 = element("view");
		prop(__nsh_hv60, "style", { gap: 22 });
		const __nsh_hv61 = element("text");
		bindProp(__nsh_hv61, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv62 = text("安排一次工作流程");
		append(__nsh_hv61, __nsh_hv62);
		append(__nsh_hv60, __nsh_hv61);
		insert(__nsh_hv60, component(Caption, { get children() {
			return "工作流程 · 设计样例";
		} }), __nsh_hv61);
		insert(__nsh_hv60, [component(Paragraph, { get children() {
			return "把过程铺开，逐步检查输入与结果。Agent 连接后才可以执行；当前步骤均未开始。";
		} }), each(() => [
			"准备资料与目标",
			"选择适用的 Agent 与工具",
			"检查计划并允许执行",
			"审阅结果，保留需要的内容"
		], (step, index) => (() => {
			const __nsh_hv63 = element("view");
			bindProp(__nsh_hv63, "style", () => ({
				flexDirection: "row",
				gap: 15,
				paddingVertical: 15,
				borderBottomWidth: 1,
				borderColor: ink("border-subtle"),
				alignItems: "center"
			}));
			const __nsh_hv64 = element("view");
			bindProp(__nsh_hv64, "style", () => ({
				width: 26,
				height: 26,
				borderRadius: 13,
				borderWidth: 1,
				borderColor: ink("border-control"),
				alignItems: "center",
				justifyContent: "center"
			}));
			insert(__nsh_hv64, component(Caption, { get children() {
				return index() + 1;
			} }));
			const __nsh_hv65 = element("view");
			prop(__nsh_hv65, "style", { flex: 1 });
			insert(__nsh_hv65, component(Label, { get children() {
				return step();
			} }));
			append(__nsh_hv63, __nsh_hv64);
			append(__nsh_hv63, __nsh_hv65);
			insert(__nsh_hv63, component(Caption, { get children() {
				return "未开始";
			} }));
			return __nsh_hv63;
		})())]);
		return __nsh_hv60;
	})();
}
function Development() {
	return (() => {
		const __nsh_hv66 = element("view");
		prop(__nsh_hv66, "style", { gap: 22 });
		const __nsh_hv67 = element("text");
		bindProp(__nsh_hv67, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv68 = text("制作一个小工具");
		append(__nsh_hv67, __nsh_hv68);
		const __nsh_hv69 = element("view");
		bindProp(__nsh_hv69, "style", () => ({
			flexDirection: "row",
			paddingVertical: 12,
			borderTopWidth: 1,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle"),
			alignItems: "center",
			gap: 8
		}));
		const __nsh_hv70 = element("view");
		prop(__nsh_hv70, "style", { flex: 1 });
		append(__nsh_hv69, __nsh_hv70);
		insert(__nsh_hv69, [component(Glyph, {
			name: "code",
			size: 16
		}), component(Label, {
			small: true,
			get children() {
				return "示例代码";
			}
		})], __nsh_hv70);
		insert(__nsh_hv69, component(Caption, { get children() {
			return "只读 · 未执行";
		} }));
		const __nsh_hv71 = element("text");
		bindProp(__nsh_hv71, "style", () => ({
			userSelect: "text",
			fontFamily: "monospace",
			fontSize: 14,
			lineHeight: 27,
			color: ink("text-primary"),
			padding: 15,
			backgroundColor: ink("surface-inset"),
			borderRadius: 5
		}));
		insert(__nsh_hv71, "// A task can choose its own tools.\nconst task = {\n  title: \"A small useful tool\",\n  agent: null,\n  views: [\"code\", \"preview\"]\n};");
		append(__nsh_hv66, __nsh_hv67);
		append(__nsh_hv66, __nsh_hv69);
		append(__nsh_hv66, __nsh_hv71);
		insert(__nsh_hv66, component(Caption, { get children() {
			return "开发 · 可选的任务界面";
		} }), __nsh_hv67);
		insert(__nsh_hv66, component(Paragraph, { get children() {
			return "代码是这个任务的内容。需要时再打开文件、预览与执行输出，其他任务继续使用各自适合的界面。";
		} }), __nsh_hv69);
		insert(__nsh_hv66, component(Caption, { get children() {
			return "上面的代码仅说明设计思路，并非已发布的 SDK。";
		} }));
		return __nsh_hv66;
	})();
}
function Steps() {
	return (() => {
		const __nsh_hv72 = element("view");
		prop(__nsh_hv72, "style", { gap: 22 });
		const __nsh_hv73 = element("text");
		bindProp(__nsh_hv73, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv74 = text("从目标到结果");
		append(__nsh_hv73, __nsh_hv74);
		append(__nsh_hv72, __nsh_hv73);
		insert(__nsh_hv72, component(Caption, { get children() {
			return "任务过程 · 设计样例";
		} }), __nsh_hv73);
		insert(__nsh_hv72, [component(Paragraph, { get children() {
			return "过程与成果可以分别查看，切换标签保留正在编辑的内容。";
		} }), each(() => [
			{
				title: "明确当前目标",
				text: "制作原生软件的视觉方向与交互样例"
			},
			{
				title: "准备上下文",
				text: "参考作品、当前草稿与用户的设计要求"
			},
			{
				title: "选择 Agent",
				text: "尚未连接，没有执行记录"
			}
		], (s) => (() => {
			const __nsh_hv75 = element("view");
			bindProp(__nsh_hv75, "style", () => ({
				gap: 8,
				paddingVertical: 15,
				borderBottomWidth: 1,
				borderColor: ink("border-subtle")
			}));
			insert(__nsh_hv75, [component(Label, {
				weight: "600",
				get children() {
					return s().title;
				}
			}), component(Caption, { get children() {
				return s().text;
			} })]);
			return __nsh_hv75;
		})())]);
		return __nsh_hv72;
	})();
}
function Inspector() {
	return (() => {
		const __nsh_hv76 = element("view");
		bindProp(__nsh_hv76, "style", () => ({
			width: compactUI() ? "100%" : 260,
			flexShrink: 0,
			backgroundColor: ink("surface-inspector"),
			borderLeftWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv77 = element("view");
		prop(__nsh_hv77, "style", {
			height: 52,
			paddingHorizontal: 16,
			alignItems: "center",
			flexDirection: "row",
			gap: 7
		});
		const __nsh_hv78 = element("view");
		prop(__nsh_hv78, "style", { flex: 1 });
		append(__nsh_hv77, __nsh_hv78);
		insert(__nsh_hv77, component(Label, {
			weight: "600",
			get children() {
				return "任务信息";
			}
		}), __nsh_hv78);
		insert(__nsh_hv77, component(Control, {
			label: "关闭任务信息",
			icon: "close",
			onPress: () => inspector.set(false)
		}));
		const __nsh_hv79 = element("view");
		prop(__nsh_hv79, "style", {
			padding: 20,
			gap: 16
		});
		const __nsh_hv80 = element("view");
		prop(__nsh_hv80, "style", {
			flexDirection: "row",
			gap: 10,
			alignItems: "center"
		});
		insert(__nsh_hv80, [component(Glyph, { get name() {
			return current().icon;
		} }), component(Label, { get children() {
			return [() => current().kind, "工作台"];
		} })]);
		append(__nsh_hv79, __nsh_hv80);
		insert(__nsh_hv79, [
			component(Caption, { get children() {
				return "当前任务";
			} }),
			component(Label, {
				weight: "600",
				get children() {
					return current().title;
				}
			}),
			component(Caption, { get children() {
				return "设计样例 · 当前窗口";
			} }),
			component(Rule, {}),
			component(Caption, { get children() {
				return "使用的界面";
			} })
		], __nsh_hv80);
		insert(__nsh_hv79, [
			component(Caption, { get children() {
				return "切换任务时，主内容随场景变化。";
			} }),
			component(Rule, {}),
			component(Caption, { get children() {
				return "Agent";
			} }),
			component(Label, { get children() {
				return "尚未连接";
			} }),
			component(Caption, { get children() {
				return "没有运行中的任务或外部操作。";
			} }),
			component(Rule, {}),
			component(Caption, { get children() {
				return "内容状态";
			} }),
			component(Label, { get children() {
				return "未保存的会话草稿";
			} }),
			component(Caption, { get children() {
				return "正式保存与恢复尚未接入本样例。";
			} })
		]);
		append(__nsh_hv76, __nsh_hv77);
		append(__nsh_hv76, __nsh_hv79);
		insert(__nsh_hv76, component(Rule, {}), __nsh_hv79);
		return __nsh_hv76;
	})();
}
function CapabilityLibrary() {
	return (() => {
		const __nsh_hv81 = element("view");
		bindProp(__nsh_hv81, "style", () => ({
			flex: 1,
			backgroundColor: ink("surface-paper"),
			padding: 0,
			gap: 18
		}));
		const __nsh_hv82 = element("text");
		bindProp(__nsh_hv82, "style", () => ({
			fontSize: 21,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv83 = text("选择适合的工作界面");
		append(__nsh_hv82, __nsh_hv83);
		const __nsh_hv84 = element("view");
		bindProp(__nsh_hv84, "style", () => ({
			flexDirection: "row",
			alignItems: "center",
			gap: 14,
			paddingVertical: 13,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv85 = element("view");
		prop(__nsh_hv85, "style", {
			flex: 1,
			gap: 5
		});
		insert(__nsh_hv85, [component(Label, {
			weight: "500",
			get children() {
				return "书架与阅读";
			}
		}), component(Caption, { get children() {
			return "选择书籍、打开文本、阅读与选文";
		} })]);
		append(__nsh_hv84, __nsh_hv85);
		insert(__nsh_hv84, component(Glyph, {
			name: "book",
			size: 22,
			color: "#6584A2"
		}), __nsh_hv85);
		insert(__nsh_hv84, component(Control, {
			label: "预览阅读工作台",
			text: "预览",
			raised: true,
			onPress: readingHome
		}));
		append(__nsh_hv81, __nsh_hv82);
		append(__nsh_hv81, __nsh_hv84);
		insert(__nsh_hv81, component(Caption, { get children() {
			return "以下是界面样例，尚未安装真实插件包。";
		} }), __nsh_hv84);
		insert(__nsh_hv81, [each(() => scenes, (s) => (() => {
			const __nsh_hv86 = element("view");
			bindProp(__nsh_hv86, "style", () => ({
				flexDirection: "row",
				gap: 14,
				alignItems: "center",
				paddingVertical: 13,
				borderBottomWidth: 1,
				borderColor: ink("border-subtle")
			}));
			const __nsh_hv87 = element("view");
			prop(__nsh_hv87, "style", {
				flex: 1,
				gap: 5
			});
			insert(__nsh_hv87, [component(Label, {
				weight: "500",
				get children() {
					return [() => s().kind, "工作台"];
				}
			}), component(Caption, { get children() {
				return s().title;
			} })]);
			append(__nsh_hv86, __nsh_hv87);
			insert(__nsh_hv86, component(Glyph, {
				get name() {
					return s().icon;
				},
				size: 22,
				get color() {
					return s().tint;
				}
			}), __nsh_hv87);
			insert(__nsh_hv86, component(Control, {
				get label() {
					return `预览${s().kind}工作台`;
				},
				text: "预览",
				raised: true,
				onPress: () => {
					choose(s().id);
					library.set(false);
				}
			}));
			return __nsh_hv86;
		})()), component(Caption, { get children() {
			return "所有场景共用任务导航、标签与外观规则。";
		} })]);
		return __nsh_hv81;
	})();
}
function App() {
	return (() => {
		const __nsh_hv88 = element("view");
		prop(__nsh_hv88, "onLayout", ({ bounds }) => compactUI.set(bounds.size.width < 700));
		prop(__nsh_hv88, "style", {
			flex: 1,
			flexDirection: "row",
			paddingTop: "env(safe-area-inset-top)",
			paddingBottom: "env(safe-area-inset-bottom)",
			paddingLeft: "env(safe-area-inset-left)",
			paddingRight: "env(safe-area-inset-right)"
		});
		insert(__nsh_hv88, [
			cond(() => sidebar() && !compactUI(), () => component(Sidebar, {})),
			cond(() => workspace() === readingEntry.id, () => component(ReadingWorkspace, {
				get sidebarVisible() {
					return sidebar() && !compactUI();
				},
				toggleSidebar: () => compactUI() ? mobileNavigation.set(!mobileNavigation()) : sidebar.set(!sidebar()),
				enterReading: () => sidebar.set(false),
				returnShelf: () => sidebar.set(true)
			})),
			cond(() => workspace() === "tasks", () => (() => {
				const __nsh_hv89 = element("view");
				bindProp(__nsh_hv89, "style", () => ({
					flex: 1,
					minWidth: 0,
					backgroundColor: ink("surface-stage")
				}));
				const __nsh_hv90 = element("view");
				prop(__nsh_hv90, "desktop", { windowDrag: true });
				bindProp(__nsh_hv90, "style", () => ({
					height: 52,
					flexShrink: 0,
					paddingLeft: sidebar() && !compactUI() ? 18 : titleBar().left + 12,
					paddingRight: titleBar().right + 16,
					flexDirection: "row",
					alignItems: "center",
					gap: 9,
					backgroundColor: ink("surface-toolbar"),
					borderBottomWidth: 1,
					borderColor: ink("border-subtle")
				}));
				const __nsh_hv91 = element("view");
				prop(__nsh_hv91, "style", {
					flex: 1,
					minWidth: 0
				});
				insert(__nsh_hv91, component(Label, {
					weight: "600",
					get children() {
						return library() ? "扩展与能力" : current().title;
					}
				}));
				const __nsh_hv92 = element("view");
				prop(__nsh_hv92, "style", { flex: 1 });
				const __nsh_hv93 = element("view");
				prop(__nsh_hv93, "style", { width: 7 });
				append(__nsh_hv90, __nsh_hv91);
				append(__nsh_hv90, __nsh_hv92);
				append(__nsh_hv90, __nsh_hv93);
				insert(__nsh_hv90, [
					cond(() => !sidebar() || compactUI(), () => component(Control, {
						label: "展开侧栏",
						icon: "panel",
						onPress: () => compactUI() ? mobileNavigation.set(true) : sidebar.set(true)
					})),
					cond(() => library(), () => component(Control, {
						label: "返回当前任务",
						icon: "back",
						onPress: () => library.set(false)
					})),
					component(Glyph, {
						get name() {
							return library() ? "plug" : current().icon;
						},
						size: 17
					})
				], __nsh_hv91);
				insert(__nsh_hv90, cond(() => !library() && !compactUI(), () => component(Segments, {
					get value() {
						return tab();
					},
					onChange: (v) => tab.set(v),
					items: [{
						id: "document",
						label: "工作台"
					}, {
						id: "steps",
						label: "任务过程"
					}]
				})), __nsh_hv93);
				insert(__nsh_hv90, [component(Control, {
					label: "任务信息",
					icon: "panel",
					get selected() {
						return inspector();
					},
					onPress: () => inspector.set(!inspector())
				}), component(Control, {
					label: "打开扩展样例",
					icon: "plug",
					onPress: showLibrary
				})]);
				const __nsh_hv95 = element("view");
				prop(__nsh_hv95, "style", {
					flex: 1,
					flexDirection: "row",
					minHeight: 0
				});
				insert(__nsh_hv95, [cond(() => !compactUI() || !inspector(), () => (() => {
					const __nsh_hv96 = element("scroll");
					prop(__nsh_hv96, "style", { flex: 1 });
					const __nsh_hv97 = element("view");
					bindProp(__nsh_hv97, "style", () => ({
						padding: compactUI() ? 14 : 30,
						alignItems: "center"
					}));
					const __nsh_hv98 = element("view");
					bindProp(__nsh_hv98, "style", () => ({
						width: "100%",
						maxWidth: 790,
						padding: compactUI() ? 20 : 38,
						minHeight: 650,
						backgroundColor: ink("surface-paper"),
						borderRadius: 3,
						borderWidth: 1,
						borderColor: ink("border-paper"),
						boxShadow: "0 2px 8px rgba(0,0,0,0.045), 0 0.5px 1px rgba(0,0,0,0.025)"
					}));
					insert(__nsh_hv98, [
						cond(() => library(), () => component(CapabilityLibrary, {})),
						cond(() => !library() && tab() === "steps", () => component(Steps, {})),
						cond(() => !library() && tab() === "document" && scene() === "research", () => component(Research, {})),
						cond(() => !library() && tab() === "document" && scene() === "writing", () => component(Writing, {})),
						cond(() => !library() && tab() === "document" && scene() === "process", () => component(Process, {})),
						cond(() => !library() && tab() === "document" && scene() === "development", () => component(Development, {}))
					]);
					append(__nsh_hv97, __nsh_hv98);
					append(__nsh_hv96, __nsh_hv97);
					return __nsh_hv96;
				})()), cond(() => inspector(), () => component(Inspector, {}))]);
				const __nsh_hv99 = element("view");
				bindProp(__nsh_hv99, "style", () => ({
					minHeight: 61,
					flexShrink: 0,
					paddingHorizontal: 23,
					paddingVertical: 11,
					flexDirection: "row",
					alignItems: "center",
					gap: 12,
					borderTopWidth: 1,
					borderColor: ink("border-subtle"),
					backgroundColor: ink("surface-toolbar")
				}));
				const __nsh_hv100 = element("view");
				bindProp(__nsh_hv100, "style", () => ({
					width: 30,
					height: 30,
					borderRadius: 15,
					backgroundColor: ink("surface-track"),
					alignItems: "center",
					justifyContent: "center"
				}));
				insert(__nsh_hv100, component(Glyph, {
					name: "plug",
					size: 16
				}));
				const __nsh_hv101 = element("view");
				prop(__nsh_hv101, "style", {
					flex: 1,
					gap: 3
				});
				const __nsh_hv102 = element("text");
				bindProp(__nsh_hv102, "style", () => ({
					fontSize: 13,
					fontWeight: "500",
					color: ink("text-primary")
				}));
				const __nsh_hv103 = text("Agent 尚未连接");
				append(__nsh_hv102, __nsh_hv103);
				const __nsh_hv104 = element("text");
				bindProp(__nsh_hv104, "style", () => ({
					fontSize: 12,
					color: ink("text-secondary"),
					numberOfLines: 1
				}));
				insert(__nsh_hv104, () => note() || "当前任务的材料与草稿，在需要时交给 Agent。");
				append(__nsh_hv101, __nsh_hv102);
				append(__nsh_hv101, __nsh_hv104);
				append(__nsh_hv99, __nsh_hv100);
				append(__nsh_hv99, __nsh_hv101);
				insert(__nsh_hv99, component(AgentMenu, {}));
				append(__nsh_hv89, __nsh_hv90);
				append(__nsh_hv89, __nsh_hv95);
				append(__nsh_hv89, __nsh_hv99);
				insert(__nsh_hv89, cond(() => !library() && compactUI(), () => (() => {
					const __nsh_hv94 = element("view");
					prop(__nsh_hv94, "style", {
						paddingHorizontal: 12,
						paddingVertical: 7
					});
					insert(__nsh_hv94, component(Segments, {
						get value() {
							return tab();
						},
						onChange: (v) => tab.set(v),
						items: [{
							id: "document",
							label: "工作台"
						}, {
							id: "steps",
							label: "任务过程"
						}]
					}));
					return __nsh_hv94;
				})()), __nsh_hv95);
				return __nsh_hv89;
			})()),
			cond(() => compactUI() && mobileNavigation(), () => (() => {
				const __nsh_hv105 = element("view");
				prop(__nsh_hv105, "style", {
					position: "absolute",
					top: 0,
					bottom: 0,
					left: 0,
					right: 0,
					zIndex: 200
				});
				const __nsh_hv106 = element("view");
				prop(__nsh_hv106, "accessibilityLabel", "关闭全局导航");
				prop(__nsh_hv106, "onPress", () => mobileNavigation.set(false));
				prop(__nsh_hv106, "style", {
					position: "absolute",
					top: 0,
					bottom: 0,
					left: 0,
					right: 0,
					backgroundColor: "rgba(0,0,0,0.28)"
				});
				append(__nsh_hv105, __nsh_hv106);
				insert(__nsh_hv105, component(Sidebar, {}));
				return __nsh_hv105;
			})())
		]);
		return __nsh_hv88;
	})();
}
setColorScheme("light");
mount(App);

//#endregion