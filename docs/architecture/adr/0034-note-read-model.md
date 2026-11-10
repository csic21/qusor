# ADR 0034：想法读取的分页游标与正文分块

日期：2026-11-10。状态：应用层读取模型已实现并通过本机测试；公开 SDK wire、插件授权与原生窗口接线尚未完成。关联 C-08/C-09/C-12、F-NOTE-01、F-SRC-01、F-READ-03、F-DATA-01。承接 [D 阶段计划](../../product/d-phase-plan.md)、[ADR 0018](0018-core-api-contract-source.md) 与 [ADR 0019](0019-atomic-knowledge-persistence.md)，不扩大 AI、云或平台范围。

## 问题

D0 要求稳定的公开想法查询契约。此前的 `pdf_notes` 一次导出整个 Core 快照、返回最多 200 条并截断预览：列表条数、预览长度都可能被界面当成产品上限，且没有读取完整正文的入口。把完整正文塞进列表会突破传输预算，也会让 UI/插件持有完整知识副本。分页若用 offset 或“已取条数”，并发保存新想法后会出现重复、跳项或重排；正文若按固定字节切分，会把中文、emoji 或组合字符切断。

## 决定

想法读取仍是只读查询：不写 Core、不签发权限、不解析锚点、不宣称页面定位。读取模型放在 `learning-application`；`core-api` 继续只做捕获/来源/锚点权威源。公开 SDK 的 wire 映射与权限属于 D1，不因本轮提前冻结。

### 列表分页

- 排序键固定为 `(commit_rev DESC, capture_id DESC)`，两者都是 Core 不可变事实。
- 单页最多 200 条；这是单次传输预算，不是资料条数上限。后续页通过 `next_cursor` 继续，`None` 表示已到末页。
- 游标是不透明、规范的字符串：`n1:<document|*>:<commit_rev>:<capture_id>`。解析拒绝非规范十进制、非正数、多余段和未知版本。
- 游标绑定精确范围：vault 由查询参数决定，`document` 段必须与本次 `document_id` 完全一致；把游标用于其他 vault、其他 document 或伪造位置都返回 `InvalidInput`，不静默降级。
- 采用 keyset 而非 offset：游标指向已返回的最后一项，后续页从该不可变位置之后开始，因此分页期间新增想法不会造成重复或跳项；新想法只出现在重新开始的第一页。
- 预览仍为每条最多 512 UTF-8 bytes，并按字符边界截断；预览只影响返回文本，不改变 Core 中完整引文与想法。

### 正文分块

- 字段显式区分 `Quote`（保存的原文）与 `Note`（用户想法）；复合选区的原文取不可分割的完整 `exact` 文本。
- 调用方提供 `offset` 与 `max_bytes`；`offset` 必须落在 UTF-8 边界，`max_bytes` 限制在 4..=16 KiB。四字节下限保证即使下一个字符是四字节 emoji，每个非末块也至少前进一个标量，不会空转。
- 块结束于不超过上限的最近字符边界；返回 `total_bytes` 与 `next_offset`，`next_offset = None` 即正文结束。空正文和 `offset == total_bytes` 返回空块。
- 越界 offset、非边界 offset、越界块大小分别返回 `InvalidInput` 与 `BudgetExceeded`；其他 vault 或不存在的 capture 统一返回 `NotFound`（不区分，避免泄露跨库存在性）；capture 存在但 document 缺失属于 `Integrity`。
- 每次调用仍从同一已验证 Core 快照读取，不引入数据库连接、SQL、NSH 或插件对象；当前整库快照端口是既有存储边界，本轮不改变它，也不把本机 1 MiB 结果外推为大资料库性能资格。

## 边界与恢复

- 查询不产生写入、审计、outbox 或索引任务，因此没有需要恢复的部分提交；快照验证失败仍按 `Integrity` 返回。
- 预览、分页与正文块都不缓存：每次调用重新读取 Core 事实，避免界面缓存与撤权/更新竞争时出现“看似已授权”的旧副本。
- 游标不携带授权或身份；它只描述位置，能否查询仍由调用链的 vault 范围决定。未来插件接入时，授权由 Host/Core 公开能力决定，游标不能替代权限检查。
- 该模型不承诺精确回源：`page_index` 只是列表提示；精确高亮和受控 Surface 定位仍须 D0 后续单独验收。

## 交付门槛

先通过应用层与真实 SQLite 重开测试，再接公开 contribution/查询契约与普通 notes 包，最后在真实窗口走通“新保存想法→关闭辅助区→列表刷新→打开所在页”。已覆盖的检查：分页稳定性（并发新增）、vault/document 范围隔离、UTF-8 边界与 NUL/emoji、1 MiB 正文分块、错误分类、SQLite 关闭重开。未覆盖：公开 SDK wire、插件授权、分页 UI、精确高亮、删除/撤权/更新竞争、长库性能、其他平台。

本机结果见 [想法读取契约记录](../../../experiments/capture-core/evidence/note-query-contract/record.json)。
