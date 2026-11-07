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
cargo run -p extension-host --example policy_walkthrough --locked --offline
cargo doc --workspace --no-deps --locked --offline
```

原生 Windows 环境使用 python 替代 python3。首次安装固定 Rust 工具链/组件及下载 serde/serde_json 的锁定依赖需要网络；先运行 cargo fetch --locked，缓存就绪后执行上述离线检查。CI 在准备阶段下载锁定依赖，验证阶段仍使用 --locked --offline。

Cargo.lock 已纳入工程。当前配置变更需要重新生成锁文件时使用 cargo generate-lockfile --offline；不要为了绕过 --locked 检查忽略锁文件。

## 独立插件执行实验

用户已授权的 [Wasm 实验](../experiments/extension-wasm/README.md) 使用独立执行器和作者 Workspace。先分别以 cargo fetch --manifest-path <对应 Cargo.toml> --locked 准备缓存，再在 WSL 从根目录运行：

```sh
python3 experiments/extension-wasm/validate.py --output /tmp/qusor-wasm-results.json
```

此命令在仓库外构建作者及全部样例，检查依赖、格式、测试和 Clippy，运行监督进程场景并保存 hash/结果。实际通过范围及限制见 [本机记录](../experiments/extension-wasm/RESULTS.md)。根 Core CI 不执行该实验，也不证明其原生/移动平台可用。

## 检查分别证明什么

| 检查 | 证明 | 不证明 |
|---|---|---|
| fmt/clippy | 当前 Rust 代码可格式化、通过静态检查 | 产品功能存在 |
| cargo test | JSON 协议、宿主授权与生命周期的真实回归通过 | 阅读器/知识入库/Agent/同步已测试 |
| policy_walkthrough 示例 | 注册、显式授权、跨库拒绝、撤权、升级与重装失效 | 插件包已安全安装执行或状态已持久保存 |
| cargo doc | 当前库文档可生成 | 设计已实机验证 |
| 架构检查及 Python 测试 | 当前直接声明、已解析传递闭包与反例有效 | 未解析第三方可选分支、FFI、运行权限均已审查 |
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

上述初始骨架记录中 Rust 业务测试为 0，当时仅有空库边界。未运行远端 GitHub Actions、原生 Windows/macOS、NSH 或移动端验收；上述检查不代表 M-1 通过。

同日插件化设计增量：已复核依赖允许清单、22份Markdown文件目标和两个修改后的项目技能格式。新增插件架构/ADR、范围与验证计划；没有新增Rust依赖、插件运行时或业务代码。EXT-P01–P10均未执行。

同日C-07确认后：第一阶段第三方开发/安装/分发和第一方功能插件化已写入ADR 0003，新增EXT-B01–B08与EXT-P11–P13。23份Markdown的本地文件目标及两个修改后的技能格式校验通过，旧的待确认分期已清理。此轮只修改文档/技能，未重跑无变更的Rust构建；SDK、独立包、隔离和平台验收均未执行。

## 无界面插件策略实现验证（2026-11-07）

用户已授权从工程骨架推进基座功能，实现范围见 [功能规格](product/foundation.md) 与 [ADR 0004](architecture/adr/0004-headless-extension-policy.md)。本次在 WSL Ubuntu / Rust 1.97.1 实测：

- 35 个 Rust 测试通过：21 个 JSON 协议测试，14 个宿主授权与生命周期测试。
- policy_walkthrough 示例执行通过，展示注册、授权、跨库拒绝、撤权拒绝旧回调、升级清空授权及重装后旧会话失效。
- fmt、Clippy 全目标检查和库文档构建通过；Clippy / rustdoc 均将警告视为错误。
- 直接依赖声明与配置模块的已解析传递闭包检查通过；21 个 Python 反例测试通过。
- 本地 Markdown 文件目标检查通过；不涵盖远端 URL 与页内锚点。

协议 Schema 已进行 Draft 2020-12 结构检查及合法/非法输入校验；该补充检查使用本机已有 jsonschema，不增加项目 Python 运行依赖。Rust 校验仍是 wire 入口的运行时强制约束。

检查使用本机依赖缓存离线完成，没有在此会话提交或推送。保留并行会话新增的 MIT / GitHub 元数据。CI 已加入锁定依赖准备与无界面示例，但本轮未执行远端流水线；未验证原生 Windows/macOS、NSH、移动、发布渠道、独立插件包、沙箱、持久化或真实知识读写。
