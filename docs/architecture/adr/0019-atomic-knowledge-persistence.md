# ADR 0019：知识用例与原子持久化适配器落位

日期：2026-11-08。状态：采用，验证结果见下文。关联 C-08/C-09、F-LIB-01、F-READ-03、F-SRC-01、F-DATA-01、F-EXT-02/05/06。

## 问题

ADR 0018 已将公开选段契约迁入 core-api；实验 Core 仍将来源不变量、SQL、CAS、Unix 锁和故障终止混在同一实现。直接搬入 application 会让业务依赖数据库；复制第二套存储会让实验无法证明生产实现。

## 决定与实际依赖

- `core-api::knowledge` 持有现有知识记录/回执/归档 DTO、输入 hash 与纯关系校验；`error` 提供稳定错误分类。保留 schema1/2/3、归档 V2/V3、旧 JSON 和 hash 算法。
- `learning-ports` 提供一致知识快照、完整追加计划和空库恢复端口。没有暴露 SQL transaction、文件路径或逐表 insert。
- `learning-application` 通过端口编排文本/PDF/复合捕获、导入、查询、定位和完整导出恢复；无 SQL、文件系统、Host、NSH 或运行时依赖。写用例从一份快照生成实体、回执、审计、outbox/index 与全局 commit。
- `storage-sqlite` 保留唯一 SQL schema/迁移、完整关系核对与一次事务；`blob-store` 保留私有目录、独占 writer、不可变 CAS 和 fsync。SQLite 连接先于 writer lock 释放。
- 实验 `core` 成为 `KnowledgeApplication<SqliteStore>` 的兼容装配门面，不再保留第二份 SQL/CAS。已有普通插件、审批、撤权门和恢复验证直接执行生产实现。

依赖为 application → ports/core-api；SQLite → ports/core-api/blob-store；blob-store → core-api。SQL/CAS 细节不会反向进入 application。学习 domain 仍零依赖，不为本次迁移生成空业务层。

## 原子性与不可信计划

`CommitPlan` 的公开字段不构成权限或可信证明。适配器重新校验整份归档：旧实体/commit 前缀不变，旧 operation/blob 保留，恰新增一种实体与一个全局 commit；自洽但篡改历史、批量追加和过期 revision 都拒绝。事务内再次核对全局 revision 和 operation 唯一性，随后一次提交实体、来源、回执、审计、outbox、index job 和 commit-log。

blob 在 SQL 引用前先写 staging、fsync、不可覆盖发布并 fsync 目录。失败允许未引用 blob，不允许已提交引用指向未发布内容。读取、同 operation 重试及新提交前均核对已引用 blob 和数据库关系；幂等依据原业务输入 hash，不把稍后分配的实体 ID/revision 算成新意图。跨文本/导入/PDF/复合类型共用 operation 身份。

Host 的撤权/批准与最终提交共享序列化门保持；拆层没有在判权与提交之间释放 gate。application/store 不接受插件自行声明的批准，插件仍不能获取私有数据库或适配器。

## 故障与用户反馈

生产仅报告 `StorageStage` 观察事件，不接受 crash 字符串、不调用 `process::exit`。真实杀进程注入留在实验门面的可信回调中。正式错误区分 InvalidInput、NotFound、Conflict、Integrity、BudgetExceeded、UnsupportedVersion、Unavailable 和 Storage；UI 根据分类呈现重试、来源损坏或版本/冲突状态，不匹配错误文本。

文件私有权限/NOFOLLOW/耐久路径在 Unix 实现内编译隔离。当前实测仅 Linux via WSL；非 Unix 构造明确返回 Unavailable，未假装 Windows 耐久通过。macOS/Windows/移动端依然须各自原生适配与验证，长期平台目标不缩减。

## 验证与当前限制

新增10项真实 SQLite/CAS 边界测试覆盖过期计划、自洽篡改历史、批量追加、坏审计提交前不发布 blob、混合全局 revision 后幂等/跨类型冲突、坏/缺 blob、恢复事务内空库复核和错误分类。原实验 Host/Core、迁移、真实进程终止和恢复测试复用本实现；根71项测试、实验41项、42项架构守卫回归均通过；完整C3与SDK仓库外验证再次通过314条命令、286项CLI及8项native场景，72项源码指纹当前一致，见 [迁移结果](../../../experiments/capture-core/PERSISTENCE_RESULTS.md) 与 [结构化证据](../../../experiments/capture-core/evidence/linux-persistence-migration.json)。

这一步是生产目录中的真实可消费切片，不等于正式 library/reader/notes 插件、产品安装器或 NSH 阅读旅程已完成。当前仍沿用资格实验的有界归档/文档预算，并通过完整快照核对关系；大资料库查询/增量投影与性能需后续独立实现和实测，不能把此 bounded 路径当作最终规模承诺。作者 SDK 继续导出捕获/锚点契约子集，不向插件导出知识存储 envelopes、端口、application 或适配器。
