# QUSOR 原生视觉与文字阅读样例

关联 M-1 P02 的原生窗口显示子项、F-LIB-01、F-READ-01/02/03、F-UX-01/02、F-EXT-04/06/07。假设：同一通用外壳能承载阅读、研究、写作、流程与开发，并用 NSH 原生显示体现字体、列表、工具栏和内容层级。

这是可交互的隔离实验，复用已存在的 macOS 原生宿主。默认显示自拟书籍与文本；用户可通过系统文件选择器主动打开 UTF-8 TXT/Markdown（按纯文本显示，文件上限 1 MiB），或输入/粘贴文本。资料、章节和任务草稿只保留在当前窗口；退出后不保留。不连接 Agent、安装真实插件、写入 Core 或提供正式保存。

阅读入口来自 `ReadingWorkspace.tsx` 的贡献 fixture；它不是已安装的阅读包或已发布的 SDK 类型。正式插件边界沿[六端基座](../../../docs/design/cross-platform-shell.md)，不得把这些局部 TSX 导入直接当作公开装配协议。

## 复现

先按 [实验入口](../README.md) 准备固定 NSH checkout、CLI 和默认 `qusor-nsh-native-host-probe`。本样例不创建 Rust Workspace、修改依赖或自动编译 Rust。当前 checkout 为 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`，实际复用的宿主哈希见 [证据](../evidence/native-design/README.md)。

从仓库根目录运行：

```sh
python3 experiments/nsh-reader/native-design/run.py
```

只构建：

```sh
python3 experiments/nsh-reader/native-design/run.py --build-only
```

关闭已有样例，再检查窄窗：

```sh
python3 experiments/nsh-reader/native-design/run.py --size 390x844
```

该尺寸会同时写入实际 companion `app.json`。这是 macOS 窄窗布局检查，不是 iOS/Android 模拟器或真机。默认宽窗为 1240×820；再次运行默认命令前先关闭已有样例。

类型检查：

```sh
cd experiments/nsh-reader/native-design
../target/nsh/target/debug/nsh check App.tsx
```

脚本生成独立的 `org.qusor.designpreview` 应用，放于被忽略的 `target/native-design/`，不替换现有阅读预览。生成后的应用可双击重开；启动器始终指定本样例 bundle，避免误连默认开发服务器。启动收据、原生日志、编译 bundle 和应用均在该目录中。关闭已有样例后再运行脚本，避免并行样例混淆。

## 评审路径

1. 从默认“书架与文本”选择示例，或打开本机文本文件；也可输入/粘贴文本。筛选与查找只作用于当前窗口的资料。
2. 在正文拖动选择，核对实际选文并复制；翻章后不应把旧选择范围套到新正文。返回书架再打开，核对窗口内章节位置。
3. 宽窗打开目录并调节字号；390pt 窄窗使用“阅读工具”进入目录，目录占单页并能返回正文。
4. 从全局导航切换研究/写作任务，编辑草稿、切换“任务过程”再返回；核对任务间草稿隔离。窄窗的检查器占主内容区。
5. 展开/收起导航，切换明暗，从“扩展与能力”选择工作界面。窄窗导航打开时，背景操作和可访问控件应隔离。
6. 打开 Agent 菜单；选择入口只更新未连接提示，不执行外部动作。

通过条件分别是实际窗口显示、输入回显、局部状态保留与可见切换。构建失败停止显示资格；看不到预期窗口或浮层就记录失败，不能凭源码宣布通过。首轮独立浮动面板未取得可见证据，当前使用主窗口中的扩展列表，原生多窗口另验。

## 证据与边界

[早期视觉记录](../evidence/native-design/README.md) 保留原任务外壳；[文字阅读与跨尺寸记录](../evidence/native-reading-entry/README.md) 留存本轮来源、失败、修复及分阶段冻结工件。文件选择与正文读取不等于正式入库，Markdown 未渲染格式；PDF/EPUB 仍未接入这个窗口。

当前仅 macOS 本机显示与交互子集。390pt 窄窗不证明移动触控、安全区或软键盘合格；完整键盘、IME、读屏、动态字号、其他五个目标及真实插件/Agent/保存恢复未验证。不要把该样例复制成业务宿主的固定场景分支。
