# 六端推进与实际资格

2026-11-09；关联 C-15、F-PLAT-01、F-EXT-07、F-READ-01/03、F-NOTE-01、F-DATA-01；决策见 [ADR 0033](../../docs/architecture/adr/0033-cross-platform-runtime.md)。**全平台目标已确认，完整六端应用支持尚未完成。**

## 当前结果

| 目标 | 当前实际结果 | 尚缺的应用门槛 |
|---|---|---|
| macOS arm64 | AppKit 宿主实际编译、链接、启动；系统文件选择、PDF 第三页、拖选、普通签名阅读/编辑插件、中文与 emoji 粘贴保存、Core 重开和空库恢复已有本机记录 | 正式安装包、完整工作区、中文 IME、原生资源释放 ACK、发行资格 |
| Linux | [既有实窗与关闭记录](evidence/native-editor-shutdown-gui-default/README.md)保留；新增共享库的 aarch64 Linux 交叉检查通过 | 此次改动后的 Linux 原生应用全量复验；历史 x86_64 记录不证明 ARM 窗口 |
| Windows | 共享 Core/API/策略/包/解释器库的 x86_64 GNU 交叉检查通过；原生入口已有条件装配 | Windows 私有存储 ACL、进程 profile、Unix 文件操作适配、PDF 工件、MSVC 应用构建与实窗 |
| iOS | 共享库及 NSH 原生 Rust 桥接库的 arm64 Simulator 交叉检查通过 | UIKit 应用链接/启动、文件选择与原件冻结、合格的文档 provider、移动解释器 profile、设备及渠道资格 |
| Android | 共享库的 arm64 Android 交叉检查通过；本机有 SDK | NDK、原生宿主和文档 provider、应用私有存储/解释器 profile、设备运行与分发资格 |
| Web | 共享库的 wasm32 交叉检查通过；兼容契约显式区分 Web | 浏览器宿主调用、worker、持久存储/恢复、文档 provider 与实际阅读流程 |

交叉检查覆盖 `extension-api`、`extension-host`、`core-api`、`learning-application`、`runtime-wasm`、`extension-package` 的 **library 编译**。不覆盖平台适配器或可安装应用，也未在目标 OS 执行解释器。2026-11-11 核对 run 42813342829：`portable-libraries` 的五个 target 全部在远端通过；同一运行的 `Core foundation` ubuntu 与 windows 失败（ubuntu 为 Linux 专用测试的 Host API 数量断言过期，windows 为 `Store::open_for_host` 失败，见 [发行与更新](../../docs/product/release-and-update.md)）。

2026-11-11 后续：ubuntu 断言与 windows 失败都已处理。Windows 的真实 store 场景改为按 Unix 平台跳过，并新增 `tests/platform_profile.rs`——Unix 断言能打开并持有 writer lease，Windows 断言 `StoreErrorCode::Unsupported` 与 `qualified native filesystem profile` 诊断，让远端 Windows 任务固定“故意拒绝”而不是把跳过当成覆盖。run 42987643346 三个 OS 的 `Core foundation` 与五个 `portable-libraries` 全部通过。**这不改变上表 Windows 的“尚缺”项**：私有存储 ACL、进程 profile、Unix 文件操作适配仍未实现，Windows 仍不能运行 store/知识库或打包应用。

生产平台描述符和兼容解析器已实现：独立 `descriptor_version = 1`，显式 OS/架构/渠道/版本、执行 profile/API、必需能力和用户固定选择。`any` 架构不隐含支持其他 OS。旧签名 Manifest 不变；签名多实现包、安装/激活/恢复的路由接线仍待完成。详见 [平台契约](../../docs/architecture/plugin-platforms.md)。

## 本机 macOS 启动

当前已准备好工件时，在仓库根目录执行：

```sh
python3 experiments/nsh-reader/dynamic-pdf/run.py --data .local/reader-macos
```

启动的是现有原生阅读实验，沿用相同 TSX、Core 与普通签名包；不是完整资料库产品。调试 `.app` 包裹器只提供进程与窗口身份，启动仍由脚本传入 bundle、资源和本地数据目录，不能当作发行安装包。

新环境先安装固定 Rust 工具链和 Xcode，再在空目录准备固定 NSH 源码与对应 CPU 的 PDFium。Apple Silicon 示例：

```sh
git clone --filter=blob:none https://github.com/longbridge/nsh.git experiments/nsh-reader/target/nsh
git -C experiments/nsh-reader/target/nsh checkout 913fd76e1f8434ec6cdf248f4476c1ebe08af19f
curl -fL 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-mac-arm64.tgz' -o /tmp/qusor-pdfium-mac-arm64-8125.tgz
python3 experiments/pdf-provider/prepare_runtime.py /tmp/qusor-pdfium-mac-arm64-8125.tgz --target macos-aarch64 --destination experiments/pdf-provider/target/pdfium-macos-aarch64
python3 experiments/nsh-reader/prepare_macos.py
```

Intel Mac 另有固定归档/库摘要，但尚无 Intel 构建或运行证据。准备程序核对 NSH 提交、lock 摘要、实际依赖图和 registry 漂移，顺序单任务构建，生成 bundle 后才准备合成测试资源；不把 PDF 塞进应用包。Mac PDFium 摘要来自本轮固定来源 HTTPS 下载，未取得独立供应商签名证明。

## 验证与记录

- [macOS 实窗、保存、恢复与工件记录](evidence/macos-native-reader-20261109/README.md)。中文通过原生输入框粘贴验证，不冒充输入法组合事件验证。Core 重开/空库恢复由真实 CLI 核对；重开应用仍从初始页进入，没有自动恢复阅读位置或已保存想法列表。
- 根工作区 218 项 Rust 测试、Clippy 和 42 项边界守卫测试通过；原生桥接 library 的 30 项测试通过。另有 5 类真实进程控制场景，新增父进程直接退出、子进程不再读取 stdin 时的父死守卫验证，本机约 30 ms 观察到管道 EOF。该数值是本次测量，不是所有设备的延迟保证。
- 保留的四个原生桌面实验入口均已在 Mac 编译链接，普通 Rust 示例也注册对应的 macOS native components；仅动态阅读入口进行了本轮实窗操作。冷启动授权冲突回归通过，未扩大为 Linux 历史全部 GUI 兼容回归。
- `platforms.py` 是只读工具/SDK 探测和命令计划，**不会构建或启动应用**。`available` 仅指它检查的前提存在，build/start 保持 `not_run` 或 `unavailable`，不能覆盖上表门槛。运行：`python3 experiments/nsh-reader/platforms.py --output /tmp/qusor-platform-preflight.json`。
- Mac 的真实进程控制使用精确启动身份、父进程观察及拥有的 `Child` 回收。恢复后的 PID 不构成发信号的权限；未知清理继续隔离。移动/Web 不能借这个桌面分支声称已经有合格运行 profile。

上游核对：2026-11-09，固定运行基线为 [913fd76](https://github.com/longbridge/nsh/commit/913fd76e1f8434ec6cdf248f4476c1ebe08af19f)，当天另检查[发布记录](https://github.com/longbridge/nsh/releases)、主分支 `8830798b1f17421e02f458eb9e2abadb66d02875`、nightly `36b91689c1e7c38379179a56381de7ddab16c6a5` 与 macOS 文档。未自动升级固定基线。

上游核对：2026-11-10，[stable v0.4.0](https://github.com/longbridge/nsh/releases/tag/v0.4.0)（2026-11-04）与 nightly `c5214aaf4bbdf33bc1d59e9595e009b6aa54d702`（2026-11-10 构建）均已检查；固定运行基线继续为 913fd76，不自动升级。与当前缺口相关的近期提交包括 macOS 输入法组合期间保留按键（`826654b`）、输入框聚焦时复制（`933b2d0`）、原生模块在主线程执行（`7a3f780`）、UI 测试按 id 查找控件（`fb341c8`）、列表增量追加不重建偏移（`64ab8a1`）、macOS 打包按 `--arch` 选择处理器（`3bc4157`）、Windows 原生文本框（`ad481fa`）与移动端触摸选择手柄（`5e2b113`）。这些改动未进入固定基线；采用前须在隔离实验中重新验证输入、选择与资源回收。
