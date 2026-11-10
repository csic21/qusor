# ADR 0035：想法读取的公开能力（协议 0.4）

日期：2026-11-10。状态：wire、能力、Host 策略、真实 Core 适配器、`plugins/notes` 普通签名包、服务层安装/停用/卸载、工作台 `NotesHost` 原生桥、工作台 live 实窗与重启恢复已通过；生产安装界面、真实回源与编辑仍未完成。关联 C-08/C-09/C-12、F-NOTE-01、F-SRC-01、F-EXT-01/03/04/05/06。承接 [ADR 0034](0034-note-read-model.md) 与 [D 阶段计划](../../product/d-phase-plan.md)，不扩大 AI、云或平台范围。

## 问题

想法工作区的 UI 已在原生样例中按规格实现，但读取仍走可信原生视图，不是普通插件能用的公开能力。现有公开 wire 每次请求/响应 4 KiB，manifest 能力上限 5 个且已被 `selection.read`、`document.metadata.read`、`capture.propose`、`reader.surface`、`note.draft.edit` 占满；想法列表与正文块远超 4 KiB，不能直接塞进 0.2/0.3。

同时，插件不能自报 vault 或 document：范围必须由 Host 从会话与精确授权绑定。正文分块与分页游标的语义已在应用层固定（ADR 0034），公开 wire 只做有界传输与错误分类，不复制 Core 查询实现。

## 决定

新增 **Host API 0.4**、能力 **`notes.read`** 与贡献类型 **`notes_workspace`**；0.1–0.3 的词义与行为保持不变，`SUPPORTED_HOST_APIS` 增加 0.4，能力上限从 5 提到 6。0.4 是独立词表：不继承 0.2/0.3 的能力，旧包继续按原版本协商。

### 公开 wire（`notes-query-v1`）

- 请求：`notes_protocol = 1`、`api_version = 0.4`、`request_id`、operation。
  - `Page { cursor?, limit }`：`limit` 1..=6；cursor 是后端签发的有界不透明字符串。
  - `Body { capture_id, field: quote|note, offset, max_bytes }`：`offset` ≤ 16 MiB，`max_bytes` 4..=2048。
- 请求不携带 vault/document；Host 从会话与 `notes.read` 的精确 Vault 授权绑定范围，后端仍按 ADR 0034 校验游标范围。
- 响应：`Page { notes[], next_cursor? }` 或 `Body(...)` 或 `Error { error }`；`request_id` 必须由调用方关联。
- 摘要：`document_title` ≤ 96 UTF-8 bytes；`quote_preview`/`note_preview` ≤ 96 bytes，不允许除换行外的控制字符；截断标记与字节数必须自洽（未截断时预览字节数等于总字节数，截断时小于）。
- 正文块：`text` ≤ 2048 raw bytes；`offset + text.len() <= total_bytes`；`next_offset = None` 表示恰好到末尾，`Some(n)` 必须等于 `offset + text.len()` 且小于总长。
- 整个响应编码必须 ≤ 4 KiB。控制字符会让 JSON 转义膨胀（NUL 变成 `\u0000`），后端必须缩小正文块直到编码预算内；wire 拒绝超预算响应。
- 游标格式与范围绑定属于可信后端（ADR 0034 的 `n1:<document|*>:<rev>:<id>`），公开 wire 只校验长度与可打印字符。

### Host 策略

- `extension-host` 新增 `Host::authorize_notes(session, request)`：先校验 wire 版本与结构，再以 `CapabilityRequest { capability: notes.read, resource: Vault(host.vault_id) }` 走既有 `authorize`。
- 既有 `dispatch(permit, action)` 在动作前重核会话、安装、授权世代与 vault，回调收到 `AuthorizedContext`；撤销、停用、关闭会话使旧 permit 失效。
- 本层不依赖 Core、不执行插件、不读数据库；可信适配器用 `AuthorizedContext.vault_id` 调 `learning-application` 并编码响应。
- 0.3 及更早会话不能请求 `notes.read`；旧能力也不能在 0.4 会话中使用（独立词表）。

### 兼容与限制

- 旧 0.1/0.2/0.3 manifest、请求、签名与 Surface 路径不变；现有回归继续通过。
- 0.4 目前只有 `notes.read`。一个包若同时需要 `reader.surface` 与 `notes.read`，当前无法在同一会话版本内声明；分别交付 reader 与 notes 包，或在后续版本合并词表。
- wire 是能力契约，不证明插件已安装、已授权或已读取真实数据。

## 可信 Core 适配器（已通过）

`experiments/capture-core/src/notes_bridge.rs` 提供 `NotesReadBridge`：以 Host 绑定并 dispatch 的 vault 调 `learning-application` 的 `pdf_notes_page`/`pdf_note_body`，把结果映射为 `notes-query-v1`。

- 摘要标题/预览按 96 bytes 截断并把除换行外的控制字符替换为空格，避免 JSON 转义膨胀；截断标记按实际预览长度重算。
- 正文块先请求 wire 上限，若编码超过 4 KiB 则逐次减半重试，直到适配或返回显式 `BudgetExceeded`；`offset`/`next_offset` 与总长保持一致。
- Core 错误映射为公开错误码（`NotFound`、`InvalidRequest`、`BudgetExceeded`、`UnsupportedVersion`、`InvalidState`），不回显内部消息。
- 适配器只读，不解析锚点、不写知识；真实 SQLite Core 上验证分页、正文分块重组、控制字符收缩、跨库与错误映射。

## 普通签名包路径（已通过）

`plugins/notes` 不再只是规划目录：

- `manifest.json` 声明 Host API 0.4、required `notes.read` 与 `notes_workspace` 贡献。
- `guest.wat` 是普通 Wasm guest，经 `qusor.call` 发送一条 `notes-query-v1` 分页请求；vault 仍由 Host 绑定。
- `build.py` 把公开 SDK 导出到仓库外临时目录，用 SDK 的 `extension-api`/`extension-package` 编译签名工具并生成包；作者 lock 不含 Host/Core/storage/NSH/runtime 私有 crate。
- 验证链路：`verify_for_host` → Host `register`/精确 Vault 授权/`enable` → 0.4 session → `run_managed_raw` 执行 Wasm → guest 请求 → `authorize_notes` + dispatch → 真实 SQLite Core 分页 → guest 输出解析；错误密钥与篡改包拒绝。见[包读取记录](../../../experiments/capture-core/evidence/notes-package/record.json)。

## 验证

- `extension-api`：wire 往返、未知字段、版本/预算、游标、摘要与正文一致性、整页 4 KiB 预算、0.4 词表与 manifest 规则。
- `capture-core` 适配器：真实 SQLite Core 上的分页/正文、控制字符收缩、跨库 NotFound 与错误映射（4 项）。
- `extension-host`：精确 `notes.read` + Vault 授权、vault 绑定 dispatch、旧会话/旧 wire 拒绝、Document 资源拒绝、撤权/停用后 permit 失效、Host 不支持该能力时拒绝注册。
- 旧回归：`cargo test --workspace` 244 项、Clippy、格式、架构守卫与文档链接通过。
- 未覆盖：live 窗口的选取/右键/键盘逐项截图、生产安装/权限界面与发布密钥、真实回源/编辑与资源释放。

本机记录见 [想法读取能力记录](../../../experiments/capture-core/evidence/notes-read-capability/record.json) 与 [可信适配器记录](../../../experiments/capture-core/evidence/notes-read-bridge/record.json)。
