# 想法读取能力记录（协议 0.4）

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-EXT-01/03/04/05/06、C-08。决策见 [ADR 0035](../../../../docs/architecture/adr/0035-notes-read-capability.md)，应用层读取模型见 [ADR 0034](../../../../docs/architecture/adr/0034-note-read-model.md)。

## 本轮范围

给普通插件一条有界的想法读取公开能力，同时不改旧协议：

- Host API **0.4**、能力 **`notes.read`**、贡献类型 **`notes_workspace`**；0.1–0.3 词义不变，能力上限 5→6。
- `notes-query-v1`：分页（≤6 条/页，标题与预览 ≤96 UTF-8 bytes）与正文分块（4–2048 bytes）；请求不自报 vault，Host 绑定精确授权；整响应 ≤4 KiB。
- Host `authorize_notes(session, request)` 复用既有 authorize/dispatch：会话、安装、授权世代与 vault 在动作前重核；撤销、停用、关闭会话使旧许可失效。
- 0.4 是独立词表：0.3 及更早会话不能请求 `notes.read`，0.4 会话也不能使用 0.2/0.3 能力。

## 实际验证

| 范围 | 结果 |
|---|---|
| wire 回归 | 6 项：请求/响应往返、未知字段、版本与预算、游标边界、摘要/正文一致性、整页 4 KiB、0.4 词表与 manifest 规则 |
| Host 回归 | 5 项：精确 `notes.read`+Vault 授权与 vault 绑定 dispatch、旧会话/旧 wire 拒绝、Document 资源拒绝、撤权/停用后许可失效、Host 不支持能力时拒绝注册 |
| 全量回归 | `cargo test --workspace` 244 项通过；Clippy、格式、42 项架构守卫、文档链接通过 |
| 旧协议 | 0.1/0.2/0.3 既有测试继续通过；仅更新了一处“未来版本”断言（0.4→0.5） |

机器记录见 [record.json](record.json)。

## 未覆盖

- 可信 Core 后端适配器：尚未用 `AuthorizedContext.vault_id` 调 `learning-application` 生成响应。
- `plugins/notes` 普通签名包：没有 manifest、仓库外 SDK 构建、安装/授权/停用/卸载。
- 真实窗口：分页、正文块与右键菜单仍未接真实 Core。
- 编辑/删除/标签/收藏写入能力、中文 IME、辅助技术、其他平台与资源释放。
- 0.4 目前只有 `notes.read`；同一包同时需要 `reader.surface` 与 `notes.read` 的合并词表尚未设计。
