# 下一阶段：插件支撑的最小本地阅读闭环

2026-11-10 公开想法读取能力：新增 Host API 0.4、`notes.read` 能力与 `notes_workspace` 贡献类型，以及 `notes-query-v1` 有界 wire（分页≤6条、标题/预览≤96 bytes、正文块4–2048 bytes、整响应≤4 KiB）；Host 绑定 vault 并复用既有授权/许可失效，0.1–0.3 保持原义。wire 与 Host 回归通过；可信适配器已在真实 SQLite Core 上通过分页、正文分块重组、控制字符收缩与错误映射，见 [ADR 0035](../architecture/adr/0035-notes-read-capability.md) 与[能力记录](../../experiments/capture-core/evidence/notes-read-capability/record.json)。`plugins/notes` 普通签名包、安装/授权链路与真实窗口仍未完成，见 [D 阶段计划](d-phase-plan.md)。

2026-11-10 想法工作区 UI v1：`native-workbench` 按正式[交互规格](../design/notes-workspace.md)实现想法工作区——范围导航、搜索、列表/详情、文本选取工具条、右键原生菜单、复制引文/想法/引用、缺原件与其他资料状态、完整正文读取状态与窄窗切换；`nsh check` 类型检查、bundle 构建与 6 项 headless 交互测试（`nsh test`）通过，另有四张 macOS 实窗截图。数据仍是窗口内 fixture，未接 Core、未成为安装包；[UI v1 记录](../../experiments/nsh-reader/evidence/notes-workspace-ui/README.md)。下一步是把 UI 接到真实 Core：公开 `notes.read` 能力 + `plugins/notes` 普通签名包 + 分页/正文预算，见[D 阶段计划](d-phase-plan.md)。

2026-11-10 想法读取契约增量：`learning-application` 的想法查询改为 keyset 分页（单页最多 200 条，游标绑定 vault/document 精确范围，分页期间新增想法不重排/不跳项），并新增 Quote/Note 正文分块读取（4–16 KiB、UTF-8 边界安全、1 MiB 笔记可逐块还原，缺失/越界/预算错误明确分类）；真实 SQLite 关闭重开后恢复同一分页与完整正文。决策见 [ADR 0034](../architecture/adr/0034-note-read-model.md)，本机记录见[想法读取契约记录](../../experiments/capture-core/evidence/note-query-contract/record.json)。该增量是应用层只读契约，不是正式 notes 包；公开 SDK wire、插件授权、分页 UI、精确高亮回源与原生窗口接线仍待 D0 余项与 D1，见 [D 阶段计划](d-phase-plan.md)。

2026-11-10 想法读取与布局集成增量：`learning-application` 新增按 vault/document 隔离、倒序、带 UTF-8 有界预览的 PDF 想法查询；SQLite 关闭重开后恢复同一列表，动态 PDF 实验工作区已真实显示 Core 中两条旧中文想法。随后在统一 `native-workbench` 的 `PluginWorkspace` 下钻机制中加入“想法”插件工作区样例，走通宿主入口→插件自有导航/列表/详情→插件内返回→宿主外层返回；该工作台仍为窗口内 fixture，真实 Core 查询尚未通过公开插件契约接入。该列表是 D 阶段读取基础，不是正式 notes 插件，也未完成完整正文编辑或精确高亮回源。证据见[Core 想法列表记录](../../experiments/nsh-reader/evidence/notes-workspace/README.md)与[插件工作区集成记录](../../experiments/nsh-reader/evidence/native-notes-workspace/README.md)，正式包门槛与后续顺序见[D 阶段计划](d-phase-plan.md)。

2026-11-09导航层级修正：按用户截图暴露的问题，原生样例将双导航收敛为一个可切换范围的导航区，顶部持续显示所属工作区与插件身份，局部返回/离开插件分别标明目的地；外层返回恢复来源对象、导航偏好和信息面板。规则见[顶层导航规格](../design/navigation-hierarchy.md)，本轮工件与实窗范围见[记录](../../experiments/nsh-reader/evidence/native-navigation-hierarchy/README.md)。旧组合报告只覆盖各自冻结版本；后续真实普通插件装配须包含相同导航验收及未保存输入、位置、焦点与资源退役，不以窗口内fixture代替正式恢复。

2026-11-09 工作区组合增量：原生工作台样例已加入阅读工作区贡献。宿主以通用 `PluginWorkspace` 角色承载贡献；进入后退出通用侧栏、内容列表与标签栏，保留可信返回，阅读贡献自行提供书库导航、搜索、格式筛选、网格/列表和正文。1280×820 与 900×680 实窗通过，且从非阅读画布进入后返回仍回到原画布。这推进了“插件自有工作区与宿主返回”的界面切片；仍是普通 TSX fixture，下一步要把同一组合边界接入真实普通包、Core 与 resource retirement，不能计作正式阅读插件。证据见[工作区组合记录](../../experiments/nsh-reader/evidence/native-workspace-composition/README.md)。

2026-11-09最新 UI 增量：参考 T3 Code、macOS 自带应用及用户提供的 Plannotator，完成 [原生工作台 v0.4 候选](../design/native-workbench.md) 的 macOS 独立实窗研究。候选优先检查分区、列表/对象切换、搜索、面板预算与输入保留，见 [证据](../../experiments/nsh-reader/evidence/native-workbench/README.md)。用户另明确书库图只参考阅读插件；下一组合切片需验证插件自有书库导航/工具栏/主区及宿主返回，避免双重侧栏。正式阅读/插件/Core 的交付门槛继续保留。

2026-11-09最新设计优先级：用户再次指出基座应优先解决布局和整体交互，阅读只是插件，并要求查找高质量设计 skill。本轮先完成 [外壳机制与技能研究](../research/13-shell-layout-interaction-study.md)、[v0.3候选规格](../design/shell-layout.md) 和静态结构/流程板；下一原生界面切片先验证通用总览、对象切换、工具绑定与紧凑返回，用阅读及非阅读交互共同检验，再装入真实普通插件。候选尚待评审，现有原生工作台为独立研究切片、尚未生产装配；既有阅读、资源释放、同权包和六端门槛继续保留。

2026-11-09阅读入口与跨端增量：原生视觉样例已补书架、窗口内 UTF-8 文本、目录、选文/复制及宽窄呈现，见[独立记录](../../experiments/nsh-reader/evidence/native-reading-entry/README.md)。后续装配按[六端基座与插件安放](../design/cross-platform-shell.md)推进，宿主统一导航/标签/返回/面板，插件提供角色与适配信息。macOS 窄窗资格不替代其他平台，窗口内阅读不计作正式入库、持久进度或公开插件交付。

2026-11-09视觉增量：按用户最新要求，优先评审[原生视觉方向](../design/native-art-direction.md)与[实窗样例](../../experiments/nsh-reader/native-design/README.md)，以具体作者的软件作品研究支撑。旧HTML稿不再作为当前视觉方案；通用Agent/插件结构和既有阅读/包门槛保持，各类真实业务仍独立接通与验收。


2026-11-09 平台优先级更新：用户要求一次推进 macOS / Windows / Linux / iOS / Android / Web。先解除 Linux 专用宿主和服务绑定，完成共享平台契约、对应运行入口及每端资格记录；原有第三方插件、Core 事务与原生优先门槛保持。平台支持以真实结果表为准，不将矩阵配置当作交付。见 [ADR 0033](../architecture/adr/0033-cross-platform-runtime.md)。
新增 C-14/F-EXT-07 设计增量见 [插件平台兼容](../architecture/plugin-platforms.md)：后续包/SDK 切片先补纯兼容解析与旧协议回归，再补签名多实现包、安装/恢复，最后按真实平台/渠道验证。纯兼容解析已实现，签名包/安装/恢复接线待推进；不自动启动语音插件或改变既有阅读闭环优先级。

更新：2026-11-09。状态：用户已同意顺序及插件化目录设计，当前实施前置验证。首版 AI、云默认行为与六端平台范围见 [范围记录](scope.md)。

## 最新优先级：先沉淀设计与可扩展分区

用户于2026-11-08明确要求UI与功能同步交付，先消化已有设计研究，并设计方便插件与组件替换的软件分区。先采用 [设计工作基线](../design/README.md) 与 [ADR 0028](../architecture/adr/0028-extensible-native-workspace.md)：外壳、工作区区域、组件角色、提供者与Core数据分开；可视稿、实际窗口和业务验收分开记录。

当前C9生产控制器/ManagedRun/真实Core消费者已完成九场景统一本机资格，108份输入和消费者二进制前后hash一致；完整包/Wasm/C3兼容回归通过，见 [监督消费者结果](../../experiments/capture-core/supervised-results.md)。这不代表正式阅读插件或原生资源释放门槛完成。原生分区界面最终bundle已通过独立数据的选文/想法、窄窗焦点修复、坏文件保留、原生复制、Core保存及空库恢复，119份源码与工件封存并独立核对，见 [原生分区结果](../../experiments/nsh-reader/dynamic-pdf/WORKSPACE_RESULTS.md)。

接下来的实施先落最小组合契约、语义token与原生样式组件，覆盖资料入口、阅读主区与按需想法区；用真实本地旅程逐步接通library/reader/notes公开插件。组件替换先验证一个实际兼容角色，不一次抽象所有未来控件。必要的C9隔离/回收门槛继续收尾，每个业务增量同时给出对应UI与真实数据/恢复证据；既有AI/云/平台待定项不因此扩展。

## 当前执行队列

当前状态以本表和各冻结工件为准；下方计数保留阶段与版本边界，不合并为一次整体验收。

| 当前切片 | 已验证 | 下一步／未覆盖 |
|---|---|---|
| 原生阅读与A编辑 | [最新默认GUI](../../experiments/nsh-reader/evidence/native-frame-retirement-gui-default/record.json)：换页／缩放／连续请求、自动聚焦、保存／放弃、正常关闭和完整恢复；30 VM／100 calls全回收，155份精选输入一致，并经[独立复核](../../experiments/nsh-reader/evidence/native-frame-retirement-gui-default/root-independent.json) | 尚为实验装配；新IME、异常操作中的GUI关窗与完整格式旅程未验 |
| Rust任务与后台操作退休 | [最终原生回归](../../experiments/nsh-reader/evidence/native-frame-retirement/maincheck-record.json)：29项native库测试、Clippy及8项真实OS；[同冻结工件独立复跑](../../experiments/nsh-reader/evidence/native-frame-retirement/root-independent.json)。choose／reopen／save统一准入和在途等待 | 不代表decoded缓存、纹理或native节点释放ACK |
| 原生owner观察 | 初片GTK4三项后，[图片／缓存修正版](../../experiments/nsh-reader/evidence/native-observer-image-weak-registry/record.json)通过9个独立场景及41项Core图像回归，并经[同冻结exe独立复跑](../../experiments/nsh-reader/evidence/native-observer-image-weak-registry/root-independent.json)；真实decode／delivery、pixel／texture与在途限额分别记录 | 仍Unprovable；native子对象、outro、完整factory与finalizer边界继续独立实施，主pin／Host释放尚未接线 |
| B段落编辑与替换 | 普通包真实进程编辑、投影与共享broker共存已有证据 | 有界原生行／Replace／Move／全文入口在隔离源码准备；A→B→A须等真实旧资源退役，不能假确认 |
| 正式本地旅程 | Core保存／导出恢复与普通包前置链已有资格 | 正式library／reader／notes包、阅读回源、长书／EPUB及SDK／平台发布门槛仍未完成 |

Rust frame任务／encoded容器接线已通过默认features全目标check、Clippy及29项native lib回归，同源八项feature-only真实OS退休回归也通过，主负责人已用冻结exe独立复跑；见[主检查归档](../../experiments/nsh-reader/evidence/native-frame-retirement/maincheck-archive.json)与[独立结果](../../experiments/nsh-reader/evidence/native-frame-retirement/root-independent.json)。实际覆盖线程退出／join、旧owner隔离、130次顺序frame保留旧encoded guard、真实ImageData共享及旧动态操作的admission/wait屏障；State屏障不冒充main＋Service整体退出资格。九份源码前后hash核对及ZIP不是完整transitive构建输入资格。新默认GUI仍须单独构建／实窗验证；decoded/native释放ACK及B原生段落视图／A→B→A继续待完成，不因Rust线程或encoded容器Drop而放行替换。

普通A全文编辑已接到真实原生窗口：共享应用草稿与同Store/Host、15个实际worker/50calls、无Input额外点击的自动聚焦、正式保存及关闭后Core重开/空库恢复通过；247份源码与工件按阶段留证，详见[原生A结果](../../experiments/nsh-reader/dynamic-pdf/COMPONENT_EDITOR_RESULTS.md)。此前两次聚焦失败保留，多段来源也有真实跨两行保存恢复证据。后续真实IBus/libpinyin已验证预编辑0 VM/保存禁用、Space提交“你好”后3worker/10calls及正式保存重开/空库恢复；Esc只证明取消输入法组合保留正文，候选窗UI和全局Esc仍未验。此进度不等于B原生替换、释放ACK或正式功能包完成；下一步继续这些门槛。

NativeEditor 后续回收修复已通过六项真实 OS 故障回归及主负责人对同一冻结二进制的独立复跑：启动后 invocation/gate 失败、实际回收后持久清理失败、等待未知后的继续排空，以及同／不同安装的 lease 隔离。实际回收与持久清理分别确认，未知时保留 owner；不改变执行预算。见[回归记录](../../experiments/nsh-reader/evidence/native-editor-retirement/record.json)与[独立复跑](../../experiments/nsh-reader/evidence/native-editor-retirement/root-independent.json)。本次 qualification feature 不计作默认原生 GUI 工件。随后[默认feature GUI2](../../experiments/nsh-reader/evidence/native-editor-retirement-gui-default-2/record.json)已重新构建并通过自动聚焦、真实输入／保存、放弃确认取消及确认丢弃、正常关闭和Core重开／空库恢复：33实际worker／110 calls全部回收，持久lease为0，并经[主负责人独立核对](../../experiments/nsh-reader/evidence/native-editor-retirement-gui-default-2/root-independent.json)工件、实际回收与SQL结果。此前portal显示环境阻塞的[失败attempt](../../experiments/nsh-reader/evidence/native-editor-retirement-gui-default/record.json)独立保留。GUI2严格对应binary `7e52ffd15c223237f94985bc11c74017425a76a87c4682f6f7ffdf70cbd31883`，不覆盖新应用的[shutdown／准备失败恢复候选](../../experiments/nsh-reader/evidence/native-editor-shutdown-integration/)。该增量随后通过[八项真实OS及16项native lib回归](../../experiments/nsh-reader/evidence/native-editor-shutdown/README.md)与同冻结exe独立复跑；最新[默认GUI](../../experiments/nsh-reader/evidence/native-editor-shutdown-gui-default/record.json)也已通过：binary `59d2febbe03057061b6e4b7a2d996451fdcc40f4f224907a156b80fecd4da549`，30个实际worker／100 calls全部回收，保存与放弃确认、正常关闭、Core重开／空库恢复及原PDF精确一致，lease为0；[主负责人独立核对](../../experiments/nsh-reader/evidence/native-editor-shutdown-gui-default/root-independent.json)42份工件与冻结binary／bundle、真实回收和SQL整行。分块准备失败、未知结果与新IME未在该GUI触发，故障收尾以各自OS证据为准。旧聚焦／IME证据仍只覆盖各自冻结版本，释放ACK仍不成立；后续图像任务／observer改动另留版本证据。

1. 原生分区最终bundle资格已通过：正常打开、真实选文/想法、宽窄窗、键盘焦点、坏文件保留、实际剪贴板、Core保存及空库恢复。首轮及居中阶段的119份源/对应工件已分别封存；窄窗遮挡正文仍可Tab的实测失败保留，修复后已用独立数据复验，不复用旧截图。真实IBus中文组合提交与Esc取消组合已有后续增量；全局Esc及候选窗仍未完成。
2. 应用层草稿文本、不可变PDF来源与原capture generation、0.3 wire/同Host临时mount/真实Store事实端口已实现，A的普通签名包实际执行及原子续传已通过，详见下述最新进度与[实际结果](../../experiments/capture-core/EVENT_CONTINUATION_RESULTS.md)。旧协议和profile2不扩大；正式保存仍重新核对原capture权限，保存未知按同操作回执查询/原样重试。B逐段输入须遵守[段落规则](paragraph-editor-contract.md)与[编辑/替换交互](../design/paragraph-editor.md)。
3. 用两个仓库外普通签名输入提供者完成真实A→B→A替换、完整文本保留、明确保存/恢复、失败回退和释放确认，再推进正式library/reader/notes公开功能包。内部props组合不是这项退出证据。

### 原子续传当前进度

[提供者A真实进程链](../../experiments/capture-core/EVENT_CONTINUATION_RESULTS.md)已通过：普通SDK作者包、同Host/真实Store、原子event/checkpoint、多次fresh VM，中文/emoji/NUL小流的3次实际中断回复恢复；全NUL1MiB的2050 VM/8198calls、正式保存、SQLite重开和空库恢复通过。191项Rust、包/Wasm/C3/旧controller完整兼容回归通过。此前“原子续传待补”属于旧阶段；B无界面进程资格已推进，真正原生组件替换、视图/释放确认尚未完成。

逐段输入协议已补充 `ReplaceParagraph { index, input_bytes }` 与对应Schema；应用层只允许同一行、同一base合并Pending输入，其他局部动作拒绝且保留原输入。相关59项及整个Workspace194项Rust测试、根Clippy/格式/边界检查通过，A消费者编译通过（[记录](../../experiments/capture-core/evidence/paragraph-input-ingress.json)）；B已有实际算法与无界面进程资格，原生UI尚未验收。当前源码的A小链已独立重跑10VM/35calls、3次丢回复、2次保存和恢复（[记录](../../experiments/capture-core/evidence/linux-paragraph-ingress-a.json)）；未重跑A大链。详见 [段落规则与输入交互](paragraph-editor-contract.md)。

B真实段落编辑/移动及无界面A→B→A已通过默认稳定包独立复验：20份草稿、183实际worker/681calls、三次实际中断恢复、20次保存/重开/空库恢复，卸载B后知识仍在；guest与独立200组oracle结果一致。详见[B与组件替换资格](../../experiments/capture-core/PARAGRAPH_PROVIDER_RESULTS.md)。原生界面未接这条链，不能据此签署组件替换交付。

消费者迟到Call/reply已加入准入、严格错误分类及有界恢复；独立八项故障复验通过104worker/440calls，故意stale reply恢复与非计划错误分别记录。1MiB B首次真实deadline失败已保留；attempt2在冻结binary/源码上已终态通过：正/逆向共81,923次实际Wasmi、368,653 calls，0 deadline，逐字节结果一致，累计约60分35秒；仅为1MiB算法证明，不代表真实OS进程或交互延迟达标，详见[记录](../../experiments/capture-core/evidence/paragraph-b-one-mib-attempt-2/record.json)。此前完整包543/Wasm20/C3对应冻结源码仍按阶段记录。

只读投影的公开wire、应用registry与Host挂载授权局部实现通过全根209项Rust及仓库外SDK检查，见[记录](../../experiments/capture-core/evidence/projection-foundation/record.json)和[ADR0030](../architecture/adr/0030-readonly-paragraph-projection.md)。component-invocation bootstrap的wire与fresh挂载方法也已局部实现，全根214项Rust通过并由仓库外SDK编译（[记录](../../experiments/capture-core/evidence/projection-foundation/bootstrap-record.json)）。只读协调器、新B签名支持声明及实际投影算法已实现。原签名包八项真实进程投影在精确材料验包缓存后通过76 worker/412 calls，0 deadline，仍为250ms/100万fuel/8calls；根Workspace223项及Clippy通过，详见[缓存验证与诊断](../../experiments/capture-core/evidence/projection-cache-profile/record.json)和[ADR0032](../architecture/adr/0032-verified-package-material-cache.md)。完整包543/Wasm20/C3（314命令/286 CLI/8 native）兼容回归已通过；共享编辑broker已抽出并通过4项局部测试。共享编辑服务与投影已通过同原包/同mount/Draft/同调度链的真实共存：原8例加4阶段共101 VM/547 calls，0中断；主负责人独立复验共存25 VM/135 calls，2次Begin/Append暂停不改输入、4次正文revision使旧查询失效，含换行替换及移动/逆移动逐字节恢复，见[记录](../../experiments/capture-core/evidence/projection-edit-coexistence/record.json)。接下来接既有NSH分区；不伪造编辑事件，不让native parent切段/拼正文。既有119源UI资格不覆盖新链，真实释放ack、IME/Esc、长书/EPUB、正式功能包与完整本地旅程继续按原门槛推进。

## 新增工作台设计范围

2026-11-09，用户进一步纠正整体定位为 [通用插件/Agent基座](../design/agent-foundation.md)。后续切片先明确通用任务/会话、Agent能力协商与工具/交互组合；用非编程任务和开发任务共同验收AF-01–07，避免把项目/文件树变成宿主强制前置。此前建议的一项目/一后端真实开发纵向流程保留为开发场景资格：文件/基础编辑→委派→实际diff→测试→重开与未保存冲突，再补第二后端、多Agent与深度IDE能力。此调整不取代已授权阅读与插件门槛，也不自动开启账号/远程/部署；AW-11–17与原AW-01–10分别留证。

2026-11-08，用户明确要求多 Agent 编程与电脑操作工作台的架构及 UI/UX/UE 设计，见 [规格](agent-workbench.md)、[ADR 0029](../architecture/adr/0029-local-agent-workbench.md) 和 [交互稿](../design/agent-workbench.md)。这是新增长期需求和本轮设计授权；下述阅读闭环仍有效，但不再代表完整长期产品。工作台建议先验证两个后端会话与恢复，再委派/成果整合，终端与电脑操作另验；首版排期和默认权限尚未确认。

## 目标与用户验收

关联 F-LIB-01、F-READ-01/02/03、F-NOTE-01、F-SRC-01、F-DATA-01 及 F-EXT-01/03/04/05/06。

最终旅程：导入资料 → 阅读选文 → 保存摘录与个人笔记 → 关闭重开 → 从笔记准确回原文 → 完整导出并恢复。先普通文本 PDF，再扩展 EPUB；第一条链通过不等于两种格式或完整 M0 完成。

“保存成功”是 Core 持久提交；引用歧义、缺原件、保存失败明确反馈。引用和个人观点分开。卸载阅读/笔记插件后，已保存规范内容仍能通用查看、完整导出恢复，不依赖原插件私有格式。

## 实施阶段

| 阶段 | 工作与产物 | 开始条件 | 退出证据 |
|---|---|---|---|
| A：目录/边界 | [目录架构](../architecture/workspace-layout.md)、Feature 归属、ADR、工程守卫 | 本次指令 | 归属一致，文档/工程检查通过 |
| B1：NSH 实验 | [独立实验](../../experiments/nsh-reader/README.md)：固定版本/环境、最小宿主、bridge、中文输入、阅读 Surface | 已授权，先核对环境 | P01/P02/P03/P05/P07/P08 分别留证；阻断可复现 |
| B2：包生命周期 | [规格](plugin-lifecycle.md)：持久安装、信任、授权恢复、升级恢复、长驻宿主 | 已有 Wasm 本机结果，按剩余风险推进 | EXT-B01 余项及 EXT-B02–04/P04/P06/P08/P11/P13 对应子项留证 |
| C：真实入库/定位 | Core 最小命令/查询、不可变摘录/来源、幂等事务、重启/导出 | 相应存储/定位、包执行门槛通过 | 无部分提交；重复/撤权/冲突明确；空库恢复引用可解析 |
| D：官方功能闭环 | reader、library、notes 独立包按用例接入，UI 收敛到旅程 | EXT-B02–06、相关 NSH 门槛和阶段条件满足 | 普通身份、独立安装/更新、阅读回源、停用后保留同时通过 |

B1/B2 并行，只创建实际需要的模块。实验使用合成或公开许可资料，不依赖外部账号/用户文件上传/云资源。完整 M-1 Go 和承诺平台发布门槛仍按 [阶段计划](../research/06-milestones-risks.md) 执行；本机单路径不能签署完整通过。

## 当前交付与剩余门槛

| 范围 | 已有证据 | 尚未完成 |
|---|---|---|
| A 目录/契约 | 目录守卫、公开 core-api、持久应用/整体端口/SQLite与CAS；SDK仓库外消费 | 公开 Surface 已有原生消费者资格；正式 SDK 发行仍待完成 |
| B1 NSH 原生 | 固定源码、真实离线窗口、Rust 按钮回调、关闭重开 | 普通包的原生单段/跨行选文、笔记保存和Core恢复已有集成资格；真实IBus组合提交／取消已有Linux增量；打包、完整桥接取消、候选窗及其他平台未验 |
| B2 插件支撑 | 签名包权威源与执行端口/适配器已生产落位；持久授权、私有迁移/回滚、监督恢复已有本机子项 | 持久SQL内核/统一Host恢复/Linux身份已落位；完整安装UI、通用权限profile与监督装配、SDK/分发及移动渠道门槛 |
| C 保存/定位 | C0–C3 实验的真实插件、原件/快照、原子提交、完整恢复；契约已落位 | 拆分后完整插件/崩溃/归档回归已通过；正式插件与UI旅程仍未交付 |
| D 产品闭环 | 用户旅程与插件职责已确定 | 正式 library/reader/notes 独立包、阅读保存回源和完整导出旅程 |

后续存储改动继续复用同一生产实现，不能留下实验分叉；Surface 接入必须核对真正窗口事件、页图、精确引文和普通插件授权。公开能力的 [用户流程/权限/兼容验收](reader-surface-capability.md) 已明确，0.2契约与Host registry已实现，普通签名包和原生服务已有选文/笔记保存及恢复的集成资格；正式功能包与完整旅程仍须验收。单项不能替代阶段 D 或 PDF/EPUB 完整要求。

## 历次推进记录

以下按推进顺序保留各轮证据；早期“未实现/阻断”描述属于当时阶段，当前状态以前表和最新增量为准。

B1 已通过隔离本地依赖前缀完成 native 平台 check 和 CLI 链接；上游默认开发宿主额外启用 WebView，独立 native 宿主已实际显示离线 Rust+TSX、按钮回调、正常关闭并重开，尚未完成打包/输入/阅读门槛，见 [NSH 结果](../../experiments/nsh-reader/RESULTS.md)；输入、阅读和打包仍须各自验收。

C-09 后，NSH 探针默认 native profile，已不再将可选 WebKit 列为原生宿主前置；[新证据](../../experiments/nsh-reader/evidence/linux-native-profile-preflight.json) 仍显示 GTK4/Graphene/GStreamer 缺失，sudo 需要交互认证。原生 [PDF provider](../../experiments/pdf-provider/RESULTS.md) 已实际渲染两页 PDF，核对裁剪/旋转后的字符几何及版本化选段；发现并修复页图/坐标尺度不同步问题。旋转页阅读行序仍不可靠，正式 PDF 原件入库/映射、NSH Surface 和完整旅程未通过。下一步不能把原始引擎字符流直接定为 canonical reading text。

随后 [C1 PDF 与混合知识资格](../../experiments/capture-core/PDF_RESULTS.md) 已将真实原件、版本化 engine 页映射和摘录接入同一个实验 Core schema2：普通签名包读取选段/构造提案、混合全局提交、原件保留、真实迁移/回退/卸载及统一导出/空库恢复均有本机证据，恢复后再次原生渲染与锚点一致。schema1 文本兼容和迁移中断也已验证。以上是实验内的 PDF 入库，不等于生产集成或可靠阅读顺序、NSH UI 通过；规格见 [PDF 捕获资格](pdf-capture-qualification.md)，架构见 [ADR 0015](../architecture/adr/0015-pdf-core-snapshot-archive.md)。

C2 [原生选区子项](../../experiments/pdf-provider/SELECTION_RESULTS.md) 已验证真实字形点命中、DIP/缩放/DPI/滚动变换、指针所有权/取消及普通 LTR 多行有序 segments。单段真实指针结果经过普通插件收录并恢复回源；多段明确被当前适配器拒绝，等待版本化复合锚点与单一 Core 事务，不通过多次保存绕过原子性。下一份存储交付应补齐该复合契约及普通插件通路；NSH 窗口、捕获和阅读 UX 仍须系统依赖就绪后实际验证，不能将事件回放计为 UI 通过。

C3 [复合摘录子项](../../experiments/capture-core/COMPOUND_RESULTS.md) 已补齐上述多段缺口：普通签名插件消费完整版本2复合上下文，整组一个 capture/回执/事务；普通页与旋转页保存、旧记录兼容、V3完整恢复、崩溃及生命周期后保留均有本机证据。旧单段提案/查询/定位入口拒绝复合记录，防止只处理首段。规格见 [复合收录](compound-pdf-capture.md)，架构见 [ADR 0017](../architecture/adr/0017-compound-pdf-capture.md)。下一步应将已验证 Core/公开契约与适配器按目录边界落位，同时推进真实 NSH 宿主/Surface；正式 library 导入和原生阅读工作区尚未完成，不能继续将这些实验本身计作最终交付。

C3 之后已执行第一步生产落位：[core-api](../../crates/core-api/README.md) 成为文本/PDF/复合捕获契约的唯一权威源，实验兼容转发和仓库外 SDK 导出共同消费；三份实现与原 C3 摘要一致。根61项测试、实验41项测试，以及迁移后完整314命令/286 CLI/8 native回归通过，见 [ADR 0018](../architecture/adr/0018-core-api-contract-source.md) 和 [迁移证据](../../experiments/capture-core/evidence/linux-core-api-migration.json)。这不是生产存储完成；下一步围绕完整提交计划拆应用用例与事务/blob端口，SQLite/CAS留在适配器，并保留撤权与提交的共享序列化门。

[ADR 0019](../architecture/adr/0019-atomic-knowledge-persistence.md) 已继续完成应用与持久化落位：单一一致快照生成完整追加计划，适配器防止自洽历史篡改/批量追加，并在事务内重读关系和核对幂等/版本。根71项测试、实验41项测试以及314命令/286 CLI/8 native迁移回归通过，见 [持久化记录](../../experiments/capture-core/PERSISTENCE_RESULTS.md)。生产没有 crash 字符串或退出注入，其他平台耐久与大资料库性能仍待验。后续重点转向生产插件安装/运行装配与公开 Surface 能力，避免再复制一套实验 Core；正式资料导入、阅读/笔记插件和完整用户旅程仍待交付。

B1 的 [原生 PDF Surface](../../experiments/nsh-reader/pdf-surface/README.md) 随后已实际显示同版原生页图，在60%倍率以真实 raw responder 拖动选择 QUSOR，旋转第二页也得到一致高亮和引文；空白/出界取消、缩放/换页清理及窄窗布局有实屏证据。报告/bundle绑定、加载就绪与世代检查防止旧图/旧回调混用。修复过程揭示并保留线程状态交接、按压阈值提前结束、页面异步加载等失败证据，见 [ADR 0020](../architecture/adr/0020-native-pdf-surface-probe.md)。这仍是受控服务探针；完整触控/键盘/中文输入、持久保存、公开插件 Surface 能力与产品阅读旅程未完成。

B2 的 [包实验结果](../../experiments/extension-package/RESULTS.md) 分轮保留证据：

- [ADR 0007](../architecture/adr/0007-signed-package-persistence-probe.md)：仓库外作者、完整签名/键绑定、禁用安装与更新/卸载进程恢复。
- [ADR 0008](../architecture/adr/0008-durable-grant-recovery-probe.md)：持续元数据授权与新策略会话恢复，旧会话不复活。
- [ADR 0009](../architecture/adr/0009-restored-wasm-execution-probe.md)：持久授权后的同步受限 Wasm 元数据调用。
- [ADR 0010](../architecture/adr/0010-paired-private-state-probe.md)：真实受限 Wasm 私有迁移、运行预检及包/状态成对回滚；432 次 CLI 检查中包含 40 次真实进程中止。失败/中断保留原配对并禁用，完整界面和生产适配器未交付。

[ADR 0011](../architecture/adr/0011-persistent-supervisor-probe.md) 随后在同一长驻 supervisor PID 中通过 62 条管理命令、14 项双插件/故障监督场景，包含回收不确定时禁止替代、真实迟到结果及正常关闭确认；详见 [监督协议](../../experiments/extension-package/SUPERVISION.md)。kill/wait 错误为可信注入，不等于实际 OS 故障已验证。

[ADR 0012](../architecture/adr/0012-supervisor-crash-reconciliation.md) 已补充 Linux 身份固定、持久执行排他记录和异常退出协调，见 [恢复协议](../../experiments/extension-package/ORPHAN_RECOVERY.md) 与 [最新证据](../../experiments/extension-package/evidence/linux-supervisor-crash-recovery.json)。默认父死守卫与显式存活残留的 pidfd 清理分别验证；已尝试 spawn 但身份未绑定的同内核窗口仍保守隔离，不自动放行或宣称已清理。

阶段 C 所需的本机捕获存储/定位资格切片现有 [C0 实测记录](../../experiments/capture-core/RESULTS.md)：真实签名插件提案、Core 事务、撤权竞争、重启、生命周期后知识保留及导出恢复已验证。完整五端 M-1 和正式 UI 门槛不因实验降低；正式 C/D 交付仍须相应门槛。任意同步父 provider I/O 抢占、未知 native 窗口的自动修复、生产 Core 集成及 NSH 仍待完成。

### C0 可审查交付：真实捕获资格切片

关联 F-EXT-02、F-SRC-01、F-DATA-01 与 F-EXT-01/03/05/06，沿用 C-08 已授权的保存/回源闭环。先在独立实验按实际消费者纵向贯通，不预生成全部业务 crate：普通签名来源插件提交本地文本选段 → Host 冻结输入并取得明确收录意图 → Core 保存不可变快照、来源和锚点 → 重启查询 → 导出到空库恢复。

退出证据至少包括：

- 两个不同来源 fixture 使用同一公开契约；原文、用户批注与来源描述分开，无服务名领域分支、插件自报批准或 SQLite 旁路。
- 原件/blob 耐久与 Core 实体、回执、操作、outbox/index job 的事务共同验证；相同请求幂等、同键改 payload 冲突，独立用户意图不能被文本 hash 吞掉。
- 提交/撤权的共享序列化门与真实杀进程覆盖；响应丢失查询回执，不把两个数据库自动当作一个原子事务。
- 中文、emoji、组合字符、重复引文的版本/hash/定位单位明确；歧义、缺文件、版本不符拒绝，本地精确不等于外部精确回跳。
- 卸载/回滚插件后已收录 Core 内容仍可通用查询、导出，并向空库恢复；包代码、授权和凭据不混进正式知识导出。

本轮规格见 [捕获资格切片](capture-qualification.md)，架构见 [ADR 0013](../architecture/adr/0013-capture-core-qualification.md)。上述本机退出证据已留存于独立实验，生产根 Workspace 尚未增加业务实现。下一步审查公开契约与存储 port 的生产落位，结合 NSH 原生依赖阻断推进最小阅读 Surface 与文档提取资格验证；不继续以合成管理元数据代替阅读。该资格切片不计为 PDF/EPUB 阅读、微信读书账号接入或正式阶段 D；相关 UI/格式/平台仍按各自证据推进。

## 插件与宿主契约

C-09 明确原生技术优先：宿主和插件贡献优先映射 NSH 原生控件，PDF 沿原生 PDFium 路线验证；EPUB 先评估可复用原生排版/阅读服务及平台适配，再按具体缺口考虑 Web 备选。不会把纯文本降级当作完整 EPUB，也不因原生偏好让功能插件直接持有 NSH/FFI 内部对象。

- 工作区、阅读和笔记由独立插件贡献，Host 不按官方包 ID 写业务分支。
- 按用例完善公开能力：导入/读取、Surface/选区、标注/笔记、来源解析、导出。这是职责目录，不是已发布接口。
- Core 持有文档版本/摘录/笔记/引用；Host 持有身份/授权/包生命周期/平台资源与恢复。
- PDF/EPUB 服务提供受控数据/句柄，不给插件 NSH 对象、数据库或任意路径。
- 官方包与仓库外作者示例使用同一协议/权限；SDK 独立构建是交付条件。

## 范围

画布、知识图谱、学习、研究 Agent 等已经明确插件归属，具体上线阶段不自动前移。Reader AI 是否进首版与云默认行为保持独立选择，平台按 C-15 覆盖六端；本地闭环顺序不预先决定这些问题。前置实验已授权，外部服务/密钥/部署仍须对应任务授权。


## 当前最新增量：包与执行适配器落位

[ADR 0021](../architecture/adr/0021-signed-package-authority.md) 与 [ADR 0022](../architecture/adr/0022-authority-free-wasm-runtime.md) 已由实际实验消费者验证：纯包实现按字节迁入，执行managed/raw/private经生产端口，Core与旧0.1策略源保持不变。根88项测试、包543项CLI、Wasm20项受监护场景及最后一次完整C3通过，见 [C5统一结果](../../experiments/capture-core/PLUGIN_PRODUCTION_RESULTS.md)。没有把纯包校验当成生产安装器，也没有把Legacy Probe的20场景当成完整生产监督证明。

下一步按 [公开Surface验收](reader-surface-capability.md) 和 [ADR 0023](../architecture/adr/0023-public-reader-surface.md) 继续验证已实现的版本协商、唯一Host registry与普通插件消费者，保持0.1兼容；临时选区lease不通过replace_grants更新，避免Surface自己撤销会话。正式安装/监督装配、输入/资源完整生命周期和产品阅读旅程仍按各自门槛继续。

### 公开 Surface 实施增量

0.2公开契约、版本固定会话、Host registry与显式版本集合的包重验已实现；默认0.1入口保持。实施后的 [完整兼容回归](../../experiments/capture-core/SURFACE_COMPATIBILITY_RESULTS.md) 通过根108项Rust、42项边界回归、包543场景、Wasm20旧受监护场景及完整C3。[原生普通签名消费者](../../experiments/nsh-reader/plugin-surface/RESULTS.md) 已在同一窗口接通单段/跨行选文、非空笔记、候选和Core回执，退出后查询及空库恢复精确解析通过。键盘层级为Xvfb GTK/XTest；它仍是资格relay，不能替代正式reader或据此推出阶段D完成。

生产持久安装/监督的下一片应先拆开持久记录、单一Host授权与平台进程执行。实验Store每次恢复所建的独立Host及synthetic metadata不能原样成为生产broker；故障注入、CLI和进程中止包装继续留在实验。正式reader/library/notes按同一公开授权链路接入，避免延长资格relay替代产品交付。


### C7 持久安装内核与统一Host恢复

[ADR 0025](../architecture/adr/0025-durable-installation-authority.md) 已实施完整SQL同源迁移：extension-store持有包/安装/授权材料/私有配对/worker lease，实验只保留协调与故障包装。统一DurableHost在每次dispatch重核持久世代、范围与精确版本；两个普通签名Wasm从实际Core保留记录读取metadata，0.1/0.2、撤权互不串扰、更新清授权及重开新session通过。Linux身份/pidfd原语同源落位，不是完整supervisor。

[完整C7结果](../../experiments/extension-package/INSTALLATION_PRODUCTION_RESULTS.md) 为根121项Rust、包543场景、Wasm20旧受监护场景、完整C3及新二进制原生选文/非空笔记/空库恢复均通过。包Rust测试46项；原4项底层测试已迁到根，另新增重放测试，未删故障覆盖。源码快照归档，历史C6不代替新工件。

下一步继续完成同一最终旅程，不能将资格relay当成reader交付：

1. 持续权限扩展到实际阅读所需的多能力/精确资源集合，旧metadata profile按原版本保留；持续材料与新会话、临时选区/批准分开，不自动继承更新授权。
2. 按原GuardReady→持久绑定→新鲜授权→Boot补齐生产异步worker传输/controller/独立watchdog，保留raw字节预算、迟到结果拒绝、真实回收确认与未知spawn隔离；实际消费者不迁入Probe/Fault。
3. NSH按 [动态PDF门槛](../../experiments/nsh-reader/dynamic-pdf/README.md) 先补预算内新PDF文件的受控动态页图与文件选择，无需重编译bundle，不再绑定两页fixture；现有16MiB/16页实验预算不等于普通长书支持，随后须补分块/页窗与格式版本门槛；验证切换/缩放/取消/关闭和精确来源一致。第一方library/reader/notes仍走普通SDK/贡献/授权，不加私有特权。
4. 门槛满足后落正式功能包和本地工作区：导入→阅读→笔记→重开→从笔记回原文→完整导出恢复。中文输入、mount释放ack、其他输入/平台和EPUB继续逐项验证，不因已完成资格而移除。


### C8 持续阅读权限与原生动态 PDF资格

[ADR 0026](../architecture/adr/0026-durable-reader-native-dynamic-pdf.md) 已记录本轮边界：profile2保存完整精确能力集合，旧profile1保持；DurableSurface每次回调重核持久事实和票据归属，同安装保留资源配额，真实卸载且释放后安全退役；跨库同编号卸载证明拒绝且不影响原会话。根134项Rust、42项架构守卫、格式/Clippy/文档构建，以及最终包543/Wasm20/C3完整回归已通过，见 [C8结果与185份源码快照](../../experiments/capture-core/READER_AUTHORITY_RESULTS.md)。

动态消费者的新二进制通过原生文件对话框打开构建后生成的三/四页PDF，第三/四页真实拖选、非空笔记及Core回执、正常退出、Core重开、完整导出/空库恢复与精确来源/原件核对均通过；native11项测试、Clippy和无Web链接核对通过。首次工件另有取消、坏PDF和预算拒绝保留旧资料/草稿的窗口证据，未冒充最终工件已重复这些失败场景。正式阅读工作区仍未交付；长书、IME、真实释放ack、EPUB和平台门槛保持。

下一片继续第2项生产异步监督：现有实验Boot/Call/Result仍绑定Fault、策略请求与Probe Report，不能整搬；生产transport传有界raw bytes，独立控制线程持有Child与watchdog，业务broker在控制线程外。GuardReady核对后先持久绑定lease，再经同一Host新鲜授权后放行Boot，最终结果/真实reap/持久清理分别确认。原run_id的boot-1..256实验限制需显式版本兼容扩展，不能转为产品执行寿命；真实双普通包/Core消费者替换直接同步执行，并保留旧543场景。NSH资源释放另须核对真实native view与decode/render job完成，JS cleanup或组件Drop不直接构成释放ack。
