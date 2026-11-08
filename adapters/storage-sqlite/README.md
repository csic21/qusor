# SQLite 知识存储适配器

`SqliteStore` 实现 `KnowledgeStore` 的一致读取、单一追加和空库恢复。复用已验证的 schema1/2/3、全局 commit-log、实体/回执/审计/outbox/index 原子事务；CAS 与独占 writer 由 [blob-store](../blob-store/README.md) 提供。正式接口报告稳定错误与耐久观察事件，不包含进程终止代码。

适配器不信任公开 `CommitPlan`：完整归档校验之外，还验证旧实体与 commit 前缀不变、旧 operation/blob 保留、恰新增一种实体和一个 commit，并在事务内重新检查 operation/revision。恢复在事务内重新检查空库。外部插件没有此私有适配器权限。

[真实边界测试](tests/transactions.rs) 直接执行 application 与 SQLite/CAS；旧捕获实验仍运行完整插件/迁移/故障/恢复验证。Linux via WSL 为当前实际耐久平台；Unix实现隔离编译，非 Unix 显式 Unavailable，不计为其他平台通过。见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。
