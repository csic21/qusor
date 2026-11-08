# C4：知识用例与持久适配器生产落位

日期：2026-11-08。关联 C-08/C-09、F-LIB-01、F-READ-03、F-SRC-01、F-DATA-01、F-EXT-02/05/06。架构见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。

## 实际迁移

知识记录/归档纯不变量位于 core-api；learning-application 生成整体命令计划，经 learning-ports 交给 storage-sqlite/blob-store。实验 Core 只有兼容装配、测试故障观察与可信退出86，不再拥有 SQL/CAS 分叉实现。普通插件、审批/撤权 gate、迁移/回滚及完整恢复都执行同一生产路径。

保留文本 schema1、SQL schema2→3、PDF/复合归档 V2/V3 与旧 hash/JSON。插件 SDK 仍只导出既有捕获/锚点三模块，不包含 knowledge envelopes、应用、端口、SQL、CAS、Host 或运行器。

## 新增真实边界测试

[10项 SQLite/CAS 集成测试](../../adapters/storage-sqlite/tests/transactions.rs) 全部通过：

- 过期与一次新增多个 commit 的合法计划拒绝，无部分追加。
- 自洽修改旧 note/input hash/commit hash 的归档先通过纯关系校验，仍被不可变旧前缀检查拒绝。
- 错误 outbox 在发布 blob 前拒绝。
- 其他全局提交后同意图仍返回旧回执，修改输入冲突。
- 文本/导入/单段/复合共用 operation 身份，各类型跨混合提交幂等。
- 坏/缺已引用 blob 阻止查询与幂等重试。
- 恢复在实际 SQL 事务内再次检查空库。
- 事务开始前可信钩子将旧数据/hash改成另一份自洽归档，事务内完整重读检测变化并拒绝追加。
- 不存在、输入无效、引用冲突、不支持版本与预算失败具有稳定分类。

根工作区71项测试（旧61+新增10）、全目标Clippy、格式检查通过。实验原41项Host/Core/真实进程崩溃/迁移恢复测试通过；42项架构守卫回归通过。

## 完整实际通路复验

完整 `python3 experiments/capture-core/validate_compound.py` 已退出0，通过314条命令、286项CLI场景与8项native场景，含原有31次CLI真实退出86、普通签名SDK仓库外构建/Clippy、schema1/2/3迁移、撤权与批准竞争、生命周期后知识保留和完整导出/空库恢复。当前72项源码指纹与实测开始/结束及当前文件完全一致。

见 [本轮结构化证据](evidence/linux-persistence-migration.json)、[完整当前C3证据](evidence/linux-compound-pdf.json)、[根71项测试日志](evidence/linux-persistence-root-tests.log) 和 [42项守卫日志](evidence/linux-persistence-architecture-tests.log)。完整C3仍分为契约26项在根、Host/Core41项在实验；本轮新增10项生产原子边界测试不混入原67项计数。

## 限制

耐久运行仅 Linux via WSL 本机验证。Unix文件实现与依赖隔离编译；非Unix明确返回Unavailable，未验证平台不由根Core CI自动认证。大资料库仍需增量读取/查询及实际性能预算，此切片沿用资格实验的有界完整快照校验。正式安装器、library/reader/notes独立包、NSH PDF/EPUB阅读和输入/无障碍仍按计划分别推进。
