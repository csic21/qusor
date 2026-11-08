# ADR 0018：捕获与锚点契约落位 Core API

日期：2026-11-08。状态：采用；落实 C-08 的实验后生产目录边界。关联 F-EXT-02/05/06、F-SRC-01、F-DATA-01、F-READ-03。

## 问题

C0–C3 已验证普通插件捕获、不可变快照、PDF/复合锚点和完整恢复，但公开契约仍位于同时依赖 Host、SQLite 和运行时的实验包。继续复制导出或把整个实验 Core 搬入 application 会模糊边界。

## 决定

新增唯一有真实消费者的根成员 `crates/core-api`：迁入 `contract.rs`、`pdf_contract.rs`、`pdf_compound.rs` 的现有实现及纯契约测试。实验旧模块只 `pub use` 转发，现有 Host/Core/作者消费者继续可用。协议字节、hash、边界/错误行为与独立版本保持；不将纯文本/PDF/复合版本合并。原始实现文件可与历史 C3 source hash 核对。

Core 数据语义与定位归 core-api，extension-api 继续管理通用 Host 能力/贡献/授权协议。此次不增加互相依赖，不将 NSH/DOM/FFI、包 Store 或 SQLite 对象加入公开模型。learning-domain 仍保持零依赖，未生成空 application/ports/engine。

实验 SDK 导出器直接从 core-api 导出完整 crate，并解析根 manifest 的发行属性；仓库外作者以 Cargo 依赖别名兼容原 import。导出工件不包含主仓库路径、Host、存储或运行时。此工件仍是验证 SDK，不声称已公开发布。

## 后续落位

实验 `core.rs`/`core/pdf.rs` 仍混合用例、SQL、CAS、Unix锁及归档。下一步围绕真实导入/捕获拆 application、事务/blob ports 与 SQLite/CAS adapters，保留一次实体/回执/审计/outbox/index 提交、先耐久 blob 后数据库引用、撤权/最终提交共享序列化门。不将 SQL transaction 类型暴露给应用或插件；不把实验 process-exit 故障注入变成正式 API。

## 验证与限制

纯契约测试迁入生产成员，实验保留 Host/事务/恢复测试；验证同时覆盖根依赖守卫、normal/dev/build/target声明、SDK仓库外构建及真实 C3 通路。根61项测试（捕获契约26+原策略35）、实验41项Core/Host测试与42项架构守卫回归通过；完整C3再次通过314条命令、286项CLI场景及8项native场景，含仓库外作者构建/Clippy。三份实现文件与迁移前C3摘要完全相同，见 [迁移证据](../../../experiments/capture-core/evidence/linux-core-api-migration.json)。复现见 [开发指南](../../development.md) 与 [C3结果](../../../experiments/capture-core/COMPOUND_RESULTS.md)。

仅契约已生产落位：持久应用服务、生产库、安装器、官方 reader/library/notes 独立包与 NSH 阅读旅程仍未完成。本次没有改变产品功能、云默认或首发平台范围。
