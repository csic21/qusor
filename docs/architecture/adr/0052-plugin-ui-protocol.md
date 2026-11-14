# ADR 0052：插件 UI 协议与受控原生槽位路线

- 日期：2026-11-14
- 状态：Accepted
- 关联：P1-4、C-07、C-09、C-12、C-15、C-18、F-EXT-01/03/04/05/06、F-UX-01/02、[ADR 0023](0023-public-reader-surface.md)、[ADR 0028](0028-extensible-native-workspace.md)、[ADR 0038](0038-capability-composition.md)、[ADR 0042](0042-native-only-plugin-ui.md)、[ADR 0051](0051-app-shell-boundary.md)、[插件 UI 协议设计](../plugin-ui-protocol.md)、[隔离实验结果](../../../experiments/plugin-ui/RESULTS.md)

## 背景

[2026-11-14 架构评审](../reviews/2026-11-14-architecture-review.md) 将「缺少插件 UI 协议」列为 P1-4 关键缺口：当前 `apps/qusor` 将工作台渲染器集中硬编码于第一方壳层（`workspaceRenderers`），尚未为仓库外独立插件提供可验证、可卸载、跨六端一致且满足无 WebView 禁令（[ADR 0042](0042-native-only-plugin-ui.md)）的扩展界面协议。

设计前于 `2026-11-14` 核对 NSH 上游（当前固定 `rev 913fd76e1f8434ec6cdf248f4476c1ebe08af19f`，并对照 `v0.5.0` 与 `main 3bf8affe`）表明：

1. NSH 原生五端可通过 `Engine` / `AppView`（iOS `NshAppView`、Android `NshView`）创建隔离 QuickJS（`quickjs-jit` 解释器，无 JIT）线程并施加 `Limits`、`Policy::harden` 与 `Access = declared ∩ granted`；但仍位于同一 OS 进程，`nsh-platform-{macos,linux,windows}/src/app.rs` 的 `AppView::load` 硬编码共享进程级 `ui_sets()`，无法对单个子 `AppView` 裁剪 `nsh/base` 白名单。
2. QuickJS 不校验外部字节码，`Bundle::bytecode` 与 `Bundle::read_trusted` 为 `unsafe`，第三方包只能加载受签名验证的源码并现场受限编译。
3. NSH Web 端（`nsh-platform-web`）在编译期裁剪 `nsh-js`，改由同源 `iframe` 直接运行浏览器 JS，官方 `docs/dev/web.md` 明确声明该 `iframe` **不是安全沙箱**且 `docs/dev/embedding.md` 明确忽略自定义原生嵌套组件 `nativeComponent`。

## 选项

详细技术规范与六维对比见 [插件 UI 协议设计](../plugin-ui-protocol.md)，核心路线如下：

- **选项 A（推荐）：自定义声明式原生 UI 树协议 + 受控宿主槽位**
  - 插件业务逻辑与 UI 投影留在既有单沙箱（Wasm / 宿主受控执行器）内，仅输出版本化、拒绝未知字段的 `UiTree` 与增量 `UiPatch`；宿主在 `PluginUiHost` 完成组件白名单、语义 Theme Token、尺寸/节点/深度硬上限、无障碍属性门禁、原子 Patch 回滚与 `generation` 代际隔离校验后，映射为原生 NSH 与 `nsh/base` 组件渲染，回传代际绑定事件 `UiEventEnvelope`。
  - 需要渲染受保护文档或原生页图时，插件在通过对应数据授权后向宿主申请不透明句柄 `ControlledSurfaceRef`（延续 [ADR 0023](0023-public-reader-surface.md)），而非直接访问本地路径或 `WebView`。
  - 槽位注册复用 [ADR 0028](0028-extensible-native-workspace.md) 八大区域模型与 [capability-composition](0038-capability-composition.md)（引入 `ui.slot@1.0` 及可选单提供者互斥服务 `slot.<region>.<role>`），与业务数据能力正交。
- **选项 B：NSH TSX/JS 受限运行时（每槽位隔离 QuickJS `AppView`）**
  - 插件分发签名 JS 源码，由宿主为槽位拉起独立 `AppView` / `NshAppView` / `NshView` + `Policy::harden` + `Limits` + 最小 `Access` + `nsh/host` 通道。
  - 若作为通用第三方协议采用，必须先解决桌面 `AppView::load` 共享 `ui_sets` 缺陷、构建跨平台 `#[native_components]` 子视图嵌套与焦点管理、应对双运行时（Wasm 业务 + QuickJS UI）状态同步与每槽位引擎线程内存成本，并为 Web 端缺失安全沙箱的架构断层提供额外隔离方案。

## 决定

**用户于 2026-11-14 正式拍板选定候选 A（3A：声明式原生 UI 树协议 + 受控宿主槽位）作为 QUSOR 插件 UI 主协议路线，并确定应用壳业务功能迁出顺序为 `词汇（Vocab）→ 笔记（Notes）→ 资料库/阅读（Library/Reader）`（[ADR 0051](0051-app-shell-boundary.md)）。**

选定候选 A（声明式原生 UI 树 / `UiPatch` + `ControlledSurfaceRef` + `ui.slot`）的核心依据：

1. **六端安全模型一致**：不依赖浏览器同源 `iframe` 执行第三方代码，满足 C-15 全六端同等边界与 2A 平台合规策略（[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)）。
2. **iOS 审核与进程面风险最小**：UI 契约只传输纯数据描述与受控宿主句柄，不动态下发独立第三方可执行界面脚本或字节码，同时避免多 QuickJS VM 膨胀。
3. **单一插件工具链与正交治理**：第三方插件统一围绕 Wasm + `qusor-sdk` 开发与签名分发，第一方按 1A 走同一公开契约，并通过 `capability-composition`、`extension-api` 与 `ControlledSurfaceRef` 保持区域可控、可替换、可回滚与可卸载。

隔离原型（[experiments/plugin-ui](../../../experiments/plugin-ui/RESULTS.md)）作为生产晋升依据进入 `extension-api::ui`、`extension-host::ui` 与宿主通用声明式渲染器实现。

## 后果

### 正面影响

- 彻底消除第三方插件注入 `WebView`/HTML/原始 CSS/未授权图片 URI 与越权创建窗口的通道。
- 将主题切换、字体缩放、窄栏折叠、无障碍角色与焦点环统一收敛在宿主壳层，保证多插件拼装体验一致。
- 与 [ADR 0028](0028-extensible-native-workspace.md) 替换冻结语义及 [ADR 0023](0023-public-reader-surface.md) 受控 Surface 句柄模型形成统一代际围栏。

### 代价与约束

- 宿主层需维护声明式组件白名单到 NSH 原生组件及 `nsh/base` 的映射渲染器，并在未来逐步扩展复杂容器与虚拟化列表能力。
- 第一方现有工作台 UI（`apps/qusor/contributions.tsx`）拆分为独立功能插件时，需迁移至声明式协议或通过受控 `ControlledSurfaceRef` 保留宿主内置高复杂度视图（如 PDF 页图）。
- 候选 B 不作为通用插件 UI 路线采纳；若未来在纯原生局部实验场景重新评估子 `AppView`，须单独立项解决 `ui_sets` 隔离、嵌套焦点与 Web 安全断层并不得破坏既有公开契约。
