# 背单词插件普通签名包与学习数据记录

日期：2026-11-11。关联 F-LEARN-01、F-EXT-01/03/04/05/06；决策见 [ADR 0036](../../../../docs/architecture/adr/0036-vocabulary-study-capability.md)，实现边界见 [plugins/vocab](../../../../plugins/vocab/README.md) 与 [词汇插件规格](../../../../docs/product/vocab-plugin.md)。

## 本轮范围

第一次把词汇学习做成普通签名包，并让它消费公开的 Host API 0.5：

- `plugins/vocab/manifest.json`：Host API 0.5、required `vocab.read` + `vocab.review`、贡献 `vocab_workspace`（`org.qusor.vocab.workspace`）。
- `plugins/vocab/guest.wat`：普通 Wasm guest，依次通过 `qusor.call` 发送 `vocab-study-v1` 的 decks、queue（wordbook 1、limit 3）与 review（card 1、Good）请求，把最后一次 Host 响应作为输出；guest 不自报 vault。
- `plugins/vocab/build.py`：公开 SDK 导出到仓库外临时目录，用 SDK 的 `extension-api`/`extension-package` 编译签名工具，生成 `vocab.qpk`；作者 lock 无 Host/Core/storage/NSH/runtime 私有 crate。
- `experiments/capture-core/src/vocab_bridge.rs`：可信适配器把有界 wire 映射到 `StudyApplication`，收缩文本预算并映射公开错误码；不授予权限、不复制调度算法。
- `experiments/capture-core/tests/vocab_package.rs`：包验证 → Host 安装/精确授权/启用 → 0.5 session → Wasm 执行 → guest 三次请求 → Host 逐次判权 → 真实 SQLite Core 读取与评分 → guest 输出解析；另测只读包拒绝评分、错误密钥与篡改包。

## 实际验证

| 范围 | 结果 |
|---|---|
| 仓库外 SDK 构建 | `python3 plugins/vocab/build.py` 通过；作者 lock 无私有 crate；`vocab.qpk` sha256 `4ce99011b45ce6c01b158e236762ea3d4465a2dd050e590b77be8e0284dd0823`，2149 bytes |
| 发布键 | 固定测试种子派生 `17cb79fb…18080ce`；不是生产发布密钥 |
| 包格式与 manifest | 0.5 / `vocab.read` + `vocab.review` / `vocab_workspace`；错误密钥、篡改字节拒绝 |
| Host 安装与贡献 | `register` → `replace_grants`（精确 Vault 授权）→ `enable`；`contributions()` 列出 `vocab_workspace` |
| Wasm 执行 | `run_managed_raw` 执行验证模块；3 次 host call，guest 请求被 Host 校验/授权/派发 |
| 真实 Core | decks/queue 读到注入的 3 张卡；review 写入一条复习与 schedule，返回 3 天/剩余计数；重开与导出路径由 storage-sqlite 测试覆盖 |
| 权限边界 | 只授予 `vocab.read` 的包在 review 处被 Host 拒绝，Core 无复习记录 |
| 回归 | 新增 3 项测试；0.1–0.4 旧协议/notes 路径与全部既有测试保持通过 |

复现：

```sh
python3 plugins/vocab/build.py
cargo test -p learning-domain -p core-api -p extension-api -p extension-host -p storage-sqlite --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --test vocab_package --locked --offline
```

包 SHA、签名者与 SDK 指纹见 [author-proof.json](author-proof.json)，源文件哈希见 [hashes.json](hashes.json)，机器可读记录见 [record.json](record.json)。

## 未覆盖

- 原生工作区 UI、实窗/键盘/焦点/恢复；生产安装/权限界面、发布密钥管理与持久安装后的复习。
- 词书随包数据、用户导入、从阅读/笔记收词；真实词表来源与许可。
- 提醒、统计、设置、云同步、移动/Web 运行 profile。
- 本机 `runtime-process --test controller` 在干净树同样失败（macOS 进程监督用例，与本切片无关）；因此记录为“工作区排除该包 282 项通过”，不宣称本机全工作区整体通过。
- 完整 capture-core 实验套件未跑；本轮只跑 `vocab_package` 目标、相关 crate 测试与插件包构建。
