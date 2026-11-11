# Core 公开数据契约

本 crate 是捕获、来源、文本/PDF 锚点契约的唯一权威源，承接 [ADR 0018](../../docs/architecture/adr/0018-core-api-contract-source.md)。从已通过 C0–C3 的实验迁入真实实现，未改变 JSON、定位单位、hash 或协议版本；不表示生产存储或阅读器已经完成。

- `contract`：文本快照、来源描述、选段/定位、版本1捕获请求。
- `pdf_contract`：原件/提取版本、几何/字符映射、版本1 PDF 捕获。
- `pdf_compound`：版本2有序复合摘录、整组 hash 与单请求预算。
- `vocab`：词书/卡片/当前 FSRS 安排/追加复习历史，导出、恢复与一致性校验，见 [ADR 0036](../../docs/architecture/adr/0036-vocabulary-study-capability.md)。

不依赖 Host、SQLite、文件系统适配器、Wasm 或 NSH。宿主策略 wire 仍由 extension-api 单独管理；包/API/选段/归档版本不能混用。对不可信 JSON 使用有界 `from_slice` 等入口；直接反序列化并不代替 `validate`。原始 PDF 引擎字符流不声称是所有排版的阅读顺序。

[实验 SDK 导出器](../../experiments/capture-core/export_sdk.py) 从这里生成完整可仓库外构建的工件。实验旧模块仅转发此实现，不能再维护分叉。正式 SDK 发布、Core API 的持久用例入口和完整产品 UI 仍未交付。


`knowledge` 另提供持久知识/回执/归档 envelopes 与纯关系校验，`error` 提供稳定失败分类，由 [应用用例](../learning-application/README.md) 和 [SQLite适配器](../../adapters/storage-sqlite/README.md) 共同消费，见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。规范知识 envelope 与 SQL schema 独立；core-api 没有数据库/文件操作。

当前作者 SDK 明确导出 `contract`、`pdf_contract`、`pdf_compound` 三模块子集；不向作者导出 knowledge/error/vocab、端口或存储实现，也不因迁移扩大插件能力。插件读取学习数据通过 extension-api 的 `vocab.read`/`vocab.review` wire，不直接依赖本 crate 的 vocab 类型。SDK 仍通过仓库外完整构建证明所选模块与依赖闭包自包含。
