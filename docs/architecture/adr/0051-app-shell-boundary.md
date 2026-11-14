# ADR 0051: 应用壳止血边界与功能迁出准备

- 日期：2026-11-14
- 状态：Accepted
- 关联：[2026-11-14 架构评审报告](../reviews/2026-11-14-architecture-review.md)、[ADR 0003](0003-plugin-first-delivery.md)、[目录架构](../workspace-layout.md)、[插件架构](../extensions.md)

## 背景

`apps/qusor` 在前期快速闭环中将大量插件应拥有的业务工作区、原生运行时与 Core 直调堆叠在应用壳内部，造成以下边界侵蚀（评审 P0-1）：

1. 应用代码多处写死 `"qusor-vault"` 与官方包/入口标识（`org.qusor.*`、`app.qusor.*`），`profile.rs` 与 `desktop-profile.json` 重复手工登记功能入口。
2. `library_native`、`notes_native`、`vocab_native`、`pdf_reader`、`app_info` 各自直接实例化 `KnowledgeApplication` / `StudyApplication`，使 Host 能力边界与业务功能直调混杂。
3. 前端界面集中在近七千行的 `contributions.tsx` 中，既包含应用壳组件与目录注册，又内联四个业务工作区及其跨模块私有状态读取，无法直接迁出为独立插件。

完整把业务功能与 UI 迁出到 `plugins/` 与 [ADR 0052](0052-plugin-ui-protocol.md) 的插件 UI 协议及第一方交付形态（1A）直接相关。本 ADR 先冻结并收口应用壳边界，消除持续腐化风险，并明确功能迁出顺序。

## 选项

- **选项 A：暂不改动 `apps/qusor`，等待插件 UI 协议与第一方形态全部拍板后一次性重构。**
  缺点：在此期间任何新功能仍会继续向应用壳和直调通道泄漏，增加后续拆迁成本。
- **选项 B：在不改变现有用户行为、不提前发明插件 UI 协议的前提下，先实施应用壳止血与结构准备。**
  把写死标识改为配置与清单驱动，把所有直调收口到单一可信门面并做静态基线冻结，将前端巨型文件按功能边界机械拆分并显性化跨功能耦合，再明确迁出顺序与对三项待决决策的依赖。

选择 **选项 B**。

## 决定

### 1. 止血规则（Accepted）

- **Vault 绑定**：由 `VaultStore::vault_id()` 统一持有当前 vault 标识；默认常量 `DEFAULT_VAULT_ID` 仅保留在 [vault.rs](../../../apps/qusor/native/src/vault.rs)，其余业务、门面与桥接不得写死 `"qusor-vault"`。
- **清单与服务驱动**：[package_catalog.rs](../../../apps/qusor/native/src/package_catalog.rs) 从已签名嵌入包 manifest 读取包与贡献元数据；[desktop-profile.json](../../../apps/qusor/desktop-profile.json) 仅声明基础宿主层，功能层入口由 [profile.rs](../../../apps/qusor/native/src/profile.rs) 根据 `package_catalog` 动态生成；[runtime.rs](../../../apps/qusor/native/src/runtime.rs) 按服务提供方与消费方解析结果动态挂载运行时。非测试 Rust/TSX 代码禁止按 `org.qusor.*` 或 `app.qusor.*` 分支（应用自身 NSH/Android JNI 类名 `org.qusor.app` / `org.qusor.app.QusorPlatform` 除外）。
- **静态守卫冻结基线**：新增 [scripts/check_app_boundary.py](../../../scripts/check_app_boundary.py) 对 `apps/qusor` 非测试源码执行只减不增的基线检查：
  - `KnowledgeApplication` / `StudyApplication` 仅允许出现在 [core_facade.rs](../../../apps/qusor/native/src/core_facade.rs) 与 `apps/qusor/bridges/src/*_bridge.rs`。
  - `core_facade.rs` 内的 `legacy_plugin_bypass` 模块冻结在现有 16 个待迁函数集合，禁止新增。
  - 禁止新增 `"qusor-vault"` 字面量、官方包/入口 ID 硬编码分支与 `workspaceRenderers` 新工作区类型键。

### 2. 可信 Core 服务门面边界（Accepted）

[core_facade.rs](../../../apps/qusor/native/src/core_facade.rs) 将应用壳对 Core 的访问划分为三层：

1. **授权 Host Wire Bridge 构造**（`new_library_bridge`、`new_notes_bridge`、`new_vocab_bridge`）：供 `Host::authorize_*` 与 `Host::dispatch` 消费的标准能力适配入口。
2. **Host 通用能力（长期留在可信应用/Host 边界）**：
   - 全库备份导出、预检与恢复（`export_backup_archives`、`inspect_backup_state`、`restore_knowledge_archive`、`restore_study_archive`）。
   - 用户选取原件的可信导入与引导（`import_pdf_document`、`import_epub_document`、`import_wordbook`、`ensure_starter_wordbook`），确保文件路径与原始字节解析不直接暴露给不受信任插件。
   - 原生 PDF 文档查询与原件字节读取（`query_pdf_document`、`read_original_pdf`），供应用层原生渲染与目录服务使用。
3. **待迁出插件直调（`legacy_plugin_bypass`，冻结基线，只减不增）**：
   - Library / Reader：PDF/EPUB 文档删除、EPUB 分页/目录/章节文本块/已保存高亮查询、EPUB 划词提交。
   - Notes：EPUB 笔记分页、EPUB 笔记正文分块、PDF/EPUB 笔记更新与删除。
   - Vocab：带每日新词上限的复习队列、词书词条分页、词书删除。

### 3. 前端模块物理边界（Accepted）

将 [contributions.tsx](../../../apps/qusor/contributions.tsx) 保留为兼容导出入口，按目录边界纯机械拆分为：

- 共享应用壳：[apps/qusor/shell/](../../../apps/qusor/shell/catalog.tsx)（`catalog.tsx`、`entries.tsx`、`editorial.tsx`、`opener.ts`）。
- 业务工作区：[apps/qusor/features/library/](../../../apps/qusor/features/library/LibraryWorkspace.tsx)、[apps/qusor/features/notes/](../../../apps/qusor/features/notes/NotesWorkspace.tsx)、[apps/qusor/features/reader/](../../../apps/qusor/features/reader/ReadingWorkspace.tsx)、[apps/qusor/features/vocab/](../../../apps/qusor/features/vocab/VocabWorkspace.tsx)。

拆分后显性保留的 4 处跨功能直接依赖（后续迁出前必须通过 Host 事件、扩展动作或应用壳上下文接口替换）：

1. Library 划词生词直接调用 `VocabHost.importWordbook` 与 Vocab 模块内部辅助函数（`vocabAnswer`、`loadVocabDecks`、`vocabFailureText`）。
2. Library 提交 PDF/EPUB 摘录后直接调用 `NotesWorkspace` 的 `refreshNotes`。
3. Notes 列表加载读取 `LibraryWorkspace` 的 `libraryOpenDoc` 以判定“当前资料”。
4. Reading 侧栏状态读取 `NotesWorkspace` 的 `notesSource`。

### 4. 迁出顺序（Accepted，2026-11-14 用户拍板确认）

确定第一方功能从 `apps/qusor` 按 **`词汇（Vocab）→ 笔记（Notes）→ 资料库/阅读（Library/Reader）`** 顺序基于声明式原生 UI 协议（[ADR 0052](0052-plugin-ui-protocol.md)）迁出，同时持续保持同一公开 SDK / Capability 契约与静态守卫：

1. **第一顺位：`vocab`（词汇学习）**：FSRS-5 调度逻辑已位于 `learning-domain`/`learning-application`，前端不依赖原生页图或复杂手势，仅存在 3 个冻结直调接口与 1 处来自 Library 的收词调用；将词书队列选项与删除补入标准化 Host 能力后，作为首个真实功能插件迁出闭环。
2. **第二顺位：`notes`（想法与笔记）**：只读部分已走 Host API 0.4，迁出前补全笔记编辑/删除能力及 EPUB 笔记统一查询，并用应用壳上下文/事件解耦 `libraryOpenDoc` 与 `refreshNotes`。
3. **第三顺位：`library` 与 `reader`（资料库、PDF/EPUB 阅读器）**：涉及可信文件导入、原生 PDF 渲染缓存、几何选文与大块文本阅读体验；将原件选择与原生渲染能力保留在 Host 通用文档服务（`ControlledSurfaceRef`）中，仅把工作区交互与数据操作迁入插件。

### 5. 与已拍板全局决策的衔接（Accepted，2026-11-14 用户拍板）

用户于 2026-11-14 拍板的三项决定与本 ADR 边界直接衔接如下：

1. **第一方交付形态（1A，[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)）**：允许第一方功能随应用同构编译或内置打包，但代码与权限必须走与第三方完全一致的公开 SDK / Capability / `UiTree` 契约，严禁私有直调或绕过 Core 事务直写 SQLite，由 `scripts/check_app_boundary.py` 与 `scripts/check_architecture.py` 持续守卫。
2. **iOS / Web 动态插件策略（2A，[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)）**：桌面（macOS/Windows/Linux）与 Android 支持签名 `.qpk`（Wasm）动态安装；iOS（App Store 版）与 Web 仅支持随包审核内置插件与纯声明式扩展，不开放任意外部可执行代码下载。
3. **插件 UI 路线（3A，[ADR 0052](0052-plugin-ui-protocol.md)）**：采用声明式原生 UI 树/Patch 协议 + `ControlledSurfaceRef` + `ui.slot`；`apps/qusor/features/*` 迁出时转为输出 `UiTree`/`UiPatch`，高复杂度受控视图（如 PDF 页图）通过宿主签发的 `ControlledSurfaceRef` 挂载。

## 后果

### 正面

- 消除非测试代码中写死 vault 名与官方包 ID 分支的问题，使包禁用与 profile 装配完全由 manifest 目录与服务依赖驱动。
- 应用壳所有 Core 旁路集中暴露在单一模块内，并由 CI/本地检查脚本锁定基线，防止后续迭代继续在壳内叠加直调与硬编码工作区类型。
- 前端四个功能工作区与壳层物理分离，跨功能耦合点被明确标注为可追踪的迁出清单。

### 代价与局限

- `legacy_plugin_bypass` 内的 16 个直调函数与 TSX 侧 4 处跨功能调用仍待按“词汇（Vocab）→ 笔记（Notes）→ 资料库/阅读（Library/Reader）”顺序基于声明式原生 UI 协议（[ADR 0052](0052-plugin-ui-protocol.md)）逐步清零并迁出应用壳。
