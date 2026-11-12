# QUSOR 求索

QUSOR 是个人学习与知识探索系统，采用 NSH 界面与独立 Rust Core。当前交付包含工程基座、插件宿主策略及真实捕获用例；普通签名包已通过原生 PDF 选文、笔记保存和空库恢复的集成资格验证，完整资料库/阅读/笔记工作区尚未交付。

本阶段已获用户同意：按清晰目录落实全部核心用户功能插件化，推进 NSH 最小阅读宿主与包安装/恢复前置工作，再按门槛实施真实本地阅读闭环。见 [下一阶段计划](docs/product/next-phase.md) 与 [目录架构](docs/architecture/workspace-layout.md)。

## 从这里开始

| 目的 | 文档 |
|---|---|
| 在 macOS 启动原生阅读预览，查看六端真实资格 | [跨平台结果与启动](experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) |
| 查看生产 macOS 应用、应用内更新与首次 GitHub draft | [发行与更新](docs/product/release-and-update.md) · [draft 验收](experiments/nsh-reader/evidence/github-release/README.md) |
| 确认我们做什么、哪些尚未决定 | [功能范围与确认记录](docs/product/scope.md) |
| 运行本轮无界面基座与查看验收范围 | [插件宿主策略规格](docs/product/foundation.md) |
| 复现独立插件执行及权限/资源实验 | [Wasm 实验与结果](experiments/extension-wasm/README.md) |
| 阅读与思考的交互设计原则 | [产品体验设计基线](docs/product/experience.md) |
| 查看正式界面方向、软件分区与组件替换 | [设计工作基线与四份设计稿](docs/design/README.md) |
| 查看原生阅读分区实际窗口与恢复验证 | [原生工作区资格进度](experiments/nsh-reader/dynamic-pdf/WORKSPACE_RESULTS.md) |
| 理解当前与未来模块边界 | [技术架构入口](docs/architecture/README.md) |
| 查代码放哪、每项功能属于哪个插件 | [目录与插件职责](docs/architecture/workspace-layout.md) |
| 推进下一阶段与验收 | [本地阅读闭环计划](docs/product/next-phase.md) |
| 查看 D 阶段正式插件门槛 | [D 阶段计划](docs/product/d-phase-plan.md) |
| 查看已保存想法列表实窗增量 | [想法列表证据](experiments/nsh-reader/evidence/notes-workspace/README.md) |
| 查看想法工作区 UI v1 与交互测试 | [想法工作区 UI 记录](experiments/nsh-reader/evidence/notes-workspace-ui/README.md) |
| 查看想法读取公开能力与 Host 策略 | [能力记录](experiments/capture-core/evidence/notes-read-capability/README.md) |
| 复现 NSH 环境与构建检查 | [阅读宿主实验](experiments/nsh-reader/README.md) |
| 设计持久安装、授权恢复和升级回滚 | [包生命周期](docs/architecture/plugin-lifecycle.md) |
| 查看持久安装内核与统一 Host 恢复的生产落位 | [C7结果](experiments/extension-package/INSTALLATION_PRODUCTION_RESULTS.md) |
| 查看持续阅读权限与动态原生PDF保存恢复 | [C8结果](experiments/capture-core/READER_AUTHORITY_RESULTS.md) |
| 查看生产进程控制器与真实Core九场景资格 | [C9结果](experiments/capture-core/supervised-results.md) |
| 查看组件契约、同Host临时挂载与兼容验证 | [0.3接线结果](experiments/capture-core/COMPONENT_CONTRACT_RESULTS.md) |
| 查看普通A组件的原生输入、焦点与保存恢复 | [原生A编辑结果](experiments/nsh-reader/dynamic-pdf/COMPONENT_EDITOR_RESULTS.md) |
| 查看普通文本组件真实进程、原子续传及1MiB保存恢复 | [提供者A结果](experiments/capture-core/EVENT_CONTINUATION_RESULTS.md) |
| 查看段落提供者与真实A→B→A进程资格 | [B与组件替换结果](experiments/capture-core/PARAGRAPH_PROVIDER_RESULTS.md) |
| 复现签名包、仓库外作者构建和持久安装事务 | [包实验与结果](experiments/extension-package/README.md) |
| 复现真实捕获、快照定位、导出恢复 | [C0 捕获资格实验](experiments/capture-core/README.md) |
| 复现普通插件的原生选文/笔记保存 | [公开 Surface 集成结果](experiments/nsh-reader/plugin-surface/RESULTS.md) |
| 复现原生 PDF 页图和字符定位 | [PDF provider 资格实验](experiments/pdf-provider/README.md) |
| 新增插件能力与外部资料接入 | [插件扩展架构](docs/architecture/extensions.md) |
| 本地构建、检查与 CI 的实际范围 | [开发指南](docs/development.md) |
| 项目技能、协作规则 | [AGENTS.md](AGENTS.md) |
| 查看源码审查和详细技术设计 | [研究总览](docs/research/README.md) |
| 查看后续 M-1/M0/M1 任务 | [阶段计划](docs/research/06-milestones-risks.md) |

## 新增 Agent 工作台设计

用户已明确希望接入多 coding agent，统一委派、管理会话并操作本机编程；当前交付 [功能与验收设计](docs/product/agent-workbench.md)、[架构决定](docs/architecture/adr/0029-local-agent-workbench.md) 和 [UI/UX/UE 设计](docs/design/agent-workbench.md)。尚未实现或接通外部 Agent，首版深度、平台与默认授权待定。

## 当前工程

- Cargo Workspace，固定 Rust 1.97.1；该版本是本机已有工具链，不代表已验证所有 NSH 依赖兼容。
- learning-domain 保持无业务类型、零外部依赖。
- capability-composition 保持零外部依赖，负责版本化 service 接缝、分层 profile、确定性激活顺序与冲突诊断；它不执行插件、不授予权限，桌面 profile 在 `apps/qusor`。
- core-api 承接已验证的文本/PDF/复合捕获与来源锚点契约，供实验及 SDK 导出共同使用。
- learning-application 经 learning-ports 生成完整提交计划；storage-sqlite/blob-store 负责 SQLite/CAS 耐久，现有捕获实验实际消费这套实现，迁移后完整插件/崩溃/恢复回归通过。
- extension-package 提供有界签名包/文件清单/键绑定验证；extension-runtime-api 与 runtime-wasm 提供不自带授权的有界执行，已有实验通过同一实现复验。extension-store 已落位持久安装内核，runtime-process 已落位 Linux 身份/父死守卫/pidfd；完整安装界面、通用持续权限和进程监督装配仍待完成。
- extension-api 与 extension-host 保留 0.1 JSON 策略入口，并实现显式 0.2 协商及阅读 Surface registry；原生普通插件消费者已通过选文/笔记保存及恢复的集成资格。现有职责包括内存安装元数据、资源授权、会话、生命周期和贡献查询。
- serde/serde_json 用于协议与示例输入，core-api 使用 SHA-256 和 Unicode 分段校验；SQLite 和文件系统依赖限定在存储适配器；业务应用不依赖 NSH、数据库、插件运行时或云服务。
- 依赖边界检查及反例测试、本地 Markdown 链接检查。
- 三桌面 Core CI 配置；是否在远端运行以真实流水线结果为准。
- apps/plugins/sdk 已建立职责入口，尚无正式功能包或应用装配；[NSH 前置实验](experiments/nsh-reader/RESULTS.md) 已固定源码并借助隔离本地依赖目录通过 Linux native 平台检查与 CLI 链接；独立原生宿主已显示离线 Rust+TSX 并验证按钮回调及关闭重开，普通签名包已接通原生单段/跨行选文、个人笔记、Core持久回执和完整恢复；真实IBus中文组合提交／取消已有Linux限定证据；候选窗、其他平台IME与正式产品阅读旅程尚未通过。

本地检查需要 Rust 工具链和 Python 3.11+。首次准备依赖缓存运行 cargo fetch --locked，之后从仓库根目录离线检查：

```sh
python3 scripts/check_architecture.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo run -p extension-host --example policy_walkthrough --locked --offline
cargo doc --workspace --no-deps --locked --offline
```

Windows 的 Python 命令通常是 python；WSL 项目应在 Linux 路径内调用 Cargo，详见开发指南。

独立 [签名包与安装事务实验](experiments/extension-package/RESULTS.md) 已验证完整包签名、仓库外构建、发布者绑定和禁用安装的进程恢复。新增 [持续授权与新策略会话恢复实验](docs/architecture/adr/0008-durable-grant-recovery-probe.md)；随后 [恢复授权后的受限 Wasm 调用](docs/architecture/adr/0009-restored-wasm-execution-probe.md) 已有本机证据；[私有状态迁移与成对回滚](docs/architecture/adr/0010-paired-private-state-probe.md) 也已留存本机实验。[同一长驻宿主双插件监督](docs/architecture/adr/0011-persistent-supervisor-probe.md) 也有本机证据；[宿主异常退出协调](docs/architecture/adr/0012-supervisor-crash-reconciliation.md) 已验证可确认身份的本机路径；已尝试创建但尚未绑定身份的进程恢复记录仍保守隔离。生产安装器、完整 Core 用例及正式阅读插件仍未完成。

## 当前产品决策状态

长期愿景与工程不变量沿用已给出的产品说明。插件化已由用户明确为基座要求；第一阶段支持第三方开发/安装/分发，自己的功能也通过同一公开插件接口开发，见 [ADR 0003](docs/architecture/adr/0003-plugin-first-delivery.md)。RSS和微信读书选段用于检验通用架构，不代表具体插件已实现或确定首版交付。首版是否包含 Reader AI、画布、同步，以及云端默认行为仍需要用户回复，见范围记录；推荐项不会自动转成确认项。

2026-11-07，用户明确推进基座功能实现。本轮交付无界面插件策略内核，见 [功能规格](docs/product/foundation.md) 与 [ADR 0004](docs/architecture/adr/0004-headless-extension-policy.md)；安装操作仅注册内存元数据，不校验包内容或执行插件代码。独立 Wasm 实验随后验证了本机包执行、权限与资源边界，见 [实验结果](experiments/extension-wasm/RESULTS.md)。生产运行时、完整安全安装、持久化和 M-1 平台/渠道验收仍未通过。

## 项目状态

QUSOR 仍是项目代号，商标/域名/组织名未核查。项目采用 [MIT 许可证](LICENSE)，仓库托管于 [GitHub](https://github.com/csic21/qusor)。当前 crate 保持 publish=false，尚不发布到 crates.io。
