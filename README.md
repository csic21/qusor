# QUSOR 求索

QUSOR 是个人学习与知识探索系统，采用 NSH 界面与独立 Rust Core。当前交付包含可检查的工程基座与无界面插件宿主策略内核；尚无阅读器、知识库等产品业务实现。

本阶段已获用户同意：按清晰目录落实全部核心用户功能插件化，推进 NSH 最小阅读宿主与包安装/恢复前置工作，再按门槛实施真实本地阅读闭环。见 [下一阶段计划](docs/product/next-phase.md) 与 [目录架构](docs/architecture/workspace-layout.md)。

## 从这里开始

| 目的 | 文档 |
|---|---|
| 确认我们做什么、哪些尚未决定 | [功能范围与确认记录](docs/product/scope.md) |
| 运行本轮无界面基座与查看验收范围 | [插件宿主策略规格](docs/product/foundation.md) |
| 复现独立插件执行及权限/资源实验 | [Wasm 实验与结果](experiments/extension-wasm/README.md) |
| 阅读与思考的交互设计原则 | [产品体验设计基线](docs/product/experience.md) |
| 理解当前与未来模块边界 | [技术架构入口](docs/architecture/README.md) |
| 查代码放哪、每项功能属于哪个插件 | [目录与插件职责](docs/architecture/workspace-layout.md) |
| 推进下一阶段与验收 | [本地阅读闭环计划](docs/product/next-phase.md) |
| 复现 NSH 环境与构建检查 | [阅读宿主实验](experiments/nsh-reader/README.md) |
| 设计持久安装、授权恢复和升级回滚 | [包生命周期](docs/architecture/plugin-lifecycle.md) |
| 新增插件能力与外部资料接入 | [插件扩展架构](docs/architecture/extensions.md) |
| 本地构建、检查与 CI 的实际范围 | [开发指南](docs/development.md) |
| 项目技能、协作规则 | [AGENTS.md](AGENTS.md) |
| 查看源码审查和详细技术设计 | [研究总览](docs/research/README.md) |
| 查看后续 M-1/M0/M1 任务 | [阶段计划](docs/research/06-milestones-risks.md) |

## 当前工程

- Cargo Workspace，固定 Rust 1.97.1；该版本是本机已有工具链，不代表已验证所有 NSH 依赖兼容。
- learning-domain 保持无业务类型、零外部依赖。
- extension-api 与 extension-host 实现 0.1 JSON 策略协议、能力协商、内存安装元数据、资源授权、会话、生命周期和贡献查询。
- serde/serde_json 用于协议与示例输入；没有 NSH/SQLite/运行时/模型/云服务连接。
- 依赖边界检查及反例测试、本地 Markdown 链接检查。
- 三桌面 Core CI 配置；是否在远端运行以真实流水线结果为准。
- apps/adapters/plugins/sdk 已建立职责入口，尚无对应生产模块；[NSH 前置实验](experiments/nsh-reader/RESULTS.md) 已固定源码并检查环境，构建被系统开发依赖和离线缓存阻断。

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

## 当前产品决策状态

长期愿景与工程不变量沿用已给出的产品说明。插件化已由用户明确为基座要求；第一阶段支持第三方开发/安装/分发，自己的功能也通过同一公开插件接口开发，见 [ADR 0003](docs/architecture/adr/0003-plugin-first-delivery.md)。RSS和微信读书选段用于检验通用架构，不代表具体插件已实现或确定首版交付。首版是否包含 Reader AI、画布、同步，以及云端默认行为仍需要用户回复，见范围记录；推荐项不会自动转成确认项。

2026-11-07，用户明确推进基座功能实现。本轮交付无界面插件策略内核，见 [功能规格](docs/product/foundation.md) 与 [ADR 0004](docs/architecture/adr/0004-headless-extension-policy.md)；安装操作仅注册内存元数据，不校验包内容或执行插件代码。独立 Wasm 实验随后验证了本机包执行、权限与资源边界，见 [实验结果](experiments/extension-wasm/RESULTS.md)。生产运行时、完整安全安装、持久化和 M-1 平台/渠道验收仍未通过。

## 项目状态

QUSOR 仍是项目代号，商标/域名/组织名未核查。项目采用 [MIT 许可证](LICENSE)，仓库托管于 [GitHub](https://github.com/csic21/qusor)。当前 crate 保持 publish=false，尚不发布到 crates.io。
