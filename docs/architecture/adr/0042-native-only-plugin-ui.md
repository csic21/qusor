# ADR 0042：插件 UI 只走原生 NSH Surface

- 日期：2026-11-12
- 状态：已确认产品方向；检查脚本与当前应用/插件源码通过，完整第三方原生 TSX UI 运行时仍待资格验证
- 关联：C-09、C-18、F-EXT-04、F-UX-01/02
- 承接：[ADR 0028](0028-extensible-native-workspace.md)、[原生插件 UI 研究](../../research/09-tsx-plugin-native-ui.md)

## 背景

用户明确表示不采用 dsh 的 Web Client 形态，也不接受插件集成后呈现网页 UI。dsh 的 profile/bundle、能力接缝和可逆注册值得借鉴，但其 Node/Electron/浏览器界面载体不是 QUSOR 的方向。

QUSOR 的 TSX 是 NSH 原生 retained-mode 描述：节点更新由 NSH 引擎转成原生视图/绘制与输入，不是浏览器 DOM；不能因为使用 JSX 就把插件 UI 解释为 WebView。

## 决策

1. **插件入口、工作区、工具栏、面板和组件 UI 只允许 NSH 原生 Surface/原生组件/声明式原生描述。**
2. **禁止把 WebView、iframe、HTML/CSS 页面、浏览器 DOM、Electron/Node 页面作为插件 UI 的默认或备选通路。**
3. **插件逻辑仍走普通签名 Wasm/受限执行；UI 若由插件提供，必须经受控原生 Slot/独立原生 Surface 和版本化 capability 接入。**
4. **当前阶段的应用工作区 renderer 是宿主原生组件**，按 Manifest contribution kind 选择；它是过渡落位，不把 Web UI 当插件能力。
5. **格式内容引擎例外不得扩张到插件外壳**：如需浏览器内核处理 PDF/EPUB 等文档内容，必须单独评审、单独留证，不能成为插件导航/工具/工作区 UI，也不能因此把应用变成 Web shell。
6. **检查可执行**：新增 `scripts/check_native_ui.py`，扫描 `apps/qusor` 与 `plugins` 的 TS/TSX、Rust 源码/依赖清单和 JSON 清单；拒绝 HTML/CSS 文件、WebView/DOM/浏览器宿主依赖以及 `web` 渲染器声明。

## 禁止项

- `nsh/web`、React DOM、Electron、Tauri、iframe 或 WebView 作为插件/工作区 UI；
- `document.*`、`window.*`、`innerHTML`、`outerHTML`、`dangerouslySetInnerHTML` 等 DOM/HTML 路径；
- 把下载的 HTML/JS 页面装成插件界面；
- 用浏览器开发服务器或网页路由表示原生工作区导航。

## 验证

- `python3 scripts/check_native_ui.py` 扫描当前应用与插件源码。
- 单元测试覆盖 WebView import、iframe、DOM/HTML 注入和 `.html/.css` 反例。
- `nsh check App.tsx` 继续验证 NSH 原生类型/构建；NSH 原生不代表每个节点都是系统标准控件，但也不是网页。

## 未验证

- 第三方原生 TSX UI 的独立 VM/Surface 挂载、资源释放、跨平台输入和权限；
- 需要浏览器内核的文档内容引擎是否允许、如何隔离；
- 现有 app-owned native renderer 何时替换为插件自带的原生贡献。
