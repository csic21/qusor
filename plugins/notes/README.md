# @qusor/notes 插件

状态：读取包已按普通签名包构建并验证；编辑、完整工作区 UI、实窗与持久安装仍未完成。

目标是 F-NOTE-01 与 F-SRC-01：Markdown 想法、不可编辑的原文引用、按资料组织、编辑保存、从引用回源，以及卸载插件后规范内容仍可读取和导出。

## 本轮已有

- `manifest.json`：Host API 0.4，required 仅 `notes.read`，贡献点 `notes_workspace`（`org.qusor.notes.workspace`）。
- `guest.wat`：普通 Wasm guest，通过 `qusor.call` 发送一条 `notes-query-v1` 分页请求（limit 6），返回 Host 的响应；guest 不自报 vault，不读取 Core/Host 私有状态。
- `build.py`：把公开 SDK 导出到仓库外临时目录，用 SDK 自带的 `extension-api` 与 `extension-package` 编译签名工具并签名；作者 lock 中没有 Host/Core/storage/NSH/runtime 私有 crate。
- 验证：`experiments/capture-core/tests/notes_package.rs` 在真实 SQLite Core 上完成 包验证 → Host 安装 → `notes.read` 授权 → 0.4 session → Wasm 执行 → guest 分页请求 → Host 授权/派发 → 真实想法页返回；错误密钥与篡改包被拒绝。

## 构建

```sh
python3 plugins/notes/build.py
```

产物在 `plugins/notes/target/`（已被 gitignore）：`notes.qpk`、`author-proof.json`。固定测试签名种子只用于资格验证，不是生产发布密钥。

## 边界

- 只消费公开 SDK、贡献点、Core 查询与授权；不依赖 Host/Core/SQLite/NSH/runtime 私有实现。
- 原文、用户想法、AI 推断保持分离；本包当前只读，不写入知识。
- guest 只看见 Host 返回的有界页；vault 由 Host 绑定，缺失/撤销授权时请求失败。
- 编辑、正文分块读取、工作区 UI 与实窗、持久安装器、恢复、发布密钥管理仍未完成。

相关设计见 [ADR 0035](../../docs/architecture/adr/0035-notes-read-capability.md)、[功能规格](../../docs/product/notes-plugin.md) 与 [UI 设计](../../docs/design/notes-workspace.md)；本轮证据见 [包读取记录](../../experiments/capture-core/evidence/notes-package/README.md)。
