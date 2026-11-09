import { append, bindProp, component, cond, each, element, insert, prop, setup, text } from "nsh:rt";
import { Canvas, Path2D, mount, setColorScheme, signal, token } from "nsh";
import { Button, Menu, MenuItem, MenuSeparator, Tab, Tabs } from "nsh/base";
import { currentWindow, titleBar } from "nsh/desktop";

//#region ui.tsx
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
			fontSize: props.small ? fonts.small : fonts.ui,
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
		get onPress() {
			return props.onPress;
		},
		get style() {
			return {
				height: 30,
				minWidth: 30,
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
						fontSize: fonts.small,
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
				height: 31,
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
				style: {
					paddingHorizontal: 17,
					justifyContent: "center",
					alignItems: "center",
					borderRadius: 5
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
							fontSize: fonts.small,
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
//#region App.tsx
setup(null, "{\n  \"name\": \"QUSOR Graphite & Paper — native study\",\n  \"cssVars\": {\n    \"theme\": {\n      \"radius\": \"7px\"\n    },\n    \"light\": {\n      \"surface-navigation\": \"rgba(231,231,226,0.82)\",\n      \"surface-stage\": \"#EFEEEA\",\n      \"surface-toolbar\": \"#F5F5F1\",\n      \"surface-paper\": \"#FFFEFC\",\n      \"surface-inspector\": \"#F6F5F1\",\n      \"surface-search\": \"rgba(255,255,255,0.46)\",\n      \"surface-control\": \"#FAFAF7\",\n      \"surface-floating\": \"rgba(250,250,247,0.96)\",\n      \"surface-track\": \"#E8E8E3\",\n      \"surface-inset\": \"#F3F3EE\",\n      \"surface-selected\": \"#D8E1E7\",\n      \"surface-hover\": \"#DFE2DF\",\n      \"surface-pressed\": \"#D2D7D5\",\n      \"border-subtle\": \"#D8D8D0\",\n      \"border-control\": \"#D2D3CC\",\n      \"border-paper\": \"#E3E2DB\",\n      \"text-primary\": \"#2D302D\",\n      \"text-secondary\": \"#6B7069\",\n      \"text-accent\": \"#386080\",\n      \"control-primary\": \"#3C4542\",\n      \"control-primary-hover\": \"#303B37\",\n      \"control-primary-pressed\": \"#25302C\",\n      \"control-primary-text\": \"#FFFFFF\",\n      \"focus-ring\": \"#467BBD\",\n      \"transparent\": \"#00000000\",\n      \"background\": \"#FFFEFC\",\n      \"foreground\": \"#2D302D\",\n      \"primary\": \"#3C4542\",\n      \"primary-foreground\": \"#FFFFFF\",\n      \"muted\": \"#E8E8E3\",\n      \"muted-foreground\": \"#6B7069\",\n      \"accent\": \"#D8E1E7\",\n      \"accent-foreground\": \"#2D302D\",\n      \"border\": \"#D8D8D0\",\n      \"input\": \"#D2D3CC\",\n      \"ring\": \"#467BBD\",\n      \"popover\": \"rgba(250,250,247,0.96)\",\n      \"popover-foreground\": \"#2D302D\"\n    },\n    \"dark\": {\n      \"surface-navigation\": \"rgba(35,38,36,0.88)\",\n      \"surface-stage\": \"#1C1F1D\",\n      \"surface-toolbar\": \"#272B28\",\n      \"surface-paper\": \"#2D312D\",\n      \"surface-inspector\": \"#272C28\",\n      \"surface-search\": \"rgba(0,0,0,0.16)\",\n      \"surface-control\": \"#3D433D\",\n      \"surface-floating\": \"rgba(47,53,48,0.97)\",\n      \"surface-track\": \"#202722\",\n      \"surface-inset\": \"#242A25\",\n      \"surface-selected\": \"#384B51\",\n      \"surface-hover\": \"#39433D\",\n      \"surface-pressed\": \"#42534A\",\n      \"border-subtle\": \"#3C443D\",\n      \"border-control\": \"#4B554B\",\n      \"border-paper\": \"#3A443A\",\n      \"text-primary\": \"#ECEEE6\",\n      \"text-secondary\": \"#B1B9AC\",\n      \"text-accent\": \"#ADCFEA\",\n      \"control-primary\": \"#B4C7B7\",\n      \"control-primary-hover\": \"#C6D6C7\",\n      \"control-primary-pressed\": \"#9AB39E\",\n      \"control-primary-text\": \"#15221B\",\n      \"focus-ring\": \"#93B7ED\",\n      \"transparent\": \"#00000000\",\n      \"background\": \"#2D312D\",\n      \"foreground\": \"#ECEEE6\",\n      \"primary\": \"#B4C7B7\",\n      \"primary-foreground\": \"#15221B\",\n      \"muted\": \"#202722\",\n      \"muted-foreground\": \"#B1B9AC\",\n      \"accent\": \"#384B51\",\n      \"accent-foreground\": \"#ECEEE6\",\n      \"border\": \"#3C443D\",\n      \"input\": \"#4B554B\",\n      \"ring\": \"#93B7ED\",\n      \"popover\": \"rgba(47,53,48,0.97)\",\n      \"popover-foreground\": \"#ECEEE6\"\n    }\n  }\n}\n", null, null);
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
function showLibrary() {
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
			onPress: () => sidebar.set(false)
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
					backgroundColor: ink(scene() === s().id ? "surface-selected" : "transparent")
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
						fontWeight: scene() === s().id ? "600" : "400",
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
		const __nsh_hv36 = element("view");
		prop(__nsh_hv36, "style", {
			flexDirection: "row",
			alignItems: "center",
			paddingLeft: 8,
			paddingTop: 6
		});
		const __nsh_hv37 = element("view");
		bindProp(__nsh_hv37, "style", () => ({
			width: 6,
			height: 6,
			borderRadius: 3,
			backgroundColor: ink("text-secondary"),
			marginRight: 7
		}));
		const __nsh_hv38 = element("text");
		bindProp(__nsh_hv38, "style", () => ({
			flex: 1,
			fontSize: 12,
			color: ink("text-secondary")
		}));
		const __nsh_hv39 = text("原生视觉样例");
		append(__nsh_hv38, __nsh_hv39);
		append(__nsh_hv36, __nsh_hv37);
		append(__nsh_hv36, __nsh_hv38);
		insert(__nsh_hv36, component(Control, {
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
		append(__nsh_hv32, __nsh_hv36);
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
						const __nsh_hv35 = text("4");
						append(__nsh_hv34, __nsh_hv35);
						return __nsh_hv34;
					})()
				];
			}
		}), __nsh_hv36);
		append(__nsh_hv6, __nsh_hv7);
		append(__nsh_hv6, __nsh_hv8);
		append(__nsh_hv6, __nsh_hv14);
		append(__nsh_hv6, __nsh_hv16);
		append(__nsh_hv6, __nsh_hv22);
		append(__nsh_hv6, __nsh_hv26);
		append(__nsh_hv6, __nsh_hv30);
		append(__nsh_hv6, __nsh_hv31);
		append(__nsh_hv6, __nsh_hv32);
		return __nsh_hv6;
	})();
}
function Caption(props) {
	return (() => {
		const __nsh_hv40 = element("text");
		bindProp(__nsh_hv40, "style", () => ({
			fontSize: 12,
			lineHeight: 18,
			color: ink("text-secondary")
		}));
		insert(__nsh_hv40, () => props.children);
		return __nsh_hv40;
	})();
}
function Paragraph(props) {
	return (() => {
		const __nsh_hv41 = element("text");
		bindProp(__nsh_hv41, "style", () => ({
			fontSize: fonts.body,
			lineHeight: 28,
			color: ink("text-primary")
		}));
		insert(__nsh_hv41, () => props.children);
		return __nsh_hv41;
	})();
}
function Heading(props) {
	return (() => {
		const __nsh_hv42 = element("text");
		bindProp(__nsh_hv42, "style", () => ({
			fontSize: 17,
			lineHeight: 26,
			fontWeight: "600",
			color: ink("text-primary"),
			marginTop: 12
		}));
		insert(__nsh_hv42, () => props.children);
		return __nsh_hv42;
	})();
}
function Research() {
	return (() => {
		const __nsh_hv43 = element("view");
		prop(__nsh_hv43, "style", { gap: 18 });
		const __nsh_hv44 = element("view");
		prop(__nsh_hv44, "style", {
			flexDirection: "row",
			alignItems: "center",
			gap: 7
		});
		const __nsh_hv45 = element("view");
		prop(__nsh_hv45, "style", {
			width: 5,
			height: 5,
			borderRadius: 3,
			backgroundColor: "#AD8A51"
		});
		const __nsh_hv46 = element("view");
		prop(__nsh_hv46, "style", { flex: 1 });
		append(__nsh_hv44, __nsh_hv45);
		append(__nsh_hv44, __nsh_hv46);
		insert(__nsh_hv44, component(Caption, { get children() {
			return "研究札记";
		} }), __nsh_hv46);
		insert(__nsh_hv44, component(Caption, { get children() {
			return "2026.11.09";
		} }));
		const __nsh_hv47 = element("view");
		prop(__nsh_hv47, "style", { gap: 8 });
		const __nsh_hv48 = element("text");
		bindProp(__nsh_hv48, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			letterSpacing: -.5,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv49 = text("原生软件的秩序与手感");
		append(__nsh_hv48, __nsh_hv49);
		append(__nsh_hv47, __nsh_hv48);
		insert(__nsh_hv47, component(Caption, { get children() {
			return "QUSOR 设计研究 · 自拟材料";
		} }));
		const __nsh_hv50 = element("view");
		bindProp(__nsh_hv50, "style", () => ({
			flexDirection: "row",
			alignItems: "center",
			gap: 14,
			paddingVertical: 12,
			borderTopWidth: 1,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv51 = element("text");
		bindProp(__nsh_hv51, "style", () => ({
			fontSize: 13,
			color: ink("text-primary")
		}));
		const __nsh_hv52 = text("Things  ·  Reeder  ·  Kino");
		append(__nsh_hv51, __nsh_hv52);
		append(__nsh_hv50, __nsh_hv51);
		insert(__nsh_hv50, [component(Glyph, {
			name: "book",
			size: 16
		}), component(Caption, { get children() {
			return "参考作品";
		} })], __nsh_hv51);
		const __nsh_hv53 = element("view");
		prop(__nsh_hv53, "style", {
			marginTop: 4,
			paddingLeft: 15,
			borderLeftWidth: 2,
			borderColor: "#B59A6C",
			gap: 8
		});
		const __nsh_hv54 = element("text");
		bindProp(__nsh_hv54, "style", () => ({
			fontSize: 14,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv55 = text("随手记");
		append(__nsh_hv54, __nsh_hv55);
		const __nsh_hv56 = element("input");
		prop(__nsh_hv56, "accessibilityLabel", "研究札记草稿");
		bindProp(__nsh_hv56, "value", () => researchDraft());
		prop(__nsh_hv56, "onChange", ({ value }) => researchDraft.set(value));
		prop(__nsh_hv56, "multiline", true);
		bindProp(__nsh_hv56, "style", () => ({
			minHeight: 72,
			padding: 0,
			borderWidth: 0,
			backgroundColor: ink("surface-paper"),
			color: ink("text-primary"),
			fontSize: 16,
			lineHeight: 27
		}));
		bindProp(__nsh_hv56, "focusStyle", () => ({
			outlineWidth: 1,
			outlineOffset: 5,
			outlineColor: ink("focus-ring")
		}));
		append(__nsh_hv53, __nsh_hv54);
		append(__nsh_hv53, __nsh_hv56);
		insert(__nsh_hv53, component(Caption, { get children() {
			return "草稿只保留在当前窗口";
		} }));
		append(__nsh_hv43, __nsh_hv44);
		append(__nsh_hv43, __nsh_hv47);
		append(__nsh_hv43, __nsh_hv50);
		append(__nsh_hv43, __nsh_hv53);
		insert(__nsh_hv43, component(Paragraph, { get children() {
			return "好的工具让人专注于眼前的内容。窗口、文字与操作保持稳定，复杂能力在需要的时候出现。";
		} }), __nsh_hv50);
		insert(__nsh_hv43, [
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
		], __nsh_hv53);
		return __nsh_hv43;
	})();
}
function Writing() {
	return (() => {
		const __nsh_hv57 = element("view");
		prop(__nsh_hv57, "style", { gap: 20 });
		const __nsh_hv58 = element("text");
		bindProp(__nsh_hv58, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv59 = text("把想法整理成文章");
		append(__nsh_hv58, __nsh_hv59);
		const __nsh_hv60 = element("input");
		prop(__nsh_hv60, "accessibilityLabel", "写作草稿");
		bindProp(__nsh_hv60, "value", () => writingDraft());
		prop(__nsh_hv60, "onChange", ({ value }) => writingDraft.set(value));
		prop(__nsh_hv60, "multiline", true);
		bindProp(__nsh_hv60, "style", () => ({
			minHeight: 320,
			padding: 0,
			borderWidth: 0,
			backgroundColor: ink("surface-paper"),
			color: ink("text-primary"),
			fontSize: 17,
			lineHeight: 30
		}));
		bindProp(__nsh_hv60, "focusStyle", () => ({
			outlineWidth: 1,
			outlineOffset: 8,
			outlineColor: ink("focus-ring")
		}));
		append(__nsh_hv57, __nsh_hv58);
		append(__nsh_hv57, __nsh_hv60);
		insert(__nsh_hv57, component(Caption, { get children() {
			return "文稿 · 设计样例";
		} }), __nsh_hv58);
		insert(__nsh_hv57, [component(Paragraph, { get children() {
			return "主内容可以直接成为写作空间。资料、想法与 Agent 建议都围绕这篇文稿出现。";
		} }), component(Heading, { get children() {
			return "我的草稿";
		} })], __nsh_hv60);
		insert(__nsh_hv57, component(Caption, { get children() {
			return "本任务的草稿只保留在当前窗口，切换标签不会清空输入。";
		} }));
		return __nsh_hv57;
	})();
}
function Process() {
	return (() => {
		const __nsh_hv61 = element("view");
		prop(__nsh_hv61, "style", { gap: 22 });
		const __nsh_hv62 = element("text");
		bindProp(__nsh_hv62, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv63 = text("安排一次工作流程");
		append(__nsh_hv62, __nsh_hv63);
		append(__nsh_hv61, __nsh_hv62);
		insert(__nsh_hv61, component(Caption, { get children() {
			return "工作流程 · 设计样例";
		} }), __nsh_hv62);
		insert(__nsh_hv61, [component(Paragraph, { get children() {
			return "把过程铺开，逐步检查输入与结果。Agent 连接后才可以执行；当前步骤均未开始。";
		} }), each(() => [
			"准备资料与目标",
			"选择适用的 Agent 与工具",
			"检查计划并允许执行",
			"审阅结果，保留需要的内容"
		], (step, index) => (() => {
			const __nsh_hv64 = element("view");
			bindProp(__nsh_hv64, "style", () => ({
				flexDirection: "row",
				gap: 15,
				paddingVertical: 15,
				borderBottomWidth: 1,
				borderColor: ink("border-subtle"),
				alignItems: "center"
			}));
			const __nsh_hv65 = element("view");
			bindProp(__nsh_hv65, "style", () => ({
				width: 26,
				height: 26,
				borderRadius: 13,
				borderWidth: 1,
				borderColor: ink("border-control"),
				alignItems: "center",
				justifyContent: "center"
			}));
			insert(__nsh_hv65, component(Caption, { get children() {
				return index() + 1;
			} }));
			const __nsh_hv66 = element("view");
			prop(__nsh_hv66, "style", { flex: 1 });
			insert(__nsh_hv66, component(Label, { get children() {
				return step();
			} }));
			append(__nsh_hv64, __nsh_hv65);
			append(__nsh_hv64, __nsh_hv66);
			insert(__nsh_hv64, component(Caption, { get children() {
				return "未开始";
			} }));
			return __nsh_hv64;
		})())]);
		return __nsh_hv61;
	})();
}
function Development() {
	return (() => {
		const __nsh_hv67 = element("view");
		prop(__nsh_hv67, "style", { gap: 22 });
		const __nsh_hv68 = element("text");
		bindProp(__nsh_hv68, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv69 = text("制作一个小工具");
		append(__nsh_hv68, __nsh_hv69);
		const __nsh_hv70 = element("view");
		bindProp(__nsh_hv70, "style", () => ({
			flexDirection: "row",
			paddingVertical: 12,
			borderTopWidth: 1,
			borderBottomWidth: 1,
			borderColor: ink("border-subtle"),
			alignItems: "center",
			gap: 8
		}));
		const __nsh_hv71 = element("view");
		prop(__nsh_hv71, "style", { flex: 1 });
		append(__nsh_hv70, __nsh_hv71);
		insert(__nsh_hv70, [component(Glyph, {
			name: "code",
			size: 16
		}), component(Label, {
			small: true,
			get children() {
				return "示例代码";
			}
		})], __nsh_hv71);
		insert(__nsh_hv70, component(Caption, { get children() {
			return "只读 · 未执行";
		} }));
		const __nsh_hv72 = element("text");
		bindProp(__nsh_hv72, "style", () => ({
			userSelect: "text",
			fontFamily: "monospace",
			fontSize: 14,
			lineHeight: 27,
			color: ink("text-primary"),
			padding: 15,
			backgroundColor: ink("surface-inset"),
			borderRadius: 5
		}));
		insert(__nsh_hv72, "// A task can choose its own tools.\nconst task = {\n  title: \"A small useful tool\",\n  agent: null,\n  views: [\"code\", \"preview\"]\n};");
		append(__nsh_hv67, __nsh_hv68);
		append(__nsh_hv67, __nsh_hv70);
		append(__nsh_hv67, __nsh_hv72);
		insert(__nsh_hv67, component(Caption, { get children() {
			return "开发 · 可选的任务界面";
		} }), __nsh_hv68);
		insert(__nsh_hv67, component(Paragraph, { get children() {
			return "代码是这个任务的内容。需要时再打开文件、预览与执行输出，其他任务继续使用各自适合的界面。";
		} }), __nsh_hv70);
		insert(__nsh_hv67, component(Caption, { get children() {
			return "上面的代码仅说明设计思路，并非已发布的 SDK。";
		} }));
		return __nsh_hv67;
	})();
}
function Steps() {
	return (() => {
		const __nsh_hv73 = element("view");
		prop(__nsh_hv73, "style", { gap: 22 });
		const __nsh_hv74 = element("text");
		bindProp(__nsh_hv74, "style", () => ({
			fontSize: 26,
			lineHeight: 36,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv75 = text("从目标到结果");
		append(__nsh_hv74, __nsh_hv75);
		append(__nsh_hv73, __nsh_hv74);
		insert(__nsh_hv73, component(Caption, { get children() {
			return "任务过程 · 设计样例";
		} }), __nsh_hv74);
		insert(__nsh_hv73, [component(Paragraph, { get children() {
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
			const __nsh_hv76 = element("view");
			bindProp(__nsh_hv76, "style", () => ({
				gap: 8,
				paddingVertical: 15,
				borderBottomWidth: 1,
				borderColor: ink("border-subtle")
			}));
			insert(__nsh_hv76, [component(Label, {
				weight: "600",
				get children() {
					return s().title;
				}
			}), component(Caption, { get children() {
				return s().text;
			} })]);
			return __nsh_hv76;
		})())]);
		return __nsh_hv73;
	})();
}
function Inspector() {
	return (() => {
		const __nsh_hv77 = element("view");
		bindProp(__nsh_hv77, "style", () => ({
			width: 260,
			flexShrink: 0,
			backgroundColor: ink("surface-inspector"),
			borderLeftWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv78 = element("view");
		prop(__nsh_hv78, "style", {
			height: 52,
			paddingHorizontal: 16,
			alignItems: "center",
			flexDirection: "row",
			gap: 7
		});
		const __nsh_hv79 = element("view");
		prop(__nsh_hv79, "style", { flex: 1 });
		append(__nsh_hv78, __nsh_hv79);
		insert(__nsh_hv78, component(Label, {
			weight: "600",
			get children() {
				return "任务信息";
			}
		}), __nsh_hv79);
		insert(__nsh_hv78, component(Control, {
			label: "关闭任务信息",
			icon: "close",
			onPress: () => inspector.set(false)
		}));
		const __nsh_hv80 = element("view");
		prop(__nsh_hv80, "style", {
			padding: 20,
			gap: 16
		});
		const __nsh_hv81 = element("view");
		prop(__nsh_hv81, "style", {
			flexDirection: "row",
			gap: 10,
			alignItems: "center"
		});
		insert(__nsh_hv81, [component(Glyph, { get name() {
			return current().icon;
		} }), component(Label, { get children() {
			return [() => current().kind, "工作台"];
		} })]);
		append(__nsh_hv80, __nsh_hv81);
		insert(__nsh_hv80, [
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
		], __nsh_hv81);
		insert(__nsh_hv80, [
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
		append(__nsh_hv77, __nsh_hv78);
		append(__nsh_hv77, __nsh_hv80);
		insert(__nsh_hv77, component(Rule, {}), __nsh_hv80);
		return __nsh_hv77;
	})();
}
function CapabilityLibrary() {
	return (() => {
		const __nsh_hv82 = element("view");
		bindProp(__nsh_hv82, "style", () => ({
			flex: 1,
			backgroundColor: ink("surface-paper"),
			padding: 0,
			gap: 18
		}));
		const __nsh_hv83 = element("text");
		bindProp(__nsh_hv83, "style", () => ({
			fontSize: 21,
			fontWeight: "600",
			color: ink("text-primary")
		}));
		const __nsh_hv84 = text("选择适合的工作界面");
		append(__nsh_hv83, __nsh_hv84);
		append(__nsh_hv82, __nsh_hv83);
		insert(__nsh_hv82, [
			component(Caption, { get children() {
				return "以下是界面样例，尚未安装真实插件包。";
			} }),
			each(() => scenes, (s) => (() => {
				const __nsh_hv85 = element("view");
				bindProp(__nsh_hv85, "style", () => ({
					flexDirection: "row",
					gap: 14,
					alignItems: "center",
					paddingVertical: 13,
					borderBottomWidth: 1,
					borderColor: ink("border-subtle")
				}));
				const __nsh_hv86 = element("view");
				prop(__nsh_hv86, "style", {
					flex: 1,
					gap: 5
				});
				insert(__nsh_hv86, [component(Label, {
					weight: "500",
					get children() {
						return [() => s().kind, "工作台"];
					}
				}), component(Caption, { get children() {
					return s().title;
				} })]);
				append(__nsh_hv85, __nsh_hv86);
				insert(__nsh_hv85, component(Glyph, {
					get name() {
						return s().icon;
					},
					size: 22,
					get color() {
						return s().tint;
					}
				}), __nsh_hv86);
				insert(__nsh_hv85, component(Control, {
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
				return __nsh_hv85;
			})()),
			component(Caption, { get children() {
				return "所有场景共用任务导航、标签与外观规则。";
			} })
		]);
		return __nsh_hv82;
	})();
}
function App() {
	return (() => {
		const __nsh_hv87 = element("view");
		prop(__nsh_hv87, "style", {
			flex: 1,
			flexDirection: "row"
		});
		const __nsh_hv88 = element("view");
		bindProp(__nsh_hv88, "style", () => ({
			flex: 1,
			minWidth: 0,
			backgroundColor: ink("surface-stage")
		}));
		const __nsh_hv89 = element("view");
		prop(__nsh_hv89, "desktop", { windowDrag: true });
		bindProp(__nsh_hv89, "style", () => ({
			height: 52,
			flexShrink: 0,
			paddingLeft: sidebar() ? 18 : titleBar().left + 12,
			paddingRight: titleBar().right + 16,
			flexDirection: "row",
			alignItems: "center",
			gap: 9,
			backgroundColor: ink("surface-toolbar"),
			borderBottomWidth: 1,
			borderColor: ink("border-subtle")
		}));
		const __nsh_hv90 = element("view");
		prop(__nsh_hv90, "style", { flex: 1 });
		const __nsh_hv91 = element("view");
		prop(__nsh_hv91, "style", { width: 7 });
		append(__nsh_hv89, __nsh_hv90);
		append(__nsh_hv89, __nsh_hv91);
		insert(__nsh_hv89, [
			cond(() => !sidebar(), () => component(Control, {
				label: "展开侧栏",
				icon: "panel",
				onPress: () => sidebar.set(true)
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
			}),
			component(Label, {
				weight: "600",
				get children() {
					return library() ? "扩展与能力" : current().title;
				}
			})
		], __nsh_hv90);
		insert(__nsh_hv89, cond(() => !library(), () => component(Segments, {
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
		})), __nsh_hv91);
		insert(__nsh_hv89, [component(Control, {
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
		const __nsh_hv92 = element("view");
		prop(__nsh_hv92, "style", {
			flex: 1,
			flexDirection: "row",
			minHeight: 0
		});
		const __nsh_hv93 = element("scroll");
		prop(__nsh_hv93, "style", { flex: 1 });
		const __nsh_hv94 = element("view");
		prop(__nsh_hv94, "style", {
			padding: 30,
			alignItems: "center"
		});
		const __nsh_hv95 = element("view");
		bindProp(__nsh_hv95, "style", () => ({
			width: "100%",
			maxWidth: 790,
			padding: 38,
			minHeight: 650,
			backgroundColor: ink("surface-paper"),
			borderRadius: 3,
			borderWidth: 1,
			borderColor: ink("border-paper"),
			boxShadow: "0 2px 8px rgba(0,0,0,0.045), 0 0.5px 1px rgba(0,0,0,0.025)"
		}));
		insert(__nsh_hv95, [
			cond(() => library(), () => component(CapabilityLibrary, {})),
			cond(() => !library() && tab() === "steps", () => component(Steps, {})),
			cond(() => !library() && tab() === "document" && scene() === "research", () => component(Research, {})),
			cond(() => !library() && tab() === "document" && scene() === "writing", () => component(Writing, {})),
			cond(() => !library() && tab() === "document" && scene() === "process", () => component(Process, {})),
			cond(() => !library() && tab() === "document" && scene() === "development", () => component(Development, {}))
		]);
		append(__nsh_hv94, __nsh_hv95);
		append(__nsh_hv93, __nsh_hv94);
		append(__nsh_hv92, __nsh_hv93);
		insert(__nsh_hv92, cond(() => inspector(), () => component(Inspector, {})));
		const __nsh_hv96 = element("view");
		bindProp(__nsh_hv96, "style", () => ({
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
		const __nsh_hv97 = element("view");
		bindProp(__nsh_hv97, "style", () => ({
			width: 30,
			height: 30,
			borderRadius: 15,
			backgroundColor: ink("surface-track"),
			alignItems: "center",
			justifyContent: "center"
		}));
		insert(__nsh_hv97, component(Glyph, {
			name: "plug",
			size: 16
		}));
		const __nsh_hv98 = element("view");
		prop(__nsh_hv98, "style", {
			flex: 1,
			gap: 3
		});
		const __nsh_hv99 = element("text");
		bindProp(__nsh_hv99, "style", () => ({
			fontSize: 13,
			fontWeight: "500",
			color: ink("text-primary")
		}));
		const __nsh_hv100 = text("Agent 尚未连接");
		append(__nsh_hv99, __nsh_hv100);
		const __nsh_hv101 = element("text");
		bindProp(__nsh_hv101, "style", () => ({
			fontSize: 12,
			color: ink("text-secondary"),
			numberOfLines: 1
		}));
		insert(__nsh_hv101, () => note() || "当前任务的材料与草稿，在需要时交给 Agent。");
		append(__nsh_hv98, __nsh_hv99);
		append(__nsh_hv98, __nsh_hv101);
		append(__nsh_hv96, __nsh_hv97);
		append(__nsh_hv96, __nsh_hv98);
		insert(__nsh_hv96, component(AgentMenu, {}));
		append(__nsh_hv88, __nsh_hv89);
		append(__nsh_hv88, __nsh_hv92);
		append(__nsh_hv88, __nsh_hv96);
		append(__nsh_hv87, __nsh_hv88);
		insert(__nsh_hv87, cond(() => sidebar(), () => component(Sidebar, {})), __nsh_hv88);
		return __nsh_hv87;
	})();
}
setColorScheme("light");
mount(App);

//#endregion