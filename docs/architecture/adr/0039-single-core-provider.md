# ADR 0039：应用级单一 Core Provider

- 日期：2026-11-12
- 状态：已实现并完成本机窗口切片；共享 SQLite/CAS 写者、旧分库迁移、Notes/Vocab 接线、隔离 HOME 下的打包应用启动/重开已通过；带图标正式包和其他平台未验证
- 关联：F-EXT-01/03、F-NOTE-01、F-LEARN-01、F-DATA-01
- 承接：[ADR 0038](0038-capability-composition.md)、[ADR 0019](0019-atomic-knowledge-persistence.md)、[ADR 0037](0037-production-app-assembly.md)

## 背景

生产应用的 NotesHost 与 VocabHost 分别打开 `data_dir/notes` 和 `data_dir/vocab`。两个目录各自有完整 schema，导致：

- 同一个应用内存在两个 writer 和两套导出范围；
- 阅读、笔记和词汇无法作为同一 vault 的数据被一致恢复；
- 在 composition 中只能描述两个孤立 Host，不能表达“共享 Core”这一真实依赖；
- 旧应用数据升级时没有明确的合并/保留策略。

把这些能力放进同一个 SQLite 文件不代表允许插件直接访问 SQLite。权限、会话、包验签和业务能力仍由 Host 控制。

## 决策

1. **应用拥有一个 `VaultStore`**：内部持有 `Arc<Mutex<SqliteStore>>`，实现既有 `KnowledgeStore` 与 `StudyStore` 端口；所有调用经同一锁序列化。它位于 `apps/qusor/native`，不作为插件可见类型。
2. **composition 增加 `app.qusor.core` provider**：提供 `vault.read`/`vault.write` 0.1。NotesHost 至少消费 `vault.read`，VocabHost 消费两者；应用启动时 `vault::shared` 也要求该 entry 处于启用状态。
3. **新数据目录为 `data_dir/core`**。首次启动时：
   - 已有 `core/core.sqlite`：直接打开；
   - 无旧数据：创建新 Core；
   - 存在旧 `notes/`、`vocab/`：读取其快照，按知识域和词汇域各选择一个非空来源，写入唯一 staging Core；
   - staging 通过 Core 端口恢复后，以目录 rename 发布为 `core`；
   - 旧目录不删除，保留为人工回退/审计副本。
4. **拒绝隐式冲突**：如果两个旧 Core 都含非空知识数据，或都含非空词汇数据，启动失败，不自动挑选“最后一个”。如果 `core` 目录存在但缺少 `core.sqlite`，也失败而不是覆盖。
5. **Host 权限不合并**：NotesHost 和 VocabHost 仍各自完成普通包验签、安装、授权、Session 和 wire 预算；共享的是 Core 存储端口，不是插件授权或 API 身份。
6. **数据写入仍经应用用例**：`VaultStore` 只实现端口，不绕过 `KnowledgeApplication`/`StudyApplication` 的 validation、revision 和幂等检查。没有插件可取得数据库路径、连接或 `VaultStore` 实例。

## 数据流

```text
ordinary package
  -> Host authorization/session
  -> notes-query-v1 / vocab-study-v1
  -> trusted bridge
  -> learning-application use case
  -> VaultStore (single mutex)
  -> SQLite schema 4 + CAS
```

## 后果

- Notes 与 Vocab 在同一个应用进程中共享一份 Core SQLite/CAS，写入按 store lock 串行。
- 现有用户首次升级会进行 staging 迁移；旧 `notes/`、`vocab/` 目录保留，便于排查。
- `NotesHost`/`VocabHost` 的静态 runtime 仍各自存在，但它们不再各自创建数据库。通用 MountRegistry/effect scope 仍待下一片。
- 迁移只在 Unix 私有存储 profile 上验证；Windows 仍需私有 ACL/SQLite 适配器资格。
- 多窗口并发打开同一应用数据目录、迁移期间外部进程持锁、跨版本真实升级尚未验证。

## 验证

应用原生测试覆盖（9 项，含 profile 解析、贡献目录和 Host 集成）：

- 同一 `VaultStore` 写想法和词汇，关闭重开后两者仍可读取；
- 首次启动只创建 `core/core.sqlite`，不创建新的分离目录；
- 旧 `notes/` 的知识和旧 `vocab/` 的词汇/复习记录进入同一新 Core；
- 旧目录仍存在；
- profile 中 `vault.read` 有唯一 provider，Host 依赖通过组合解析。
- 真实 `NotesHost`/`VocabHost` 初始化函数在旧分库数据上启动：两者都读取迁移后的同一 Core，CompositionHost 能列出 Core entry，旧目录仍在。
- 隔离 HOME 下的真实打包应用窗口：旧 `notes/`/`vocab/` 启动后生成 `core/core.sqlite`；想法显示迁移后的 1 条记录，背单词显示 `Legacy deck` 和 `core` 卡；关闭重开后两者仍可读取。记录见 [实窗迁移记录](../../../experiments/nsh-reader/evidence/core-migration/README.md)。

未覆盖：带图标的正式包（本机 `iconutil` 返回 `Invalid Iconset`，测试包临时省略 icon 且已恢复配置）、打包应用升级、旧数据库冲突的实际 UI 提示、长书/大库迁移性能、Windows 和其他平台。Host 集成测试直接传入 `System`，不能替代发行安装验收。
