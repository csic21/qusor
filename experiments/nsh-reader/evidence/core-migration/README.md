# 单一 Core 实窗迁移记录

日期：2026-11-12。关联 F-DATA-01、F-NOTE-01、F-LEARN-01、F-EXT-03。验证目标：生产应用的 Notes/Vocab 不再分别打开两个 Core；旧分库在真实应用启动时迁入一个 `core/`，窗口重开后仍读取同一 vault。

## 工件与隔离

- 应用：`/tmp/qusor-core-gui-app/QUSOR.app`，从当前工作树打包。
- 隔离 HOME：`/tmp/qusor-core-gui-home`。
- 应用数据：`/tmp/qusor-core-gui-home/Library/Application Support/org.qusor.app`。
- 旧数据：用 `apps/qusor/native/tests/migrated_hosts.rs` 的 `seed_legacy_fixture_for_manual_app` 测试辅助写入 `notes/` 与 `vocab/`。
- 启动方式：`HOME=/tmp/qusor-core-gui-home QUSOR.app/Contents/MacOS/QUSOR`。

`iconutil` 在本机对测试图标返回 `Invalid Iconset`，因此测试包临时省略 `nsh.json.icon`；验证后已恢复。该无图标包只用于本轮内部实窗/迁移检查，不是发行工件。

## 观察结果

首次启动：

- 应用窗口正常打开。
- “想法”工作区显示 `全部想法 1 条想法 · 本机 Core · 由 notes 插件读取`，条目为《Legacy note》第 1 页的“迁移后的想法”。
- “背单词”工作区显示 `1 本词书`，词书为 `Legacy deck`，打开后显示 `core` 新词卡。
- 旧 `notes/core.sqlite`、`vocab/core.sqlite` 保留；新 `core/core.sqlite` 存在。

同一应用退出后重新启动，再打开两个工作区：

- “想法”仍显示同一条迁移想法。
- “背单词”仍显示同一本 `Legacy deck`。
- 两个插件均标注来源为本机 Core。

SQLite 检查：

```text
PRAGMA user_version = 4
pdf_document = 1
pdf_capture = 1
vocab_wordbook = 1
vocab_card = 1
```

## 范围

已证明：真实打包入口、隔离 HOME、旧分库迁移、Notes/Vocab 共享 Core、应用重开后的数据读取。

2026-11-12 后续统一 runtime 回归：当前源码再次生成临时无图标包，隔离 HOME 相同旧数据启动；Notes/Vocab 仍通过 Core→Host 顺序挂载并显示迁移内容。该次包路径为 `/tmp/qusor-runtime-scope-app/QUSOR.app`。

2026-11-12 manifest 工作区目录回归：当前源码临时包路径 `/tmp/qusor-contribution-catalog-app/QUSOR.app`；侧栏显示“扩展工作区：想法、背单词”和“界面样例：阅读”。两个扩展工作区来自普通包 Manifest，打开后仍读取同一 `core/core.sqlite`。

未证明：带图标的正式包（本机 iconutil 阻断）、Developer ID/公证、其它平台、长书/大库迁移性能、迁移冲突的 UI 提示。当前只保留本机窗口观察和 SQLite 记录，没有把截图复制进仓库。
