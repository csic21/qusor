# QUSOR 项目协作约定

权威范围：不变量、入口与约定；进度见 [状态表](docs/status.md)，路线见 [计划](docs/product/next-phase.md)。

## 入口

- 必读：[入口](README.md)、[范围](docs/product/scope.md)、[架构](docs/architecture/README.md)、[状态表](docs/status.md)、[开发](docs/development.md)；按需读研究。
- 改正式 UI 前读 [设计](docs/design/README.md)、流程与 [组合](docs/design/composition.md)：复用 token/组件，明示区域/角色/提供者，功能与 UI 同验；探针不作正式方案，视觉/交互/真数据/恢复分证。

## 不变量

- UI 方向为 NSH；业务核心不依 NSH、GPUI、DOM、窗口或平台控件。
- 原生优先（C-09）：UI/编辑/绘制/服务优先 NSH/native，少用 WebView/DOM；缺口记原因/代价/证据（NSH TSX/JS 非网页）。
- 插件 UI 仅用 NSH 原生 Surface/组件或声明式描述；禁 WebView/iframe/HTML/DOM/Electron/Node；dsh 仅借组合，不用 Web（C-18）。
- 插件化：经版本化接口接入，知识归 Core；禁直写 SQLite 或依 NSH 内部对象；卸载后内容仍可读/引用/导出。
- 首阶段开放第三方开发/安装/分发；第一方同走公开 SDK/贡献点/授权，预装无特权，独立包与隔离为首版前置（[ADR 0003](docs/architecture/adr/0003-plugin-first-delivery.md)）。
- 外部选段存不可变快照与来源；回本地摘录≠跳回原文；RSS/微信读书仅作验证例，不入领域特例。
- 本地优先，原文件不默认上传；云同步、云 AI、原文件备份为不同出站范围。
- 文档与引用保留版本、定位单位与来源；歧义禁静默定位相似文本。
- 区分原文、用户观点与 AI 推断；AI 正式修改须审核、版本校验、记录与撤销。
- 写入、审计与 outbox 事务边界禁被 UI/Agent/同步绕过。
- 画布位置/连线与知识实体/语义关系分离；格式不绑定渲染器。
- 支持完整导出与迁移；凭据不入业务数据或导出。

## 协作与验证

- 决策：用户指令优先历史研究，[范围](docs/product/scope.md) 区分已确认/建议/待决；未定不开工（首版 AI/画布/同步、云默认、首发平台、待拍板架构）。改行为关联 Feature ID/验收，架构变更按 [ADR 0048](docs/architecture/adr/0048-doc-governance.md) 写 ADR（禁流水），细节自处（不把改文件变确认），缺产品选择先问并并行推进独立工作。
- NSH 跟踪（2026-11-08）：NSH 设计/实现/验证前核对官方发布/提交/文档，记日期/版本/提交与影响并重核缺口；新能力先隔离实验再升级，留固定版本与证据（定时另配）。
- 目录与依赖：`apps` 装配/`crates` 核心协议策略/`adapters` 设施/`plugins` 独立包/`sdk` 契约/`experiments` 隔离（[layout](docs/architecture/workspace-layout.md)）。插件不入根 Workspace/不依内部源码，实测普通身份与仓外构建；禁空 crate。新依赖/成员改 [architecture.toml](config/architecture.toml) 写理由（normal/dev/build/目标平台）。
- 验证与隔离：命令见 [开发](docs/development.md)，状态记 [状态表](docs/status.md)。源码/CI/构建/本机/模拟器/真机分记：Core CI≠NSH 跨平台；策略回归≠业务，内存注册≠安装，授权≠写库（[ADR 0004](docs/architecture/adr/0004-headless-extension-policy.md)）。manifest/签名/进程≠沙箱，动态运行隔离与移动分发分验（[extensions](docs/architecture/extensions.md)）；M-1 放 [experiments](experiments/README.md)。外部服务/账号/密钥/部署/上传不自动授权。
- 技能与表达：[architecture](.agents/skills/qusor-architecture/SKILL.md)、[feature-spec](.agents/skills/qusor-feature-spec/SKILL.md)、[nsh-validation](.agents/skills/qusor-nsh-validation/SKILL.md) 随仓维护，未自动发现按链读，不宣称全局安装。默认中文/代码英文，路径可点击，明示已验/未验。
- dsh 协作：边界清晰小任务可交 dsh 降 token，明示目标/输入/文件/约束/验收，主责复核集成；先核启动，不假定已装或等同工作台。2026-11-08 实测 `npx --yes @deepseek-ai/dsh@0.2.2-rc.1 --profile headless "任务"`（scoped，非裸包 `dsh`）；stdout/stderr 分盘只读结果，禁读打密钥。
