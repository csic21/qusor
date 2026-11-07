# QUSOR 求索

QUSOR 是个人学习与知识探索系统，采用 NSH 界面与独立 Rust Core。当前交付是可检查的工程基座、项目技能和功能设计，尚无产品业务实现。

## 从这里开始

| 目的 | 文档 |
|---|---|
| 确认我们做什么、哪些尚未决定 | [功能范围与确认记录](docs/product/scope.md) |
| 阅读与思考的交互设计原则 | [产品体验设计基线](docs/product/experience.md) |
| 理解当前与未来模块边界 | [技术架构入口](docs/architecture/README.md) |
| 新增插件能力与外部资料接入 | [插件扩展架构](docs/architecture/extensions.md) |
| 本地构建、检查与 CI 的实际范围 | [开发指南](docs/development.md) |
| 项目技能、协作规则 | [AGENTS.md](AGENTS.md) |
| 查看源码审查和详细技术设计 | [研究总览](docs/research/README.md) |
| 查看后续 M-1/M0/M1 任务 | [阶段计划](docs/research/06-milestones-risks.md) |

## 当前工程

- Cargo Workspace，固定 Rust 1.97.1；该版本是本机已有工具链，不代表已验证所有 NSH 依赖兼容。
- 一个 learning-domain library，只有边界文档，没有 Document/Note 等占位业务类型。
- 无第三方 Rust 依赖，无 NSH/SQLite/模型/云服务连接。
- 依赖边界检查及反例测试、本地 Markdown 链接检查。
- 三桌面 Core CI 配置；是否在远端运行以真实流水线结果为准。

本地检查需要 Rust 工具链和 Python 3.11+。从仓库根目录运行：

```sh
python3 scripts/check_architecture.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo doc --workspace --no-deps --locked --offline
```

Windows 的 Python 命令通常是 python；WSL 项目应在 Linux 路径内调用 Cargo，详见开发指南。

## 当前产品决策状态

长期愿景与工程不变量沿用已给出的产品说明。插件化已由用户明确为基座要求；第一阶段支持第三方开发/安装/分发，自己的功能也通过同一公开插件接口开发，见 [ADR 0003](docs/architecture/adr/0003-plugin-first-delivery.md)。RSS和微信读书选段用于检验通用架构，不代表具体插件已实现或确定首版交付。首版是否包含 Reader AI、画布、同步，以及云端默认行为仍需要用户回复，见范围记录；推荐项不会自动转成确认项。

本轮允许的基座工作不表示 M-1 通过，也不表示业务功能已获准开工。后续先执行独立技术验证，按真实证据更新阶段状态。

## 项目状态

QUSOR 仍是项目代号，商标/域名/组织名未核查。项目采用 [MIT 许可证](LICENSE)，仓库托管于 [GitHub](https://github.com/csic21/qusor)。当前 crate 保持 publish=false，尚不发布到 crates.io。
