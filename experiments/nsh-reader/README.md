# NSH 最小阅读宿主实验

新增 [TSX 插件 UI 通路研究](tsx-plugin-ui/README.md)：已核对最新上游并用现有固定 CLI 完成仓库外 TSX 作者构建；普通包的独立 VM/原生挂载/权限和替换仍待验证，完整结论见 [研究09](../../docs/research/09-tsx-plugin-native-ui.md)。

状态：隔离的本地原生依赖目录已解除平台编译阻断，native 平台 Cargo check 与 NSH CLI 链接通过；Rust 示例描述程序和 TSX 字节码已构建。独立原生窗口已显示离线 Rust+TSX、实际按钮回调并正常关闭重开；原生 PDF 页图/窗口拖选已有子项，完整打包、输入与阅读仍待验证。实际结果见 [RESULTS.md](RESULTS.md)。

本实验承接用户批准的下一阶段：先验证真实 NSH 宿主，再经公开插件接口实现阅读流程。它关联 [M-1 的 P01/P02](../../docs/research/06-milestones-risks.md)，为 P03/P05/P07/P08/P09 提供前置证据；对应 F-PLAT-01、F-READ-01/02、F-SRC-01、F-EXT-04/06。没有把 NSH 加入生产 Workspace，也没有实现产品阅读器。

2026-11-08 用户确认 C-09 原生优先、少用 Web。探针默认 `--profile native`，检查 NSH 原生 GTK/JS 宿主及其真实原生依赖；只有显式 `--profile webview` 才额外检查 WebKit 并启用 webview feature。历史 JSON 留存原先 WebView profile，不能与新的 native profile 混为同一验证。PDF 主线沿原生 PDFium 服务，见 [provider 资格实验](../pdf-provider/README.md)；它不替代 NSH 窗口与选区 UI。

## 假设与通过条件

**假设**：固定 NSH 提交能在目标桌面以默认 native backend 启动 Rust+TSX 混合应用，并继续承载受控的离线阅读 Surface。

分阶段证伪，不能以后一阶段的设计替代运行：

| 阶段 | 通过条件 | 本轮状态 |
|---|---|---|
| P01 本机准备 | 固定源码、lockfile、工具链、系统库、设备与显示条件可复查 | 固定源码、缓存与隔离本地依赖前缀已检查；五端环境/发布许可未齐 |
| P02 混合宿主 | 编译、链接、打包、真实窗口启动、关闭重开各有记录 | native 平台 check、CLI及独立宿主链接通过；实际原生窗口、按钮回调与正常关闭重开通过；打包未验 |
| P03 桥接 | 请求/错误/事件/取消、关闭后迟到消息行为有真实结果 | 原生PDF配置/真实raw事件及世代拒绝已有子项；完整异步取消/迟到回调未通过 |
| P05 输入/选区 | 中文组合态、emoji、复制、撤销、显式 offset 单位正确 | 桌面鼠标PDF选区已有子项；中文组合态/键盘/多指未通过 |
| P07/P08/P09 阅读 | 安装包离线加载资源、真实 PDF/EPUB、选区与锚点、出站/桥接隔离通过 | PDF原生页图与选区已有局部证据；EPUB/正式安装包/插件桥接和业务旅程未通过 |

缺相应 profile 的开发库时停在环境准备；在具备依赖的 Linux 环境复验，或另行验证已配置的 Windows 原生工具链。native profile 不要求 WebKit，但仍须真实原生窗口与 PDF Surface；不能用关闭 WebView 后的占位框或纯文本页面填写“阅读通过”。WebView 备选若启用，仍须完整验证其依赖与桥接隔离。保留 NSH 方向及 [已有主备阅读路线](../../docs/research/02-reading-canvas-routes.md)。

## 目录和架构边界

```text
nsh-reader/
├── README.md                     假设、阶段门槛与复现流程
├── RESULTS.md                    实际结果和未验证范围
├── preflight.py                  Python 标准库环境检查；可选离线 cargo check
├── test_preflight.py             libclang 缺失/加载失败/就绪状态回归
├── fixtures/selection.txt        自建公开合成语料，尚未渲染
├── evidence/                     首轮、补齐缓存后的检查与声明许可清单
└── target/                       被忽略的本地源码/构建缓存，不提交
    └── nsh/                      固定 SHA 的上游独立 Workspace
```

本轮复用上游自己的 Workspace/lockfile，没有添加 QUSOR Cargo member，也未声明新的生产或实验 Rust 包。后续真正编写宿主探针时才建立独立 probe Workspace、固定 lockfile，并在 [依赖策略](../../config/architecture.toml) 增加实验条目；缓存中的上游源码不作为 QUSOR 模块。

实验中的 NSH adapter 只处理平台窗口、事件、资源与 Surface 生命周期。阅读工作区、命令、选区动作由独立插件通过公开能力 broker 请求；不可让第一方阅读插件直接 import NSH 内部对象或写 Core 数据库。专业 PDF/EPUB renderer 是宿主受控服务，文档来源/版本/稳定定位归 Core；窗口节点和 DOM offset 不能直接成为永久引用。完整边界沿用 [插件架构](../../docs/architecture/extensions.md)。

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

原生优先路径在自己的终端准备必要系统库（sudo 需要交互认证，代理无法代输）：

```sh
sudo apt-get install --no-install-recommends libgtk-4-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
python3 experiments/nsh-reader/preflight.py --profile native
python3 experiments/nsh-reader/preflight.py --profile native --cargo-check --output experiments/nsh-reader/target/native-compile-preflight.json
```

安装前可用同参数 `apt-get -s` 审查机器当前的依赖变更；本机候选方案涉及 Mesa/systemd 等依赖升级，不由此文档自动执行。可选 WebView profile 再另行安装 `libwebkitgtk-6.0-dev` 并显式 `--profile webview`。以下历史构建与许可清单使用 js,webview，应按当前要验证的 profile 区分，不将 WebKit 作为原生宿主的前置。

当前缺少 GTK4/Graphene、GStreamer core/base development packages 和 WebKitGTK 6.0。固定 Linux 目标的 Cargo 缓存已补齐，离线 fetch 成功；[后续编译证据](evidence/linux-cached-preflight.json) 记录了真实进入 Rust 编译后的 GStreamer build script 失败。Ubuntu/Debian 的上游构建清单包含 clang、libclang-dev、libgtk-4-dev、libwebkitgtk-6.0-dev、libgstreamer1.0-dev、libgstreamer-plugins-base1.0-dev，运行测试还包含 GStreamer base/good plugins、Xvfb。以目标发行版的包版本和上游固定文档为准。**本轮未执行系统软件安装。**

本机 apt 模拟安装（`apt-get -s --no-install-recommends install ...`）会新增 36 包、升级 18 包，包含 systemd/udev 和当前配置的 Kisak Mesa；非交互 sudo 也需要身份认证。本轮只保留 [候选版本与模拟方案](evidence/linux-dependency-preparation.json)，没有执行系统安装、升级或替换软件源。在确认目标机器的依赖准备方式后再安装，不能用 Web target 或隐藏 WebView 占位框绕过门槛。

指定编译路径的 [329 项 Cargo 声明许可清单](evidence/linux-native-dependencies.tsv) 来自下列离线命令，按名称/版本去重。它覆盖 normal/build 依赖，不是发布包 SBOM 或许可证审批；系统库、捆绑资源、默认 feature、CLI/应用包及其他平台仍须单独核查：

```sh
cd experiments/nsh-reader/target/nsh
export RUSTUP_TOOLCHAIN=1.97.1
cargo fetch --locked --target x86_64-unknown-linux-gnu --offline
cargo tree -p nsh-platform-linux --no-default-features --features js,webview \
  --locked --offline --target x86_64-unknown-linux-gnu --edges normal,build \
  --prefix none --format '{p}|{l}|{r}' > ../linux-dependency-tree.txt
```

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

上述 fetch 已通过，平台 cargo check 在缺少 GStreamer 开发库处失败；CLI 完整构建及应用构建/启动步骤尚未通过本机验证。先完成首个 run 并关闭窗口，再执行 package；CLI 为混合应用生成的辅助宿主如报告新的依赖缓存缺口，记录它的 manifest/lock 与失败，不通过自动更新依赖跳过 --locked 基线。打包结果、辅助 lockfile、运行库版本和日志另存于缓存，验收结果登记到对应证据文件。

P02 留证顺序：记录编译与链接；启动真实窗口并操作 Rust 导出的组件；正常关闭；重新启动；打包后退出开发服务再启动；记录显示后端、OS/SDK、字体/输入法、窗口日志和截图。WSLg 运行必须标注 WSLg，不外推成 Windows 原生或所有 Linux 发行版通过。

通过 P02 后，另建最小受控阅读探针执行 P03/P05/P07，再按 P08/P09 加入真实 PDF/EPUB fixture。使用 [验证记录模板](../../docs/templates/validation-record.md)，单独填写输入、选区、离线子资源、权限和重开定位结果；不要将“能打开网页”当成阅读或插件能力完成。


## 本地隔离依赖目录（2026-11-08）

不需要修改系统软件：本机将21份官方 Ubuntu deb 解包至被忽略的 `target/native-prefix`，并将其中 pkg-config 的 `/usr` 前缀重定位到该目录；保留原始元数据，校验包摘要。此路径复用系统已有的其余依赖，不是通用发行包或完整自包含 SDK。清单、来源、摘要和编译记录见 [本地依赖证据](evidence/linux-native-prefix.json)。

已下载清单中的包后，可在空目录复现解包与环境检查：

```sh
python3 experiments/nsh-reader/native_prefix.py --downloads experiments/nsh-reader/target/native-prefix-downloads --prefix experiments/nsh-reader/target/native-prefix
python3 experiments/nsh-reader/preflight.py --profile native --native-prefix experiments/nsh-reader/target/native-prefix --cargo-check --output experiments/nsh-reader/target/local-prefix-preflight.json
python3 -m unittest discover -s experiments/nsh-reader -p 'test_*.py' -v
```

`native_prefix.py` 不下载软件，拒绝摘要不符或非空目标；环境只传给相关子进程。首次准备须按清单 URI 获取同版本文件，不能拿不同版本包套用旧摘要。实测 [prefixed preflight](evidence/linux-local-prefix-preflight.json) 退出0，原生平台 `js` check 通过；12项 Python 回归通过。系统默认 pkg-config 仍可能找不到这些开发库，历史失败记录保留。

上游 `nsh run` 生成的 Linux 开发宿主默认启用 `nsh-linux-host/webview`，会额外请求 JavaScriptCore/WebKit，即使示例没有 WebView。原生平台 check 通过不代表这条完整启动命令通过；已用显式选择 native 能力的独立宿主验证可见窗口、按钮与重开，不修改固定上游源码，也不安装 WebKit 充当原生门槛。


## 原生窗口与桥接子项已实测

[独立 native 宿主](native-host/README.md) 使用自己的 Workspace/lock，根生产模块不链接 NSH。已完成离线 CLI/宿主构建、真实 Rust+TSX bundle、可见 GTK 窗口、Deposit 按钮通过 Rust method/listener 更新显示、WM_DELETE 正常退出和重新启动。依赖图与实际 ELF 链接均无 WebKit/JavaScriptCore/libsoup，固定上游 tracked 源码保持干净。

实测环境为 WSLg X11；首次窗口实际385×900、重开600×900，示例账户从120变145有截图及事件日志，重开回初始值属于无持久化示例行为。见 [结构化证据](evidence/linux-native-window.json)、[操作后截图](evidence/native-window-after-deposit.png) 及 [重开截图](evidence/native-window-reopened.png)。原生窗口不等于正式阅读 UI；打包安装、中文输入/组合态、异步取消/迟到事件、PDF/EPUB和其他平台仍待逐项验证，Mesa/GTK警告保留。


## 原生 PDF Surface 子项

[PDF Surface 探针](pdf-surface/README.md) 继续将真实 PDFium 页图、同版字符报告、NSH 原生 Image/Canvas 与 raw responder 贯通。实际普通页/旋转页高亮和引文一致，已核对空白、出界、缩放/换页清理与窄窗显示。探针候选明确“尚未保存”；没有将受控服务验证替代普通插件或产品持久化旅程。输入法、键盘/多指、系统级捕获与完整异步资源生命周期仍分别留证，见 [ADR 0020](../../docs/architecture/adr/0020-native-pdf-surface-probe.md)。


## 普通签名包接入原生 Surface

[公开Surface集成资格](plugin-surface/RESULTS.md) 已贯通仓库外SDK/普通0.2签名Wasm、唯一Host registry、真实NSH Image/Canvas/Input、单段与跨行完整提案、明确保存及Core导出空库恢复。实测与重现入口、Xvfb/当前桌面输入差别、资源清理限制均在结果中。它不代替正式reader/library/notes功能包或完整输入/多平台验收。
