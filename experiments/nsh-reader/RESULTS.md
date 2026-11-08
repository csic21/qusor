# 最新增量：原生 PDF 页图与真实拖选子项

2026-11-08：[PDF Surface 探针](pdf-surface/README.md) 已在 WSLg X11 实际显示原生 PDFium 页图，以 NSH 原生 Image/Canvas 和真实 raw responder 选择字符。普通页60%选择 QUSOR、旋转页70%选择 Selection，候选/高亮/页码核对一致；空白、出界取消、缩放/换页/resize清理和窄窗已有运行子项。报告与bundle绑定、图像加载世代门均有明确边界。正式 reader 插件、保存回源、输入法、键盘/触控与其他平台仍未完成。

该轮修复并保留了实际发现的失败：main与JS线程的thread-local状态不互通、press移动阈值提前发出结束、页面资源异步加载和初始零尺寸布局。映射窗口或最终截图不能单独作为输入链通过证据；详情与复现以局部记录为准。以下保留更早的窗口/编译历史。

# 最新增量：原生窗口、实际按钮与重开已有证据

2026-11-08：使用21份校验过的官方 Ubuntu deb，在实验 target 内准备隔离前缀，未安装/升级系统软件。固定源码和根 lockfile 不变，原生 GTK/Graphene/GStreamer 实际参与编译；`nsh-platform-linux --no-default-features --features js --locked --offline` check通过，`nsh-cli` build/link通过。见 [包与命令证据](evidence/linux-native-prefix.json)、[完整前置复验](evidence/linux-local-prefix-preflight.json) 和 [复现入口](README.md#本地隔离依赖目录2026-11-08)。

Rust+TSX 示例进一步完成原生描述程序链接/执行及 TSX 字节码构建；上游默认开发宿主随后因默认启用 WebView 而请求缺失的 JavaScriptCore。该默认启动路径未通过；不能将平台 check 当成 P02 完成。生成 helper 的 lock 由固定上游 lock 衍生，只增加本地入口并裁剪未用包，registry 版本/来源/摘要无新增漂移，见 [锁与失败日志证据](evidence/linux-helper-lock.json)。随后采用 [独立 native 宿主](native-host/README.md)，已完成离线 bundle 实际显示、按钮经 Rust method/listener 改变界面、正常关闭与重开，见 [结构化证据](evidence/linux-native-window.json)。依赖图和 ELF 链接均无 WebKit/JavaScriptCore/libsoup，固定源码保持干净。完整 P02 打包安装、中文输入、取消/迟到事件和阅读仍未通过。12项 Python 回归通过，覆盖前缀摘要/重定位和环境传递边界。

以下是各阶段历史记录，原生开发库缺失的旧结论不再代表上述隔离前缀环境。

# NSH 阅读宿主实验结果

2026-11-08 C-09 增量：按用户“尽量 native、少 Web”要求，探针增加 native/webview profile，默认 native 的 Cargo 特性为 `js`，可选 WebView 才启用 `js,webview` 并检查 WebKit。9 项 Python 测试通过；[真实新探针](evidence/linux-native-profile-preflight.json) 仍因 GTK4/Graphene/GStreamer 阻断，WebKit 不再是原生路径的硬前置。未重复执行已知缺库的 Cargo 构建；没有窗口/输入/阅读成绩。沙箱外 `sudo -n -l` 也确认需要交互认证，系统库仍未安装；可在自己的终端按 [原生准备步骤](README.md) 执行。以下保留早先 WebView profile 的历史证据，不改写为新 profile 通过。

日期：2026-11-07。结论：**已取得固定源码与本机 P01 环境证据；本轮补齐 Linux Cargo 缓存，P02 已进入 Rust 依赖编译，随后因缺少 GStreamer 原生开发库阻断。尚未完成应用构建或启动窗口。** 这不证明 NSH 功能失败，也不能据此将阅读 Surface 标为可用。

## 假设与门槛

验证固定 NSH 默认 Linux backend 是否具备本机 Rust+TSX 混合启动条件；P02 必须完成编译、链接、打包、真实窗口启动和重开。关联 P01/P02、F-PLAT-01、F-READ-01/02、F-EXT-04/06。后续阅读、桥接与输入任务见 [实验入口](README.md)，本轮没有把它们提前标为完成。

## 固定环境

| 项目 | 实际记录 |
|---|---|
| NSH | 913fd76e1f8434ec6cdf248f4476c1ebe08af19f，0.4.0，Git detached checkout，无受跟踪文件修改 |
| 源码/lockfile | 直接获取公开 Git 仓库；关键文件 SHA-256 见 [JSON](evidence/linux-preflight.json) |
| Rust | rustc 1.97.1，8bab26f4f；cargo 1.97.1；x86_64-unknown-linux-gnu |
| OS | Ubuntu 26.04.1 LTS，WSL2 kernel 6.18.33.2-microsoft-standard-WSL2 |
| CPU | x86_64，12 个逻辑 CPU；RAM/GPU/DPI 未采集，未开展性能测试 |
| 显示 | DISPLAY=:0、WAYLAND_DISPLAY=wayland-0；WSLg、X11 与 Wayland socket 存在；未实际连接窗口 |
| 工具 | git、rustup、clang、cc、pkg-config 在 Linux PATH |
| libclang | 实际加载 libclang-21.so.1，C API 符号可用，查询版本 Ubuntu clang 21.1.8；未执行 bindgen 生成 |
| 已有原生库 | Pango/PangoCairo 1.57.0、Cairo 1.18.4 |
| 缺失原生开发库 | pkg-config 未找到 GTK4、Graphene、GStreamer core/app/audio、WebKitGTK 6.0 |
| 输入法/WebView | 未运行；WebKitGTK development package 未找到，不据此推断所有 runtime 安装状态 |
| fixture | [合成中文/Unicode 语料](fixtures/selection.txt)，项目 MIT；已 hash，尚未渲染 |

NSH 源码、构建缓存保存在被忽略的实验 target 目录；没有创建 QUSOR Rust Workspace 或生产依赖。Windows 的初步只读检查未在当前 PATH 找到 cargo/rustc/node/MSVC，默认 Windows Kits/Visual Studio 路径也未返回安装目录；这不是全盘安装清查或 Windows 原生构建结论。

## 首轮步骤与结果

| 步骤 | 预期 | 实际 | 证据/状态 |
|---|---|---|---|
| 获取固定 NSH 提交 | SHA 一致、源码无修改 | 完整 SHA 匹配；tracked status 为空 | [JSON source](evidence/linux-preflight.json)，通过 |
| 固定版本与语料 | 版本和 hash 可追溯 | manifest、lockfile、toolchain、混合示例、语料 hash 已记录 | 通过此项，不等于完成依赖许可清单 |
| 检查原生前置 | GTK4 >=4.14、Pango >=1.50、WebKitGTK >=2.40、GStreamer 可用 | Pango/Cairo 可用；六个 pkg-config 模块缺失 | [JSON native_packages](evidence/linux-preflight.json)，阻断 |
| libclang 动态库 | 真实加载 C API；不能仅检查 clang 命令 | 系统 libclang-21.so.1 加载成功，版本/必需符号可查询 | [JSON libclang](evidence/linux-preflight.json)，通过此项 |
| libclang 负向回归 | 缺库/伪库不通过，且不泄露显式路径 | 空目录与普通文本伪装 .so 均被拒绝；仅缺 libclang 也使其余就绪条件返回 blocked | [3 项回归](test_preflight.py) 全部通过 |
| 检查显示线索 | 有可供后续窗口连接的显示环境 | WSLg/socket 存在；未测实际连接 | 仅环境证据 |
| 离线 Cargo check | 固定 lock 与工具链检查 Linux+JS+WebView | 退出 101，解析依赖时缺 imara-diff | [JSON cargo_check](evidence/linux-preflight.json)，失败于解析，未进入编译 |
| 缺少源码的负向检查 | 返回阻断，跳过 Cargo，不下载 | 脚本退出 1，cargo_check.status=skipped | 实际执行，行为符合预期 |
| 应用编译/链接 | 可生成完整混合应用 | 未执行完整 build | 未验证 |
| 打包/安装/启动/交互/重开 | 分别有真实日志和结果 | 未执行 | 未验证 |
| 中文输入、PDF/EPUB、选区/回源、离线资源和桥接隔离 | 对应 P05/P07/P08/P09 门槛 | 未执行 | 未验证 |

离线 check 的首个错误：

```text
error: no matching package named `imara-diff` found
location searched: crates.io index
required by package `nsh-base v0.4.0`
```

命令显式关闭平台默认 feature，只打开 js,webview，仍由上游 Workspace 的完整锁定解析触发此缓存需求。此日志证明当前离线缓存不齐；GTK 等缺失来自单独的 pkg-config 检查，并非这个 Cargo 命令已经跑到系统库链接失败。

## 同日继续执行：解除 Cargo 缓存阻塞

固定 SHA、工具链和原始 lockfile 保持不变，旧 [首轮证据](evidence/linux-preflight.json) 保留。新增 [依赖准备记录](evidence/linux-dependency-preparation.json) 和 [补齐缓存后的环境/编译记录](evidence/linux-cached-preflight.json)，不是覆盖原始失败结论。

| 步骤 | 实际结果 | 判断 |
|---|---|---|
| 在线固定目标 fetch | `cargo fetch --locked --target x86_64-unknown-linux-gnu` 退出 0；仅获取公开构建依赖 | 此目标所需缓存已准备 |
| 离线再次 fetch | 同一命令增加 `--offline`，退出 0 | 验证缓存足以离线解析固定目标 |
| 离线平台 check | 保持 `--no-default-features --features js,webview --locked --offline`；进入真实依赖编译，退出 101 | 首个失败为 gstreamer-sys 0.25.4 的 build script；完整 check 未通过 |
| 固定源码完整性 | HEAD 仍为固定 SHA，tracked status 为空，lockfile SHA-256 未变 | 未通过更改依赖或关闭 WebView 绕过问题 |
| 系统软件准备评估 | apt 模拟方案新增 36 包、升级 18 包；sudo 非交互认证不可用 | 未安装或升级任何系统软件 |
| 声明许可清单 | 指定 Linux+JS+WebView 路径 normal/build 依赖共 329 项 | [清单](evidence/linux-native-dependencies.tsv) 为 crate 声明元数据，不代表发布许可审核通过 |

本次 check 的实际失败已从“缺 imara-diff 缓存”推进到原生构建脚本：

```text
error: failed to run custom build command for `gstreamer-sys v0.25.4`
pkg-config --libs --cflags gstreamer-1.0 'gstreamer-1.0 >= 1.14'
The system library `gstreamer-1.0` required by crate `gstreamer-sys` was not found.
The file `gstreamer-1.0.pc` needs to be installed
```

GTK4/Graphene、GStreamer app/audio、WebKitGTK 6.0 仍在独立 pkg-config 检查中缺失；本次首个 build script 失败不代表其他库已可用。apt 候选 GTK 为 4.22.4，WebKitGTK 为 2.52.6，GStreamer 为 1.28.2；实际模拟同时涉及 systemd/udev 和 Kisak Mesa 升级，因此没有把它作为局部缓存操作自动执行。候选版本来自当前 apt 索引，未执行软件源刷新，也不承诺今后仍有同一版本。

依赖清单记录名称、版本、仓库与声明的 license 表达式，未改写原始表达式。系统动态库、库内捆绑的代码/资源、完整 CLI/应用默认功能及其他目标尚未完成清单或分发核查。没有编译/链接成功、窗口、截图、打包重开、中文输入或 PDF/EPUB 的新成绩。

## 平台与阅读证据矩阵

| 平台/场景 | 编译/链接 | 安装/启动 | 输入/阅读/恢复 |
|---|---|---|---|
| Linux via WSLg，本机 | 已进入 Rust 依赖编译；GStreamer 原生开发库缺失阻断 | 未验证 | 未验证 |
| 原生 Linux 发行版 | 未验证 | 未验证 | 未验证 |
| Windows 原生 | 未验证 | 未验证 | 未验证 |
| macOS | 未验证 | 未验证 | 未验证 |
| iOS 模拟器/真机 | 均未验证 | 均未验证 | 均未验证 |
| Android 模拟器/真机 | 均未验证 | 均未验证 | 均未验证 |

## 结论与下一动作

P01 是部分完成：固定源码、环境检查和 Linux Cargo 缓存可复现，已有指定路径的 Cargo 声明许可清单；系统依赖、完整发布包依赖/许可核查和五端工具链未齐。P02 未通过；P03/P05/P07/P08/P09 仍在各自前置之后。没有据此开始生产 NSH adapter、PDF/EPUB renderer 或官方阅读产品插件。

下一步先准备上述原生开发库；缓存已补齐，在库就绪后重跑离线 check，再按 [准确复现步骤](README.md#依赖补齐后执行-p02) 执行上游 Rust+TSX 示例的构建、窗口与打包重开；取得首个平台证据后，再编写最小受控阅读探针。若使用其他机器或新工具链，新增结果记录，不覆盖这份失败证据。

[preflight.py](preflight.py) 只检查前置条件，退出 0 也不等于应用/阅读可用。此次未安装系统软件、未改全局设置、未上传用户资料，也没有把 Web target、纯文本或禁用 WebView 的占位框作为替代验收。


## 2026-11-08 只读前置复查

继续 B2 异常恢复工作时按原 preflight 入口重查环境，见 [本次 JSON](evidence/linux-20261108-preflight.json)。固定 NSH 源码仍匹配、tracked 状态干净；GTK4、Graphene、GStreamer core/app/audio 和 WebKitGTK 6.0 共6个 pkg-config 项仍缺失，脚本退出1。未再调用 cargo-check，未安装系统包或关闭 WebView 绕过前置。

此次不增加编译、链接、窗口、输入、阅读或其他平台成绩，原 P01 部分/P02 阻断结论保持。Linux/WSL 的 B2 新进程恢复证据也不能替代 NSH 平台/Surface 验收。
