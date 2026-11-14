# 知识持久化端口

存储端口拆分为三组职责正交的 trait，不再把整库快照与常规读写混在一起：

- 只读查询：`KnowledgeQuery`（按 ID 点查、按 `vault` 有界 keyset 分页、单页文本/范围查询，常规查询不读原件 blob）与 `StudyQuery`（词书统计、卡片分页、学习队列与单卡复习上下文）。
- 增量事务命令：`KnowledgeCommand`（文本/PDF/复合/EPUB 捕获、导入、笔记更新与级联删除）与 `StudyCommand`（词书幂等播种、带乐观守卫 `ReviewGuard` 的复习提交与删除）。
- 归档端口：`KnowledgeArchive` 与 `StudyArchivePort`，仅用于完整导出与空库恢复。

适配器必须完整实现对应端口，不再依赖带默认 `unavailable()` 的方法掩盖遗漏。旧 `KnowledgeStore` 与 `StudyStore` 保留为 `#[deprecated]` 过渡兼容层（经隐藏 `*_port()` 钩子与 blanket 实现委派到拆分端口）。没有 SQL、文件路径、NSH 或运行时类型。Host 授权与撤权仍在外层共享序列化门执行。

真实消费者是 [learning-application](../learning-application/README.md)，真实实现是 [storage-sqlite](../../adapters/storage-sqlite/README.md)。边界、事务语义与拆分演进见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)、[ADR 0036](../../docs/architecture/adr/0036-vocabulary-study-capability.md) 与 [ADR 0050](../../docs/architecture/adr/0050-split-storage-ports-and-incremental-transactions.md)。
