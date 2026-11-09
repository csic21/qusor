# UI 细节、设计 skill 与新的原生样例

2026-11-09。关联 F-UX-01/02、F-EXT-01/03 与 M-1。接续 [布局研究](13-shell-layout-interaction-study.md)，本轮重点是实际窗口的视觉和操作细节。

## 用户明确的参考

用户确认 T3 指 **T3 Code**，喜欢 macOS 自带应用，反感 DeepSeek Harness Desktop 的 Web 壳观感；随后提供 Plannotator 截图，并表示也喜欢。产品继续是通用基座，阅读只是插件。具体 token、尺寸与本轮样例仍为设计候选。

| 参考 | 实际观察范围 | 对 QUSOR 的取舍 |
|---|---|---|
| [T3 Code 官网](https://t3.codes/) | 官网的 [2880×1800 深色界面图](https://t3.codes/_astro/app-desktop.BHuzLVLF.webp)，通过浏览器实看；源码检出到 `ea876cc4acc28287974d87b1ede2e5725f6c947d`，仅辅助核对样式入口 | 学习紧凑列表、安静的工作面与局部工具层次；Agent 会话是一个可选视图 |
| [Apple macOS HIG](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos) | 官方 Markdown 正文；本机 NSH 实窗的系统窗口、输入与菜单路径 | 平台字体、适合鼠标键盘的密度、窗口与菜单习惯；各平台保留有意义的差异 |
| 用户提供的 Plannotator 截图 | 对话中的静态截图；未据此推断其技术栈、版本成熟度或完整交互 | 灰色外框、独立的列表/工作面板、轻分组、柔和选中态和靠近对象的操作；只在空间合适时展开列表 |
| 用户提供的书库截图 | 用户明确指定为阅读插件参考；只观察所附图片，没有识别其产品、技术栈或支持格式的实际资格 | 阅读插件拥有书库切换、格式分类、检索筛选、封面网格和列表；基座支持这种插件工作区形态，不采用书库作为全局导航 |

截图中的三栏是“导航—对象列表—当前工作”的连续关系。右侧能呈现结构化选择，说明主内容可以因任务而异；这支持 QUSOR 的可替换视图方向。它不意味着所有插件都要使用三栏或对话界面。

书库图的重点是封面保留各自比例、按底边对齐，名称/作者退到次级文字，检索/筛选聚集在书库工具区。阅读插件可在基座内提供独立书库工作区；宿主应能收起通用导航和对象列表，并保留可信返回、命令、权限与恢复入口。这是本轮新增的组合设计要求，当前通用对象样例还没有实现完整书库形态。

## 本轮选用的技能

使用方式是读取固定源码、按当前任务适配；没有全局安装或运行外部 skill 引擎。筛选依据是源码可查、对工具软件的细节有帮助、能适配 NSH，没有客观的“顶尖排名”。

| Skill | 来源与固定版本 | 本轮应用 |
|---|---|---|
| `apple-design` | [emilkowalski/skills](https://github.com/emilkowalski/skills/blob/5206a950b715e0cabe7043ea0b2304332b91494e/skills/apple-design/SKILL.md)，`5206a950b715e0cabe7043ea0b2304332b91494e` | 平台字体、字重/行高配套、清楚的层次、即时反馈、可操作样例。这是社区 skill，其 Apple 相关论述以官方资料核对；Web 动画示例仅作思路参考 |
| `emil-design-eng` | [同仓库源码](https://github.com/emilkowalski/skills/blob/5206a950b715e0cabe7043ea0b2304332b91494e/skills/emil-design-eng/SKILL.md)，同一提交 | 高频操作保持直接；检查未选中/选中、按下、焦点、浮层、输入保留与退出；在实窗里反复修正细节 |
| NSH design guides | [固定源码](https://github.com/longbridge/nsh/tree/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/skills/nsh-design-guides)，沿用固定 runtime | 复用 `nsh/base` 的 Button、Tab/Tabs、Menu、Dialog、Input 和 Command，保持窗口拖动与系统窗口控制 |

另检查 `ibelick/ui-skills` 的 `baseline-ui`。其 Tailwind/React/Motion 栈要求不适合作为 NSH 实现指令，保留为约束清单参考。上一轮的 interface-design、Impeccable 与 Anthropic frontend-design 筛选见研究 13；不因新增候选而引入 Web UI 依赖。

## 从代码审查到实窗修正

| Before | After | Why |
|---|---|---|
| 所有功能装进同一个纸张矩形 | 集合、文稿、画布和会话分别决定内部几何；宿主只组合区域 | 功能插件可以有适合自己的工作形态 |
| 常驻 Agent 状态占据整窗底部 | 未连接状态留在会话样例，通用外壳让出主内容 | 手动工作同样是主路径 |
| 对象入口混进全局侧栏 | 稳定全局导航，加可收起的当前工作区列表 | 对象切换保持上下文，也控制重复入口 |
| 首次搜索浮层停在左上角，输入/退出不顺 | 居中上方的原生 Command；自动聚焦、过滤、回车打开、Esc 清空/返回 | 视觉、键盘和退出路径共同构成体验 |
| 深色浮层可能保留模块初始化时的浅色值 | token 在响应式样式读取，菜单和搜索一起随外观变化 | 材质、文字和状态必须成套变化 |
| 继续叠加左右面板 | 信息面板与对象列表择一；窄窗优先给主内容空间 | 布局预算需要真实窗口检验 |

最新 [视觉/交互候选](../design/native-workbench.md)、[运行入口](../../experiments/nsh-reader/native-workbench/README.md) 与 [最终证据](../../experiments/nsh-reader/evidence/native-workbench/README.md) 分开维护。历史截图只记录迭代，不当作同一工件的资格集。
