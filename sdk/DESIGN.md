# QUSOR 插件 ABI、SDK 发行与本地开发工具设计

- 状态：设计落地稿（阶段 1 Core Wasm ABI + 运行时预算 + 调试开关已落地代码与测试；阶段 2 Component Model / `qusor-sdk` / `qusor dev` CLI 为工程设计规格）
- 关联：[WIT 契约](wit/qusor-plugin.wit)、[运行端口契约](../crates/extension-runtime-api/src/lib.rs)、[Wasm 适配器](../adapters/runtime-wasm/src/lib.rs)、[包格式与 dev 开关](../crates/extension-package/src/package.rs)、[ADR 0053](../docs/architecture/adr/0053-runtime-budgets-abi-and-platforms.md)

## 1. 目标与问题

此前的手写线性内存 ABI（`run() -> u32`、`qusor.call(req_ptr, req_len, out_ptr, out_cap) -> i32`，配合 256 KiB 模块 / 128 KiB 线性内存 / 4 KiB 返回上限）仅能承载极简 `.wat` 探针，无法承载带标准库分配器、`serde_json` 与真实业务逻辑的 Rust 编译插件，也缺少：

1. 类型化、可生成多语言绑定的组件边界（WIT / Component Model）。
2. 流式分块响应与跨回合长驻会话能力（避免每次调用重新实例化模块或把多 MiB 结果塞入单一 4 KiB 静态缓冲）。
3. 独立于主仓库的 `qusor-sdk` 发行方案及 `qusor dev` 本地免签名快速迭代工作流。

## 2. 两阶段 ABI 路线（Core Wasm → WIT / Component Model）

### 2.1 `wasmi 2.0.0` 对 Component Model 的支持核实结论

核查当前工作区锁定的 `wasmi` 版本（[Cargo.lock](../Cargo.lock#L867-L877) 中为 `wasmi = 2.0.0`，依赖 `wasmparser`、`wasmi_ir`、`wasmi_core`、`wasmi_collections`、`spin`）：

1. **源码核实结果**：检视本地 crate 源码（`wasmi-2.0.0`）未发现 Component Model / Canonical ABI 原生支持；`wasmi` 2.0.0 仅支持 **Core WebAssembly 模块**（魔数 `\0asm\x01\0\0\0`）。
2. **外部生态调研（未引入新依赖）**：社区存在基于 `wasmparser` 组件解析层的封装方案（例如 `wasm_component_layer` + `wasmi_runtime_layer`、`wcmp-wasm-core-wasmi`），在纯解释器之上实现 Canonical ABI 类型编解码；但本轮不新增运行时依赖（需经 `@coordinator` 与依赖策略评审）。
3. **结论**：短期继续使用无 `unsafe`（`unsafe_code = "forbid"`）的 `wasmi 2.0.0` Core Wasm 引擎，在其上先补齐 **MiB 级预算注入 + `qusor.emit_chunk` 流式输出 + 长驻 `WasmiSession` 实例**，同时将 [qusor-plugin.wit](wit/qusor-plugin.wit) 确立为规范接口，供下一步在编译期生成 Core ABI shim 或评估 `wasmtime`（桌面可选）/ Component polyfill（跨端）。

### 2.2 Core Wasm ABI 与 WIT 的映射关系

| WIT 概念（[qusor-plugin.wit](wit/qusor-plugin.wit)） | 当前 Core Wasm 端口实现 | 说明 |
| --- | --- | --- |
| `broker.call(request)` | `qusor.call(req_ptr, req_len, out_ptr, out_cap) -> i32` | 经 `wasmi::errors::HostError` 可恢复陷阱（`HostYield::BrokerCall`）跳出执行，Host Broker 校验授权后回写线性内存并恢复，不借用跨层静态状态，满足 `unsafe_code = "forbid"`。 |
| `stream.emit-chunk(chunk)` | `qusor.emit_chunk(ptr, len) -> i32` | 通过 `HostYield::StreamChunk` 将分块切片推入 `ResponseSink` / `BoundedStreamCollector`；受 `max_stream_chunk_bytes`、`max_stream_chunks` 与 `max_stream_total_bytes` 严格约束。 |
| `handler.handle` / `streaming-handler.handle-stream` | `run() -> i32` 或具名导出 `fn() -> i32`（配合 `qusor_alloc` / 约定请求写回） | 阶段 1 `WasmiSession::invoke` 支持调用任意无参返回指针的命名导出；下一步引入线性内存 `(ptr, len)` 入口与 WIT Canonical ABI 编解码器。 |
| `lifecycle.migrate` / `lifecycle.preflight` | `qusor_migrate(from, to, ptr, len) -> i32` / `qusor_preflight(schema, ptr, len) -> i32` | 对应 `qusor-migration-plugin` 世界，无 `broker`/`stream` 导入，无业务权限。 |

### 2.3 运行时与包预算（MiB 级默认 + Host 注入 + 失败关闭）

真实 Rust `wasm32-unknown-unknown` / `wasm32-wasip1` 插件在开启 `opt-level = "z"`、`lto = true`、`strip = true` 后，即使包含 `serde`/`serde_json` 与基础领域逻辑，编译产物通常也在 **200 KiB – 3 MiB** 区间，启动堆/栈与反序列化缓冲需要 **数 MiB 至十余 MiB** 线性内存，以及 **数千个 `funcref` 表项**（闭包/虚表/状态机）。因此将硬编码常量升级为 Host 按 `RuntimeBudgetProfile` 与 `PackageBudgets` 注入，同时保留安全硬上限（exceeding fails closed）：

- **运行时预算**（[crates/extension-runtime-api/src/lib.rs](../crates/extension-runtime-api/src/lib.rs)）：
  - `LegacyProbe`：保留 `256 KiB` 模块、`128 KiB` 内存、`4 KiB` 输出、`1M` fuel，确保既有实验/探针回归零破坏。
  - `MobileStandard`：`4 MiB` 模块、`16 MiB` 线性内存、`4096` 表项、`1 MiB` 响应、`256 KiB × 64`（上限 `4 MiB`）流式分块、`20M` fuel、`1000 ms` deadline、`64` 次 Host 调用。
  - `DesktopStandard`（`RuntimeBudgets::default()`）：`8 MiB` 模块、`32 MiB` 线性内存、`8192` 表项、`2 MiB` 响应、`256 KiB × 128`（上限 `8 MiB`）流式分块、`50M` fuel、`2000 ms` deadline、`128` 次 Host 调用。
  - **绝对安全硬上限（不可被 Host 配置突破）**：`16 MiB` 模块、`64 MiB` 内存、`16384` 表项、`2 MiB` 栈高、`512` 递归深度、`8 MiB` 单次响应、`16 MiB` 流式总字节、`1024` 分块、`200M` fuel、`10 s` deadline、`256` 次 Host 调用；任何越界配置在实例化前失败关闭（`runtime_configuration`）。
- **包格式预算**（[crates/extension-package/src/package.rs](../crates/extension-package/src/package.rs)）：
  - 保留 `build_signed` / `verify` 的 `legacy_v1`（1 MiB 单文件 / 4 MiB 编码包）兼容语义，不破坏旧包测试与已有 `.qpk`。
  - 新增 `build_signed_with_budgets` / `verify_with_budgets`，支持 `PackageBudgets::mobile_standard()`（`4 MiB` 单文件 / `8 MiB` 解码文件总计 / `18 MiB` 编码包，对应 hex 编码 2x 膨胀）与 `PackageBudgets::desktop_standard()`（`8 MiB` 单文件 / `16 MiB` 总计 / `36 MiB` 编码包），硬上限 `16 MiB` 单文件 / `32 MiB` 总计 / `68 MiB` 编码包。

> 说明：JSON 中文件字节当前以 hex 编码存储（膨胀 2x），后续若引入二进制 `.qpk` v2 容器（zstd + raw binary section），可将编码包体积再压缩约 3–5 倍；该格式升级需单列 ADR，不在此轮修改 v1 签名域。

## 3. `qusor-sdk` 发行方案（Rust 优先，可选 TypeScript）

遵循 [ADR 0003](../docs/architecture/adr/0003-plugin-first-delivery.md) 与 [目录边界](../docs/architecture/workspace-layout.md)，SDK 从公开契约（`extension-api`、`extension-package`、`core-api`、`wit/qusor-plugin.wit`）导出，作者在仓库外构建，不依赖主仓库内部 crate：

### 3.1 `qusor-sdk`（Rust 优先）分层

1. **`qusor-sdk`（作者主 crate，发布到独立 tarball 或私有/公开 crates registry）**：
   - `wit` 模块：内嵌 `wit/qusor-plugin.wit`，提供构建期宏/生成胶水，将强类型 trait（`PluginHandler`、`StreamingHandler`、`Lifecycle`）生成阶段 1 Core Wasm 导出（`qusor_alloc`、`run`）或阶段 2 Component 导出。
   - `host` 桩模块：基于 `extension-api` 公开 wire DTO 生成强类型 Host Capability 客户端（封装 `qusor.call` / `qusor.emit_chunk` 的编码、错误码映射与缓冲区增长）。
   - `ui` 模块：声明式 NSH Surface / Component 构建器（对齐原生 UI 约束 C-09 / C-18，禁止 HTML/DOM/WebView）。
2. **`qusor-sdk-cli`（`cargo-qusor` / `qusor` 开发工具）**：
   - `qusor new` / `qusor build`（自动选择 `wasm32-unknown-unknown` + `opt-level = "z"` + `wasm-opt` 校验导出和体积预算）
   - `qusor pack`（生成规范 JSON 并执行 Ed25519 签名或 dev 未签名包）
   - `qusor dev`（本地热重载/无签名调试循环）
3. **可选 TypeScript / JS 绑定（`@qusor/sdk`，次优先级）**：
   - 仅当非 Rust 插件作者生态明确需要时提供：通过 `jco` / `componentize-js` 将 TS 逻辑编译为满足同一 [qusor-plugin.wit](wit/qusor-plugin.wit) 契约的 Wasm 组件/模块，并**受同一 `RuntimeBudgets` 与无 WebView 声明式 UI 约束限制**；绝不引入 Node/Electron/浏览器 DOM 作为插件运行环境。

## 4. `qusor dev` 本地调试与开发期未签名加载开关

### 4.1 调试工作流设计

```text
插件仓库修改 src/lib.rs
  └─> qusor dev --watch
        ├─ 1. cargo build --target wasm32-unknown-unknown --release
        ├─ 2. 校验 wasm32 导出、预算 profile（DesktopStandard）与 manifest schema
        ├─ 3. 生成临时 dev-unsigned .qpk（或使用本地临时开发密钥签名）
        └─ 4. 通过宿主调试端口热卸载旧模块并加载新模块到独立 WasmiSession
```

### 4.2 开发期未签名加载开关（已落地包层校验实现）

为避免作者每次本地调试都管理正式私钥，同时防止生产发行版出现任何信任绕过，包验证层（[crates/extension-package/src/package.rs](../crates/extension-package/src/package.rs)）采用三重安全门禁：

1. **编译期物理隔离**：`build_dev_unsigned_with_budgets` 与 `verify_dev_unsigned_with_budgets` 全部标注 `#[cfg(debug_assertions)]`；在 `--release` 发行构建中**代码不存在、无法链接或通过运行时参数开启**（`dev_unsigned_supported()` 在发行构建恒返回 `false`）。
2. **显式运行期开关**：必须显式传入 `DevUnsignedOptions { allow_unsigned: true }`（`Default` 为 `false`）；未开启时即使在 debug 构建也失败关闭（`UntrustedKey`）。
3. **只跳过签名，不降级结构与预算检查**：
   - 包必须显式声明 `algorithm = "dev-unsigned"` 且无签名串（防止合法/伪造 Ed25519 包静默降级）。
   - 规范 JSON 排序、路径防穿越、SHA-256 文件哈希、Wasm 魔数检查、manifest schema 与 `PackageBudgets` 上限校验**全部照常执行**。
   - 返回的 `VerifiedPackage::is_dev_unsigned()` 恒为 `true`，供宿主在状态栏/审计记录中强制标记「未签名开发包」，且严禁写入正式生产信任库或长期签名授权记录。
