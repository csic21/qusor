# 插件 UI 协议设计与 NSH 运行时核对（P1-4）

- **状态**：Proposed（待用户确认「插件 UI 路线」拍板；关联 [ADR 0052](adr/0052-plugin-ui-protocol.md)）
- **关联需求与约束**：P1-4、C-07、C-09、C-12、C-15、C-18、F-EXT-01/03/04/05/06、F-UX-01/02
- **隔离原型**：[experiments/plugin-ui](../../experiments/plugin-ui/RESULTS.md)
- **前置参考**：[架构评审 2026-11-14](reviews/2026-11-14-architecture-review.md)、[UI 组合规格](../design/composition.md)、[TSX 原生插件 UI 研究](../research/09-tsx-plugin-native-ui.md)、[ADR 0023](adr/0023-public-reader-surface.md)、[ADR 0028](adr/0028-extensible-native-workspace.md)、[ADR 0038](adr/0038-capability-composition.md)、[ADR 0042](adr/0042-native-only-plugin-ui.md)

---

## 1. 设计目标与约束边界

QUSOR 要求所有核心用户功能与第三方扩展都通过统一、可验证、可卸载的插件体系交付，且坚持原生 UI 优先与无 WebView 插件禁令（C-09 / C-18 / [ADR 0042](adr/0042-native-only-plugin-ui.md)）。当前宿主应用 `apps/qusor` 将工作区渲染器集中写在第一方壳层（`apps/qusor/contributions.tsx` 中的 `workspaceRenderers`），尚未向仓库外独立插件开放标准 UI 协议。

任何正式插件 UI 协议必须同时满足：

1. **边界可守卫**：插件不能绕过宿主直接创建窗口、注入 `WebView`/HTML/DOM、读取未授权本地路径或写 Core SQLite。
2. **契约与权限正交**：插件展示区域由 [UI 组合规格](../design/composition.md) 与 [ADR 0028](adr/0028-extensible-native-workspace.md) 定义，通过 [capability-composition](adr/0038-capability-composition.md) 校验槽位与提供者生命周期；UI 挂载权不等于知识库读写权或文件系统权限。
3. **跨六端与上架可行**：覆盖 C-15 的 macOS、Windows、Linux、iOS、Android、Web 六端，并评估 App Store 对第三方可执行脚本下发的审核风险。
4. **生命周期隔离**：支持宿主切换、热替换、错误熔断与卸载清理，旧代际（`generation`）的补丁与事件必须被硬拒绝。

---

## 2. NSH 上游源码与发布核对（2026-11-14）

按 `AGENTS.md`「NSH 更新跟踪」规范，设计前已核对官方仓库 `https://github.com/longbridge/nsh` 与当前生产固定版本：

- **核对日期**：`2026-11-14`
- **当前固定版本**：`rev 913fd76e1f8434ec6cdf248f4476c1ebe08af19f`（[apps/qusor/native/Cargo.toml](../../apps/qusor/native/Cargo.toml)）
- **上游最新动态**：
  - 最新正式 Release：`v0.5.0`（`2026-11-11`，领先固定 commit 368 个提交），新增多窗口 `<Window>`/`WindowRef` 模型与 `capabilities.windows` 权限项、媒体加载 `fs.read`/`${pluginDir}` 强制校验、签名应用/整包 OTA 更新及移动商店发布工具 `nsh publish`。
  - 最新 Nightly 与主干：Nightly tag `2d3f04b42777f247de2ba0155c651f555c1e1dc9`（`2026-11-14`）；`main` HEAD `3bf8affe15f1057ca251fb17af286e560e2b88aa`（含 PR `#1954` `ce85e1fc`，限制 JS decoder 读取深度与长度）。
- **对本决策的影响**：`v0.5.0` 的签名 OTA 面向整包应用级更新，而不是应用内细粒度第三方 UI 槽位沙箱；同时下面 5 项结构性限制在固定基线与 `main` 上仍存在，因此**暂不因候选 B 触发升级 NSH，升级与否需在隔离实验中另行验证**。

### 2.1 五项关键能力核对结论与源码证据（基于 `913fd76`）

| 核对问题 | 结论 | 直接证据与限制 |
| --- | --- | --- |
| **1. 多个隔离 JS 上下文** | 5 个原生平台支持多 `Engine`/`AppView` 各自持有独立 QuickJS 线程与 VM；但共享进程空间且桌面 `AppView` 共用全局组件集 | `nsh-engine/src/lib.rs` 为每个 `Engine` 启动独立线程（`ENGINE_STACK_SIZE = 8 MiB`）与 `nsh-js::Host`；`nsh-js/src/sandbox.rs` 提供 `Limits`（默认 256 MB 堆 `HeapAllocator`、4 MB 栈、10s 加载、2s/call 中断、200,000 节点）及 `Policy::harden`（删除 `eval`、禁止构造 `Function`/`AsyncFunction`/Generator、冻结内建原型）。**限制**：(a) 均在同一 OS 进程内，原生组件 panic 或 VM 外分配仍波及宿主；(b) `nsh-platform-{macos,linux,windows}/src/app.rs` 的 `AppView::load` 硬编码进程全局 `ui_sets: ui_sets()` 与 `context: Default::default()`（尽管 `EngineConfig` 字段本身是 per-engine 的），桌面无法对不同子 `AppView` 下发不同组件白名单；(c) `apps/qusor` 主壳自身运行于 NSH TSX（`"rust": "native"`），在 TSX 中嵌套子 `AppView` 须走 `#[native_components(<platform>)]`，而 `docs/dev/embedding.md` 明确 Web 忽略/不支持 `nativeComponent`。 |
| **2. 限制可用 API** | 宿主 Rust 侧可强制裁剪能力矩阵 `Access = declared ∩ granted`，JS 标准模块目录仅 11 个 bare specifier | `nsh-core/src/access.rs` 覆盖 `network`、`webview`、`statusBar`、`process`、`fs`、`clipboard`、`picker`、`speech`、`notifications`、`audio`、`host`（`host.call` 函数白名单）、`openUrl`、`windows`；`node:fs`/`child_process` 不可导入。**注意**：若 manifest `capabilities` 省略/为空，默认会回落为宽松权限 `Capabilities::all()`，宿主必须显式构造最小 `granted`（如 `Capabilities::default()` 仅开指定 `host` 函数）。 |
| **3. 从包加载 TSX/JS bundle** | 可通过 `Bundle::source(code)` 加载编译后 JS；**禁止加载第三方 `.nshc` 字节码** | `docs/dev/signed-packages.md` 明确指出 QuickJS **不会校验输入字节码**，`Bundle::bytecode` 与 `Bundle::read_trusted` 均为 `unsafe`。第三方插件 `.qpk` 只能分发经包签名验真的 JS 源码并在本机通过 `Policy::harden` 装载编译（或经过可信本机转译检查）。 |
| **4. 原生组件集合** | 原语 + `nsh/base` 覆盖阅读、表单、树、表格、Dock、编辑器 | `docs/dev/elements.md` / `components.md` 与 `crates/nsh-base/src/lib.rs`（`parts::ALL`）列出核心原语（`View`, `Text`, `Scroll`, `Input`, `Overlay`, `Image`, `List`, `Canvas`, `Svg`, `Shader`/`ShaderView`, `WebView`, `Video`, `NativeView`, `Stack`/`Screen`/`Modal`, `InputAccessoryView`, `If`/`Each`）与 `nsh/base` 组件包（`core`, `pickers`, `color_picker`, `carousel`, `tree`, `fields`, `text_view`, `editor`, `diff`, `rich_editor`, `data_table`, `table`, `dock`, `resizable`, `plot`, `term`）。注意 `WebView` 必须对第三方插件绝对禁止（C-18）。 |
| **5. iOS / Android / Web 的 JS 引擎形态** | iOS/Android 均内嵌 QuickJS 解释器（无 JIT）；**Web 不存在 JS 沙箱隔离** | `nsh-platform-ios` 与 `nsh-platform-android` 使用同一 `nsh-js`（`rquickjs` -> `quickjs-jit = 0.14.2`，纯 C 字节码解释器，不依赖系统 `JavaScriptCore` / V8，也不开启 JIT）；`NshAppView`（Swift）与 `NshView`（Java）支持 `Limits` 与 `HostChannel`。但 `nsh-platform-web` 在构建期去掉了 `nsh-js`/QuickJS，直接在同源 `iframe` 内的浏览器引擎中运行 ES module；`docs/dev/web.md` 明确警示：*“The iframe is not a security sandbox: standalone code has the page origin's authority. Do not use it for untrusted plugins.”* |

---

## 3. 候选方案详细设计

### 3.1 候选 A：自定义声明式原生 UI 树协议 + 受控宿主槽位

#### 核心思路

插件的 UI 状态机与交互逻辑留在现有单一插件沙箱内（Wasm 运行时 / 宿主托管工作线程），插件**不直接持有 NSH 运行时，也不直接操纵原生对象或运行第三方 JS**。插件只向宿主输出版本号受控的纯数据 `UiTree` / 增量 `UiPatch`；宿主校验通过后，在对应 [ADR 0028](adr/0028-extensible-native-workspace.md) 区域中将其映射为原生 NSH 组件渲染；用户点击或输入时，宿主通过带 `mount_id` 与 `generation` 的 `UiEventEnvelope` 回调插件。

```text
+-----------------------------------+        UiTree / UiPatch (JSON/CBOR/WIT)        +-------------------------------------+
|   Plugin Sandbox (Wasm / Worker)  | ---------------------------------------------> |     PluginUiHost (Rust Validator)   |
|  - plugin state & view projection |                                                |  - whitelist + token + a11y + budget|
|  - handles UiEventEnvelope        | <--------------------------------------------- |  - atomic patch + generation fence  |
+-----------------------------------+     UiEventEnvelope(mount_id, generation)      +------------------+------------------+
                                                                                                        |
                                                                                                        v  mapped to native NSH + nsh/base
                                                                                     +-------------------------------------+
                                                                                     |   apps/qusor SlotRenderer + Theme   |
                                                                                     |   & ControlledSurfaceRef Broker     |
                                                                                     +-------------------------------------+
```

#### 关键协议约束（已在 [experiments/plugin-ui](../../experiments/plugin-ui/src/schema.rs) 原型验证）

1. **协议版本与严格结构（`PLUGIN_UI_PROTOCOL = 1`）**
   - 顶层快照 `UiTree { protocol_version, revision, root }`，增量补丁 `UiPatch { protocol_version, mount_id, generation, base_revision, next_revision, ops }`。
   - 所有 Wire 类型严格拒绝未知字段（`#[serde(deny_unknown_fields)]`），任何非白名单字段（如注入 `style: "color:#f00"` 或 `html`）在反序列化阶段直接失败。
2. **组件白名单（`UiComponent`）与原生禁令**
   - **布局与容器**：`VStack`、`HStack`、`Scroll`、`Card`、`Section { title }`、`Split { ratio_percent }`（限定 `10..=90`）、`List`、`ListItem`。仅容器型节点可携带 `children`。
   - **展示与反馈**：`Text`、`Badge { label, tone }`、`Divider`、`EmptyState`、`KeyValue`、`Progress { completed, total }`（校验 `completed <= total`）。
   - **交互控件**：`Button`、`IconButton`（仅限宿主白名单图标名）、`TextInput`、`Toggle`、`Select`、`Tabs`。
   - **受控原生复杂 Surface 接缝**：`ControlledSurfaceRef { surface_id, surface_kind }`（例如 `ReaderDocument`、`PdfPagePreview`、`MarkdownPreview`），句柄只能由宿主在插件通过相应 Capability 检查后签发，绝不接受原始路径或 URL（对齐 [ADR 0023](adr/0023-public-reader-surface.md)）。
   - **硬排除项**：不提供 `WebView`、`Html`、`Script`、`Shader`、任意外部图片 URL、`NativeView` 逃逸接口。
3. **语义主题 Token 与布局规模**
   - 样式字段 `NodeStyle` 只接受 `SemanticToken`（完整对齐 `apps/qusor/theme.json` 的 22 个语义角色，如 `surface_base`、`surface_toolbar`、`surface_control`、`text_primary`、`text_accent`、`border_subtle`、`focus_ring`、`primary` 等）、有界 `SpacingScale`（`none | xs | sm | md | lg | xl`）、`TypographyRole` 与 `compact_hide`。
   - 插件无法传十六进制色值或像素级绝对坐标，确保系统浅色/深色主题切换、高对比度与窄栏折叠由宿主统一兜底。
4. **节点、深度与载荷预算（`UiBudget` + 系统硬上限）**
   - 即使宿主 profile 传入更高配置，`UiBudget::clamp_to_ceiling()` 也强制夹紧在系统硬上限以内：
     - `HARD_MAX_PAYLOAD_BYTES = 64 KiB`
     - `HARD_MAX_NODES = 512`
     - `HARD_MAX_DEPTH = 16`
     - `HARD_MAX_CHILDREN = 128`
     - `HARD_MAX_TEXT_BYTES = 64 KiB`
     - `HARD_MAX_PATCH_OPS = 256`
     - `HARD_MAX_EVENT_BINDINGS = 256`
   - 对大列表/长文档场景，协议要求结合分页数据游标或宿主虚拟化列表/受控 Surface，而不是一次把上万节点塞进单棵 `UiTree`。
5. **无障碍门禁（`AccessibilityProps`）**
   - `Button`、`IconButton`、`TextInput`、`Toggle`、`Select`、`Tabs` 等所有交互节点必须提供非空 `accessibility.label`（其中 `IconButton` 绝不允许缺失可读标签）以及显式 `accessibility.role`（`AccessibilityRole`）。
   - 宿主将语义标签、状态标志（`selected`、`expanded`、`busy`）与禁用态直接映射至 NSH 平台无障碍属性，并将键盘焦点环统一定向到宿主可访问的焦点管理器。
6. **原子 Patch 与代际隔离（Generation Fencing）**
   - `MountedSlot::apply_patch` 在内存副本上执行整批 `UiPatchOp`（`ReplaceTree`、`SetProps`、`InsertChild`、`RemoveNode`、`ReplaceSubtree`），执行后全量复检预算、唯一节点 ID、文本与无障碍约束；任何单条失败立即丢弃工作副本，保持原有 `active_tree` 与 `revision` 完整回滚，并累计 `fault_count`（超过阈值可熔断该槽位并展示宿主故障恢复卡）。
   - `replace_provider` 按 [UI 组合规格](../design/composition.md) 流程执行：冻结输入 -> 清空旧树 -> `generation += 1` -> 切换提供者 -> 恢复接收。旧代际迟到的 `UiPatch` 与 `UiEventEnvelope` 均返回 `StaleGeneration`。

---

### 3.2 候选 B：NSH TSX/JS 受限运行时（隔离 QuickJS `AppView`）

#### 核心思路

沿用 [09-tsx-plugin-native-ui.md](../research/09-tsx-plugin-native-ui.md) 的思路，让第一方和第三方插件继续使用 TSX 编写 UI，但运行时按插件槽位隔离：

1. **包与装载格式**：插件 `.qpk` 签名包携带转译后的 JS 源码（禁止第三方分发 `.nshc` 字节码，因为 `Bundle::bytecode` 无校验且 `unsafe`）；宿主以 `Bundle::source` 挂载并强制启用 `Policy::harden` 与 `Limits`。
2. **宿主权限收敛**：每个槽位创建独立 `AppView` / `NshAppView` / `NshView` 与 `HostChannel`（`nsh/host`），Rust 宿主显式构造受限 `Access`（禁止 `network`、`fs`、`process`、`webview`、`openUrl`、`windows` 等，只开放经审核的 `host.call` 白名单函数）。
3. **补齐当前缺失的 4 项前置条件（缺一不可）**：
   - 修改或绕过 `nsh-platform-{macos,linux,windows}/src/app.rs` 中 `AppView::load` 硬编码全局 `ui_sets()` 的问题，使得子 `AppView` 只注册不含 `WebView`、`NativeView`、`Shader` 的裁剪版 `nsh/base`；
   - 在 `apps/qusor/native` 注册 `#[native_components(...)]` 子视图桥接组件，将子 `AppView` 嵌入主壳 TSX 的 [ADR 0028](adr/0028-extensible-native-workspace.md) 槽位区域，并自建宿主-子视图焦点/快捷键/尺寸协同；
   - 解决 `nsh-platform-web` 同源 `iframe` 非安全沙箱问题（要么 Web 端禁止加载第三方候选 B 插件，要么为 Web 另建一套跨源沙箱/postMessage 协议，从而破坏六端一致性）；
   - 建立「业务 Wasm 沙箱 + UI QuickJS VM」双运行时通信桥与每个活动槽位 ≥8 MiB 线程栈 + QuickJS VM 内存预算管理。

---

## 4. 六维对比评估（A vs B）

| 评估维度 | 候选 A：声明式原生 UI 树协议 | 候选 B：NSH TSX/JS 受限运行时 |
| --- | --- | --- |
| **1. 安全隔离** | **强**：UI 侧零动态脚本执行，只有严格白名单的数据树与增量 patch；业务逻辑继续留在单一 Wasm 隔离层内；不触发 QuickJS 同进程组件集共享、`.nshc` 字节码不安全风险，也不受 Web 同源 `iframe` 无沙箱问题影响。 | **中（有结构性缺口）**：原生 5 端虽有 QuickJS `Limits` + `Policy::harden` + `Access` 交集，但与宿主同进程、桌面 `AppView::load` 共用全局 `ui_sets()`、`AppView` 子视图嵌入需要平台原生桥接，且 **Web 端明文无安全沙箱**。 |
| **2. iOS 审核风险** | **极低**：插件仅输出静态 Schema 描述的 UI 数据结构，宿主本地原生组件渲染；如果连业务逻辑运行在纯解释 Wasm 上，UI 层完全不构成动态下发/JIT 可执行代码或热更新独立 JS 应用。 | **中-高**：App Store Guideline 2.5.2 / 3.3.2 虽对无 JIT 解释脚本（QuickJS）存在边界灰区，但把完整独立 TSX/JS bundle 作为可下载第三方扩展界面运行时，审核解释成本、行为审查不可预测性及拒审风险显著高于纯数据声明树。 |
| **3. 开发体验** | **前期中等、长期统一**：第三方开发者只需使用单一语言与 SDK（如 Rust/`qusor-sdk` 编译 Wasm，未来可叠加声明式 DSL 宏），无需同时维护 Wasm 业务代码与 TSX JS 包及两套调试/序列化链路；缺点是无法直接使用自由排版 TSX 语法与任意社区 npm JSX 生态。 | **TSX 直观，但双栈割裂**：写 UI 接近 React/TSX 习惯；但如果业务逻辑在 Wasm 而 UI 在 QuickJS，开发者需在两种语言/工具链/状态同步协议间来回桥接；如果干脆让第三方逻辑全进 QuickJS，又违背当前 Wasm 插件沙箱与无 JS 核心业务路线。 |
| **4. 第一方迁移成本** | **可控、可渐进**：第一方现有 `apps/qusor/contributions.tsx` 属于壳层渲染器（`workspaceRenderers`）；迁出独立第一方插件（阅读/词汇/笔记）时，需通过协议映射层对接宿主白名单组件。宿主侧可先保留内置受控 Surface（如 PDF 页图/富文本编辑器）作为 `ControlledSurfaceRef` 复用，避免重写高复杂度绘制组件。 | **近期最低、长期反增**：近期最接近 `apps/qusor/contributions.tsx` 现有写法；但长期必须改造 `AppView` 子视图嵌套、`ui_sets` 裁剪、焦点路由以及为 Web 端另写一套安全隔离渲染器，基座维护成本反而更重。 |
| **5. 六端可行性（C-15）** | **六端一致**：协议校验与状态机为纯 Rust（`no_std`/Wasm 友好），渲染由已适配六端的 `apps/qusor` 原生宿主完成；Web 端直接由 QUSOR Web 宿主按相同白名单组件渲染，不依赖浏览器沙箱运行第三方代码，无架构分裂。 | **五端有条件、Web 断层**：macOS/Windows/Linux/iOS/Android 可跑 QuickJS，但桌面须改 `ui_sets` 隔离且依赖 `#[native_components]` 子视图嵌入；Web 端 `nsh-platform-web` 明确不支持 `nativeComponent` 且 `iframe` 无沙箱，**无法用同一方案覆蓋六端第三方插件**。 |
| **6. 与 capability / Slot 注册衔接（ADR 0028）** | **原生契合**：`SlotDescriptor`（区域、角色、最小宽度、紧凑模式）直接作为 `capability-composition` 的参与条目与 `ui.slot@1.0` 能力绑定；宿主完全掌控容器尺寸、替换冻结、`generation` 围栏与错误熔断，不发生插件跨区域越界。 | **需要额外胶水治理**：子 `AppView` 有独立内部窗口/事件循环与布局上下文，需要额外在桥接层同步 ADR 0028 替换冻结态、裁剪越界浮层（`Overlay`/`Modal`）、抑制 `<Window>` 逃逸并拦截跨视图快捷键抢占。 |

---

## 5. 与 Capability / Slot 注册的衔接规范

协议将 UI 槽位与功能能力解耦为正交契约，直接复用 `crates/extension-api` 与 `crates/capability-composition`：

1. **宿主能力服务定义**：宿主壳在组合层注册必选平台条目 `app.qusor.ui.host`，提供核心服务 `ui.slot@1.0`（当引入新控件版本时遵循 `major` 精确匹配、`min_minor` 向后兼容规则，与 [ADR 0038](adr/0038-capability-composition.md) 一致）。
2. **插件 Manifest 声明**：插件除声明业务权限（如 `Capability::ReadLibrary`、`Capability::WriteNotes`）外，声明一个或多个 `SlotDescriptor`：
   - `contribution_id`：符合 `extension-api` 命名规范的点分小写标识符（如 `org.qusor.vocab.study-card`）；
   - `region`：[ADR 0028](adr/0028-extensible-native-workspace.md) 与 [UI 组合规格](../design/composition.md) 定义的 8 个受控区域之一（`ShellWorkspaces`、`WorkspaceToolbar`、`WorkspaceLeading`、`WorkspaceContent`、`WorkspaceAuxiliary`、`WorkspaceContextActions`、`WorkspaceObjectStatus`、`WorkspaceOutput`）；
   - `role`：槽位语义角色（如 `study.flashcard`、`inspector.summary`）；
   - `min_width_pt` / `supports_compact`：窄屏或侧栏折叠降级提示。
3. **单一默认提供者与冲突处理**：对需要单一默认提供者的 `(region, role)`（例如主阅读编辑器或工作区主内容卡），组合器将 `slot.<region>.<role>@1.0` 建模为提供服务；`capability-composition` 会在多插件同时宣称同一默认角色时返回确定性提供者冲突错误，由用户或 profile 显式二选一，杜绝静默覆盖。
4. **受控 Surface 正交授权**：插件 UI 树中嵌入 `ControlledSurfaceRef` 时，必须同时满足：(a) 插件已被授权对应数据/Surface 能力（如 `reader.surface` / `pdf.page_view`）；(b) `surface_id` 由宿主 broker 签发并绑定当前会话租约（[ADR 0023](adr/0023-public-reader-surface.md)）；插件仅决定该表面在槽位布局中的位置占位，无法直接读取像素或拦截未授权输入。

---

## 6. 推荐结论与后续演进路径

- **推荐路线**：推荐 **候选 A（声明式原生 UI 树协议 + 受控宿主槽位）** 作为 QUSOR 阶段三及第三方插件生态的主协议路线；候选 B 保留为备选对比方案（若后续确有必要，仅可在原生端作为内部/特许实验受限通道讨论，而不宜作为六端通用第三方插件标准）。
- **推荐核心理由**：
  1. 唯一能够在 C-15 全六端（尤其 Web 端）提供同等安全边界的方案；
  2. 将 App Store 审核风险与多 VM/线程内存膨胀降到最低；
  3. 与现有 Wasm 插件宿主模型、`capability-composition`、`extension-api`、`ControlledSurfaceRef`（ADR 0023）和区域槽位模型（ADR 0028）无缝衔接；
  4. 已在 [experiments/plugin-ui](../../experiments/plugin-ui/RESULTS.md) 完成协议 Schema、白名单、语义 Token、预算裁剪、无障碍门禁、原子 Patch 回滚与代际隔离的可运行原型验证。
- **待用户确认边界**：路线选型属于产品与架构重大拍板项（详见 [评审报告](reviews/2026-11-14-architecture-review.md)），本设计与 [ADR 0052](adr/0052-plugin-ui-protocol.md) 当前状态保持 `Proposed`，待用户正式确认后再纳入生产 `extension-api` / `apps/qusor` 装配工作。
