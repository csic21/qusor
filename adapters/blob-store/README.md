# 不可变文件 CAS

私有原件/快照按 SHA-256 寻址，校验真实普通文件、长度与摘要。发布顺序为 staging 写入 → 文件 `fsync` → 不可覆盖发布（Linux/Android 使用 `renameat2(RENAME_NOREPLACE)`，其它 Unix 使用 `hard_link`）→ 目录 `fsync`。`BlobStore` 提供 `read_count()` / `reset_read_count()` 以便规模测试断言常规路径零原件读取，并在错误中保留 `[category=io kind=... os=...]` 等结构化上下文而不暴露路径；非 Unix 目标提供显式返回 `Unavailable` 的 `unsupported.rs`。

同一根目录的 `writer.lock` 保持整个 SQLite 连接生命期排他，防止多 writer 分别分配全局 revision。真实消费者为 [storage-sqlite](../storage-sqlite/README.md)，设计边界见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md) 与 [ADR 0050](../../docs/architecture/adr/0050-split-storage-ports-and-incremental-transactions.md)。
