# 想法读取可信适配器记录

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-EXT-01/03/04/05/06。决策见 [ADR 0035](../../../../docs/architecture/adr/0035-notes-read-capability.md)，应用层读取模型见 [ADR 0034](../../../../docs/architecture/adr/0034-note-read-model.md)。

## 本轮范围

把 `notes-query-v1` 接到真实 Core，仍不涉及安装包与窗口：

- `experiments/capture-core/src/notes_bridge.rs` 的 `NotesReadBridge`：用 Host dispatch 绑定的 vault 调 `learning-application::pdf_notes_page` / `pdf_note_body`，映射为公开 wire 响应。
- 摘要标题/预览截断到 96 bytes，除换行外的控制字符替换为空格，并按实际预览长度重算截断标记，避免 JSON 转义膨胀。
- 正文块在编码超过 4 KiB 时逐次减半重试，直到适配或返回显式 `BudgetExceeded`；`offset`/`next_offset`/`total_bytes` 保持一致。
- Core 错误映射为公开错误码，不回显内部消息。

## 实际验证

| 范围 | 结果 |
|---|---|
| 真实 SQLite Core | 4 项测试通过：分页与游标、正文分块重组（含中文/emoji/NUL）、控制字符导致的分块收缩、跨库 NotFound 与错误映射 |
| Host 策略 | 每项读取先 `authorize_notes` 再 `dispatch`；Host 绑定 vault，撤销/停用/关闭会话使旧许可失效（见 extension-host 回归） |
| 编码预算 | 每个响应 `to_json` 均 ≤4 KiB；收缩后的正文块仍能按 `next_offset` 继续读取 |
| Clippy | `cargo clippy --manifest-path experiments/capture-core/Cargo.toml --all-targets --offline -- -D warnings` 通过 |

复现：

```sh
cargo test --manifest-path experiments/capture-core/Cargo.toml --offline --test notes_read
```

机器记录见 [record.json](record.json)。

## 未覆盖

- 未跑完整 capture-core 实验套件（进程监督等长测），本轮只跑新增 `notes_read` 目标与 Clippy。
- `plugins/notes` 普通签名包、安装/授权/停用/卸载、工作台真实 Core 接线、真实窗口、分页 UI、编辑/删除、IME/辅助技术与其他平台。
- 0.4 只有 `notes.read`；同一包同时需要 `reader.surface` 的合并词表未设计。
