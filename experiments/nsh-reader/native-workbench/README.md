# Native workbench · UI craft study

2026-11-09，关联 F-UX-01/02、F-EXT-01/03、M-1；承接 [外壳布局](../../../docs/design/shell-layout.md)。这是独立的 NSH 原生交互样例。外壳与多数 fixture 仍是窗口内自写内容；想法与背单词两个工作区在 `prepare.py` 注入普通签名包与数据目录时可改为真实 SQLite Core（未经安装器、无网络）。未注入宿主时自动回退窗口内样例并标注来源。

导航层级修正见[设计规格](../../../docs/design/navigation-hierarchy.md)和[本轮记录](../evidence/native-navigation-hierarchy/README.md)：宿主顶部保留唯一的“导航”开关、所属工作区与插件身份；空间入口和当前工作区内容共享一个导航区；完整插件的外层返回明确显示来源，并恢复进入前对象与布局。阅读内部“书架 / 书库导航”不再与宿主显隐控件同形无字。

## 假设与门槛

参考用户指定的 T3 Code、macOS 自带应用及 Plannotator 附图，统一窗口比例、平台字体、连续列表、工具栏、对象标签和按需信息面板，能够让同一个外壳承载集合、文稿、画布、会话等不同视图。默认进入工作区集合，不以阅读或对话充当基座首页。宿主默认一个导航区，使用范围切换而非同时显示全局和内容两栏；信息面板按空间收敛，窄窗保留主内容。

用户另提供书库图片，明确只作为阅读插件参考。本轮新增一个自写的阅读工作区贡献：宿主隐藏通用侧栏和内容列表，只保留可信返回、标题与外观菜单；阅读贡献提供书库导航、格式筛选、搜索、网格/列表和阅读视图。普通 TSX import 与窗口内 fixture 仍不代表真实安装包、格式解析或正式阅读插件已完成。

2026-11-10 补充“想法”插件工作区样例，接入同一 `PluginWorkspace` 下钻机制：从统一工作台侧栏进入后，宿主通用侧栏和标签退出，顶部保留 `返回产品设计` 与 `想法〔插件〕`；插件提供自己的分类导航、想法列表、详情和局部返回。它修正了把想法视图挂在独立动态 PDF 预览中的展示方式，但仍使用窗口内 fixture，不是正式 notes 包或 Core 数据装配。实窗记录见 [想法插件工作区](../evidence/native-notes-workspace/README.md)。

本轮门槛：构建；macOS 实窗；集合→文稿→画布；返回集合；标签关闭/重新打开；文稿输入在切换后保留；信息面板按需打开；浅/深色；搜索对话框。截图记录真实窗口，不能用来证明正式插件隔离、保存/恢复、性能或其他端已交付。

结构：`App.tsx` 按通用 `Entry`/`PluginWorkspace` 角色组装外壳；`contributions.tsx` 提供样例元数据、普通内容视图与阅读工作区 fixture；`ui.tsx` 是共享原生控件/图标；`theme.json` 集中语义 token。普通 TSX import 不等于插件 SDK、安装、授权或运行隔离。

本轮新增门槛：

- 从非阅读画布进入阅读贡献，宿主侧栏、内容列表和标签栏退出，阅读贡献自己的导航与工具进入。
- 宿主返回回到进入前的画布标签或功能目录，并恢复导航范围、开合和信息面板，不回到固定首页或强制展开侧栏。
- 1280×820 验证阅读贡献自有导航、搜索和格式筛选；900×680 验证贡献导航收起后可打开、返回并进入正文。
- 阅读工具只修改当前窗口状态；导入和更多工具明确禁用，不显示虚假的成功状态。
- 想法插件走同一外层返回来源；列表→详情→想法列表只移动插件内部，宿主返回仍回到进入前工作区。

实窗范围和限制见 [工作区组合记录](../evidence/native-workspace-composition/README.md)。

## 想法工作区 UI v1（2026-11-10）

想法工作区按[正式交互规格](../../../docs/design/notes-workspace.md)实现到可评审状态：范围导航（全部/当前资料）、搜索、列表与详情、文本选取工具条、右键原生菜单（`Menu.popup`，先选中行）、复制引文/想法/引用、缺原件与其他资料状态、完整正文读取状态、窄窗列表/详情切换与键盘操作。数据仍是窗口内 fixture；不是安装包，也不连 Core。

验证：`nsh check App.tsx` 类型检查、`prepare.py --build-only` 构建、`nsh test tests/notes-workspace.test.tsx` 6/6 通过；实窗截图与机器记录见 [UI v1 记录](../evidence/notes-workspace-ui/README.md)。

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --build-only
cd experiments/nsh-reader/native-workbench
../target/nsh/target/debug/nsh test tests/notes-workspace.test.tsx
# real macOS window renders for the evidence screenshots
../target/nsh/target/debug/nsh test tests/notes-workspace-visual.test.tsx --target macos
```

## 真实 Core 模式（headless 与实窗均已通过）

`prepare.py` 现在会：构建 `qusor-nsh-workbench-host`、用 `plugins/notes/build.py` 生成普通签名包、用 `seed_workbench_notes` 播种真实 SQLite Core，并通过 `QUSOR_NOTES_*` 环境把数据目录、包、公钥与当前 document 交给宿主。宿主注册 `qusor/pdf-surface` 中的 `NotesHost`，工作台想法 UI 优先读取 `NotesHost.page()` 并用 `NotesHost.body` 分块读正文；失败时回退窗口内样例，列表头明确标注“样例数据”或“本机 Core”。

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --evidence
cargo test --manifest-path experiments/nsh-reader/native-host/Cargo.toml --offline --test notes_native
```

headless 模块测试证明普通包 + 公开 wire + 真实 Core 的分页/正文；`prepare.py --evidence` 会在应用进程内把真实 `NSWindow` 渲染成 PNG（不依赖屏幕录制授权），自动进入想法工作区并截取列表/详情/截断/完整正文。同一 Core 重启后截图逐字节相同，构成实窗与恢复证据；详见[实窗记录](../evidence/notes-workspace-live/README.md) 与 [原生桥记录](../evidence/notes-workbench-bridge/README.md)。

按钮与文本动作统一走 `ui.tsx` 的 `control()` 刻度（28/30/34），搜索框 32，菜单项 30；引文强调线用独立 3pt View，避免固定 NSH 的 macOS 开发宿主把 `borderLeftWidth` 画成横线。

## 背单词工作区（2026-11-11）

新增 `fixture.vocab` 插件工作区（title「背单词」）：词书导航（两本合成词书）、先回想后揭晓的学习卡、四档评分（不认识/模糊/认识/很熟，显示 FSRS 安排区间）、待复习/今日新词计数与「今天完成」。UI 与想法工作区共用同一外壳、语义 token 与控件刻度；900×680 用顶部词书条替代侧栏。

数据有两种来源并始终标注：headless / 普通开发宿主使用窗口内 fixture（“样例数据”）；`prepare.py` 注入 `QUSOR_VOCAB_*` 后，宿主验证普通签名 `plugins/vocab` 包、建立 0.5 session，经 `VocabHost.decks/queue/review` 读取真实 SQLite Core（“本机 Core”）。评分操作编号由 UI 生成并在同一次点击重试时复用，失败两次才回退 fixture。

验证：`nsh check App.tsx`、`nsh test tests/vocab-workspace.test.tsx`（17 项）、`nsh test tests/notes-workspace.test.tsx`（6 项回归）、`cargo test --manifest-path ../native-host/Cargo.toml --offline --test vocab_native`（1 项真实 Core 模块测试）与 `--lib vocab_native`（3 项语音选择/校验单元测试）；`prepare.py --size 1280x820 --evidence vocab` 生成七张应用内实窗渲染（decks/settings/study/accent/revealed/reviewed/done），并在同一 Core 重启后补两张恢复图（重启保留读音偏好）。记录见[背单词工作区实窗](../evidence/vocab-workspace-live/README.md)。

### 音标与读音（2026-11-11 增量）

学习卡在揭晓前就显示词性与 IPA，并提供“朗读”按钮与美音/英音切换；`R` 键朗读当前词，揭晓后的例句另有“朗读例句”。音标来自同目录 `pronunciation.ts` 的**样例词书资源**（七个人工核对条目；不是 Core 字段，也不由 wire 传输），未收录的 term 明说“音标未收录 · 由词书资源提供”。读音由宿主 `VocabHost.pronounce(text, accent, rate)` 交给 macOS 系统语音（`say`）离线朗读：优先 Samantha/Daniel 一类已安装语音，未安装则退回同语言其他语音再退回默认；新的一次朗读会停掉上一句，`rate` 取 `normal`（约 165 WPM）或 `slow`（约 120 WPM）。没有实验宿主或 Core 运行时未就绪时，状态行显示“当前宿主没有系统读音”，不伪装已播放。

宽窗侧栏底部（窄窗在词书条下方）有“读音”设置：**自动朗读**（卡片成为当前卡即读一次，环境式朗读，不覆盖已有的评分回执；没有系统读音时保持安静）与**慢速**。两项存应用自己的存储（`vocab.autoRead` / `vocab.slowRead`），重开窗口恢复；`--evidence` 第一次运行会把它们恢复默认再打开，第二次（重启）运行则展示保留下来的偏好。

边界：这些是工作台样例的界面、应用存储与实验宿主增量，**不改变** 0.5 能力、`vocab-study-v1` wire、SQLite schema 或 `plugins/vocab` 包；生产读音需要平台语音能力或词书随包音频，另行设计。

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --evidence vocab
cd experiments/nsh-reader/native-workbench
../target/nsh/target/debug/nsh check App.tsx
../target/nsh/target/debug/nsh test tests/vocab-workspace.test.tsx
```

注：本目录补了根 `tsconfig.json`（`extends ./.nsh/tsconfig.json`），否则固定 NSH CLI 的 `nsh check` 会因缺 tsconfig 失败；顺带修正了想法工作区既有 `note.full` 的严格类型错误。

## 复现

沿用 `../target/nsh` 的固定 CLI 和 native-host，不引入依赖，不升级 NSH：

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820
```

随后打开输出的 `.app`。脚本只准备工件，不自动关闭或替换已有设计窗口。仅需构建时加 `--build-only`。当前最小研究窗口 900×680；紧凑移动交互由既有布局规格定义，本轮样例不据此宣称通过。

证据与上游核对见 [本轮记录](../evidence/native-workbench/README.md)。
