# ADR 0055：生命周期简化与扩展端口解耦

- 日期：2026-11-14
- 状态：Accepted
- 关联：[架构审查](../reviews/2026-11-14-architecture-review.md) / [插件生命周期](../plugin-lifecycle.md) / [统一目录架构](../workspace-layout.md) / [ADR 0003](0003-plugin-first-delivery.md) / [ADR 0025](0025-durable-installation-authority.md) / [ADR 0027](0027-supervised-raw-process-runtime.md) / [ADR 0043](0043-persistent-package-enable-intent.md) / [ADR 0048](0048-doc-governance.md)

## 背景

1. `adapters/extension-store` 原先直接依赖 `crates/extension-host` 以实现 `AuthoritySource`、`ComponentAuthoritySource` 与共享 `Grant`/`AuthoritySnapshot` 等端口类型，形成从适配器层反向指向策略核心的依赖倒置，且 `extension-store` 测试也因此引入了 `DurableHost`。
2. 原普通更新路径无条件清空全部持久授权并重置启用状态，未区分“同权修复/功能迭代”与“扩权变更”，导致用户每次日常升级都需要重新授权。
3. 进程模型在文档与代码边界上尚未将普通跨平台插件（`runtime-wasm`）与桌面原生 helper / 外部 MCP 进程监督（`runtime-process`）明确切割，容易在生命周期讨论中混淆基线运行时定位。

## 选项

1. **在 `crates/extension-api` 中直接塞入持久端口与升级策略**：
   - 优点：不新增 workspace crate。
   - 缺点：将安装快照、组件挂载与持久化端口细节混入对外稳定 wire contract，破坏 `extension-api` 的纯净契约边界。
2. **新增纯端口 crate `crates/extension-ports`（仅依赖 `extension-api`），并收敛升级授权与进程运行时边界**（采纳）：
   - `crates/extension-ports` 承载持久安装/授权/恢复端口数据结构、trait 与纯升级授权判定 `preserve_grants`。
   - `crates/extension-host` 与 `adapters/extension-store` 共同依赖 `crates/extension-ports`；`extension-host` 通过 `pub use` 保留既有公共符号路径，`adapters/extension-store` 从依赖及架构白名单中完全移除 `extension-host`。
   - 新增同权升级保留授权路径，仅在扩权、API 不兼容或撤销状态下清除授权；同步将 `adapters/runtime-process` 定位收敛为桌面原生 helper 与外部 MCP 进程监督。

## 决定

1. **消除 `extension-store -> extension-host` 依赖倒置**：
   - 新增 `crates/extension-ports`（零第三方新增依赖，仅依赖 `crates/extension-api`），定义 `Grant`、`DurableGrant`、`VersionedRequest`、`AuthoritySnapshot`、`AuthorityRead`、`RemovedInstallation`、`AuthoritySource`、`ComponentInstallation`、`ComponentAuthoritySource`、`GrantUpgradeDecision` 与 `preserve_grants`。
   - `crates/extension-host` 依赖 `extension-ports` 并完整重导出上述符号，保持外部与既有实验调用兼容；`adapters/extension-store` 仅依赖 `extension-ports`，并在 [architecture.toml](../../../config/architecture.toml) 中从 `extension-store` 的允许依赖移除 `extension-host`。
2. **简化升级授权策略（同权保留、扩权拦截/重授）**：
   - 保留 `GrantUpgradeDecision`（`Preserve` / `Reauthorize`）与纯函数 `preserve_grants(old_grants, old_manifest, new_manifest, enabled, supported_apis, granted_api)`：当已授予集合仍覆盖新清单全部 `required_capabilities`、支持兼容宿主 API 且原有 `enable_intent` 未被撤销时保留授权（可选能力收窄时仅裁剪不再声明的可选授权）；当新版本扩大 `required_capabilities`、破坏已授权 API 兼容或原授权已被撤销/缺失时返回重新授权决策。
   - `adapters/extension-store` 新增 `Store::upgrade_package` 与 `UpgradeReceipt { installation, grants_preserved }`：校验包身份/签名键/版本单调与修订冲突后，按上述策略原子更新安装摘要与版本并将持久授权重绑定至新世代；同权升级保留授权与 `enable_intent`，扩权升级清除授权材料并要求用户重新授权；既有 `install("update", ...)` 与 `upgrade_private` 显式清除路径保持兼容语义不变。
   - `DurableHost` 提供 `DurableHost::preserve_grants` 供纯策略与端口层统一复核升级结果。
3. **进程运行时定位收敛**：
   - `adapters/runtime-wasm` 作为普通跨平台插件的唯一进程内标准执行运行时。
   - `adapters/runtime-process` 仅用于桌面原生 helper 与外部 MCP 进程监督，不承担普通插件生命周期。

## 后果

1. 依赖方向严格收敛为 `adapters/extension-store -> crates/extension-ports -> crates/extension-api` 与 `crates/extension-host -> crates/extension-ports -> crates/extension-api`，架构门禁可直接阻止适配器反向依赖策略层。
2. 同权插件升级不再迫使用户重复确认授权且保留既有 `enable_intent`，扩权升级与撤销状态仍受硬拦截并强制重授，兼顾安全底线与生命周期体验。
3. 端到端 `Store` 与 `DurableHost` 的联合集成测试继续集中在装配与集成边界（`crates/extension-host` 与集成测试套件），`adapters/extension-store` 自身测试仅通过 `extension-ports` 与 `extension-api` 验证持久化与端口契约。
4. 新增 `crates/extension-ports` 会增加根 Workspace 成员与模块索引维护点，需要在目录架构与状态文档中同步登记。
