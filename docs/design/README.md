# QUSOR 界面与交互设计

2026-11-09导航修正：用户指出重复侧栏开关易误导，收起后看不出插件下钻。新增[基座与插件导航层级](navigation-hierarchy.md)：一个宿主导航区、稳定的工作区路径与插件身份、具名的局部/外层返回和来源布局恢复。该增量优先于下方旧候选的双导航呈现，具体外观继续待评审；实际范围见[实窗记录](../../experiments/nsh-reader/evidence/native-navigation-hierarchy/README.md)。

2026-11-09最新视觉方向：用户确认参考 T3 Code、macOS 自带应用和所附 Plannotator 界面；另提供书库图，明确仅用于阅读插件，基座须能适配这种插件工作区。先读 [UI技能与参考取舍](../research/14-ui-craft-and-native-references.md)、[原生工作台候选 v0.4](native-workbench.md) 与 [实窗证据](../../experiments/nsh-reader/evidence/native-workbench/README.md)。本机已制作可操作样例；具体视觉仍待用户评审，正式包、数据与恢复另验。结构沿用 [外壳布局 v0.3](shell-layout.md)，不以阅读、代码或固定对话定义整个基座。

更新：2026-11-09。功能结构沿 [通用 Agent 基座与场景组合](agent-foundation.md)，视觉改按 [原生软件方向](native-art-direction.md) 评审。用户进一步否定已有稿的 Web 感，要求参考具体设计师的软件作品；v0.1/v0.2 与早期 HTML 稿保留为历史流程草案，不再作为当前视觉方案。本轮有独立 NSH 实窗样例，仍非生产装配或最终品牌；六端资格分别验证。

## 从研究到可交付界面

| 文档 | 回答的问题 |
|---|---|
| [导航层级与返回](navigation-hierarchy.md) | 收起、下钻、标签切换怎样区分；如何辨认当前插件并返回真实来源 |
| [原生工作台 v0.4](native-workbench.md) / [研究 14](../research/14-ui-craft-and-native-references.md) | T3 Code、macOS、Plannotator 的取舍，固定版本 UI skill、实窗修正及阅读插件参考边界 |
| [基座布局与交互研究](../research/13-shell-layout-interaction-study.md) | 最新界面哪里偏离基座；设计 skill 如何筛选；四种外壳模型怎样取舍 |
| [外壳布局与交互 v0.3](shell-layout.md) | 对象、功能、任务怎样分层；区域预算、切换/返回、工具目标与插件增长怎样落实 |
| [布局板](assets/shell-layout-study.svg) / [流程板](assets/shell-context-flow.svg) | 同一外壳承载不同插件的结构，以及输入绑定和紧凑返回；均为静态研究稿 |
| [六端基座与插件安放](cross-platform-shell.md) | 桌面/移动怎样共享语义，导航、标签、返回、面板与插件增长怎样保持秩序 |
| [原生软件作品研究](../research/12-native-software-design-study.md) | 具体作者、实际作品图片、观察范围及对 QUSOR 的取舍 |
| [原生视觉方向](native-art-direction.md) | 平台字体、工具区、内容舞台、材质和控件怎样落实 |
| [可运行 NSH 样例与证据](../../experiments/nsh-reader/native-design/README.md) | 原生书架、窗口内文本读取/选文/复制、宽窄布局，以及任务输入、标签和菜单；尚未正式入库或装配插件 |
| [通用 Agent 基座](agent-foundation.md) | 宿主、Agent接入、工具、视图和场景怎样组合；不同任务怎样改变主交互 |
| [基座机制补充研究](../research/11-agent-foundation-composition.md) | 从原生扩展、场景包、工具UI和Agent协议得到哪些取舍 |
| [设计基础](foundations.md) | 吸收了哪些设计思想；颜色、字体、间距、组件与状态如何统一 |
| [产品设计研究与结论](../research/10-product-design-synthesis.md) | 阅读、知识、开发、Agent、插件和手机各应借鉴谁，采用什么、舍弃什么 |
| [视觉精修与对象标签 v0.2](visual-refinement.md) | 字阶、空间、主题、横向/纵向标签和窄窗怎样落实；候选 token 尚未接通运行时 |
| [软件分区与组件扩展](composition.md) | 内容放在哪里；谁提供组件；如何替换、卸载与恢复 |
| [本地阅读工作区](reading-workspace.md) | 资料库、阅读、选段、写想法、保存和回源如何连贯 |
| [背单词工作区](vocab-workspace.md) | 一屏一张卡、先回想后揭晓、四档评分、音标与系统读音；样例数据与真实 Core 分别标注 |
| [软件更新面板](update-dialog.md) | 侧栏入口、检查/下载/安装状态、失败与不能自替换的文案；证据模式与真实界面行为分开 |
| [想法编辑与提供者切换](paragraph-editor.md) | 逐段/全文、待处理输入、编辑器替换与失败恢复如何协同 |
| [开发工作区](development-workspace.md) | 怎样把项目、代码编辑、Agent、终端、Git、运行和预览放在同一工作流；2026-11-09新增设计，尚未实现 |
| [设计验收](review.md) | 每个功能怎样同时验收 UI、交互和真实业务结果 |
| [已有研究](../research/07-ui-ux-design-study.md) | 原始参考、证据范围及尚未验证的假设 |

阅读顺序：外壳研究 → 布局与交互 → 分区契约 → 对应场景流程 → 原生视觉与验收。工程边界继续以 [目录架构](../architecture/workspace-layout.md) 与 [ADR 0028](../architecture/adr/0028-extensible-native-workspace.md) 为准；功能范围以 [scope](../product/scope.md) 为准。

## Agent 工作台扩展

用户于 2026-11-08 明确新增本机编程、电脑操作、多 Agent 委派和会话管理需求。见 [工作台 UI/UX/UE](agent-workbench.md)、[目标界面](assets/agent-workbench.svg) 与 [功能规格](../product/agent-workbench.md)。任务工作状态沿用统一外壳和可替换组件，阅读保持安静；这是设计扩展，尚未接通外部 Agent。

2026-11-09，用户进一步明确希望今后在 QUSOR 中完成日常编程。新增 [项目开发工作区](development-workspace.md)：代码、任务、会话、终端与成果检查共享项目/工作树身份，主视图可在代码、任务、差异和应用预览之间切换。桌面新布局方向已得到用户认可，手机方案仍为待评审设计；原有静态稿保留为 v0.1 历史基线。

## 可视设计稿

- [软件分区](assets/composition.svg)：稳定区域、组件角色、提供者与Core的关系。
- [资料库](assets/library.svg)：稳定导航、继续阅读和资料列表。
- [专注阅读](assets/reading.svg)：正文优先，目录与工具按需出现。
- [选段与想法](assets/capture.svg)：引用、观点、保存状态分开，并可回到原位置。

图中内容为自拟示例。视觉细节是可评审的工作决定，不代表用户已经批准最终品牌方案，也不证明对应功能已实现。当前稀疏 PDF 探针只保留为技术证据，后续正式界面按本目录实施。

SVG是可编辑原稿，同名PNG用于直接预览；原四份设计稿由 [离线生成脚本](assets/generate_boards.py) 生成，新布局/流程板由 [外壳研究脚本](assets/generate_shell_boards.py) 生成。仅使用自拟内容和矢量元素，未加入Web产品实现或前端依赖。

## 当前采用的方向

依据用户最新纠正，基座负责稳定导航、对象/视图、布局、焦点和恢复，阅读、开发、任务与 Agent 通过普通贡献组合。[v0.3候选](shell-layout.md)优先解决结构和连续交互；主内容可以是会话、资料、文稿、表单、画布、过程或代码，手动操作不以创建任务或连接 Agent 为前置。视觉仍沿原生方向精修，具体候选均未最终冻结，其他功能排期不因此提前。

后续交付采用可运行的纵向流程：先画清主流程和状态，再把原生界面与真实数据一起接通；审查同一批中文材料、窄窗、错误恢复和键盘路径。功能验收必须同时给出界面证据与业务证据。
