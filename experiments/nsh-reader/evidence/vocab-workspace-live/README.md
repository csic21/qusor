# 背单词工作区实窗记录

日期：2026-11-11。关联 F-LEARN-01、F-UX-01/02、F-EXT-01/03/04/06。设计见[背单词工作区设计](../../../../docs/design/vocab-workspace.md)，能力/包见 [ADR 0036](../../../../docs/architecture/adr/0036-vocabulary-study-capability.md) 与[词汇包记录](../../../capture-core/evidence/vocab-package/README.md)。

## 捕获方式

与想法工作区相同：在**应用进程内**用 `VocabHost.capture` 把当前 `NSWindow` 的 content view 渲染成 PNG（`cacheDisplayInRect`），不需要屏幕录制授权，也不依赖窗口是否 frontmost。`prepare.py --size 1280x820 --evidence vocab` 设置 `QUSOR_WORKBENCH_EVIDENCE`、`QUSOR_WORKBENCH_CAPTURE_DIR` 与 `QUSOR_WORKBENCH_EVIDENCE_TARGET=vocab`；应用自动进入 `fixture.vocab`，等真实 Core 数据就绪后依次截图。

## 实际验证

| 范围 | 结果 |
|---|---|
| 实窗 + 真实 Core | 词书导航显示两本来自 SQLite Core 的词书（CET-4 示例 3 张、QUSOR 术语 2 张），来源标注“本机 Core · 由 vocab 插件读取” |
| 先回想后揭晓 | 学习卡默认只显示 term 与“先回想这个词的意思，再显示答案”，底部只有「显示答案」；揭晓后才出现释义、例句与四档评分 |
| FSRS 安排 | 四档评分显示约 10 分钟 / 30 分钟 / 3 天 / 16 天；评分后提示“已记录：认识 · 下次 3.0 天后”，并进入下一张 |
| 计数同步 | 每张评分后左侧词书计数从“共 3 · 待复习 0 · 新词 3”降为“新词 0”，与真实 Core 一致 |
| 队列清空 | 三张卡全部评分后显示“今天完成”，明确“不会自动扩大新词预算”，来源仍为“本机 Core” |
| 重开恢复 | 同一 Core 重启应用（不重新播种）后词书显示 0 到期 / 0 新词，进入即“今天完成”；复习进度未丢失 |
| 包/宿主 | 普通签名包 `plugins/vocab`、`VocabHost` 原生模块、0.5 session 与 `vocab.read`/`vocab.review` 授权；包 SHA 见 [app-receipt.json](app-receipt.json) |

截图与机器记录：

| 文件 | 内容 |
|---|---|
| [decks.png](captures/decks.png) | 两本真实词书、来源与计数 |
| [study.png](captures/study.png) | 未揭晓的学习卡与「显示答案」 |
| [revealed.png](captures/revealed.png) | 揭晓后的释义、例句与四档评分 |
| [reviewed.png](captures/reviewed.png) | 评分成功提示与下一张卡 |
| [done.png](captures/done.png) | 今天完成、侧栏计数已归零 |
| [restart-decks.png](captures/restart-decks.png) | 重启后同一 Core 的 0/0 计数 |
| [restart-done.png](captures/restart-done.png) | 重启后进入即今天完成 |

重建与捕获：

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --evidence vocab
"experiments/nsh-reader/target/native-workbench/QUSOR 工作台研究.app/Contents/MacOS/qusor-workbench-study"
# 等待 captures/ 出现 decks/study/revealed/reviewed/done 后关闭应用
```

headless 回归：

```sh
cd experiments/nsh-reader/native-workbench
../target/nsh/target/debug/nsh check App.tsx
../target/nsh/target/debug/nsh test tests/vocab-workspace.test.tsx
../target/nsh/target/debug/nsh test tests/notes-workspace.test.tsx
cargo test --manifest-path experiments/nsh-reader/native-host/Cargo.toml --offline --test vocab_native
```

## 未覆盖

- 实窗中的评分失败重试/回退横幅未逐项截图；失败路径由 UI 代码与 fixture 回退逻辑覆盖，但没有注入真实 Core 故障。
- 深色外观、IME/辅助技术、900×680 live 与其它平台未在本轮截图；900×680 由 headless 测试覆盖。
- 正式产品安装/权限界面、发布密钥管理、词书导入与从阅读收词仍未完成。
- `NotesHost`/想法工作区回归保持 6/6，但本轮未重新采集想法实窗截图。

机器记录见 [record.json](record.json)，应用工件接收见 [app-receipt.json](app-receipt.json)。
