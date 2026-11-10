# 想法插件普通签名包读取记录

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-EXT-01/03/04/05/06；决策见 [ADR 0035](../../../../docs/architecture/adr/0035-notes-read-capability.md)，实现边界见 [plugins/notes](../../../../plugins/notes/README.md)。

## 本轮范围

第一次把 `plugins/notes` 做成普通签名包，并让它真正消费公开 `notes.read`：

- `plugins/notes/manifest.json`：Host API 0.4、required `notes.read`、贡献 `notes_workspace`（`org.qusor.notes.workspace`）。
- `plugins/notes/guest.wat`：Wasm guest 通过 `qusor.call` 发送一条 `notes-query-v1` 分页请求（limit 6），把 Host 响应作为输出返回；不自报 vault。
- `plugins/notes/build.py`：把公开 SDK 导出到仓库外临时目录，用 SDK 的 `extension-api` 与 `extension-package` 编译签名工具，生成 `notes.qpk`；作者 lock 中无 Host/Core/storage/NSH/runtime 私有 crate。
- `experiments/capture-core/tests/notes_package.rs`：包验证 → Host 安装/授权/启用 → 0.4 session → Wasm 执行 → guest 请求 → Host `authorize_notes` + dispatch → 真实 SQLite Core 分页 → guest 输出解析；另测错误密钥与篡改包拒绝。

## 实际验证

| 范围 | 结果 |
|---|---|
| 仓库外 SDK 构建 | `python3 plugins/notes/build.py` 通过；作者 lock 无私有 crate；两次重建得到同一包 SHA |
| 包格式与授权 | 签名包验证通过；manifest 0.4 / `notes.read` / `notes_workspace`；错误密钥、篡改字节均拒绝 |
| Host 安装与贡献 | `register` → `replace_grants`（Vault 精确授权）→ `enable`；`contributions()` 列出 `notes_workspace` |
| Wasm 执行 | `runtime_wasm::run_managed_raw` 执行验证后的模块；1 次 host call，guest 请求被 Host 校验/授权/派发 |
| 真实 Core | guest 输出解析为 `NotesResponse::Page`，两条真实 SQLite 想法按新到旧返回，预览 ≤96 bytes |
| 回归 | 新增 2 项测试通过；旧 0.1–0.3 协议未改动 |

复现：

```sh
python3 plugins/notes/build.py
cargo test --manifest-path experiments/capture-core/Cargo.toml --offline --test notes_package
```

机器记录见 [record.json](record.json)；包 SHA、签名者与 SDK 指纹见 [author-proof.json](author-proof.json)，源文件哈希见 [hashes.json](hashes.json)。

## 未覆盖

- 工作区 UI 与真实窗口、实窗恢复；持久安装器/Store 落盘与卸载、发布密钥管理。
- guest 正文分块读取（Host 适配器的正文路径已有独立测试）、错误授权的窗口反馈。
- 编辑/删除/组织、IME、辅助技术、其他平台与资源释放。
- 完整 capture-core 实验套件未跑；本轮只跑 `notes_package` 目标、包构建与 Clippy。
