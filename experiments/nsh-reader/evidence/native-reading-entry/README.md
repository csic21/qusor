# 原生阅读入口与宽窄基座：本机记录

日期：2026-11-09。关联 F-LIB-01、F-READ-01/02/03、F-UX-01/02、F-EXT-04/06/07，范围为 macOS 上的隔离界面与窗口内文字阅读子集。不是正式阅读插件、持久书库或六端产品资格。入口和命令见[样例](../../native-design/README.md)，设计约束见[六端基座](../../../../docs/design/cross-platform-shell.md)。

## 实际结果

| 场景 | 结果与证据 |
|---|---|
| 选择书籍/文本 | 默认书架有两份自拟示例；系统文件选择器可打开真实 UTF-8 文件，中文、English、emoji 均来自文件字节：[最终书架](42-receipted-ready-shelf.jpg)、[实际文件正文](36-final-local-file.ax.txt) |
| 原生选择与复制 | 真实拖选得到 23 字，复制成功：[最终选文](33-final-desktop-selection.jpg)、[复制状态](34-final-copy.ax.txt)。较早功能版另以 Cmd+V 粘贴回输入框核对精确文字：[剪贴板回环](10-clipboard-roundtrip.ax.txt) |
| 切换章节 | 原生选择容器按资料/章节重建；翻章后没有旧选区/复制动作：[最终第二章](35-final-chapter-reset.ax.txt)。旧失败独立保留，不覆盖成成功 |
| 输入/粘贴与返回再读 | 功能版实际粘贴并进入阅读；返回书架再打开仍停在本窗口第二章：[输入后阅读](11-pasted-reading.ax.txt)、[章节保持](15-resumed-chapter.ax.txt)。未测精确滚动位置或退出恢复 |
| 取消与错误 | 取消文件选择、选择空文件后，原来四份窗口内资料保留：[取消](13-picker-cancelled.ax.txt)、[空文件](14-empty-file-retained.ax.txt)。大文件/坏编码拒绝有实现，本轮未通过系统选择器逐一实测 |
| 宽窄呈现 | 实际 1240×820 与 390×844 的 macOS 原生窗口：[宽窗书架](42-receipted-ready-shelf.jpg)、[窄窗书架](38-receipted-narrow-shelf.jpg)、[窄窗阅读](40-receipted-narrow-reading.jpg)、[单页目录](41-receipted-narrow-toc.jpg) |
| 通用任务与检查器 | 窄窗仍能进入非阅读任务；标签分行，检查器替代内容而非挤成三栏：[任务](27-final-narrow-task.jpg)、[检查器](28-narrow-inspector.jpg) |
| 导航隔离 | 原生 Dialog 抽屉使用实体背景；打开后 AX 树仅保留抽屉，背景交互设为禁用：[最终导航](39-receipted-narrow-navigation.jpg)、[AX 状态](39-receipted-narrow-navigation.ax.txt)。不等于完整读屏资格 |

最终窗口留在宽屏书架，可直接继续选择内容。当前支持 UTF-8 TXT/Markdown 纯文本，所选文件不修改、不上传；资料、章节和输入仅在当前窗口中保留。复制不是摘录入库，显示片段也不是永久引用锚点。

## 失败与版本边界

1. `01–06` 是初期阅读界面，不指定为最终工件。`07` 显示旧原生选区套入第二章的失败；[失败版源码与 bundle](before-selection-fix/record.json) 单独冻结。
2. `08–16` 在选区修正版上执行功能检查；`17` 记录只设置宿主窗口尺寸却被 bundle 元数据覆盖的失败。脚本随后同步修改 companion `app.json`，`18–21` 才是实际 390pt 窄窗。
3. `22` 暴露透明抽屉与背景混叠，`23` 暴露窄窗任务标题不合适；[功能检查版](functional-check-source/record.json) 保存这一阶段源码和 bundle。它的 bundle 元数据最后处于窄窗配置，不能拿来冒充所有早期截图的启动配置。
4. `24–25` 是触控目标、标题和原生 Dialog 的中间修订，`25` 的背景 AX 控件仍可见。随后显式隔离背景，`26–37` 对应最终 UI 源码：`26–31` 为窄窗，`32–37` 为宽窗；在最终宽窗重新验证实际拖选、复制、翻章清区和文件读取。
5. 首次归档发现窄窗启动收据早于最终 bundle，不能将其树哈希写成一致。保留 `earlier-narrow-launch.json` 后，用最终源码重新启动，`38–41` 复验书架、导航隔离、阅读和目录；`42` 恢复默认宽窗。新 `narrow-launch.json` / `wide-launch.json` 分别与冻结 bundle 的完整树哈希一致。早期宽窗收据另保留为 `earlier-wide-launch.json`。

最终 UI [源码](final-source/App.tsx)、[窄窗 bundle](final-narrow-bundle/app.json)、[宽窗 bundle](final-wide-bundle/app.json) 与启动收据分别留存。两个最终 profile 的 `bundle.nshc` SHA-256 相同，只有窗口元数据不同。源 README 以 `.source.txt` 原样留存，避免归档目录改变相对链接含义。早期启动后仅更正过 `run.py` 的实验/收据说明文字；新一轮使用更正后的脚本，早期收据保留原文。

图片是 CUA 返回的原始 JPEG 字节，未加工；`.ax.txt` 是实际原生可访问状态。带 `final` 的早期文件名不构成最终版本证明，以此处阶段表和[哈希清单](record.json)为准。哈希清单验证归档完整性，不等于全部交互已回归或独立复跑。

## 上游核对与环境

本轮重新核对[官方发布](https://github.com/longbridge/nsh/releases)、[近期 main](https://github.com/longbridge/nsh/commit/21648204e66b3792090c216383f345a9d628f0b8)和[官方设计指南](https://github.com/longbridge/nsh/blob/21648204e66b3792090c216383f345a9d628f0b8/skills/nsh-design-guides/references/design-guides.md)：stable 为 v0.4.0；nightly 指向 `36b91689c1e7c38379179a56381de7ddab16c6a5`；main 为 `21648204e66b3792090c216383f345a9d628f0b8`，最新两条为网站链接修正。未据此推断运行时能力升级。

继续固定 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`，复用现有 debug native-host，没有新增依赖或编译 Rust。系统为 macOS 27.0 / 26A428；字体为 NSH 系统默认与 OS 中文回退，实际字体家族、DPI 和输入法资格未测。早期宽窗日志有一条 IMK mach-port 信息，界面与复制/文件路径可用，不据此声明 IME 通过；日志按阶段原样保留。

## 未覆盖

Windows、Linux、iOS、Android、Web 未运行此界面；窄 Mac 窗口不证明真机触控、安全区、系统返回、动态字号或软键盘。完整键盘/读屏、更多主题状态、性能与长文也未通过本轮资格。

PDF/EPUB、持久资料/进度、正式摘录/笔记/引用、退出恢复、公开 UI 包安装/隔离/替换和真实 Agent 接入仍分别交付。独立 TSX 贡献 fixture 只检查候选界面，不声称六端公开组合协议已经实现。
