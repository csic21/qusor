# 不可变文件 CAS

私有原件/快照按 SHA-256 寻址，校验真实普通文件、长度与摘要。发布顺序为 staging 写入 → 文件 fsync → 不可覆盖 hard-link 发布 → 目录 fsync；未引用 blob 可在中断后残留，已提交引用不能指向尚未耐久的内容。

同一根目录的 writer lock 保持整个 SQLite 连接生命期排他，防止多 writer 分别分配全局 revision。平台文件权限、NOFOLLOW/NONBLOCK 与目录操作封装在 Unix实现内。当前实测 Linux via WSL，不声明 macOS/Windows/移动耐久通过；其他平台仍须完成对应实现。

真实消费者为 [storage-sqlite](../storage-sqlite/README.md)。边界见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。
