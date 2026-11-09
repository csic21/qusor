# 原生工作台视觉与交互 · 本机记录

2026-11-09；关联 F-UX-01/02、F-EXT-01/03、M-1。[源码与复现](../../native-workbench/README.md)、[设计候选](../../../../docs/design/native-workbench.md)、[参考与 skill](../../../../docs/research/14-ui-craft-and-native-references.md)。状态：macOS 独立样例已验证；视觉候选尚未最终确认，生产装配未完成。

## 环境、版本与本轮上游核对

- macOS 27.0 / 26A428，arm64，AppKit 原生后端；沿用已构建的 NSH CLI 与默认 native-host，本轮只构建 TSX 工件，未重新编译 Rust 或升级运行时。
- 固定 checkout：`913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。实际 host SHA-256：`42b60abbdf6a332757af0ce94dec578ce11b1388978ab0d19cbd068f40d3edc9`。
- 开始核对 main 为 `091049a67e5597787a11c7f154d45fb93c49701d`；收尾再次核对并只抓取远端引用，main 为 [`a3cc0c663b25a56c228cb3a5229586224dab8218`](https://github.com/longbridge/nsh/commit/a3cc0c663b25a56c228cb3a5229586224dab8218)。固定 checkout 未变。该提交补 GPUI 与 headless 的窗口键盘匹配；推断：它不能替代本轮 AppKit 实窗观察，后续多窗口资格需独立验证。
- 官方 [releases](https://github.com/longbridge/nsh/releases) 与当前官方设计指南已核对；稳定版 v0.4.0 的 tag 引用 `06771fff7150e9ae115a0edde2ab764ec4e8454d`，nightly 引用 `36b91689c1e7c38379179a56381de7ddab16c6a5`。本轮没有把新能力直接合入固定运行时。
- 复现 profile：1280×820 与 900×680。两个工件的 JS/样例源相同，仅启动尺寸与伴随元数据不同，见 [1280工件](artifact-1280.json)、[900工件](artifact-900.json) 和 [hash清单](manifest.json)。截图原始编码为 JPEG，文件按实际编码命名，没有修图。

## 通过的本轮场景

| 场景 | 实际观察 | 证据 |
|---|---|---|
| NW-01 外壳与列表 | 默认工作区集合；打开对象后显示全局导航、工作区列表和当前视图；分组与选中态一致 | [总览](01-workspace-light.jpg)、[列表/文稿](02-master-detail-light.jpg) |
| NW-02 视图几何 | 文稿使用排版宽度，画布使用自己的工作面，会话与阅读平级打开；宿主不将它们统一装进纸张卡片 | [画布浅色](03-canvas-master-light.jpg)、[深色](04-canvas-master-dark.jpg)、[会话](09-session-dark.jpg)、[文本阅读](08-reader-dark.jpg) |
| NW-03 输入保留 | 原生输入加入“先把最常用的操作放到手边，再处理低频工具。”；切换画布后返回，以及关闭标签再打开，文字仍在当前窗口 | [保留](05-draft-retained.ax.txt)、[关闭](06-tab-closed.ax.txt)、[重新打开](06-tab-reopened.ax.txt) |
| NW-04 搜索与退出 | 打开即聚焦；“阅读”按类型过滤；Esc 清空，再 Esc 退出；重新搜索后回车打开阅读视图；模态期间背景从 AX 树隐藏 | [搜索](07-search-dark.jpg)、[过滤AX](07-search-dark.ax.txt)、[清空](07-search-cleared.ax.txt)、[退出](07-search-escape.ax.txt)、[回车](08-search-enter-reader.ax.txt) |
| NW-05 面板预算 | 打开信息面板时内容列表收起；信息绑定当前对象，未增加第四栏 | [面板替换列表](10-inspector-replaces-list.jpg) |
| NW-06 窄窗配置 | 在900×680启动；信息面板打开时收起全局导航；返回恢复导航；显式打开局部列表时让出全局导航空间 | [信息](11-narrow-inspector.jpg)、[恢复AX](11-narrow-navigation-restored.ax.txt)、[局部列表](12-narrow-list.jpg) |
| NW-07 局部滚动 | 窄窗正文滚动，工具栏、对象标签和局部列表保持位置 | [滚动](13-narrow-document-scroll.jpg) |
| NW-08 真实状态 | 会话发送禁用，未连接任何 Agent；输入为窗口内草稿，没有正式保存或执行成功声明 | [会话AX](09-session-dark.ax.txt)、[文稿](05-document-master-dark.jpg) |

部分 AX 文件是操作前后的增量输出，和对应的全量 `.ax.txt` 快照共同阅读。最后恢复1280×820的浅色文稿/列表窗口，见 [最后观察](final-ui-observation.json)。

## 限制与过程问题

所有书目/文稿/关系/会话均为自写 fixture。TSX import 是实验内分离，不是已安装的普通插件或隔离执行证明。没有连接 Core、Agent 或外部服务，没有验证正式保存、恢复、真实书籍格式、包安装、性能、读屏完整性、字体放大或其他平台。用户最后提供的封面书库图仅进入阅读插件设计，完整插件工作区仍待下一组合切片。

最初共用宿主二进制硬链接时，独立应用窗口发现超时；复制出相同字节的独立可执行文件后恢复。原因尚未精确定位，不能宣称上游业务崩溃。首次搜索浮层的左上角位置和键盘退出问题已改用原生 Command 修正；深色浮层 token 改为响应式读取。一轮拖拽缩放没有改变窗口，因而本轮窄窗结论来自独立启动配置，**不宣称连续拖动缩放通过**。

[早期迭代](iterations/README.md) 仅保留排查过程，其抓取版本不统一，不计入最终资格集。完整 SL-01–12、公开 SDK 的通用区域组装及真实功能/数据/恢复旅程继续单独验收。
