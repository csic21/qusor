# SQLite 知识存储适配器

`SqliteCore` 直接实现 `KnowledgeQuery` / `KnowledgeCommand` / `KnowledgeArchive` 与 `StudyQuery` / `StudyCommand` / `StudyArchivePort` 六组拆分端口；`SqliteStore` 持有 `SqliteCore` 并保留 `#[deprecated]` 的 `KnowledgeStore` / `StudyStore` 兼容委派。常规读写使用 SQLite schema v6（含 `pdf_document_meta` / `pdf_capture_meta` 与 12 项分页/队列索引）与 `Immediate` 增量事务，不再通过全库快照或扫描 CAS blob 完成日常操作；实体、元数据、回执、审计、outbox/index job 与全局 `commit_log` 仍在同一事务内原子提交。CAS 与单 writer 排他锁由 [blob-store](../blob-store/README.md) 提供。

结构化错误保留来源类别与错误码标签（`[category=sqlite ...]`、`[category=io ...]` 等），不泄露本地文件系统路径或凭据。非 Unix 目标（`unsupported.rs`）实现完整端口集，显式返回 `KnowledgeErrorCode::Unavailable`（`[category=unsupported platform=non-unix]`），不用假实现替代。

[真实边界测试](tests/transactions.rs) 与 [规模与迁移测试](tests/scale_and_migration.rs) 直接执行 application 与 SQLite/CAS。见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md) 与 [ADR 0050](../../docs/architecture/adr/0050-split-storage-ports-and-incremental-transactions.md)。
