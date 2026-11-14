# ADR 0053 运行时预算配置化、插件 ABI 演进与六端执行策略

- 日期：2026-11-14
- 状态：Proposed
- 关联：P1-5、C-09、C-14、C-15、F-EXT-01/03/05/07、F-PLAT-01、[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0022](0022-authority-free-wasm-runtime.md)、[ADR 0031](0031-platform-aware-plugins.md)、[ADR 0033](0033-cross-platform-runtime.md)

## 背景

既有插件运行时与包验证格式在工程验证阶段使用了面向 `.wat` 探针的硬编码小常量（`MAX_WASM_BYTES = 256 KiB`、`MAX_RESPONSE_BYTES = 4 KiB`、`ExecutionLimits::default()` 为 `1M fuel / 250 ms`、`extension-package` 单文件上限 `1 MiB`、`128 KiB` 线性内存与 `16` 个表元素），且每次调用重新实例化模块、只能一次性写回单一 `4 KiB` 线性内存输出。对于真实编译后的 Rust `wasm32` 插件（带分配器、`serde_json`、函数表与业务数据结构，通常在数百 KiB 至数 MiB 量级）以及长文检索/阅读分块输出，该边界无法承载正式业务插件。

同时，仓库中尚无面向 Component Model 的 WIT 接口契约与本地免签名调试规范；`wasmi` 的 Component Model 支持能力、`qusor-sdk` 的发行形态、六端在存储/进程监督上的真实分化、`runtime-process` 的适用范围，以及第一方与 iOS/Web 动态插件策略需要架构定型。

## 选项

### 1. 运行时与包预算注入策略

- **选项 1A（采纳）**：保留既有 `MAX_*` 与 `ExecutionLimits::default()` / `verify` 作为 `LegacyProbe` / `legacy_v1` 兼容基线；新增 Host 按环境 profile 注入的有界配置 `RuntimeBudgets`（`LegacyProbe` / `MobileStandard` / `DesktopStandard`，默认 `DesktopStandard` 为 `8 MiB` 模块、`32 MiB` 线性内存、`8192` 表项、`2 MiB` 单次响应、`256 KiB × 128` 流式分块、`50M` fuel、`2000 ms` deadline）与 `PackageBudgets`（`legacy_v1` / `mobile_standard` / `desktop_standard`），并设置不可突破的硬上限（如 `16 MiB` 模块、`64 MiB` 内存、`200M` fuel），任何越界配置在实例化前失败关闭。
- **选项 1B**：直接把全局常量调大至数 MiB。代价：破坏现有探针与进程协议边界测试语义，且无法按桌面与移动端差异化限制内存和燃料。

### 2. ABI 与 `wasmi 2.0.0` Component Model 演进路径

- **选项 2A（采纳）**：经过对工作区锁定依赖 `wasmi 2.0.0` 源码与生态的核实，`wasmi 2.0.0` 自身仅执行 Core WebAssembly 模块（依赖 `wasmparser` 解析，无原生 Component Model / Canonical ABI 支持；社区适配层如 `wasm_component_layer` + `wasmi_runtime_layer` 需引入额外依赖）。在本轮不新增运行时依赖的前提下，先定义目标 Component Model 契约 [qusor-plugin.wit](../../../sdk/wit/qusor-plugin.wit)，并在 Core Wasm 引擎上落地两项核心执行能力：`qusor.emit_chunk` 流式分块sink（`ResponseSink` / `BoundedStreamCollector`）与跨回合保持线性内存状态的长驻实例会话（`ModuleSession` / `SessionExecutor` / `WasmiSession`，发生 trap 或预算违例即永久毒化），通过 `wasmi::errors::HostError` 可恢复陷阱（`HostYield`）实现且保持 `unsafe_code = "forbid"`。下一步由 `qusor-sdk` 提供 Rust-first 代码生成/打包与 `qusor dev` 调试流，并视桌面/跨端需要决定采用 WIT→Core ABI 代码生成或引入组件层。
- **选项 2B**：立即切换或引入 `wasmtime` / 组件层 crate。代价：违反本轮不擅自新增依赖约束，且 `wasmtime` 在 iOS 禁止 JIT 与 Web `wasm32` 目标上受限，跨六端仍须保留纯解释器路径。

### 3. `runtime-process` 定位建议

- **选项 3A（Proposed 推荐）**：将 `adapters/runtime-process` 明确限定为**桌面平台（macOS、Linux 及后续补齐的 Windows）原生 helper 与外部 MCP/本地服务进程监督器**；六端通用插件隔离统一由进程内 `runtime-wasm`（`wasmi 2.0.0` + `RuntimeBudgets` + `CapabilityBroker`）承担。
- **选项 3B**：尝试在六端普及独立 OS 子进程跑 Wasm 插件。代价：iOS 与 Web 架构上不可行，Android 应用沙箱受限且代码现状显式返回 `Unsupported`。

### 4. 待用户拍板的交付与平台策略选项（Proposed）

以下两项待用户拍板，不在本 ADR 中擅自终局：

- **待决项 A：第一方功能交付形态（是否放宽 [ADR 0003](0003-plugin-first-delivery.md)）**
  - *选项 A1（维持 ADR 0003）*：第一方功能继续全部以独立构建、独立签名的 `.qpk`（使用公开 `qusor-sdk` 与公开 Host API）交付，仅宿主与原生基础设施保留在主工作区。优点：强制验证第三方 SDK 完整闭环、避免特权路径；代价：早期装配与跨仓联调成本高。
  - *选项 A2（有条件放宽）*：允许精选第一方基础模块随应用同构编译或内置打包，但**必须在架构与权限上走同一公开 capability/Host API 契约**，严禁绕过 Core 事务直写 SQLite 或引入私有特权接口。优点：降低首发装配复杂度与包体开销；代价：需持续守卫边界不退化为紧耦合单体。
- **待决项 B：iOS 与 Web 是否允许动态安装第三方代码插件**
  - *选项 B1（Proposed 推荐默认：合规分级）*：桌面（macOS/Windows/Linux）与 Android 支持签名 `.qpk` 动态安装；iOS（App Store）与 Web 仅支持随包/同源静态内置的已审核插件及纯声明式扩展（主题、词书数据、声明式规则/布局），不开放任意 URL/文件导入动态执行代码。优点：避免 App Store 2.5.2 动态可下载可执行代码审核退回与浏览器跨源安全/持久配额风险；代价：iOS 与 Web 第三方扩展生态受限。
  - *选项 B2（全端动态第三方 `.qpk`）*：利用 `wasmi` 纯解释器无 JIT 特性，在 iOS 与 Web（Worker + OPFS/IndexedDB）也开放用户导入任意第三方 `.qpk`。优点：六端体验统一；代价：面临高概率 App Store 2.5.2 拒审风险及大量 Web 持久存储与防滥用工程成本。

## 决定

1. **预算配置化与兼容性**：在 [crates/extension-runtime-api/src/lib.rs](../../../crates/extension-runtime-api/src/lib.rs)、[adapters/runtime-wasm/src/lib.rs](../../../adapters/runtime-wasm/src/lib.rs) 与 [crates/extension-package/src/package.rs](../../../crates/extension-package/src/package.rs) 中以 Host 注入的 `RuntimeBudgets` 与 `PackageBudgets` 替代仅靠单一硬编码小常量驱动的路径；默认 profile 为 `DesktopStandard`（8 MiB 模块 / 32 MiB 内存 / 50M fuel），同时维持旧 `execute` / `verify` 接口为 `LegacyProbe` / `legacy_v1` 不变，确保既有实验与包兼容测试不退化。
2. **流式响应与长驻会话**：在 `extension-runtime-api` 定义 `ResponseSink`、`BoundedStreamCollector`、`ModuleSession` 与 `SessionExecutor`，并在 `runtime-wasm` 实现 `qusor.emit_chunk`（含单块、总块数、总字节数上限校验）与 `WasmiSession`（保留跨回合线性内存状态、每次调用重装 fuel/deadline/host-calls 预算、发生 trap 即永久毒化），保持 `#![forbid(unsafe_code)]`。
3. **目标 ABI 与 `qusor dev` 调试门禁**：以 [sdk/wit/qusor-plugin.wit](../../../sdk/wit/qusor-plugin.wit) 与 [sdk/DESIGN.md](../../../sdk/DESIGN.md) 确定 Component Model 契约及 Rust-first `qusor-sdk` / `qusor dev` 方案；在 `extension-package` 落地 `DevUnsignedOptions` 与 `verify_dev_unsigned_with_budgets`，通过 `#[cfg(debug_assertions)]` 与显式开关双门禁确保发行构建物理不可开启未签名加载，且即使在 debug 模式也不绕过规范 JSON、路径安全、文件哈希、Wasm 头与预算上限。
4. **六端矩阵与范围收敛（Proposed）**：在 [plugin-platforms.md](../plugin-platforms.md) 记录六端代码级现状与缺口，拟议将 `runtime-process` 限定为桌面原生 helper / MCP 监督；待决项 A 与 B 维持 `Proposed` 等待用户拍板后落盘后续实施任务。

## 后果

- **正面影响**：
  - 真实编译规模的 Rust Wasm 插件（MiB 级体积与线性内存、数千函数表项、流式大结果及跨回合状态）现已具备明确且经无特权测试证明的运行时与包验证基座。
  - `qusor dev` 本地免签名迭代具备编译期不可旁路的安全边界，为后续独立 `qusor-sdk` 工具链扫清障碍。
  - 六端真实代码差距（Windows/Web 缺少持久存储与进程监督、iOS 缺少应用链与 TTS/PDFium、Android 仅模拟器且缺公开插件导入 UI）得到诚实对齐，避免以交叉编译库代表可交付应用。
- **代价与约束**：
  - 未引入新依赖前，Core Wasm 到 WIT Canonical ABI 的自动胶水生成及 Component 二进制容器解析仍需下一阶段在 `qusor-sdk` 或运行时层补齐；长驻会话在多次调用间会持续占用单实例最高至 profile 上限的线性内存，需要宿主在产品装配层结合 LRU/闲置回收管理生命周期。
  - `.qpk` v1 的 hex 编码带来约 2 倍封装体积膨胀，桌面 8 MiB 模块对应包预算需 36 MiB 编码上限；若后续需要更大资源包，需独立设计二进制容器 v2。
  - 若用户最终拍板对待决项 A 或 B 选择不同方向，仅需依据选项更新安装策略、分发门控与平台适配排期，不推翻本 ADR 已落地的预算边界、流式/会话执行端口与开发期门禁设计。
