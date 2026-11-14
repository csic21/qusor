# ADR 0049: Host API 累积版本兼容与正交能力演进方案

- 日期：2026-11-14
- 状态：Accepted（第一部分已随基座合入；第二部分为下一阶段演进规划）
- 关联：[2026-11-14 架构评审报告](../reviews/2026-11-14-architecture-review.md)、[插件扩展架构](../extensions.md)、[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0004](0004-headless-extension-policy.md)、[ADR 0038](0038-capability-composition.md)、[ADR 0046](0046-library-host-api-0-6.md)、[ADR 0052](0052-plugin-ui-protocol.md)、[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)、[extension-api](../../../crates/extension-api/src/lib.rs)、[extension-host](../../../crates/extension-host/src/lib.rs)、[capability-composition](../../../crates/capability-composition/src/lib.rs)

## 背景

QUSOR 插件协议在 `0.1` 至 `0.7` 演进过程中逐步引入了表面渲染（`0.2`）、草稿组件（`0.3`）、笔记读取（`0.4`）、词汇学习（`0.5`）与文库元数据/选区（`0.6`–`0.7`）等能力。但在评审（P0-2）时，既有实现存在两层结构性限制：

1. **按业务域锁死单一版本**：
   - `capability_available_in` 将 `ReaderSurface` 锁死在 `0.2`、`NoteDraftEdit` 锁死在 `0.3`、`NotesRead` 锁死在 `0.4`、`VocabRead`/`VocabReview` 锁死在 `0.5`、`LibraryRead` 锁死在 `0.6|0.7`，且 `0.3`–`0.5` 丢失了 `0.1` 的基础能力。
   - `Manifest::validate` 对 `min_minor == 3 / 5 / 6|7` 强制要求声明对应单一域能力；`Manifest::supports_api` 对 `ContributionKind` 也做精确 `==` 匹配。
   - `MAX_CAPABILITIES` 为 `6`，小于 `Capability` 当前的 `9` 个枚举项；各子域请求与 Host 授权入口（`notes`、`vocab`、`surface`、`component`、`draft`、`continuation`、`projection` 等）硬编码 `api_version == HOST_API_V0_X`。
   - 结果是单个插件无法同时声明跨域能力（如 `notes.read + vocab.read + library.read` 或 `reader.surface + notes.read`），也无法在最高共同协议版本上开启统一会话。
2. **业务协议与宿主耦合过紧**：
   - 每增加一个业务域就需要修改 `extension-api::Capability`、`ContributionKind` 并向 `extension-host` 添写新的业务子模块，使宿主核心与产品业务演进强绑定，且未与 [ADR 0038](0038-capability-composition.md) 已建立的 `provides`/`requires` 服务组合规划打通。

## 选项

1. **仅放开少数硬编码组合**
   - 例如只特批 `notes.read + library.read`。
   - 缺点：仍按域特判，每新增域能力都需修补枚举与校验分支，无法从根本上支持任意跨域扩展。
2. **立即破坏性重写为全局 `1.0` 新架构**
   - 直接删除 `Capability` 枚举、废弃现有 `manifest_version = 1` 及 `0.1`–`0.7` 子域 wire。
   - 缺点：破坏既有插件清单与资产包兼容，且在三项产品/平台待决事项未拍板前过早锁定上层装配。
3. **两步演进方案（采用）**
   - **第一部分（在 `0.1`–`0.7` 上直接落地）**：将 Host API 改为“能力与贡献点自引入 minor 起向后累积兼容”，提供统一协商会话入口，保持现有清单/资产包兼容，立即解除单包跨域能力锁死。
   - **第二部分（下一阶段规划，暂不实现）**：冻结新增业务专属枚举与 Host 子模块，设计“命名空间化正交能力 + 每能力独立 semver 协商 + 业务协议由提供者声明为扩展点、Host 只路由与授权”的下一代协议，并与 `capability-composition` 的 `provides`/`requires` 合并。

## 决定

### 第一部分：Host API `0.1`–`0.7` 累积兼容规则（Accepted，已合入基座）

1. **能力累积可用**：`capability_available_in(capability, version)` 要求 `is_supported_api(version)` 且 `version >= intro_version(capability)`：
   - `0.1+`：`SelectionRead`、`DocumentMetadataRead`、`CapturePropose`
   - `0.2+`：`ReaderSurface`（同时 `SelectionRead` 对 `Resource::Document` 的支持自 `0.2+` 累积生效）
   - `0.3+`：`NoteDraftEdit`
   - `0.4+`：`NotesRead`
   - `0.5+`：`VocabRead`、`VocabReview`
   - `0.6+`：`LibraryRead`
2. **清单校验与跨域声明**：
   - 移除 `Manifest::validate` 中 `min_minor == 3 / 5 / 6|7` 强制绑定某域能力的限制，仅保留每项已声明能力/贡献点所需的最小 `min_minor` 下界检查。
   - 将 `MAX_CAPABILITIES` 提升至 `9`，允许单个清单同时声明跨全部已实现域的能力集；未知能力名、未知贡献点类型与未知版本号继续 fail-closed。
3. **贡献点累积与会话协商**：
   - `Manifest::supports_api(version)` 对所有 `ContributionKind` 改为 `version >= intro_version(kind)`（`Command`、`SelectionAction` 为 `0.1+`，其余工作区/组件贡献与对应域一致）。
   - `extension-host` 新增 `Host::open_negotiated_session(id, supported)`，按 `Manifest::negotiate_api(supported)` 计算出的最高共同支持版本打开会话；保留 `open_session`（`0.1`）和 `open_session_for_api` 供显式版本调用方使用。
   - 各子域 wire 校验与 Host 策略适配（`notes`、`vocab`、`library`、`surface`、`component`、`draft`、`continuation`、`projection`、`invocation`、`component_invocation`、`composition`）改为检查 `is_supported_api(api) && api >= intro_version`（`LibraryOperation::SavedSelections` 要求 `>= 0.7`），并使响应/策略帧与会话协商版本保持一致而非硬编码旧 minor。
4. **旧包兼容**：既有锁定单一 minor 区间（如 `0.2..=0.2`、`0.4..=0.4`、`0.5..=0.5`、`0.7..=0.7`）的清单仍受 `host_api` 上下界严格约束，既不会静默越界升级，也可继续按原版本打开会话与授权。

### 第二部分：正交能力、独立 Semver 协商与扩展点路由演进（下一阶段演进方案，仅设计不实现）

1. **冻结新增业务专属枚举与 Host 业务模块**
   - 自本 ADR 起，冻结在 `extension_api::Capability` 与 `extension_api::ContributionKind` 中新增具体业务域变体（不再新增如画布、AI 助手、特定来源同步等域专属能力枚举），并冻结在 `extension-api` / `extension-host` 中新增具体业务协议子模块。
   - 后续宿主层只允许在独立 ADR 评审通过后扩充通用正交原语；所有新业务能力均按下方扩展点方案设计，待第二阶段实施时接入。
2. **命名空间化正交能力（Orthogonal Namespaced Capabilities）**
   - 能力标识符由封闭枚举演进为受语法与长度约束的命名空间字符串（与 `capability-composition::ServiceId` 对齐的段式命名）：
     - **宿主正交原语**：如 `host.surface.render`、`host.storage.kv`、`host.network.fetch`、`host.secrets.broker`、`host.jobs.schedule`、`core.knowledge.read`、`core.capture.propose`；
     - **扩展点/服务消费能力**：如 `ext.<provider_ns>.<endpoint>`，表达对某个已注册扩展点或服务契约的调用权限。
   - 未知或未被宿主/已装提供者识别的 `required` 能力必须在安装校验与会话协商阶段 fail-closed；`optional` 能力可记录降级原因而不阻断其他正交能力。
3. **每能力独立 Semver 协商（Per-Capability Semver Negotiation）**
   - 清单不再用单一 `host_api: ApiRange` 绑定所有域协议的版本号，而是拆分为：
     - `negotiation_protocol`：轻量包封与握手元数据版本；
     - `capabilities` / `requires`：每项能力或扩展点独立携带 `VersionRequirement { major, min_minor, max_minor }`（兼容 `capability-composition::VersionRequirement` 语义，同一主版本内向后兼容）。
   - 宿主与包在打开会话时逐项求版本交集并冻结为 `SessionCapabilityTable`（记录每个已授权能力的协商版本号及降级状态）。单一项原语升级（例如 `host.surface.render` 升至 `2.0`）不会破坏仍停留在 `1.x` 的 `core.capture.propose` 或业务扩展点。
4. **业务协议由提供者声明为扩展点，Host 只做路由与授权（与 `capability-composition` 合流）**
   - 合并路径：将 `crates/capability-composition`（[ADR 0038](0038-capability-composition.md)）现有的 `ServiceId`、`ServiceVersion`、`VersionRequirement` 与 `Entry { provides, requires }` 从独立规划器提升为扩展宿主统一契约：
     - **提供者声明（`provides`）**：由功能插件或受信任 Core Adapter 声明其对外提供的命名空间服务端点、契约版本（`ServiceVersion`）、有界帧规格/Schema 摘要及所需的宿主正交原语；
     - **消费者声明（`requires`）**：由插件或宿主工作区声明所需的服务端点与 Semver 范围、以及直接请求的宿主正交原语；
     - **无环装配规划**：沿用 `CompositionProfile` / `CompositionPlan` 与 `RuntimeScope` 在安装/启用前校验无循环依赖、无多提供者冲突且能力交集闭环。
   - **Host 职责收窄为路由器与权限仲裁器**：`extension-host` 不再解析具体业务载荷结构，只负责：
     1. 身份与信任边界（`InstallationId`、`package_digest`、`grant_epoch`、`vault_id` 绑定，禁止载荷自报身份）；
     2. 策略授权与委托交集（调用链 `caller -> provider` 的能力取交集，禁止提权）；
     3. 资源配额、帧大小上限、取消令牌与崩溃/卸载围栏；
     4. 将有界不透明消息帧（opaque bounded envelope）路由至对应提供者端点或受控 Core Adapter，由端点执行域内解码与事务校验。
5. **分期迁移步骤（下一阶段规划）**
   - **M1（当前基座能力）**：在 `0.1`–`0.7` 保持累积向后兼容；执行“冻结新增业务专属 `Capability`/`ContributionKind`”护栏（除 `ui.slot` 等经独立 ADR 批准的通用正交原语外，不新增具体业务域分支）。
   - **M2（`manifest_version = 2` 与组合清单映射）**：在 `extension-api` 定义 `CapabilityId` / `CapabilityRequirement` 与 `provides`/`requires` 清单扩展；为 `manifest_version = 1` 提供只读映射投影（将现有 `0.1`–`0.7` 能力/贡献自动投影为命名空间需求），不破坏旧包验证。
   - **M3（通用 `ExtensionRouter` 落地）**：在 `extension-host` 接入基于 `capability-composition::CompositionPlan` 的服务注册表与通用授权路由；新业务（AI、画布、外部来源等）完全通过提供者扩展点 + 宿主正交原语交付。
   - **M4（存量域逐步迁出与收口）**：将既有的 `notes`、`vocab`、`library`、`surface` 子域适配为标准扩展点端点，保留 `0.1`–`0.7` 兼容桥接直到所有第一方/存量包完成迁移后再评估废弃计划。
6. **与已拍板全局决策的衔接（2026-11-14 用户确认）**
   - 用户已正式拍板三项全局架构决策：
     1. **1A 第一方交付形态**（[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)）：允许第一方同构编译或内置打包，但必须走同一公开 SDK / Capability 契约；
     2. **2A iOS / Web 策略**（[ADR 0053](0053-runtime-budgets-abi-and-platforms.md)）：桌面与 Android 支持动态 `.qpk`，iOS / Web 仅支持随包审核内置与纯声明式扩展；
     3. **3A 插件 UI 路线**（[ADR 0052](0052-plugin-ui-protocol.md)）：采用声明式原生 UI 树协议 + `ControlledSurfaceRef` + `ui.slot`。
   - 正交能力命名空间、独立 Semver 协商以及基于 `provides`/`requires` 的路由授权边界同时服务于内置第一方提供者与动态第三方提供者，按阶段三排期推进 M2–M4。

## 后果

### 正面后果

- 单个插件可在 `0.1`–`0.7` 区间合法声明跨域能力并在最高共同版本上运行统一会话，消除了此前“按功能域锁死单一版本”的阻塞缺陷。
- 既有插件清单与 `.qpk` 资产包保持向后兼容，旧包仍严格受限于其声明的 `host_api` 版本边界。
- 通过合并 `extension-api` 与 `capability-composition` 的演进蓝图并冻结域专属枚举膨胀，宿主层职责收敛为稳定的授权、配额与有界路由基础设施。

### 约束与风险

- 在第二部分实施之前，`extension-api::Capability` 仍为单一共享 `host_api` 下的封闭集合；虽然多能力可组合，但 `Manifest::supports_api` 仍要求所声明的 `optional_capabilities` 在该协商会话版本下有定义（单能力独立降级需待第二部分每能力版本表落地）。
- 新业务若在第二部分实施前急需接入，必须在设计阶段按“通用正交原语 + 扩展点”先期评审，不得绕过冻结规则直接向 `Capability` 或 `extension-host` 追加业务专有模块。
- 第二部分将路由责任扩展至插件间/服务间调用时，必须同步满足权限交集、故障隔离与独立包验收门槛（`EXT-P04`、`EXT-P06`、`EXT-P11`–`EXT-P13`），不得跳过运行隔离验证。
