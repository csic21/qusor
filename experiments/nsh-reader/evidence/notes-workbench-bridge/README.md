# 工作台想法原生桥记录

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-EXT-01/03/04/06、F-UX-02。设计见[想法工作区设计](../../../../docs/design/notes-workspace.md)，能力与包见 [ADR 0035](../../../../docs/architecture/adr/0035-notes-read-capability.md) 与[包记录](../../../capture-core/evidence/notes-package/README.md)。

## 本轮接线

- `native-host/notes_native.rs`：工作台侧的 `NotesHost` 原生模块。启动时读取受信环境变量，验证普通签名 `plugins/notes` 包，Host 注册/精确 Vault 授权/启用/0.4 session，再用 `NotesReadBridge` 读取真实 SQLite Core；请求只走公开 `notes-query-v1`。
- `native-host/workbench_main.rs` + `nsh.json` 的 `"rust": "../native-host"`：工作台宿主注册 `qusor/pdf-surface` UI 集（其中含 `NotesHost`）。`prepare.py` 现在构建该宿主、生成并复制签名包、用 `seed_workbench_notes` 播种真实 Core，再启动应用。
- 工作台想法 UI：进入后先尝试 `NotesHost.page()`，把公开 wire 摘要映射成现有设计模型；成功时标注“本机 Core”，失败时保留窗口内样例并标注“样例数据”。正文“读取完整内容”改走 `NotesHost.body` 分块；真实回源在阅读窗口接入前明确显示“阅读未接入”。

## 实际验证

| 范围 | 结果 |
|---|---|
| 原生模块 + 真实 Core | `cargo test --test notes_native` 通过：签名包 → Host 授权 → 0.4 session → `NotesHost.page()` 两条真实想法、`NotesHost.body` 分块读取、非法字段拒绝 |
| 工作台 fixture 交互 | 6 项 headless 测试通过；真实 macOS 开发宿主重截四张图，列表新增“样例数据 / 本机 Core”数据来源标识 |
| 包与安装 | 仍是上一轮的普通签名包与安装/停用/卸载证据；本轮未重复运行 |
| 构建 | `prepare.py --size 1280x820` 构建宿主、播种 Core、生成 `notes.package` 并封装 app；工件接收见 [app-receipt.json](app-receipt.json) |
| 实窗与重启恢复 | `prepare.py --evidence` 后在应用内捕获真实 NSWindow：列表/详情/截断/完整正文；同一 Core 重启后四张图逐字节相同，见[实窗记录](../notes-workspace-live/README.md) |

复现：

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820
cargo test --manifest-path experiments/nsh-reader/native-host/Cargo.toml --offline --test notes_native
```

## 未覆盖

- 实窗里的文本选取工具条、右键原生菜单与键盘路径由同一 UI 的 headless 测试覆盖，未逐项做 live 截图。
- 工作台数据映射（标题取想法首行、日期用提交号）还需产品确认；真实回源、精确高亮、编辑未接。
- 未验证窄窗 live、IME/辅助技术、其他平台与资源释放。

机器记录见 [record.json](record.json)，测试日志见 [tests.log](tests.log)。
