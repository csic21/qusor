# 通用基座的布局与交互研究

日期：2026-11-09。关联 C-07/C-11/C-12/C-13/C-15/C-16，F-EXT-01/04/06/07、F-UX-01/02、F-PLAT-01、F-AGENT-01–05。

本轮用户明确指出最新界面仍不理想：QUSOR 是基座，阅读属于插件，优先研究合理布局与整体交互，并寻找高质量设计 skill。**已确认的是这些要求；本文的工作区模型、区域预算和交互方案是研究建议，尚非用户最终批准或运行验收。** 不扩大业务包、Agent 接入、云服务或运行权限的实施范围。

交付对应 [外壳布局与交互规格](../design/shell-layout.md)、[布局板](../design/assets/shell-layout-study.svg) 与 [流程板](../design/assets/shell-context-flow.svg)。本轮审查实际原生截图与 TSX，查阅官方机制和技能固定源码；没有把静态线框计为 NSH 原型。

## 1. 问题不止是视觉

审查对象为 [当前宽窗书架](../../experiments/nsh-reader/evidence/native-reading-entry/42-receipted-ready-shelf.jpg)、[App.tsx](../../experiments/nsh-reader/native-design/App.tsx)、[ReadingWorkspace.tsx](../../experiments/nsh-reader/native-design/ReadingWorkspace.tsx) 和现有分区规格。

| 实际观察 | 对用户的影响 | 应改变的设计规则 |
|---|---|---|
| `workspace` 默认取 `readingEntry.id`，书架入口直接写进全局侧栏 | 第一次打开就把整个产品解释为阅读软件；贡献 fixture 并没有形成真实插件边界 | 初次进入通用总览，之后恢复用户最后工作；阅读由已安装贡献和用户固定项进入 |
| 功能入口、任务列表、空间入口放在同一侧栏；任务查找与书架查找并存 | 不易判断“切换工作”“打开对象”和“筛选当前集合”分别影响什么 | 分开空间/工作区、功能目的地、打开对象和后台活动；全局搜索与局部筛选写明范围 |
| `Research / Writing / Process / Development` 由宿主分支选择，非阅读内容大多嵌在同一张最大宽度 790 的纸面中 | 场景更像替换文章；表格、流程、代码和画布的操作密度没有独立成立 | 统一的是外壳交互；主区域由内容类型决定版式，不强制文稿材质与版心 |
| 非阅读场景底部一直出现“Agent 尚未连接”，至少占 61pt | 尚未参与工作的能力持续争夺主区域，也把手动操作解释为 Agent 的附属品 | 未使用 Agent 时不常驻连接条；执行者属于具体会话/任务，活动按需集中 |
| `enterReading / returnShelf` 直接隐藏或展开外壳侧栏 | 功能流程覆盖用户布局选择；返回资料集合与恢复全局导航耦合 | 功能只提出呈现需求，用户布局与宿主可用空间负责安放 |
| 现有默认宽度同时预留全局导航 208、局部导航 224、主内容至少 560、辅助区 336 | 合计 1328pt，尚未计分隔线；在 1100/1280 窗口不能同时成立 | 默认共享一个导航区；从任务最低可用宽度计算组合，不能只写“自适应” |
| 场景切换重置当前标签，示例没有通用的多对象标签身份 | 从阅读去看成果/另一插件时，原位置与未提交输入缺少一致约定 | 定义对象、视图、导航历史和执行寿命；标题变了不等于对象切换机制完成 |

这些是源码/截图支持的专家走查结论，不是用户实验的成功率或耗时测量。现有保存恢复与原生实验成绩保持各自版本边界。

## 2. 设计 skill 的筛选与使用

通过 `npx skills find 'interface design'`、`npx skills find 'interaction design'` 发现候选，再从作者仓库读取实际 `SKILL.md`，不使用安装量或营销描述证明设计水平。固定提交见 [来源记录](../design/assets/shell-foundation-research.json)。

| 候选与一手来源 | 实际适用性 | 本轮处理 |
|---|---|---|
| [interface-design](https://github.com/Dammyjay93/interface-design/blob/2f9be3206855bcb2d1d0af262c8bae25cba6658d/.claude/skills/interface-design/SKILL.md) | 面向产品工具，要求先明确人、动作、导航与层级；复用既有组件和语义 token | 采用意图、主次、比例和状态走查方法；不照搬其网页实现或“避免系统字体”等偏好 |
| [Impeccable](https://github.com/pbakaus/impeccable/blob/613fbf88e0ef08709eaf238653285f430250c0ea/.agents/skills/impeccable/SKILL.md) 的 [shape](https://github.com/pbakaus/impeccable/blob/613fbf88e0ef08709eaf238653285f430250c0ea/.agents/skills/impeccable/reference/shape.md) 与 [Operate](https://github.com/pbakaus/impeccable/blob/613fbf88e0ef08709eaf238653285f430250c0ea/.agents/skills/impeccable/reference/operate.md) | 区分执行工作与内容阅读，强调流程、状态和熟悉的产品操作 | 作为研究资料，吸收先定交互后做表现、按表面选密度的原则；未执行其引擎、评分、多代理或 hooks 流程，不声称通过其自动审查 |
| [UI UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill/blob/4f2dbb67e64969a9a30ab00742f2d55d66ddec2b/.claude/skills/ui-ux-pro-max/SKILL.md) | 有可检索的样式、配色、UX 与技术栈指导 | 留作细节检查候选；样式检索不能替代基座信息架构，当前支持列表也不能证明 NSH 适配 |
| [Anthropic frontend-design](https://github.com/anthropics/skills/blob/2c5958ecac7cee2b583967a2c4981d0f4a75c579/skills/frontend-design/SKILL.md) | 强调具体内容、视觉意图、自我审查及避免模板 | 适合后续视觉表达；本轮不以 hero、装饰字体或网页出场动效定义工作台 |
| [NSH Design Guides](https://github.com/longbridge/nsh/blob/091049a67e5597787a11c7f154d45fb93c49701d/skills/nsh-design-guides/SKILL.md) | 原生窗口、区域滚动、焦点、菜单、跨端行为与自有样式层 | 作为平台约束使用；继续采用系统 UI 字体、真实原生控件和已有 token |

采用顺序为：**QUSOR 产品事实 → 信息架构与交互 → 平台约束 → 视觉精修 → 真实验证**。技能之间不靠叠加数量提高质量。通用技能若与用户或项目不变量冲突，以后者为准；例如网页的 CSS 动画限制不等于原生性能事实，技能提到的触控尺寸也不能单独当作 WCAG 合规证明。

本轮按固定源码读取使用相关方法，没有安装全局技能、运行第三方安装脚本、建立新的设计文档体系或启用 hooks。后续如安装，优先通过实际工具核实技能名与版本；常见入口是 `npx skills add Dammyjay93/interface-design --skill interface-design`。Impeccable 当前是合并技能，不能依据旧索引假设 `critique` 仍是独立可安装包。

### 本轮设计简报

- 使用者：会长期在同一工具中切换资料、写作、编程和 Agent 工作的人；有连续工作，也有临时打开一个对象即离开的操作。
- 主要动作：找到/打开当前工作对象，按需要组合另一个对象或工具，离开后能回到原上下文。
- 领域词汇：空间、对象、来源、视图、工作区、能力、活动、草稿、成果。项目、书籍和会话是具体对象，不是宿主的唯一模型。
- 材料与色彩意象：中性设备表面、清晰的墨色文字、分区灰阶、选中状态的单一强调色、内容自身的纸面/图像/语法色。后者属于插件内容，不染成全局阅读主题；本轮不重选品牌色。
- 识别性来自交互连续：同一个对象可换视图、同一工具可改变安放方式，返回仍保留身份/位置/输入；不是通过复制某款软件的侧栏证明“高级”。
- 明确排除的默认套路：功能商店首页、所有事情先建 Agent 任务、所有内容放同一张文稿纸、安装一个插件就增加一个常驻导航图标。

## 3. 从成熟产品取机制

以下均于 2026-11-09 查阅官方资料。事实仅指文档描述的机制；右侧取舍是 QUSOR 的设计推论，不是这些产品替 QUSOR 作的验证。

| 研究问题 | 官方证据 | 对 QUSOR 的采用与限制 |
|---|---|---|
| 插件放在哪里 | [VS Code UX 总览](https://code.visualstudio.com/api/ux-guidelines/overview) 把容器与其中的视图区分；[Views](https://code.visualstudio.com/api/ux-guidelines/views) 建议减少贡献的视图数量 | 学受控贡献与统一容器；不采用代码编辑器、文件树、左图标栏和右聊天栏作为默认产品结构 |
| 导航为何会失控 | [Apple Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) 讨论空间成本、分组及层级深度；[Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars) 区分顶层导航与动作 | 保留稳定目的地和明确作用范围；插件命令不伪装一级目的地，移动导航不按插件数量增长 |
| 对象与布局如何保留 | [Obsidian Tabs](https://obsidian.md/help/tabs) 支持对象标签、分组及关联视图；[Workspaces](https://help.obsidian.md/Plugins/Workspaces) 保存布局 | 学标签身份、用户控制的并置和布局恢复；不照搬无限分栏与图标密集侧栏，也不把布局恢复当作内容持久化 |
| 何时主从、何时并排 | [Android Canonical layouts](https://developer.android.com/develop/adaptive-apps/guides/canonical-layouts) 区分集合/详情与主内容/支持视图，并说明窄屏转换与状态保留 | 独立对象可并排；目录/检查器围绕对象；紧凑呈现转页时保留所选对象和返回路径，不压缩成细条 |
| 多种工作怎样用同一外壳 | [Blender Workspaces](https://docs.blender.org/manual/en/latest/interface/window_system/workspaces.html) 用不同编辑区域组成面向工作的布局 | 学工作区预设与可替换区域；不继承专业 DCC 的自由拼接学习成本，不以布局名称代替执行状态 |
| 大量命令如何可发现 | [Raycast Action Panel](https://developers.raycast.com/api-reference/user-interface/action-panel) 按当前对象提供分组动作；[Navigation](https://developers.raycast.com/api-reference/user-interface/navigation) 使用导航栈 | 学命令作用域、主动作与更多动作；不把整个长时间工作台压进启动器。Esc 必须先尊重输入法和当前控件 |
| 标签怎样支持键盘 | [W3C Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/) 区分焦点移动和激活，讨论慢内容的手动激活 | 原生实现采用等价键盘行为；这是行为参考，不要求在 NSH 中模拟 DOM/ARIA，也不证明现有控件已经合格 |

共同点是：**稳定的位置关系、有明确身份的内容、可预测的局部动作、跨切换保留上下文**。它们不能证明固定三栏最优，更不支持“所有工具始终可见”。

## 4. 比较四种外壳模型

不用未经用户实验支持的数值评分；比较实际工作代价。

| 模型 | 适合的工作 | 在 QUSOR 的主要代价 | 结论 |
|---|---|---|---|
| 功能启动台：先选阅读/写作/开发 App | 少量独立、短时工具 | 跨插件来回跳，材料与成果分散；插件增多后变成应用抽屉 | 作为“全部功能”入口，不作为每次工作的必经首页 |
| 任务/聊天中心：先描述目标，再运行 Agent | 委派、自动化与异步工作 | 看文档、改表格也得先建任务；手动操作依附聊天，未连接 Agent 时显得不可用 | 作为任务插件的完整主视图，不作为基座前置 |
| 对象工作区：打开资料、草稿、会话或流程，可按需组合 | 长时间手动与 Agent 混合工作 | 需要严格定义标签、视图、面板绑定与恢复，不能只做一条标签栏 | **推荐作为基座骨架**；功能目的地与任务仍可进入同一主区域 |
| 完全自由的停靠画布：所有窗口随意拼接 | 少数高密度专业工作 | 配置成本、焦点/恢复/触控负担高；插件可把界面变成小窗口集合 | 后续高级能力候选；先提供少量可解释的组合 |

推荐“对象工作区”不表示只允许文件。表单实例、集合页面、会话、流程、画布都可以有稳定身份；也不要求用户理解架构术语。首次打开只有一个默认工作区，直接工作即可，复杂分组按需要出现。

## 5. 推荐的结构与交互取舍

### 稳定外壳，内容按需组合

外壳负责空间切换、搜索/打开、对象标签、布局、返回、焦点、后台活动和可信系统界面。功能负责自己的对象、视图、动作和内容密度。阅读入口来自阅读插件；代码、表格、表单、会话具有同等主区域资格。

默认是 **一个导航区 + 一个主区域**。目录/文件树等局部导航默认复用导航区，通过明确的“空间 / 当前内容”切换选择范围；当前对象和空间身份始终保留。该方案的代价是多一次范围切换，所以必须有快捷键、工具栏入口和最近范围恢复，并在用户测试中与双导航方案比较；不能把它写成已经证明最佳。

需要比较两个独立成果时使用并排主视图；目录、评论或属性使用支持视图。Agent 对话可以成为主视图，也可按用户动作成为支持视图。后台日志放活动/输出，不与对话及当前对象属性混成一个“万能右栏”。

### 恢复上下文，比增加面板重要

打开另一插件的对象时，默认在工作区新增或激活对象标签；已有脏视图不能被替换。辅助工具显示“跟随当前对象”或“固定到某对象”。已有输入时不暗中跟随新对象，先保持原目标，明确提示后再切换。

切换空间、工作区、对象、局部历史分别有状态；返回不等于关闭，关闭不等于删除或停止执行。安装不等于固定入口，更不等于授权或运行。可恢复状态只在真实耐久提交后标为可重开恢复。

### 六端是状态映射，不是同一张图缩放

桌面用可调整区域和对象标签，手机用一个活动对象及系统导航栈。相同工具从旁边变成一页时，不新建第二个编辑会话、不重发任务。窗口变化按当前输入和区域最小宽度决定；尺寸阈值是推导结果，不是无条件的平台断点。

详细尺寸算例、打开/并排/返回/关闭、插件增长、活动与失败行为见 [执行规格](../design/shell-layout.md)。

## 6. NSH 上游与实现可行性

2026-11-09 重新核对 [官方发布](https://github.com/longbridge/nsh/releases)：stable 为 v0.4.0；`git ls-remote` 核对 main 为 `091049a67e5597787a11c7f154d45fb93c49701d`，nightly 为 `36b91689c1e7c38379179a56381de7ddab16c6a5`。只更新本地源码缓存的远端引用，运行 checkout 仍为 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。

读取最新固定 [Design Guides](https://github.com/longbridge/nsh/blob/091049a67e5597787a11c7f154d45fb93c49701d/skills/nsh-design-guides/references/design-guides.md)，并复读当前运行基线对应指南。指南支持按角色选择稳定外壳、显式焦点/区域滚动、系统字体与平台导航；组合布局和各端能力仍须实际探针。

近期五条提交已从 Git 核对：窗口快捷键只由所属窗口处理、Linux 单项菜单高度修正、Android API26 运行、Windows 分数缩放图片测试、网站锚点修正。前两项与多区域焦点/菜单相关，应在隔离升级时回归；Android 提交不自动改变 QUSOR 最低支持版本或设备资格。此次不升级运行基线、不宣称新导航/停靠已经可用。

## 7. 下一次交付如何证明改进

先验证通用外壳，而非再把一个阅读页做漂亮：

1. 同一外壳至少装入阅读、表格/流程、开发、独立会话四种有不同主交互的 fixture，移除阅读 fixture 后外壳仍能完整解释和使用。
2. 做“打开 → 并排 → 支持工具 → 切换对象 → 返回”的连续原生交互；每步保留对象、位置、输入和工具目标。fixture 验证不计普通插件 SDK 交付。
3. 在 1440/1280/1100/800/390、较大字号、键盘和触控路径记录布局/焦点；六端分别实测，不能用窄 Mac 窗口代替手机。
4. 以 0/5/30 个插件贡献与 1/8/30 个打开对象检查可发现性、溢出和固定顺序；关闭、缺提供者和布局恢复失败都有路径。
5. 原生骨架通过后，先让阅读与一个非阅读普通插件走同一贡献/权限链，再把真实本地阅读闭环装入；不继续给每个功能增加宿主分支。

当前没有受试者可用性实验或上述原生骨架的新成绩。不得填“易用性通过”、主观总分或未经测量的效率提升。此次可审查产物是问题证据、来源、方案比较、规格、静态布局/流程板及相应文档检查。
