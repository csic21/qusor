# 原生视觉样例：macOS 实窗记录

日期：2026-11-09。关联 F-UX-01/02、M-1 P02 的原生窗口显示子项。状态：**最终候选已实际显示并验证局部交互；不是生产应用、完整 P02 或六端资格通过。**

## 固定环境与工件

- macOS 27.0 / 26A428；截图来自本机原生窗口。工具输出的实际图片尺寸、工件哈希及详细范围见 [record.json](record.json)。不推断截图尺寸等于物理 DPI。
- 缓存源码：NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。复用现有 debug 路径宿主，未在本轮重编译 Rust；源码提交和实际执行文件分别记录，不假定两者自动构成完整构建证明。
- 实际宿主 SHA-256：`42b60abbdf6a332757af0ce94dec578ce11b1388978ab0d19cbd068f40d3edc9`。
- 最终 bundle 目录 SHA-256：`b283387b04a26265f12c3e2e1becbb06e949db4173b087076d765787188be4a0`。算法为相对路径、NUL 与文件字节按路径排序串联后计算 SHA-256。
- [冻结的 UI 源码](frozen-source/App.tsx)、[共享控件](frozen-source/ui.tsx)、[主题](frozen-source/theme.json)、[bundle](frozen-bundle/bundle.nshc) 与上游 lockfile 已归档。没有把 245MB 宿主复制进证据目录。
- 内容均为自拟 fixture。中文通过系统粘贴输入；没有因此给中文输入法组合态资格。字体是 NSH 系统默认与 OS 中文回退，实际字族和字形度量未仪器测量。

本样例使用 NSH 原生宿主、AppKit 窗口和原生显示 / 输入路径，TSX 不导入 WebView、DOM 或业务桥接。公开插件包接入仍另验。图片保留系统屏幕共享提示与指针显示，未做内容修饰；原始截图字节为 JPEG，仅将文件扩展名改正为 `.jpg`。

## 最终候选的实际结果

| 场景 | 实际结果 | 证据 |
|---|---|---|
| NV-01 显示 | 原生窗口显示中文工具栏、列表、正文与输入 | [浅色](06-light-final.jpg)、[最终默认状态](15-default-final.jpg) |
| NV-02 输入与局部保留 | 写作任务输入新中文草稿，切到过程标签再返回后仍在；研究任务保留自己的原始草稿 | [写作草稿](08-writing-retained-final.jpg)、[输入观察](08-writing-retained-final.ax.txt)、[研究任务完整树](09-dark-final.ax.txt) |
| NV-03 扩展列表 | 打开列表，选择写作预览，回到写作任务 | [列表](07-capabilities-final.jpg)、[可访问树](07-capabilities-final.ax.txt) |
| NV-04 明暗外观 | 显示深色并切回浅色 | [深色](09-dark-final.jpg)、[后续浅色](10-flow-final.jpg) |
| NV-05 多种任务 | 流程与开发样例分别显示；步骤未执行、代码只读 | [流程](10-flow-final.jpg)、[开发](11-development-final.jpg) |
| NV-06 临时区域 | 检查器与 Agent 菜单展开；导航收起并恢复 | [检查器](12-inspector-final.jpg)、[菜单](13-agent-menu-final.jpg)、[收起导航](14-sidebar-collapsed-final.jpg)、[恢复](15-default-final.jpg) |

构建与类型检查均实际退出 0，具体命令见 [复现入口](../../native-design/README.md)。最终阶段 `06–15` 图片与观察属于上述最终 bundle；部分 AX 文件是相对前次观察的 diff，完整状态另有明确文件，不能把 diff 当完整树。

## 首轮尝试与保留的限制

`01–05` 属于 [首轮启动收据](initial-launch.json) 的版本，不计入最终版本工件集合。首轮 bundle 没有归档，仅保留当时哈希、图片和 AX 观察，不能宣称可完全复现该历史二进制。首轮 Agent 菜单选择后实际回显“待配置 / 未连接”。

首轮独立 `Window kind="panel"` 尝试未在主窗口截图及 AX 观察中出现可见变化，保留 [当时图片](05-panel-not-shown.jpg) 和 [观察](05-panel-not-shown.ax.txt)。它没有取得浮窗资格，也不能据此宣布 NSH 缺少多窗口能力。最终样例改为同一原生窗口内的扩展列表。

点击标签后尝试 Left / Tab，未建立可观察的键盘激活路径；完整键盘访问仍未通过。共享控件虽然定义了 focus 样式，也不能据此宣布焦点交互合格。

尚未验证：完整 IME / undo / 光标、读屏、动态字号、触控、窄窗与各屏幕缩放、其他平台、真实性能、原生多窗口、实际 Agent / 插件 / Core 保存和退出恢复。窗口内草稿明确不落盘；重新启动后回到自拟默认内容，符合样例说明。

## 上游核对

2026-11-09 核对 [官方发布](https://github.com/longbridge/nsh/releases) 和 [main 提交](https://github.com/longbridge/nsh/commit/84b2cf9195d00ba4e5dbfaca25473a959add1c54)：稳定版 v0.4.0，nightly target 为 `36b91689c1e7c38379179a56381de7ddab16c6a5`，main 为 `84b2cf9195d00ba4e5dbfaca25473a959add1c54`。最新提交为 Linux shader device 优先 OpenGL；本次未用 shader，也未据此更换运行时。

同时查阅该提交的 [官方设计指南](https://github.com/longbridge/nsh/blob/84b2cf9195d00ba4e5dbfaca25473a959add1c54/skills/nsh-design-guides/references/design-guides.md)，并按固定源码核对 Text、Canvas、Input、Tabs、Menu 和 desktop 窗口 API。指南用于组件密度、材质和平台约束；视觉方向来自 [具体软件作品研究](../../../../docs/research/12-native-software-design-study.md)，不把指南视作已经完成的美术设计。
