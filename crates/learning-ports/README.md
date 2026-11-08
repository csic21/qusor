# 知识持久化端口

`KnowledgeStore` 提供一致完整快照、一次追加 `CommitPlan` 和空库恢复。一个提交包括实体、来源、回执、审计、outbox/index job 与全局 revision，不能通过逐表方法绕过原子性。

计划字段公开但不可信；实现必须重核归档、不可变旧前缀、单一追加、operation/hash 和预期 revision。没有 SQL、文件路径、NSH 或运行时类型。Host 授权与撤权仍在外层共享序列化门执行。

真实消费者是 [learning-application](../learning-application/README.md)，真实实现是 [storage-sqlite](../../adapters/storage-sqlite/README.md)。边界与限制见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。
