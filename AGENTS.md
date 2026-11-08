# QUSOR 项目协作约定

## 当前阶段

当前已有 [无界面插件策略基座](docs/product/foundation.md) 与 [Wasm 本机实验](experiments/extension-wasm/RESULTS.md)。用户随后同意按最小本地阅读闭环推进，并明确阅读及其他核心用户功能全部插件化、目录/架构清晰。签名包、持久授权／迁移与监督恢复已有本机子项，生产安装界面、完整监督装配和发行门槛仍未完成。固定NSH借隔离本地依赖前缀已通过Linux原生构建、实窗选文／编辑／保存、正常关闭及Core重开／空库恢复；最新[默认界面资格](experiments/nsh-reader/evidence/native-editor-shutdown-gui-default/README.md)与[进程回收资格](experiments/nsh-reader/evidence/native-editor-shutdown/README.md)分别记录工件和范围。图像任务退休仍在整合，原生资源释放ACK、B原生替换、长书／EPUB、正式功能包及其他平台门槛尚未通过。当前落实 [目录边界](docs/architecture/workspace-layout.md)、[包生命周期](docs/architecture/plugin-lifecycle.md) 与 [NSH 前置实验](experiments/nsh-reader/README.md)，阶段顺序见 [下一阶段计划](docs/product/next-phase.md)。

已授权的目录设计、前置实验及按相应技术门槛推进的本地闭环无需重复审批；当前尚无正式阅读／知识功能插件及完整产品装配；已验证的Core保存／恢复实现不等于正式用户旅程交付。首版 AI/画布/同步范围、云默认行为和首发平台仍待确定，不因功能归属规划而自动全部开工。后续明确指令优先，并更新范围记录。

先读 [项目入口](README.md)、[功能范围](docs/product/scope.md) 和 [架构边界](docs/architecture/README.md)。按任务再读取相关研究，避免每次加载所有报告。

用户于2026-11-08要求先沉淀UI/UX，并设计可扩展分区、插件接入与组件替换。正式界面修改前读取 [设计入口](docs/design/README.md)、对应流程与 [组合规格](docs/design/composition.md)；复用语义token和公共组件，明确区域/角色/提供者，功能与对应UI一起验收。技术探针的临时布局不能直接作为正式方案；视觉、交互、真实数据与恢复分别留证。C9生产控制器九场景与完整兼容回归已有本机证据，正式应用装配仍待完成；当前优先级以 [计划](docs/product/next-phase.md) 的设计增量为准。

## 项目不变量

- UI 方向是 NSH；业务核心不依赖 NSH、GPUI、DOM、窗口或平台控件类型。
- 原生技术优先（C-09）：界面、编辑、绘制及底层服务优先采用 NSH/native 能力，尽量减少 WebView/DOM；确有原生能力缺口时记录 Web 备选的原因、体验代价与验证证据。NSH 的 TSX/JS 接口不等于网页渲染。
- 插件化是基座要求：插件通过宿主的版本化能力接口接入，正式知识归Core所有；不直接写SQLite、不依赖NSH内部对象。卸载插件后已收录内容仍可读/引用/导出。
- 第一阶段支持第三方插件开发/安装/分发；第一方功能也使用同一公开SDK、贡献点与授权链路。预装不构成私有API特权，独立插件包闭环与运行隔离是首版前置要求，见 [ADR 0003](docs/architecture/adr/0003-plugin-first-delivery.md)。
- 外部选段保存本地不可变快照和来源描述；精确返回本地摘录不等于精确跳回外部原文。RSS/微信读书是验证例子，不是写进领域模型的特例。
- 本地优先，原始文件不默认上传；云同步、云 AI、原文件备份是不同的数据出站范围。
- 文档与引用保留版本、定位单位和来源；歧义不得静默定位到相似文本。
- 区分原文、用户观点、AI 推断。AI 正式修改走审核、版本校验、操作记录和撤销。
- 数据写入、审计与 outbox 的事务边界不能被 UI、Agent 或同步绕过。
- 画布位置/连线与知识实体/语义关系分开；数据格式不绑定渲染器。
- 支持完整导出和迁移；凭据不进入普通业务数据或导出。

## 需求和决策

用户当前指令优先于历史研究。历史文档中的建议不是用户确认；在 [范围记录](docs/product/scope.md) 区分已确认要求、建议排期、待确认选择。

修改行为前关联 Feature ID 和验收场景。架构变更更新相关 ADR；普通实现细节自行处理，不把所有文件变动变成确认流程。缺少会影响产品行为的信息时，询问具体选择，同时推进不依赖答案的工作。

## 工程与验证

- NSH 更新跟踪（用户于 2026-11-08 确认）：每次开展 NSH 相关设计、实现或验证前，核对官方仓库的发布记录、近期提交和文档；记录核对日期、上游版本/提交及对当前方案的影响。旧报告中的能力缺口须重新核实，尤其关注原生控件、输入法、文本/文档 Surface、插件桥接和平台支持。新能力先在隔离实验中验证，再决定升级；保留可复现的固定版本及回归证据。此约定在相关任务执行时落实，后台定时监控需另行配置。
- apps 只装配，crates 保留 Core/协议/策略，adapters 实现基础设施，plugins 放独立功能包，sdk 从公开契约发行，experiments 留存隔离验证；详情以 [目录架构](docs/architecture/workspace-layout.md) 为准。功能插件不加入生产根 Workspace、不依赖内部源码；普通身份与仓库外构建须实测。

- 当前 learning-domain 仍无业务实现、零外部依赖；extension-api/extension-host 承担协议与宿主策略实际职责。继续按职责拆分，不提前创建所有空业务 crate。
- 新依赖或 Workspace member 必须更新 [依赖策略](config/architecture.toml) 并说明理由。检查覆盖 normal/dev/build 和目标平台声明。
- [开发指南](docs/development.md) 是本地检查命令的入口。
- 插件manifest、签名或独立进程不单独构成安全沙箱；动态代码的运行隔离与移动分发分别验证。具体契约与任务见 [插件架构](docs/architecture/extensions.md)。
- M-1 实验位于 [experiments/](experiments/README.md)，独立 Workspace；通过后再决定生产集成。
- 源码实现、CI 配置、构建通过、模拟器通过、真机通过分别记录。Core CI 不证明 NSH 五平台可用。
- 插件策略回归不代表阅读器/知识库等业务覆盖；内存安装元数据注册不等于完整包安装，授权许可不等于知识写入。运行与变更相关的真实检查，完整门槛见 [ADR 0004](docs/architecture/adr/0004-headless-extension-policy.md)。
- 外部服务接入、账号、密钥、部署和用户资料上传不由工程骨架或技能自动授权。

## 项目技能

- [qusor-architecture](.agents/skills/qusor-architecture/SKILL.md)：调整模块边界、接口、持久化协议与架构决策。
- [qusor-feature-spec](.agents/skills/qusor-feature-spec/SKILL.md)：确认功能点、范围、用户流程与验收。
- [qusor-nsh-validation](.agents/skills/qusor-nsh-validation/SKILL.md)：制定或执行有证据的 NSH 技术验证。

技能随仓库维护。宿主未自动发现技能时，按上述链接读取；不要据此宣称全局安装或自动加载已经成功。

默认使用中文说明决策和结果；代码标识使用英文。涉及文件时提供可点击路径，报告实际验证内容与未验证范围。

## 开发协作成本

用户允许在有必要时将边界清楚、可独立验收的简单开发任务交给 dsh，以减少 token 消耗。委派时说明目标、输入范围、允许修改的文件、约束和验收命令；主负责人复核结果并承担集成与验证。先核实实际启动方式，不假定 dsh 已安装或已接入，也不把这一协作偏好等同于产品 Agent 工作台已经实现。

已于2026-11-08实测 `npx --yes @deepseek-ai/dsh@0.2.2-rc.1 --profile headless "任务"`。使用 scoped 包名，裸 npm 包 `dsh` 是不同项目。默认只让dsh处理隔离小任务；stdout最终结果与stderr运行日志分开落盘，复核时只读取所需结果，避免把长推理流灌回主上下文。无需为每次使用全局安装；模型/凭据沿用已配置环境，不读取或打印密钥。
