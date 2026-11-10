# 工作台真实 Core 实窗记录

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-UX-01/02、F-EXT-01/03/04/06。设计见[想法工作区设计](../../../../docs/design/notes-workspace.md)，能力/包见 [ADR 0035](../../../../docs/architecture/adr/0035-notes-read-capability.md) 与[包记录](../../../capture-core/evidence/notes-package/README.md)。

## 捕获方式

本机屏幕录制权限不可用（`screencapture` 返回全黑），工作台窗口也不在前台 Space。因此本轮在**应用进程内**用 `NotesHost.capture` 把当前 `NSWindow` 的 content view 渲染到 PNG：和 NSH 测试宿主截图同一条 `cacheDisplayInRect` 路径，不需要屏幕录制授权，也不依赖窗口是否 frontmost。证据模式由 `prepare.py --evidence` 设置 `QUSOR_WORKBENCH_EVIDENCE`/`QUSOR_WORKBENCH_CAPTURE_DIR` 触发：自动进入想法工作区，等真实 Core 页加载后依次截图列表、详情、截断、完整正文。

## 实际验证

| 范围 | 结果 |
|---|---|
| 实窗 + 真实 Core | 列表显示“本机 Core · 由 notes 插件读取”，3 条来自 SQLite Core 的想法（提交 2/3/4），来源为“准原生应用的秩序” |
| 详情 | 引文、我的想法、提交序号、“阅读窗口尚未接入，无法回源”状态均来自真实数据 |
| 正文分块 | `NotesHost.body` 按 `next_offset` 分块读取，长想法完整显示 |
| 重启恢复 | 同一 Core 数据重启应用后再次捕获，四张图与第一次**逐字节相同**（SHA 见 record） |
| 包/宿主 | 普通签名包 `plugins/notes`、`NotesHost` 原生模块、0.4 session 与 `notes.read` 授权；包 SHA 见 [app-receipt.json](../notes-workbench-bridge/app-receipt.json) |

重建与捕获：

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --evidence
"experiments/nsh-reader/target/native-workbench/QUSOR 工作台研究.app/Contents/MacOS/qusor-workbench-study"
# 等待 captures/ 出现 list/detail/truncated/full-body 后关闭应用
```

## 未覆盖

- 实窗里的文本选取工具条、右键原生菜单与键盘路径由同一 UI 的 6 项 headless 测试覆盖，但**未逐项在 live 窗口截图**。
- 真实回源/精确高亮依赖阅读窗口接入，当前明确显示“阅读未接入”；编辑/删除/组织不在本增量。
- IME、辅助技术、窄窗 live、其他平台与资源释放未验证。

机器记录见 [record.json](record.json)。
