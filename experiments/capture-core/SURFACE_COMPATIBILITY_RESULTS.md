# 公开 Surface 实施后的兼容回归

日期：2026-11-08。关联 C-08/C-09、F-READ-01/03、F-SRC-01、F-EXT-01/03/04/05/06、F-DATA-01 和 [ADR 0023](../../docs/architecture/adr/0023-public-reader-surface.md)。这是既有策略/包/运行/捕获通路的回归记录，原生窗口旅程由 NSH 实验另行取证。

## 实际改动

公开 extension-api 增加显式 0.2 协商、有界 Surface 与每次执行上下文查询，Host 增加唯一 registry、可信 frame/input/gesture 票据和临时选区 lease。默认 HOST_API、默认会话和纯包入口仍为 0.1。纯包验证新增显式支持版本集合；实验持久 Store 从安装、重开到 frozen action 均按可信集合重验，不自动保存授权。

SDK 导出同时包含公开 API 源码和 schema，并逐文件校验权威字节。SDK 中解析成功不能替代 Host 判权；普通插件不获得 NSH、Host、运行器、数据库或内部应用依赖。

## 本机检查

Linux via WSL、Rust 1.97.1、锁定依赖离线执行。精确命令、摘要及124份源码指纹见 [机器证据](evidence/linux-surface-compatibility.json)。

| 范围 | 实际结果 |
|---|---|
| 根工作区 | fmt、全目标 Clippy、108项 Rust 测试、无警告 rustdoc 通过 |
| 依赖边界 | 架构守卫及42项 Python 回归通过 |
| 签名包/持久实验 | 49项 Rust、543个 CLI 场景、593项检查通过，包含原40次真实事务进程中止 |
| Wasm | 32项运行器、4项作者、3项生产执行端口测试和20项旧受监护场景通过 |
| 完整 C3 | 314命令、286 CLI场景、8个 native案例通过；仓库外 SDK 消费10份源/schema摘要 |

测试环境设置 RUST_TEST_THREADS=1，保持精确燃料断言所用250ms期限；测试内的撤权与监督竞争不因此改成串行实现。20项旧受监护场景包含 Legacy Probe，不能把它们全部记作生产 managed supervisor 验证。

根 Rust 检查的 [完整输出](evidence/linux-surface-root-checks.log)、[包证据](../extension-package/evidence/linux-surface-package-regression.json)、[Wasm证据](../extension-package/evidence/linux-surface-wasm-regression.json) 和 [本次C3证据](evidence/linux-compound-pdf.json) 分别保留。旧 C5 摘要属于当时源码，不拿它们证明当前 Surface 改动。

## 未证明的范围

这些结果不证明真实 NSH 输入/保存/重开在同一产品旅程中通过，也不交付生产持久安装/进程监督、正式 library/reader/notes 独立包、中文输入法、无障碍、触控、EPUB或其他平台。继续按 [闭环计划](../../docs/product/next-phase.md) 推进，不将兼容回归代替正式功能验收。


历史记录说明：共用 `linux-compound-pdf.json` 已由后续C7复验刷新，C6当时的详细C3 JSON未单独归档；本记录保留当时摘要与根/包/Wasm证据。后续完整详情固定保存为 [C7 C3快照](evidence/linux-compound-pdf-installation-c7.json)，不能将其新源码摘要解释成C6原始源码。
