# 插件平台兼容与实现选择

日期：2026-11-08；增量：2026-11-09。状态：纯兼容描述符与策略解析器已实现，尚未接入签名多实现包/安装/恢复链路或完成平台验收。关联 C-14、F-EXT-07，以及 F-EXT-01/03/04/05/06、F-PLAT-01。决策见 [ADR 0031](adr/0031-platform-aware-plugins.md)。

2026-11-09：按 C-15 增加公开 `extension-api::platform` 描述符及 `extension-host::platform::PlatformResolver`，显式支持六种目标 OS、架构、渠道、版本、运行 profile/API 和必需能力；按优先级/ID 稳定选择，用户固定失效不回退。旧 Manifest/签名不变，解析结果不授予权限、不代表实际运行就绪。

## 目标与范围

用户明确要求插件系统区分 Android、iOS 与桌面，接受插件无法全部通用。2026-11-08 先交付契约与验收设计，2026-11-09 按 C-15 实现纯描述符和兼容解析；签名多实现包、安装/恢复路由仍待接通。平台范围现为六端，语音功能排期与渠道分发资格单独推进。

共享插件身份、功能语义和规范数据；实现、布局及可用能力可因设备不同。允许仅桌面、仅移动、单一系统插件，也允许一个插件版本含多个实现。不得要求所有作者为五端提供实现。每个承诺平台仍须完成既有第三方包门槛，不能以平台声明替代运行资格。

## 兼容维度

| 维度 | 设计字段或事实 | 判断来源 |
|---|---|---|
| 操作系统 | windows / macos / linux / android / ios / web，最低版本；可选显式最高版本 | Host 探测；模拟环境与原生设备分开留证 |
| 架构 | x86_64 / aarch64 等；可移植代码可声明不约束架构 | Host 与工件格式核验 |
| 运行环境 | profile、profile 版本、Host API 范围 | Host 支持的执行器与协议版本 |
| 分发渠道 | desktop_direct / mac_app_store / android_play / ios_app_store / web；新渠道另行登记 | 可信应用构建/分发配置，插件不能自报宿主渠道 |
| 能力 | 带版本的 required / optional capabilities、宿主服务及外部依赖 | Host registry、服务探测与逐次授权 |
| 资源 | 内存、模型存储等明确的最低需求 | 有界探测；声明不构成无限资源授权 |

桌面/移动是展示分组，不能替代具体系统判断。窗口大小、触控、键鼠和布局偏好单独协商，不能从 Android 推导“只触控”，也不能从 Linux 推导固定桌面环境。未声明的系统/渠道视为不匹配；未知必需能力或 profile 视为不支持。

## 六端执行与基础设施现状矩阵

截至 2026-11-14，六端在插件执行引擎、存储适配器、进程监督与原生宿主服务上的代码级现状与缺口如下（编译通过不代表真机通过，详见 [跨平台结果](../../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) 与 [ADR 0053](adr/0053-runtime-budgets-abi-and-platforms.md)）：

| 平台 | 插件执行形态（Wasm / 进程） | 存储与包 Store 现状（代码证据） | 进程监督现状（代码证据） | 原生宿主服务现状与缺口 |
|---|---|---|---|---|
| **macOS** (`aarch64` / `x86_64`) | `runtime-wasm`（[wasmi 2.0.0](../../adapters/runtime-wasm/Cargo.toml#L13) 纯 Rust `portable-dispatch`，无 JIT）+ `DesktopStandard` 预算；桌面外置 helper 可选 `runtime-process` | **已实现（Unix）**：[storage-sqlite](../../adapters/storage-sqlite/Cargo.toml#L16-L17)（`rusqlite`）、[blob-store](../../adapters/blob-store/src/unix.rs#L165-L168)（`fs::hard_link`）、[extension-store](../../adapters/extension-store/src/lib.rs#L79-L87)（`0700` 目录、`writer.lock`）；arm64 本机已实测 | **已实现（macOS）**：[runtime-process](../../adapters/runtime-process/src/lib.rs#L14-L17) 使用 `libproc` + 启动身份核对；仅覆盖同一可执行 `--managed-worker` | `macos-aarch64` 已接通 PDFium `chromium/8125`（[pdf_reader.rs](../../apps/qusor/native/src/pdf_reader.rs#L115-L129)）与系统读音 `/usr/bin/say`（[vocab_native.rs](../../apps/qusor/native/src/vocab_native.rs#L512-L554)）。缺口：`x86_64` 仅摘要未实窗、PDFium 尚未进程隔离、中文 IME / 公证与商店签名未完成 |
| **Linux** (`x86_64` / `aarch64`) | `runtime-wasm`（`wasmi 2.0.0`）+ `DesktopStandard` 预算；桌面外置 helper 可选 `runtime-process` | **已实现（Unix）**：`storage-sqlite`（`cfg(unix)`）、[blob-store](../../adapters/blob-store/src/unix.rs#L152-L163)（`renameat_with` `NOREPLACE`）、`extension-store`（`cfg(unix)`） | **已实现（Linux）**：[runtime-process](../../adapters/runtime-process/src/lib.rs#L9-L12)（`rustix` pidfd/prctl）；注意 `runtime-process --test controller` 与平台环境耦合度高 | [apps/qusor/native](../../apps/qusor/native/src/pdf_reader.rs#L141-L149) 未打包 Linux PDFium 运行库且无系统 TTS（[vocab_native.rs](../../apps/qusor/native/src/vocab_native.rs#L565-L571) 返回无语音）。缺口：PDFium 分发包、发行版打包更新与 `aarch64` 实窗复验 |
| **Windows** (`x86_64`) | `runtime-wasm` 共享库交叉编译通过；`runtime-process` 返回 `Unsupported` | **未实现（明确拒绝）**：[storage-sqlite](../../adapters/storage-sqlite/src/unsupported.rs#L5-L33) 在 `#[cfg(not(unix))]` 返回 `Unavailable`；[blob-store](../../adapters/blob-store/src/lib.rs#L2-L5) 仅编译 Unix 模块；[extension-store](../../adapters/extension-store/tests/platform_profile.rs#L26-L44) 在 Windows CI 明确断言 `StoreErrorCode::Unsupported` | **未实现（明确拒绝）**：[runtime-process](../../adapters/runtime-process/src/lib.rs#L19-L48) 与 [controller.rs](../../adapters/runtime-process/src/controller.rs#L188-L194) 返回 `Unsupported` | [apps/qusor/native/Cargo.toml](../../apps/qusor/native/Cargo.toml#L32-L34) 与 [src/lib.rs](../../apps/qusor/native/src/lib.rs#L14-L17) 将 `png`/`pdf_reader` 门控在 `cfg(unix)`。缺口：Windows ACL 私有目录存储、SQLite/CAS 适配、Job Object 进程监督（仅桌面 helper）、PDFium DLL 与 MSVC 构建 |
| **Android** (`aarch64`) | 进程内 `runtime-wasm`（`wasmi 2.0.0` 纯解释器，无 JIT/RWX 要求）+ `MobileStandard` 预算；**不使用独立子进程隔离插件** | **已实现（Unix/Android）**：`storage-sqlite`、`extension-store` 可编译运行；[blob-store](../../adapters/blob-store/src/unix.rs#L152-L163) 专门改用 `renameat_with(RenameFlags::NOREPLACE)` 绕过应用数据目录禁止硬链接限制 | **不可用**：[runtime-process](../../adapters/runtime-process/src/controller.rs#L189-L194) 显式返回 `Unsupported`，且 Android 应用沙箱禁止桌面式任意子进程模型 | API 36 arm64 模拟器已运行 APK、PDFium `libpdfium.so`（[pdf_reader.rs](../../apps/qusor/native/src/pdf_reader.rs#L131-L139)）与 Android JNI TTS（[vocab_native.rs](../../apps/qusor/native/src/vocab_native.rs#L556-L563)）。缺口：移动插件动态安装/更新授权界面、真机与平板性能/耗电、商店签名、公开插件 Wasm 实窗闭环 |
| **iOS** (`aarch64`) | 进程内 `runtime-wasm`（`wasmi 2.0.0` 纯解释器，满足 iOS 禁止第三方 JIT/W^X 内存约束）+ `MobileStandard` 预算 | **条件可编译（Unix 子集）**：`storage-sqlite`/`blob-store`/`extension-store` 在 Darwin Unix 族下可编译，但仅完成 arm64 Simulator 交叉检查，未在 UIKit 沙箱路径与文件保护属性下实测 | **不可用**：iOS 沙箱禁止派生通用子进程，[runtime-process](../../adapters/runtime-process/src/controller.rs#L189-L194) 返回 `Unsupported` | PDFium 与系统 TTS（`AVSpeechSynthesizer`）均未登记 iOS 实现；缺口：UIKit 启动链接、文档选择器、App Store 审核策略定型（见下文待决项）、真机测试 |
| **Web** (`wasm32`) | 进程内 / Web Worker 内运行 `runtime-wasm`（`wasmi 2.0.0` 无 `cfg(unix)` 限制，已过 `wasm32` 库交叉编译检查，运行于浏览器 Wasm 沙箱）+ `MobileStandard` 或更低预算 | **未实现（无 OS 文件系统）**：`storage-sqlite`、`blob-store`、`extension-store` 均因依赖 POSIX 文件系统与本地文件锁在 `wasm32` 不可用/拒绝；需面向浏览器的 `StoragePort`（OPFS / IndexedDB） | **不适用**：浏览器无 OS 进程控制，依赖 Web Worker 隔离线程 | 无 PDFium 原生动态库、无原生 `qusor/pdf-surface` 文件系统桥接；缺口：Web Storage/CAS 适配、Worker 消息通道、CSP 下可加载插件形态与浏览器持久化恢复 |

## `runtime-process` 定位与待决平台策略（Proposed）

> 以下条目属于 **Proposed** 架构建议与待用户拍板事项，不替代正式用户决策。

### 1. `runtime-process` 定位建议：仅服务桌面原生 Helper / MCP

建议明确将 [adapters/runtime-process](../../adapters/runtime-process/src/lib.rs) 的职责边界收敛为**桌面平台（macOS、Linux 及后续补齐 Job Object 适配的 Windows）原生辅助进程与外部 MCP / Hex 服务监督器**，而不是通用六端插件隔离运行器：

- **通用插件隔离**：统一由进程内 `runtime-wasm`（`wasmi 2.0.0` 解释器 + Host 注入的 `RuntimeBudgets` 内存/表/fuel/deadline/chunk 预算 + `CapabilityBroker`）承担，保证 Wasm 插件语义与能力边界在六端一致，且兼容 iOS / Android / Web 的进程/JIT 限制。
- **`runtime-process` 专职场景**：仅在桌面端用于崩溃/资源隔离原生 C/C++ 库（如后续独立的 PDFium worker 进程）、本地 MCP server / CLI 工具与受控桌面服务连接；Android / iOS / Web 显式不启用该路径，由 Host 原生平台能力（如系统 API 或进程内单线程服务）替代并在 [扩展平台解析器](../../crates/extension-host/src/platform.rs) 中拒绝声明需要桌面进程服务的插件实现。

### 2. 待用户拍板：iOS 与 Web 是否允许动态安装第三方代码插件

该决策影响 App Store 合规、Web 安全边界与包分发体系（参见 [ADR 0053](adr/0053-runtime-budgets-abi-and-platforms.md)）：

| 选项 | iOS 策略 | Web 策略 | 优势 | 代价与风险 |
|---|---|---|---|---|
| **选项 A（Proposed 推荐默认）：商店/浏览器合规分级** | **随包附带已审核第一方/精选插件 + 声明式配置扩展**（如主题、键位、声明式规则/词汇词书资源），禁用未随包任意第三方 `.qpk` 下载执行 | 仅允许加载 **受同源/CSP 审查的可信静态/内置 Wasm 插件或纯声明式配置**，不提供任意 URL 上传 Wasm 执行 | 符合 App Store 2.5.2（禁止下载未随包可解释动态代码/可执行逻辑）与浏览器 CSP 沙箱预期；降低审核退回与存储配额失效风险 | iOS 第三方插件生态无法像桌面/Android 那样随时侧载更新，只能随宿主发版或走纯声明式/数据扩展 |
| **选项 B：全平台开放动态第三方 `.qpk`（Wasm 解释器）** | 允许用户导入第三方 `.qpk` 并在 `wasmi` 纯解释器中运行 | 允许浏览器导入 `.qpk` 并在 Web Worker 内的 `wasmi` 解释器运行（存 OPFS / IndexedDB） | 六端插件安装体验高度一致，第三方插件可直接复用 | **iOS 面临 App Store Review Guidelines 2.5.2 拒审高风险**（虽是字节码解释器而非 JIT，仍常被判定为可下载可执行功能模块）；Web 需承担大量浏览器存储持久化、跨源隔离与滥用防护成本 |

> 若同时涉及「第一方核心功能是否允许随宿主编译或继续全部独立签名包（ADR 0003）」，需与上表一并由用户确认后再修改生产安装与分发门控。

## 包与实现契约

以下是拟议新版本概念模型，不是现有 JSON Schema，也不向旧 Manifest 追加未知字段：

```text
PackageRelease
  package_id / publisher_id / package_version
  package_manifest_version / host_api_range
  contributions[]: 稳定贡献ID、功能契约、必需/可选性质
  implementations[]:
    implementation_id
    targets[]: os、arch、os_version_range、channels
    execution: profile、profile_api_range
    entrypoint / assets[]: 包内路径、摘要、大小
    provides[]: 贡献ID、服务契约版本
    required / optional capabilities
    external_requirements[]: 服务身份、协议范围、可用性探测
    priority / private_state_schema
```

目标条目之间是 OR，单条目内部约束是 AND。字段无约束必须显式表达，禁止空数组含义随解析器变化。条目数、范围、文件与解压体积均有预算；路径、入口、模型清单和所有工件摘要都受签名覆盖。可移植的 Wasm 仍须显式列出支持的系统与渠道，不能借“架构无关”推导平台支持。

一个签名发布包可以携带多个实现，Host 只启动匹配实现。首个切片先使用自包含包，不引入安装时任意下载可执行文件；未来拆分按平台交付的子包必须由签名发布清单绑定同一发布身份、版本与摘要，不能仅凭同名自动组合。模型下载属于显式服务操作，不能伪装成转写时的隐式代码安装。

同一贡献只能路由到一个已选择实现。不同实现可提供不同贡献集合；例如桌面提供听写和全局快捷键，手机仅提供应用内听写。缺少包级必需贡献则整包不能激活；可选贡献不可用不影响其他贡献。实现内部 required 能力缺失则该实现不可选，optional 能力缺失只关闭明确声明的附加功能；授予能力与实际能力可用分别表达。

第一方和第三方使用同一契约。原生平台服务归 Host，接受版本化调用；可安装插件不得声明自己为 `host_native` 或装载任意 FFI 来补能力。外部 Hex/Agent 接入需要受控连接能力，不给插件任意本机网络或执行权限。

## 解析与授权

1. 先验证容器、签名、版本、所有路径/摘要与预算，再静态计算目标匹配；解析不执行插件或外部工具。
2. 检查 profile、API、分发策略、必需贡献及能力支持，形成候选集合；静态匹配不代表外部服务就绪或权限已授予。
3. 在可信 broker 中做有界、无安装/登录/启动副作用的外部依赖探测。外部程序身份、协议、设备绑定须验证，不能仅相信 PATH 或一个同名进程。
4. 保留用户固定选择；若固定实现失效，显示原因并等待选择，不暗中换成其他提供者。首次无固定选择时按作者签名的 priority 排序、ID 做稳定决胜，展示建议实现及其能力/数据范围。
5. 经用户有效授权后，事务提交精确包摘要、实现ID、profile、契约版本、权限集合与设备绑定，再创建新运行世代。
6. 每次调用重新检查当前授权、实现租约和资源范围；外部服务消失或权限撤回时停止相关调用并报告，不在一次操作中重路由。

授权不能补足系统不支持的能力。安装检查、激活检查与调用检查各自保留，不能因为安装时可用就永远放行。新增实现不能通过较低 priority 隐藏额外能力；变更实现需要重新评估权限和数据范围。

## 安装、升级与恢复体验

插件详情按当前设备显示“可用”“部分功能可用”“不支持当前设备”“需要设置”，并列出具体缺项。已知不匹配的包在正常安装入口拒绝安装，不创建可运行安装实例；用户另行保留的下载文件不等于已安装。已安装插件因系统/策略变化而失去兼容时保留禁用记录及私有数据。

持久兼容状态与运行状态分开：`NoCompatibleImplementation`、`UnsupportedRuntime`、`ChannelRestricted`、`MissingCapability`、`DependencyUnavailable`、`PermissionRequired`、`Ready`；激活失败另报 `ActivationFailed`。状态可携带多个有序原因，不用通用“插件损坏”掩盖不支持。

升级先暂存与验证新包、解析目标并验证迁移，再切换代码/状态配对；没有兼容实现时保留当前可用版本并提示更新不可用，不强制升级。旧版本因撤权或安全策略不能继续运行时保持禁用，不能以回退规避策略。升级失败按既有配对回滚机制恢复，不把权限批准和实际进程退出当成同一事务。

系统升级、渠道变更、运行器变化及 Host 重启后重新解析；旧运行与租约不得复活。正在录音、调用 Agent 或执行写入时不热切实现，先取消/核对释放，再允许新运行；退出不确定则保持隔离。旧结果绑定运行世代和目标版本，不能写入新草稿或新会话。

知识与来源由 Core 保留，不依赖任何平台实现。可同步“希望使用某插件”的偏好，但安装代码、实现选择、绝对路径、外部登录和设备权限不自动同步激活。插件私有设置区分可移植 schema 与设备专属 schema；切换实现必须有声明的迁移，不能互读未兼容私有状态。凭据继续由安全存储管理。

## 语音例子与提供者替换

以下仅用于检验通用设计，不承诺具体插件/模型或其平台可用性：

| 提供者或实现 | 目标 | 依赖与不同能力 |
|---|---|---|
| Hex 连接 | 经验证的桌面系统 | 本机 Hex 服务、兼容协议、受控连接；可复用其采集 |
| iOS 语音实现 | 经验证的 iOS 版本/渠道 | Host 原生录音/识别服务，不启动桌面子进程 |
| Android 语音实现 | 经验证的 Android 版本/渠道 | Host 原生录音/识别服务，后台能力另行协商 |

既可由同一包提供上述多个实现，也可由不同作者提供独立包；跨包替换通过 Host 公开服务角色发现和授权路由，不建立插件直接 import 插件链。服务契约协商语言、离线能力、最终/增量结果、取消与输入格式；本地提供者失效不能自动改用云识别。

语音请求绑定输入目标、草稿版本与运行世代，结果作为可撤销编辑候选；切换编辑目标不把迟到文字粘到新位置。保存正式知识继续走 Core 用例。通话中断、后台挂起与取消分别验证；系统键盘听写是独立输入途径，不伪装为某语音插件已可用。

Hex 文档核对：2026-11-08，检查 `dev` 分支 [SDK](https://github.com/anomalyco/hex/blob/dev/sdk/typescript/README.md)、[嵌入服务](https://github.com/anomalyco/hex/blob/dev/docs/specs/local-transcription-service.md)、[iOS 原型](https://github.com/anomalyco/hex/blob/dev/ios/README.md)。这是可变分支的文档证据，未固定可复现提交、未本机运行；接入实验必须固定版本。桌面连接、嵌入转写和 iOS 原型不是统一移动 SDK，不能据此承诺 Android 或商店分发通过。

## 兼容迁移与模块边界

新增包格式/manifest 版本及独立平台协商协议；现有 Host API0.1/0.2/0.3 与旧签名包验证保持原义。旧包只允许进入此前已验证且显式配置的执行路径，记为 legacy 未声明目标；不默认五端通用，也不修改旧签名内容补字段。旧 Host 拒绝未知必需新格式，不静默忽略平台约束。

纯目标匹配/候选选择/原因模型放协议和策略层；OS/架构/设备/服务探测放 adapters；持久选择沿用安装内核；apps 只装配，功能实现留在独立 plugins。具体 crate 与字段发行版本在真实切片实施时决定，本轮不创建空模块或增加依赖。Core 不依赖平台、Hex、NSH 或插件执行器。

## 验收场景与实施顺序

以下是 F-EXT-07 的待执行验收，不是已通过结果：

| ID | 场景与可观察结果 |
|---|---|
| PLUG-PLAT-01 | 同包桌面/Android/iOS 实现，在不同 Host 环境仅选对应目标；单平台包在其他设备明确不支持 |
| PLUG-PLAT-02 | 错误架构、系统版本、渠道、profile/API及未知必需字段分别被拒绝；无插件代码执行 |
| PLUG-PLAT-03 | 可选贡献不支持时其入口不激活，其余功能可用；必需贡献缺失则整包不激活 |
| PLUG-PLAT-04 | 能力支持但未授权显示需要授权；授权后才可运行，撤权后下一调用拒绝 |
| PLUG-PLAT-05 | 多候选选择稳定；用户固定失效不静默切换，尤其不从本地改云端 |
| PLUG-PLAT-06 | 外部服务未安装/协议不兼容/断线给出原因，不自动安装、登录或启动进程 |
| PLUG-PLAT-07 | 升级无兼容实现或迁移失败保留有效旧配对；禁止运行的旧版不能通过回滚复活 |
| PLUG-PLAT-08 | 重启/系统变更重验；旧世代结果拒绝，取消与退出不确定不允许替代执行 |
| PLUG-PLAT-09 | 同步到另一平台不自动激活代码/权限；已保存内容仍可读、引用、导出 |
| PLUG-PLAT-10 | 新旧协议/包兼容回归，未知新格式被旧入口明确拒绝，签名篡改目标/入口被拒绝 |
| PLUG-PLAT-11 | 普通第三方身份、仓库外SDK构建与安装，按真实平台/渠道执行并记录，不以模拟匹配代表真机通过 |
| PLUG-PLAT-12 | 语音示例停止/取消/中断、输入目标改变及卸载均不误写；未接通时不显示成功 |

先完成纯解析/协商及兼容回归，再做签名多实现包与安装/恢复切片，最后在实际平台验证普通插件与服务调用。合成环境可证明选择算法，不能证明移动执行、原生录音、商店分发或正式 UI。语音接入须单独授权排期；本轮不改变既有阅读闭环优先级。
