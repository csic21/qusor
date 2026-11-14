# ADR 0054：Core 分层清理与命名演进路线

- 日期：2026-11-14
- 状态：Accepted
- 关联：F-DATA-01、F-SRC-01、F-LEARN-01、ADR 0018、ADR 0019、ADR 0036、ADR 0048、ADR 0050、架构评审 P1-6

> [!NOTE]
> 权威范围：本 ADR 只记录架构与模块分层决策（背景、选项、决定、后果）。实现与验证状态统一写入 [项目状态表](../../status.md)，禁止在本文件中追加日期流水或测试计数日志。

## 背景

在 [2026-11-14 架构评审](../reviews/2026-11-14-architecture-review.md)（P1-6）中识别出生产 Core 与应用层存在以下结构混杂与命名漂移：

1. **`core-api` 契约层职责过重与错误模板重复**：
   - `core-api/src/knowledge.rs` 同时承载纯线缆 DTO、锚点/输入纯守卫，以及偏重持久化内部结构的整库归档审计、`outbox`/`index_job` 构造与 `commit_log` 全局一致校验逻辑。
   - `contract`、`pdf_contract`、`pdf_compound`、`epub_contract`、`vocab` 各自重复定义错误结构体、`Display`/`Error` 实现、有界 JSON 编解码与字符串/摘要校验辅助函数。
2. **`learning-application` 内部域边界混杂**：
   - 持久知识用例（`KnowledgeApplication`、PDF/EPUB 文库与笔记查询、不可变来源绑定 `annotation_source`）、交互式临时草稿/事件/投影会话状态机（`draft_text`、`annotation_draft`、`provider_events`、`provider_projections`）以及 FSRS-5 词汇学习用例（`study`）平铺在同一模块平面，缺少清晰域分层。
3. **命名与真实职责不匹配**：
   - `learning-domain` 仅包含 FSRS-5 调度内核，`learning-ports` 与 `learning-application` 同时承载“知识捕获/文库/笔记”与“词汇学习”。若在当前阶段立即对多个生产 crate 及跨工作区清单（`Cargo.toml`、`config/architecture.toml`、`apps/qusor/*`、`experiments/*`）执行整体重命名，会在多条并行演进主线同时落地期间引入大范围破坏性冲突。

## 选项

1. **方案 A：一次性全仓重命名 `learning-*` 为 `knowledge-*` / `study-*` 并彻底删除 `core-api` 旧归档入口**
   - 优点：依赖图与 crate 名称一步到位。
   - 代价：同时改动根工作区与独立工作区 `apps/qusor`、`experiments/*` 以及 `config/architecture.toml` 成员清单，破坏正在并行推进的宿主与插件迁出工作流。
2. **方案 B：先清理 `core-api` 与 `learning-application` 内部边界并保留兼容层，再分步推进 crate 拆分（采用）**
   - 优点：在不破坏 `apps/qusor` 与 `experiments/*`、零新增第三方依赖的前提下，先完成 `core-api` 错误基元统一、归档/`outbox`/`index_job` 核验下沉到 `learning-ports::archive`，并在 `learning-application` 内部建立 `knowledge/`、`session/`、`study/` 三层模块边界与渐进拆分路径。
   - 代价：过渡期内需保留 crate 根 `pub use` 与 `core_api::knowledge::validate_archive` 的 `#[deprecated]` 兼容垫片，待下游调用方迁移后再删除。

## 决定

采用**方案 B**，确立以下分层规范与演进路线：

1. **`core-api` 统一错误基元与纯契约边界**：
   - 在 `core-api/src/error.rs` 提供统一错误构造器、`define_contract_error!` 宏、`ContractWireError` 与共享有界 JSON 编解码/文本/摘要/字素边界辅助函数，消除 `contract`、`pdf_contract`、`pdf_compound`、`epub_contract`、`vocab` 的重复模板代码，并保持所有既有公开错误类型与错误码枚举兼容。
   - `core-api/src/knowledge.rs` 收敛为纯线缆 DTO（含 `ArchiveV2`/`ExportBundle` 线缆信封检查与 `BinaryBlob` 规范 Base64 编解码）、纯输入边界守卫（`validate_native`、`native_import`、`validate_note_input`）与锚点辅助（`capture_draft_hash`、`capture_anchor`）。
   - 将 `outbox`、`index_job`、操作载荷/摘要构造、`commit_log` 一致性及整库跨实体校验下沉至 `crates/learning-ports/src/archive.rs`（`learning_ports::archive`），并在 `learning_application::knowledge::archive` 重导出。`core-api::knowledge::validate_archive` 标记 `#[deprecated]` 并收敛为线缆信封结构校验垫片，供尚未迁移的独立工作区调用方（如 `apps/qusor/native` 备份解析前置校验）零破坏过渡；实际持久化恢复与存储适配层统一调用 `learning_ports::archive::validate_archive`。
2. **`learning-application` 内部三层模块边界**：
   - **`knowledge/`**（`KnowledgeApplication`、`pdf_library`、`pdf_notes`、`epub_library`、`annotation_source`、`archive`）：承载面向 Core 持久化端口的持久知识用例，不依赖任何交互式草稿或学习调度状态机。
   - **`session/`**（`draft_text`、`annotation_draft`、`provider_events`、`provider_projections`）：承载纯内存/会话级的临时编辑草稿、事件与投影状态机，单向依赖 `knowledge` 与 `core-api`，不得绕过 `KnowledgeApplication` 直接写库。
   - **`study/`**（`StudyApplication`）：承载 FSRS-5 词汇学习用例，只依赖 `learning-domain`、`learning-ports`（`StudyQuery`/`StudyCommand`/`StudyArchivePort`）与 `core-api::vocab`，与 `knowledge/`、`session/` 互不耦合。
   - 在 `learning-application/src/lib.rs` 保留原有模块名与 `KnowledgeApplication` 的 `pub use` 兼容导出。
3. **`learning-*` 渐进拆分与命名路线（后续执行）**：
   - **阶段 1（本 ADR 落位）**：保持 crate 物理名不变，在 `learning-ports` 中区分知识端口（`KnowledgeQuery`/`KnowledgeCommand`/`KnowledgeArchive`/`archive`）与学习端口（`StudyQuery`/`StudyCommand`/`StudyArchivePort`），在 `learning-application` 内部完成 `knowledge/` / `session/` / `study/` 模块解耦。
   - **阶段 2（下游切换）**：待 `apps/qusor` 与宿主插件桥接层全部迁至 `learning_application::{knowledge, session, study}` 与 `learning_ports::archive` 后，移除 `learning-application` 根兼容重导出及 `core_api::knowledge::validate_archive` 垫片，并视情况决定是否将 `core-api` 拆分为 `knowledge-contract` / `study-contract` 子模块或独立 crate。
   - **阶段 3（物理 crate 拆分）**：在确认独立工作区（`apps/qusor`、`experiments/*`）与 `config/architecture.toml` 可同步切换的窗口期，再将 `learning-domain` 重命名为 `study-domain`（或归并进 `study-core`），并将 `learning-ports` / `learning-application` 物理拆分为 `knowledge-ports` / `knowledge-application`（含 `session` 或独立 `draft-session`）与 `study-ports` / `study-application`。

## 后果

- **正面收益**：
  - `core-api` 剥离持久化内部 `outbox`/`index_job`/`commit_log` 核验细节，子模块错误定义与有界 JSON 编解码收归单一基元。
  - `learning-application` 在不破坏任何外部工作区的前提下建立清晰的 `knowledge` / `session` / `study` 分层边界，为后续物理拆分提供零歧义依赖方向。
- **代价与约束**：
  - 过渡期内 `core_api::knowledge::validate_archive` 与 `learning-application` 根重导出仍保留；根工作区内所有代码与测试禁止继续调用 `#[deprecated]` 入口。
  - 物理 crate 改名延后至阶段 3 窗口执行，不在此期间引入跨仓库破坏。
- **兼容性**：
  - 所有既有公开 DTO、错误码枚举、`apps/qusor/native` 与 `experiments/capture-core` 的编译和运行时契约保持完全兼容。
