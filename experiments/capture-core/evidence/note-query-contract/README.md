# 想法读取契约记录

日期：2026-11-10。关联 C-08/C-09/C-12、F-NOTE-01、F-SRC-01、F-READ-03、F-DATA-01。决策见 [ADR 0034](../../../../docs/architecture/adr/0034-note-read-model.md)，阶段见 [D 阶段计划](../../../../docs/product/d-phase-plan.md)。

## 本轮范围

把此前“整库导出 + 200 条截断 + 512 bytes 预览”的想法列表升级为可继续的只读读取契约：

- 列表 keyset 分页，排序键为 `(commit_rev DESC, capture_id DESC)`；游标 `n1:<document|*>:<commit_rev>:<capture_id>` 绑定 vault/document 精确范围，分页期间新增想法不会造成重复或跳项。
- 正文按 `Quote` / `Note` 分块读取，块大小 4–16 KiB；offset 必须是 UTF-8 边界，块结束于不超上限的最近字符边界，`next_offset = None` 表示结束。
- 缺失 capture、跨 vault、越界 offset、非边界 offset、越界块大小分别返回明确错误；跨 vault 与不存在统一 `NotFound`，不泄露存在性。

## 实际验证

| 范围 | 结果 |
|---|---|
| 应用层分页/正文测试 | 10 项集成测试 + 4 项模块测试通过，含并发新增下的分页稳定性、范围绑定游标拒绝、UTF-8/NUL/emoji、1 MiB 笔记 64 块还原、错误分类 |
| SQLite 关闭重开 | 真实 SQLite 保存长中文想法后重开，列表、预览、`total_bytes` 与完整正文分块一致 |
| 根 Workspace | 233 项 Rust 测试、Clippy、格式、42 项架构守卫、174 份 Markdown 链接检查通过 |
| 实验消费者 | `experiments/nsh-reader/native-host` 按新分页接口编译通过（未重新构建/启动原生窗口） |

机器记录见 [record.json](record.json)。

## 未覆盖

- 公开 SDK wire、版本协商与插件授权；分页/正文读取尚未进入公开贡献契约。
- 原生窗口的“新保存想法→关闭辅助区→列表刷新→打开所在页”与精确高亮回源。
- 撤权、缺原件、文档版本变化、迟到结果、删除/更新竞争。
- 长资料库性能：每次查询仍读取整库快照，本轮不把它当作大库性能资格。
- 中文 IME、辅助技术、其他平台与原生资源释放 ACK。
