# C0 真实捕获、快照与定位实验

最新生产集成见 [C5统一结果](PLUGIN_PRODUCTION_RESULTS.md)：实验直接消费生产知识用例/存储、签名包和执行适配器，完整普通插件与恢复通路已复验；正式产品旅程仍未完成。

复合多行增量现见 [schema3 本机记录](COMPOUND_RESULTS.md)，完整复现为 `python3 experiments/capture-core/validate_compound.py`。同一个 Core/Host 支持整组原子保存，旧单段入口拒绝复合记录；没有另建知识数据库。

2026-11-08 增量：本实验现有 schema2 PDF 原件与混合知识资格实现，见 [PDF 本机记录](PDF_RESULTS.md)。完整复现为 `python3 experiments/capture-core/validate_pdf.py`，需先按 [原生 PDF provider](../pdf-provider/README.md) 准备固定运行库。原 C0 文本接口仍兼容；混合资料库应使用 `export-all/restore-all`，旧 `export` 明确拒绝漏掉 PDF。以下保留最初文本 profile 的说明。

关联 F-EXT-01/02/03/05/06、F-SRC-01、F-DATA-01；规格见 [捕获资格切片](../../docs/product/capture-qualification.md)，决定见 [ADR 0013](../../docs/architecture/adr/0013-capture-core-qualification.md)。独立 Workspace；原始C0轮次不接生产Core或NSH。随后知识用例与SQL/CAS按ADR0019生产落位，实验门面直接消费同一实现，仍未接NSH阅读旅程。

普通签名插件在受限 Wasm 中提交真实文本；宿主预览后按指定草稿 hash 接受用户意图，Core 保存不可变 UTF-8 blob、来源、锚点、用户备注及事务凭据。插件仅提案，不能直接访问 SQLite，也不能在 JSON 中声明批准。来源没有服务名分支。

## 复现

从仓库根目录运行，要求固定 Rust 工具链和已缓存的锁定依赖：

```sh
python3 experiments/capture-core/validate.py
```

脚本在仓库外导出公开契约和签名 SDK、构建作者工具，检查依赖图，安装两个普通身份来源包，执行真实捕获、重启、故障、卸载、导出和空库恢复。夹具为 MIT 合成文本，固定种子是公开测试数据；不访问网络或用户文件。结果见 [本机记录](RESULTS.md) 与 [JSON 证据](evidence/linux-capture-core.json)。

独立作者输入 `REQUEST MANIFEST SEED_HEX OUTPUT`；`--unchecked-test-request` 仅用于创建签名恶意夹具，宿主仍须拒绝。导出的作者依赖图不含 Host、Core应用/存储、SQLite 或 Wasm 运行器。仓库内作者开发路径依赖不是这项隔离的证据，仓库外实际构建才是。

## CLI 与保存语义

```text
install PACKAGES OP EXPECTED_REV PACKAGE KEY
capture PACKAGES CORE VAULT OP PACKAGE_ID USER_NOTE
capture PACKAGES CORE VAULT OP PACKAGE_ID USER_NOTE --accept EXPECTED_DRAFT_HASH
receipt CORE OP
query CORE CAPTURE_ID
resolve-version CORE CAPTURE_ID VERSION HASH NORMALIZATION UNIT
export CORE
restore EMPTY_CORE PLAIN_BUNDLE_JSON
```

第一条 capture 只预览；保存须再次提供已查看的摘要。CLI 是可信原生用户动作的实验入口，每次显式授予这次捕获所需权限，不把 manifest 当授权。长期产品授权及 UI 另行实现。保存中断后按 OP 查询凭据；预提交无记录、提交后有唯一凭据，同 OP 同内容重试返回原凭据。源包管理单写锁覆盖原生动作，撤权与最终提交共享同一宿主门。

可信测试入口可给接受或恢复追加 `--crash POINT`，进程以 86 退出；点包括 `after_blob_write`、`before_blob_publish`、`after_blob_publish`、`before_transaction`、`after_entities`、`before_commit`、`after_commit`。它们不是插件能力。提交前允许留下无引用 blob，不允许留下半份正式知识。

## 数据边界

`contract.rs` 转发core-api公开DTO/定位实现；`host.rs` 持有原生身份、批准和串行门；`core.rs` 只装配生产application与SQLite/CAS适配器，不保留另一份存储实现。导出只有知识、快照、历史来源身份和操作事实，不携带活动授权、会话、代码或凭据。恢复只接受空库，严格校验所有关系。

本轮限定 64KiB 捕获 wire、4096 条知识和 64MiB 导出，使用 `qusor-raw-text-v1` 原始文本 profile。Unicode scalar 与 byte offset 分开，边界不得切开 grapheme；重复引文歧义不静默选第一处。外部来源入口不等于外部精确跳转。

本机 CLI 和进程崩溃不证明断电耐久、系统重启、OS 沙箱、五平台、NSH、PDF/EPUB 或微信读书接入通过。生产 SDK、正式 Core 及阅读闭环尚未交付。


## 持久用例与适配器生产落位

[ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md) 将实际知识用例、原子端口、SQLite和CAS按职责落位。`src/core.rs` 仅为兼容装配及实验故障回调；原 SQL/CAS 已移除，旧 Host 保持原审批/撤权 gate 后调用同一生产实现。最新状态与复验见 [持久化迁移记录](PERSISTENCE_RESULTS.md)。

生产接口只报告 StorageStage；本实验可信回调将指定阶段转为真实退出86，不把故障字符串或进程终止放进业务API。作者SDK继续导出捕获/锚点模块子集，知识存储envelope/端口/适配器不赋给插件。
