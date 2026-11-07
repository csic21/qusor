# QUSOR 基座开发与验证

## 环境

Rust 版本以 [rust-toolchain.toml](../rust-toolchain.toml) 为准；Python 3.11+ 用于标准库检查脚本。没有 Node/npm、数据库、Supabase CLI 或模型服务的启动要求。

当前工作区位于 WSL 的 /workspace/qusor。建议在 Ubuntu 中进入这个 Linux 路径运行 Cargo，避免从 Windows 跨 UNC 编译。将来使用 SQLite 时运行数据放在应用本地目录，不能把源码工作区的位置当作数据库部署位置。

```sh
cd /workspace/qusor
cargo fmt --all -- --check
python3 scripts/check_architecture.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo doc --workspace --no-deps --locked --offline
```

原生 Windows 环境使用 python 替代 python3。首次安装固定 Rust 工具链/组件可能需要网络；安装完成后当前无外部依赖的 Cargo 检查可以离线执行。

Cargo.lock 已纳入工程。当前配置变更需要重新生成锁文件时使用 cargo generate-lockfile --offline；不要为了绕过 --locked 检查忽略锁文件。

## 检查分别证明什么

| 检查 | 证明 | 不证明 |
|---|---|---|
| fmt/clippy | 当前 Rust 代码可格式化、通过静态检查 | 产品功能存在 |
| cargo test | 当前测试编译运行；基座业务测试数量是零 | 阅读器/Agent/同步已测试 |
| cargo doc | 当前库文档可生成 | 设计已实机验证 |
| 架构检查及 Python 测试 | 当前成员/依赖边界和反例有效 | 未来任意依赖图都已审查 |
| 文档链接检查 | 仓库内 Markdown 文件目标存在 | 所有远端URL或页内锚点有效 |
| GitHub Actions 配置 | 计划在三桌面runner执行相同检查 | 流水线已运行、NSH三桌面或移动通过 |

GitHub Actions 采用官方 [checkout](https://github.com/actions/checkout) 与 [setup-python](https://github.com/actions/setup-python) 当前文档的 v7 主版本，只读 contents、关闭凭据持久化；没有部署、发布或远端数据库步骤。

## 新增工作

先关联 [Feature ID](product/scope.md)。涉及架构决策时使用 [架构技能](../.agents/skills/qusor-architecture/SKILL.md)；技术验证使用 [实验约定](../experiments/README.md)。

新增 Workspace member 或依赖需修改 [允许清单](../config/architecture.toml)，给出使用理由和相应验证，不能只是删掉检查。当前不需要为纯文案改动编写业务测试。

## 技能校验

三个项目技能都是仓库文件；格式为 YAML frontmatter 加 Markdown。创建/修改时运行环境中 skill-creator 的 quick_validate.py，并检查引用路径。本仓库不复制该系统技能的校验器，也不依赖用户机器上的绝对技能路径来运行 CI。

## 基座验证记录（2026-11-07）

已在本地 WSL Ubuntu、Rust 1.97.1 下通过格式检查、Clippy、离线编译/测试和库文档生成。依赖边界检查通过，6 个 Python 测试方法通过；三个项目技能通过系统 skill-creator 的格式校验，仓库 Markdown 本地链接检查通过。

Rust 业务测试为 0，符合当前仅有空库边界的状态。未运行远端 GitHub Actions、原生 Windows/macOS、NSH 或移动端验收；上述检查不代表 M-1 通过。

同日插件化设计增量：已复核依赖允许清单、22份Markdown文件目标和两个修改后的项目技能格式。新增插件架构/ADR、范围与验证计划；没有新增Rust依赖、插件运行时或业务代码。EXT-P01–P10均未执行。

同日C-07确认后：第一阶段第三方开发/安装/分发和第一方功能插件化已写入ADR 0003，新增EXT-B01–B08与EXT-P11–P13。23份Markdown的本地文件目标及两个修改后的技能格式校验通过，旧的待确认分期已清理。此轮只修改文档/技能，未重跑无变更的Rust构建；SDK、独立包、隔离和平台验收均未执行。
