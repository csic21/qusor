# ADR 0050 拆分存储端口与增量事务持久化

- 日期：2026-11-14
- 状态：Accepted
- 关联：F-DATA-01、F-READ-01、F-LEARN-01、ADR 0019、ADR 0036、ADR 0047、架构评审 P0-3 / P1-5

## 背景

原有存储端口（`KnowledgeStore` 与 `StudyStore`）把日常读写、管理变更新增方法和整库导出/恢复耦合在同一个 trait 中：

1. 正常用例必须通过 `snapshot()` / `study_snapshot()` 拉取全库状态与全部 CAS 原件 blob 再生成整体 `CommitPlan`，在数千条摘录或包含数 MB PDF 原件的资料库上造成 $O(N)$ 读取放大与无谓的磁盘/哈希校验开销。
2. `KnowledgeStore` 上多个后期新增方法带有默认 `unavailable()` 实现，编译器无法约束适配器是否真正覆盖所有契约。
3. 非 Unix 目标（`unsupported.rs`）缺失 `StudyStore` 与独立拆分端口实现，且 `storage-sqlite` / `blob-store` 的部分底层 SQLite / I/O 错误被压扁为固定字符串，缺少错误分类与来源代码；若直接透传底层字符串又存在泄露本地路径的风险。

## 选项

- **选项 A：保留单一 `KnowledgeStore` / `StudyStore`，仅在适配器内部增加 SQLite 查询缓存**。用例层依然被要求围绕整库快照与 `CommitPlan` 编写，无法从类型系统消除常规路径对 `snapshot()` 与原件 blob 的依赖，也无法解决只读消费者过度授权及 `unavailable()` 默认实现掩盖遗漏的问题。
- **选项 B：将端口拆分为正交的 Query / Command / Archive 三组六端口，适配器直接实现增量 SQL 与索引，并通过 `#[deprecated]` blanket 委派维持既有调用方源码兼容**。`learning-application` 常规读写只在对应端口切片约束下编译与运行，不再调用全库快照；`SqliteCore` 在单连接排他 writer 与单 `Immediate` 事务内保持原子语义不变。

## 决定

采用**选项 B**，核心设计如下：

1. **三组六端口职责边界（`crates/learning-ports`）**：
   - 只读查询：`KnowledgeQuery`（按 ID 精确查询、按 `vault` 有界 keyset 分页、单页文本与保存范围查询、仅显式调用时读取单份原件 `read_original_pdf`）与 `StudyQuery`（词书统计、卡片分页、学习队列、单卡复习上下文与余量计数）。常规查询方法一律不得触发整库快照或加载无关原件 blob。
   - 增量命令：`KnowledgeCommand`（文本捕获、PDF/EPUB 导入、PDF/复合/EPUB 选区捕获、笔记更新与级联删除）与 `StudyCommand`（词书幂等播种、带乐观守卫 `ReviewGuard` 的复习提交、词书删除）。所有默认 `unavailable()` 从主端口移除。
   - 归档端口：`KnowledgeArchive`（`is_empty`、`snapshot`、`restore`）与 `StudyArchivePort`（`study_snapshot`、`restore_study`），专供完整备份导出与空库恢复。
   - 用例约束收窄：`KnowledgeApplication<S>` 按方法所需分别约束 `S: KnowledgeQuery`、`S: KnowledgeCommand`、`S: KnowledgeQuery + KnowledgeCommand` 或 `S: KnowledgeArchive`；`StudyApplication<S>` 同理，去掉结构体级别的 `StudyStore` 约束。
2. **原子事务与单 writer 不变式证明点（`adapters/storage-sqlite` & `adapters/blob-store`）**：
   - **单 writer 互斥**：`BlobStore::open` 在整个 `SqliteStore` 生命周期持有根目录 `writer.lock` 的排他非阻塞文件锁，确保同一存储目录同时至多存在一个写连接来分配全局单调 `commit_rev`。
   - **单事务全量边表一致写入**：`SqliteCore` 的增量写命令全部包裹在 SQLite `TransactionBehavior::Immediate` 事务内；同一事务内原子更新实体表、`pdf_document_meta` / `pdf_capture_meta` 投影表、operation/`input_hash`、receipt、outbox、index_job、`knowledge_audit` 与全局 `commit_log`（含 `commit_rev` 递增），并继续保留 `StorageStage` 观察点以支持崩溃/故障注入测试。
   - **Blob 先落盘再提交**：涉及原件或文本快照 blob 写入时，先在 `BlobStore::publish` 完成 staging 写入、`fsync`、不可覆盖发布（Linux/Android 使用 `renameat2(RENAME_NOREPLACE)`，其它 Unix 使用 `hard_link`）及目录 `fsync`，再在 SQLite 事务中记录摘要引用，保证已提交记录绝不指向未耐久内容。
   - **打开阶段消去全量扫描**：`SqliteCore::open_inner` 仅检查 schema 版本/迁移、`PRAGMA quick_check(1)` 与 `PRAGMA foreign_key_check`，不再全库导出记录或遍历校验全部 CAS blob；仅当显式读取或幂等重放具体文本 capture 时才读取并校验该条目的单个 blob。
3. **SQLite Schema v6 迁移与索引**：
   - Schema 版本升至 `6`；`migrate_v6` 在 `Immediate` 事务内新增 `pdf_document_meta`（保存 `commit_rev` 与 `page_count`）与 `pdf_capture_meta`（保存 `document_id` 与 `commit_rev`），并从已有 `pdf_document` / `pdf_capture` 记录自动回填。
   - 新增 12 个覆盖全局 `commit_log`、PDF/EPUB 文档与笔记分页、`pdf_operation` 及 `vocab_card`/`vocab_schedule`/`vocab_review` 队列与统计的索引；v1–v5 旧库打开时顺序执行既有迁移并补全 v6 结构及外键检查。
4. **非 Unix 完整端口覆盖与无泄露结构化错误**：
   - `storage-sqlite` 与 `blob-store` 的 `unsupported.rs` 为非 Unix 平台实现完整的 `SqliteCore`、`SqliteStore`、`BlobStore` 及全部六组拆分端口与过渡兼容端口，统一返回 `KnowledgeErrorCode::Unavailable`（含 `[category=unsupported platform=non-unix]`），消除缺失 `StudyStore` 或局部编译缺口。
   - `storage-sqlite` 与 `blob-store` 的错误映射保持对外稳定 `KnowledgeErrorCode`，在消息尾附结构化类别标签（`[category=sqlite code=... extended=...]`、`[category=io kind=... os=...]`、`[category=json]`、`[category=utf8]`、`[category=integrity]`），绝不拼接本地文件路径、文件名或凭据内容。
   - 在 `BlobStore` 增加基于 `AtomicU64` 的 `read_count` / `reset_read_count` 观测计数，并在 `SqliteStore` 暴露同名辅助方法，供规模回归测试直接断言常规路径 blob 读取次数为 `0`。
5. **过渡兼容与后续清理**：
   - 保留 `#[deprecated]` 的 `KnowledgeStore` / `StudyStore` 与隐藏 `*_port()` 委派钩子及 blanket 实现，使未在本阶段直接改造的 `apps/qusor`（`VaultStore`、`LibraryReadBridge`、`NotesReadBridge`、`VocabBridge`）零改动继续通过 `SqliteStore::core()` 走增量路径；后续由壳层迁移直接依赖拆分端口，并在第二阶段（P1-6）配合 `core-api` 领域类型拆分一并移除兼容层。

## 后果

- **正面影响**：
  - 数千条摘录、数千张词汇卡片与多 MB PDF 共存时，分页列表、点查正文、保存范围查询、学习队列与单条笔记/复习写入均不再读取全库快照或原件 blob（CAS blob 读取计数为 `0`）。
  - 只读应用组件可仅凭 `KnowledgeQuery` / `StudyQuery` 完成编译与执行，缩小内部依赖面。
  - SQLite v5 及更早数据库无损升级至 v6 并自动拥有分页与队列索引。
  - 非 Unix 目标（如 `wasm32` / Windows）与 Unix 在端口契约上对齐，错误诊断不依赖猜测且不泄露本地路径。
- **代价与后续边界**：
  - 过渡期 `learning-ports` 同时维护六组拆分端口与 `#[deprecated]` blanket 兼容实现；`apps/qusor` 在未迁移前编译会产生 deprecation 提示，需后续由应用壳层切换至 `SqliteCore` 与拆分端口。
  - `ArchiveV2` 自身内部仍有既有 4096 条/64 MiB 导出与恢复预算（属归档格式语义，在第二阶段 P1-6 统一按领域拆分优化）。
  - 非 Unix 目标仅验证可编译性与显式 `Unavailable` 契约，各端原生持久化适配仍须按平台门槛独立交付。
