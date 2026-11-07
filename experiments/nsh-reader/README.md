# NSH 最小阅读宿主实验

状态：P01 部分完成，已有本机环境检查与固定源码留证；P02 构建检查被依赖条件阻断，尚无窗口或阅读运行证据。实际结果见 [RESULTS.md](RESULTS.md)。

本实验承接用户批准的下一阶段：先验证真实 NSH 宿主，再经公开插件接口实现阅读流程。它关联 [M-1 的 P01/P02](../../docs/research/06-milestones-risks.md)，为 P03/P05/P07/P08/P09 提供前置证据；对应 F-PLAT-01、F-READ-01/02、F-SRC-01、F-EXT-04/06。没有把 NSH 加入生产 Workspace，也没有实现产品阅读器。

## 假设与通过条件

**假设**：固定 NSH 提交能在目标桌面以默认 native backend 启动 Rust+TSX 混合应用，并继续承载受控的离线阅读 Surface。

分阶段证伪，不能以后一阶段的设计替代运行：

| 阶段 | 通过条件 | 本轮状态 |
|---|---|---|
| P01 本机准备 | 固定源码、lockfile、工具链、系统库、设备与显示条件可复查 | 源码与环境已留证；系统库/依赖缓存不齐 |
| P02 混合宿主 | 编译、链接、打包、真实窗口启动、关闭重开各有记录 | Cargo check 在依赖解析失败；其余未验证 |
| P03 桥接 | 请求/错误/事件/取消、关闭后迟到消息行为有真实结果 | 未实施 |
| P05 输入/选区 | 中文组合态、emoji、复制、撤销、显式 offset 单位正确 | 仅准备合成语料 |
| P07/P08/P09 阅读 | 安装包离线加载资源、真实 PDF/EPUB、选区与锚点、出站/桥接隔离通过 | 未实施 |

缺 GTK/WebKit 开发库时停在环境准备；在具备依赖的 Linux 环境复验，或另行验证已配置的 Windows 原生工具链。缺系统库不能用 Web target、纯文本页面或关闭 webview feature 的占位框填写“阅读通过”。保留 NSH 方向及 [已有主备阅读路线](../../docs/research/02-reading-canvas-routes.md)。

## 目录和架构边界

```text
nsh-reader/
├── README.md                     假设、阶段门槛与复现流程
├── RESULTS.md                    实际结果和未验证范围
├── preflight.py                  Python 标准库环境检查；可选离线 cargo check
├── test_preflight.py             libclang 缺失/加载失败/就绪状态回归
├── fixtures/selection.txt        自建公开合成语料，尚未渲染
├── evidence/linux-preflight.json 本机命令、退出码、版本和 hash
└── target/                       被忽略的本地源码/构建缓存，不提交
    └── nsh/                      固定 SHA 的上游独立 Workspace
```

本轮复用上游自己的 Workspace/lockfile，没有添加 QUSOR Cargo member，也未声明新的生产或实验 Rust 包。后续真正编写宿主探针时才建立独立 probe Workspace、固定 lockfile，并在 [依赖策略](../../config/architecture.toml) 增加实验条目；缓存中的上游源码不作为 QUSOR 模块。

未来实验中的 NSH adapter 只处理平台窗口、事件、资源与 Surface 生命周期。阅读工作区、命令、选区动作由独立插件通过公开能力 broker 请求；不可让第一方阅读插件直接 import NSH 内部对象或写 Core 数据库。专业 PDF/EPUB renderer 是宿主受控服务，文档来源/版本/稳定定位归 Core；窗口节点和 DOM offset 不能直接成为永久引用。完整边界沿用 [插件架构](../../docs/architecture/extensions.md)。

此处的上游 Rust+TSX 示例只验证平台桥接，不是 QUSOR 插件 SDK、权限或隔离的验收。合成文本也不是 PDF/EPUB 渲染验收。

## 固定基线

- NSH：[913fd76e1f8434ec6cdf248f4476c1ebe08af19f](https://github.com/longbridge/nsh/tree/913fd76e1f8434ec6cdf248f4476c1ebe08af19f)，Workspace 0.4.0；使用默认 Linux GTK backend。
- NSH 根 lockfile SHA-256：`3495ffc4a1fd150bb583e069afcb5b4f2d389dbfb5f8b1c942279eaf9e80414b`。
- 本轮显式选择已安装 Rust 1.97.1；兼容性仍未由完整构建证明。上游 toolchain 文件写的是 stable 且列有移动目标，因此运行时用局部 RUSTUP_TOOLCHAIN，避免自动准备无关目标。
- 合成语料与其他源码文件的 SHA-256 均在 [机器可读结果](evidence/linux-preflight.json)。[selection.txt](fixtures/selection.txt) 由本项目编写，采用 [MIT 许可](../../LICENSE)，不含用户资料。
- 固定源码说明工具链为纯 Rust；Node/npm/Bun 不属于此实验的前置条件。系统库要求来自 [Linux 开发文档](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/linux.md) 和 [Linux CI](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/linux.yml)。

## 复现环境检查

需要 Python 3.11+。以下命令在仓库根目录的 Linux/WSL shell 运行。Windows PowerShell 可用：

```powershell
wsl.exe -d Ubuntu --cd /workspace/qusor --exec bash -lc 'python3 experiments/nsh-reader/preflight.py'
```

首次获取公开源码；已有固定 checkout 可通过 --source 指定，不覆盖已有目录：

```sh
git clone --no-checkout --depth 1 --filter=blob:none https://github.com/longbridge/nsh.git experiments/nsh-reader/target/nsh
git -C experiments/nsh-reader/target/nsh fetch --depth 1 origin 913fd76e1f8434ec6cdf248f4476c1ebe08af19f
git -C experiments/nsh-reader/target/nsh checkout --detach 913fd76e1f8434ec6cdf248f4476c1ebe08af19f
python3 experiments/nsh-reader/preflight.py --output experiments/nsh-reader/target/preflight.json
python3 experiments/nsh-reader/preflight.py --cargo-check --output experiments/nsh-reader/target/compile-preflight.json
```

默认只执行只读检查；--output 明确写 JSON，--cargo-check 在上游 Workspace 发起有时间上限的离线 cargo check（会写构建缓存）。脚本不安装软件、不联网、不改全局配置、不打开窗口。源码 SHA 不符、受跟踪文件有改动或未安装固定工具链时跳过 Cargo。退出码 1 表示环境/检查阻断，0 只表示本脚本已检查的前置项齐备，不表示 P02 或阅读通过。报告不采集完整环境变量、凭据或用户资料。

libclang 另做动态库加载与 C API 符号/版本查询；clang 命令存在不能代替该检查。Linux 默认搜索动态链接器及常见 Debian/Ubuntu LLVM/multilib 目录；定制安装可通过 LIBCLANG_PATH 指定目录或文件。显式覆盖无效时不回退到系统库掩盖配置错误，报告不记录覆盖路径或可能带路径的 loader 异常。无法搜索/加载列为 unknown 或 unavailable，并阻断 prerequisites_ready；检查通过仍不代表 bindgen 已生成绑定。

运行相关回归：

```sh
python3 -m unittest discover -s experiments/nsh-reader -p test_preflight.py -v
```


## 依赖补齐后执行 P02

当前缺少 GTK4/Graphene、GStreamer core/base development packages、WebKitGTK 6.0，以及 Cargo 依赖缓存。Ubuntu/Debian 的上游构建清单包含 clang、libclang-dev、libgtk-4-dev、libwebkitgtk-6.0-dev、libgstreamer1.0-dev、libgstreamer-plugins-base1.0-dev，运行测试还包含 GStreamer base/good plugins、Xvfb。以目标发行版的包版本和上游固定文档为准。**本轮未执行系统软件安装。**

在这些工具已准备好的环境中，先重跑 preflight，再按 [上游混合示例](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/examples/rust-exports/README.md) 执行：

```sh
cd experiments/nsh-reader/target/nsh
export RUSTUP_TOOLCHAIN=1.97.1
cargo fetch --locked --target x86_64-unknown-linux-gnu
cargo check -p nsh-platform-linux --no-default-features --features js,webview --locked --offline
cargo build -p nsh-cli --locked --offline
CARGO_NET_OFFLINE=true ./target/debug/nsh run examples/rust-exports --target linux
CARGO_NET_OFFLINE=true ./target/debug/nsh package examples/rust-exports --target linux --out ../packaged-rust-exports --run
```

这些构建/启动步骤尚未通过本机验证。先完成首个 run 并关闭窗口，再执行 package；CLI 为混合应用生成的辅助宿主如报告新的依赖缓存缺口，记录它的 manifest/lock 与失败，不通过自动更新依赖跳过 --locked 基线。打包结果、辅助 lockfile、运行库版本和日志另存于缓存，验收结果登记到对应证据文件。

P02 留证顺序：记录编译与链接；启动真实窗口并操作 Rust 导出的组件；正常关闭；重新启动；打包后退出开发服务再启动；记录显示后端、OS/SDK、字体/输入法、窗口日志和截图。WSLg 运行必须标注 WSLg，不外推成 Windows 原生或所有 Linux 发行版通过。

通过 P02 后，另建最小受控阅读探针执行 P03/P05/P07，再按 P08/P09 加入真实 PDF/EPUB fixture。使用 [验证记录模板](../../docs/templates/validation-record.md)，单独填写输入、选区、离线子资源、权限和重开定位结果；不要将“能打开网页”当成阅读或插件能力完成。
