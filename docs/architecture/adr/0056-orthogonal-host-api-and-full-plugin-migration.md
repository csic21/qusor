# ADR 0056: Host API 0.8 正交能力路由与第一方全工作区插件迁出

- 日期：2026-11-14
- 状态：Accepted
- 关联：[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0049](0049-cumulative-host-api-and-capability-evolution.md)、[ADR 0050](0050-split-storage-ports-and-incremental-transactions.md)、[ADR 0051](0051-app-shell-boundary.md)、[ADR 0052](0052-plugin-ui-protocol.md)、[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)、[ADR 0054](0054-core-layering-and-naming.md)、[extension-api::orthogonal](../../../crates/extension-api/src/orthogonal.rs)、[extension-host::router](../../../crates/extension-host/src/router.rs)、[orthogonal_bridge.rs](../../../apps/qusor/bridges/src/orthogonal_bridge.rs)

## 背景

在完成 [ADR 0049](0049-cumulative-host-api-and-capability-evolution.md) 第一部分（Host API `0.1`–`0.7` 累积兼容）、[ADR 0050](0050-split-storage-ports-and-incremental-transactions.md) 六端口拆分、[ADR 0051](0051-app-shell-boundary.md) 应用壳止血及首个 Wasm `ui.slot` 插件 `plugins/vocab` 迁出后，仓库仍保留三项阶段三收口任务：

1. **正交能力协议与通用路由尚未落位**：`notes`、`library` 与 `reader` 的编辑、删除、EPUB 目录/分块查询与摘录提交尚未收敛为正交协议，`extension-host` 仍需落地 [ADR 0049](0049-cumulative-host-api-and-capability-evolution.md) 第二部分规划的命名空间正交能力、每能力独立 Semver 协商与 `ExtensionRouter`。
2. **`#[deprecated]` 存储兼容层尚未清除**：`apps/qusor/native` 与 `experiments/capture-core` 仍保留对已弃用的聚合 trait `KnowledgeStore` / `StudyStore` 及 `core_api::knowledge::validate_archive` 的引用或实现。
3. **剩余三个第一方工作区仍在应用壳内**：`NotesWorkspace.tsx`、`LibraryWorkspace.tsx`、`ReadingWorkspace.tsx` 仍留在 `apps/qusor/features/`，且 `core_facade.rs` 仍保留 13 个 `legacy_plugin_bypass` 旁路函数与 3 个 `FROZEN_WORKSPACE_RENDERER_KINDS` 特例。

## 选项

- **选项 A：继续为每个剩余工作区扩充 `0.7` 业务专属枚举，或在壳内保留 EPUB/笔记编辑旁路。**
  缺点：违反 [ADR 0049](0049-cumulative-host-api-and-capability-evolution.md) 冻结新增业务专属枚举的决定，且无法将 `legacy_plugin_bypass` 与 `FROZEN_WORKSPACE_RENDERER_KINDS` 清零。
- **选项 B：落地 Host API `0.8` 正交能力与 `ExtensionRouter`，全面清理 `#[deprecated]` 存储调用，并将 `notes`、`library`、`reader` 全部迁出为独立 Rust `wasm32-unknown-unknown` `ui.slot` 插件（采用）。**

## 决定

### 1. Host API `0.8`、正交能力与 `ExtensionRouter`

1. **协议与清单扩展（[orthogonal.rs](../../../crates/extension-api/src/orthogonal.rs)）**：
   - 新增 `HOST_API_V0_8`（`0.8`）与 `manifest_version: 2`。
   - 定义 7 项标准命名空间正交能力（`OrthogonalCapabilityId`）及每能力独立版本区间协商（`OrthogonalCapabilityRequirement` → `SessionCapabilityTable`）：
     - `core.knowledge.read`、`core.knowledge.write`
     - `core.study.read`、`core.study.write`
     - `core.capture.propose`
     - `host.ui.slot`、`native.document.render`
   - `manifest_version: 1` 的既有清单通过 `Manifest::effective_orthogonal_requirements()` 自动只读投影为上述正交能力需求，保持 `0.1`–`0.7` 包完全向后兼容。
2. **统一正交知识协议（`CORE_KNOWLEDGE_PROTOCOL = 1`）**：
   - 以 `CoreKnowledgeRequest` / `CoreKnowledgeResponse` 承载跨 PDF/EPUB 的 11 类正交知识与文档操作（`ListPdfDocuments`、`DeletePdfDocument`、`ListEpubDocuments`、`DeleteEpubDocument`、`EpubToc`、`EpubSectionBlocks`、`EpubSavedSelections`、`ProposeEpubSelection`、`ListEpubNotes`、`ReadEpubNoteChunk`、`UpdateNote`、`DeleteNote`），由 `Host::authorize_core_knowledge` 按操作所需正交能力（`core.knowledge.read` / `core.knowledge.write` / `core.capture.propose`）精确判权。
   - 在 [orthogonal_bridge.rs](../../../apps/qusor/bridges/src/orthogonal_bridge.rs) 实现 `OrthogonalKnowledgeBridge`，直接对接 `KnowledgeApplication` 的增量查询与命令端口。
3. **通用扩展点路由器（[router.rs](../../../crates/extension-host/src/router.rs)）**：
   - 在 `extension-host` 落位 `ExtensionRouter`，基于提供者/消费者声明、协商后的 `SessionCapabilityTable` 与有界帧配额执行委托权限交集校验与不透明消息帧路由。

### 2. 全面消除 `#[deprecated]` 存储接口调用

- 移除 [vault.rs](../../../apps/qusor/native/src/vault.rs) 与 `experiments/capture-core` 中对已弃用 `KnowledgeStore` / `StudyStore` 的实现，统一使用拆分后的 `KnowledgeQueryPort`、`KnowledgeCommandPort`、`KnowledgeArchivePort`、`StudyQueryPort`、`StudyCommandPort`、`StudyArchivePort`。
- 将 [app_info.rs](../../../apps/qusor/native/src/app_info.rs) 等归档校验调用统一迁至 `learning_ports::archive::validate_archive`。

### 3. 四个第一方工作区全部迁出为独立 Wasm `ui.slot` 插件

1. **仓外独立构建的 `.qpk` 插件**：
   - 新增并登记 [plugins/notes/Cargo.toml](../../../plugins/notes/Cargo.toml)（`org.qusor.notes`）、[plugins/library/Cargo.toml](../../../plugins/library/Cargo.toml)（`org.qusor.library`）、[plugins/reader/Cargo.toml](../../../plugins/reader/Cargo.toml)（`org.qusor.reader.preview`），全部采用 `manifest_version: 2`、`host_api: 0.8` 与 `ui_slot` 贡献，经 `export_sdk.py` 与各插件 `build.py`（如 [plugins/notes/build.py](../../../plugins/notes/build.py)）在仓外隔离编译为 `wasm32-unknown-unknown` 并签名打包为 `apps/qusor/native/assets/{notes,library,reader}.qpk`。
   - [sdk/qusor-sdk](../../../sdk/qusor-sdk/src/lib.rs) 新增 `OrthogonalKnowledgeClient`（通过 `qusor.host_call` 的 `"core.knowledge"` 通道调用 Host `0.8` 正交协议）与语义 `UiTree` 构造辅助。
2. **受控文档表面与宿主槽位**：
   - 新增 [ControlledDocumentSurface.tsx](../../../apps/qusor/shell/ControlledDocumentSurface.tsx)，由 [PluginSlotView.tsx](../../../apps/qusor/shell/PluginSlotView.tsx) 在渲染插件 `ControlledSurfaceRef`（`ControlledSurfaceKind::ReaderDocument`）时挂载，负责宿主受控的 PDF 页图渲染、几何选文与 EPUB 章节展示，文件选择器（PDF/EPUB 原件导入）保留在可信宿主壳层。
3. **删除壳内业务工作区与旁路归零**：
   - 删除 `apps/qusor/features/{notes/NotesWorkspace,library/LibraryWorkspace,reader/ReadingWorkspace}.tsx`。
   - 删除 [core_facade.rs](../../../apps/qusor/native/src/core_facade.rs) 中的 `legacy_plugin_bypass` 模块（13 个遗留函数全部清零）。
   - 将 [check_app_boundary.py](../../../scripts/check_app_boundary.py) 中的 `FROZEN_LEGACY_BYPASS_FNS`（`16 -> 13 -> 0`）与 `FROZEN_WORKSPACE_RENDERER_KINDS`（`3 -> 0`）收紧为空集。

## 后果

### 正面

- 四个第一方工作区（词汇、笔记、资料库、阅读）全部统一为真实 Rust `wasm32-unknown-unknown` + 声明式原生 `UiTree` + `ControlledSurfaceRef` 的独立 `.qpk` 插件，应用壳内不再包含任何业务工作区实现或私有写库旁路。
- `check_app_boundary.py` 的遗留豁免集合彻底归零，任何尝试向 `apps/qusor` 重新塞入业务工作区 renderer 或 Core 旁路函数的修改都会在静态检查阶段直接失败。
- Host API `0.8` 正交能力与 `ExtensionRouter` 为后续第三方扩展与跨插件服务协作提供了统一的权限交集与 Semver 协商基座。

### 约束与后续工作

- 四个 Wasm 插件已全部通过无界面真实 `.qpk` Wasm → `PluginSlotHost` → SQLite Core 端到端测试（`plugin_slot_{vocab,notes,library,reader}_e2e`）；桌面真窗口视觉/手势交互与 Android 模拟器全流程仍需在 GUI 环境中复测并补充截图证据。
