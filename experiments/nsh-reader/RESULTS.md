# NSH 阅读宿主实验结果

日期：2026-11-07。结论：**已取得固定源码与本机 P01 环境证据；P02 被依赖准备阻断，未构建或启动 NSH 应用。** 这不证明 NSH 功能失败，也不能据此将阅读 Surface 标为可用。

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

## 步骤与结果

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

## 平台与阅读证据矩阵

| 平台/场景 | 编译/链接 | 安装/启动 | 输入/阅读/恢复 |
|---|---|---|---|
| Linux via WSLg，本机 | Cargo 依赖解析阻断 | 未验证 | 未验证 |
| 原生 Linux 发行版 | 未验证 | 未验证 | 未验证 |
| Windows 原生 | 未验证 | 未验证 | 未验证 |
| macOS | 未验证 | 未验证 | 未验证 |
| iOS 模拟器/真机 | 均未验证 | 均未验证 | 均未验证 |
| Android 模拟器/真机 | 均未验证 | 均未验证 | 均未验证 |

## 结论与下一动作

P01 是部分完成：固定源码与环境检查可复现，系统依赖、完整依赖/许可清单和五端工具链未齐。P02 未通过；P03/P05/P07/P08/P09 仍在各自前置之后。没有据此开始生产 NSH adapter、PDF/EPUB renderer 或官方阅读产品插件。

下一步在具备上述原生库的环境补齐锁定 Cargo 缓存，按 [准确复现步骤](README.md#依赖补齐后执行-p02) 执行上游 Rust+TSX 示例的构建、窗口与打包重开；取得首个平台证据后，再编写最小受控阅读探针。若使用其他机器或新工具链，新增结果记录，不覆盖这份失败证据。

[preflight.py](preflight.py) 只检查前置条件，退出 0 也不等于应用/阅读可用。此次未安装系统软件、未改全局设置、未上传用户资料，也没有把 Web target、纯文本或禁用 WebView 的占位框作为替代验收。
