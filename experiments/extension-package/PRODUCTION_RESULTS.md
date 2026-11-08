# 签名包格式生产权威源迁移

日期：2026-11-08。状态：纯格式、两份仓库外SDK和完整包回归通过；主任务在全部迁移冻结后已统一完成C3。见 [ADR 0021](../../docs/architecture/adr/0021-signed-package-authority.md)。

`crates/extension-package` 成为包格式/签名验证的唯一权威源；旧模块仅转发。迁移前与生产源SHA-256均为 `7a33834bc043a90cdcae5e592f74e53f7b74c532e0437db0d83866ceb5a87ef3`，两份实际仓库外SDK的包源摘要也完全相同。14项固定格式测试与Clippy通过，签名domain、包字节、格式版本及键绑定语义不变。

两个SDK作者均在仓库外独立生成锁、构建并运行Clippy，实际图中无Host、SQLite、文件锁或Wasm运行器，所有local source均在工件内；本地开发作者只通过Cargo别名消费公开契约，仍非正式功能插件。

[结构化证据](evidence/linux-package-production.json) 保存两份SDK闭包与源摘要。[完整包回归](evidence/linux-package-production-regression.json) 已退出0：593条检查、543项CLI场景、40次CLI真实退出86；另有同宿主14项监督检查和14项异常恢复场景。宿主二进制摘要前后相同。原59项包实验Rust覆盖分为14项纯格式测试迁入根与45项Store/生命周期/监督测试留在实验，后者完整通过；没有通过删除测试减少覆盖。

两套仓库外SDK build/Clippy各自通过，package SDK53包、capture SDK55包，源摘要与生产权威源一致。最终全C3也已通过314命令/286 CLI/8 native，见 [C5统一结果](../capture-core/PLUGIN_PRODUCTION_RESULTS.md)。543包管理场景本身不代替真实知识或产品阅读旅程验收。

此项不等于生产安装器、完整SDK发行、公开Surface或正式阅读旅程；其他平台和移动分发未因此通过。
