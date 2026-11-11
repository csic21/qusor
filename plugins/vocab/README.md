# @qusor/vocab 插件

状态：普通签名包已按仓库外 SDK 构建并通过真实 SQLite end-to-end；原生工作台背单词工作区已用同一 0.5 wire 接入真实 Core（先回想后揭晓、四档评分、重开恢复）；生产安装界面、正式产品工作区与持久安装后的复习仍未完成。

目标是 F-LEARN-01 的首个切片：选择词书、按到期顺序复习、四档评分、FSRS-5 安排下次复习，并在关闭/重开与卸载插件后仍保留学习数据。

## 本轮已有

- `manifest.json`：Host API 0.5，required `vocab.read` + `vocab.review`，贡献 `vocab_workspace`（`org.qusor.vocab.workspace`）。
- `guest.wat`：普通 Wasm guest，依次通过 `qusor.call` 发送 decks、queue（wordbook 1、limit 3）与 review（card 1、Good）三条 `vocab-study-v1` 请求；vault 不由 guest 自报，由 Host 绑定。
- `build.py`：把公开 SDK 导出到仓库外临时目录，用 SDK 的 `extension-api`/`extension-package` 编译签名工具并生成 `target/vocab.qpk`；作者 lock 中没有 Host/Core/storage/NSH/runtime 私有 crate。
- 验证：`experiments/capture-core/tests/vocab_package.rs` 在真实 SQLite Core 上完成 包验证 → Host 安装 → `vocab.read`/`vocab.review` 授权 → 0.5 session → Wasm 执行 → guest 三次请求 → Host 逐次判权 → 可信适配器读写 Core → guest 输出解析；只读包在 review 处被拒绝且 Core 无写入；错误密钥与篡改包被拒绝。

## 构建

```sh
python3 plugins/vocab/build.py
```

产物在 `plugins/vocab/target/`（已被 gitignore）：`vocab.qpk` 与 `author-proof.json`。固定测试签名种子只用于资格验证，不是生产发布密钥。

## 边界

- 只消费公开 SDK、0.5 能力与贡献点；不依赖 Host/Core/SQLite/NSH/runtime 私有实现，不持有完整知识副本。
- 评分的 `request_id` 是 Core 幂等键；重复评分不重复安排。
- 词书内容当前由宿主/可信应用播种合成示例；插件随包数据、导入与从阅读收词在后续增量。
- 生产安装/权限界面、正式产品工作区装配、发布密钥管理、词书导入/提醒与移动渠道仍未完成；工作台样例 UI 不等于这些门槛通过。

相关设计见 [ADR 0036](../../docs/architecture/adr/0036-vocabulary-study-capability.md)、[功能规格](../../docs/product/vocab-plugin.md)、[工作区设计](../../docs/design/vocab-workspace.md) 与 [实窗记录](../../experiments/nsh-reader/evidence/vocab-workspace-live/README.md)。
