# ADR 0038：能力接缝与声明式 Profile 组合

- 日期：2026-11-12
- 状态：首片实施中；纯组合内核、桌面 profile、共享 Core provider、RuntimeScope 与统一 Host runtime 已完成，动态卸载/重装和用户 profile 未完成
- 关联：C-07、C-11、C-12、F-EXT-01/03/04/06
- 承接：[dsh 组合研究](../../research/15-dsh-composition-study.md)、[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0028](0028-extensible-native-workspace.md)、[ADR 0037](0037-production-app-assembly.md)

## 背景

生产应用虽然通过普通签名包读取想法与词汇，但 `apps/qusor/native` 仍分别手工初始化 NotesHost、VocabHost、包验签、授权、会话和 Core 路径。这个结构能证明单条链路，却无法回答：

- 当前桌面实际组合了哪些能力，按什么顺序启动；
- 两个包是否提供了同一服务，或消费了未提供的服务；
- 禁用或替换一个插件时，哪些 Host/UI 贡献应停止；
- 未来 reader/library/AI/Agent 加入时，是否又需要在应用里增加包名分支。

dsh 的 profile/bundle 与能力接缝提供了可借鉴的结构，但其 Cordis/Node 运行模型和进程内任意插件不适合 QUSOR。本决策只采用声明式组合、版本化服务边沿和确定性校验，不引入其运行时。

## 决策

1. **新增纯 `capability-composition` crate**，不依赖 Cargo、NSH、SQL、Host、运行时或平台 API。它只处理 Entry、Layer、Profile、ServiceRequirement、ServiceProvision 和激活顺序。
2. **能力版本采用 major 精确、minor 向后兼容**。消费者声明最低 minor；provider major 必须相同且 minor 不低于要求。
3. **Profile 由有序层组成**。后续层可按 entry id 替换完整行并保留原位置；启用项按服务依赖拓扑排序，同一层内重复 entry、重复层、重复 provider、缺失 provider、版本不兼容、必需项禁用和依赖环均明确失败。
4. **服务可达不等于授权**。组合只描述依赖与提供者；包签名、Host 授权、Vault/文件/网络资源和运行隔离继续由原有链路判断。
5. **应用 profile 是数据**。`apps/qusor/desktop-profile.json` 是当前桌面组合；应用只解析一次并用于 Host 启用门。配置不使用可执行 JS/YAML 表达式。
6. **诊断可见**。`CompositionHost.dump()` 返回 profile、激活顺序、层来源、provides/requires 和稳定文本摘要，便于本机和发布证据复核。
7. **本轮只登记真实能力**。Core provider 提供 `vault.read`/`vault.write` 0.1；NotesHost 提供 `notes.read` 0.4；VocabHost 提供 `vocab.read`/`vocab.review` 0.5；两个工作区消费这些能力。未实现的 reader/library 不写入声称可用的 profile。

## 数据与依赖方向

```text
apps/qusor/desktop-profile.json
  -> apps/qusor/native/profile
  -> capability-composition::CompositionProfile::resolve
  -> CompositionPlan
  -> Host activation gate

plugin package manifest / Host authority
  -X-> capability-composition
```

组合内核不认识包内容、Core、NSH 或插件执行，因此可以从其他 profile、测试或未来 CLI 复用。应用负责把 profile JSON 映射为纯类型；Host 仍负责实际安装、授权和启动。

## 后果

- 桌面 profile 无效时，相关 Host 不静默启动，而是返回具体组合错误。
- 同一服务的两个启用提供者会在启动前失败，避免“最后加载者胜出”的隐式行为。
- Notes/Vocab 的手工初始化已收进应用 `RuntimeScope` 和统一 Host runtime；停用/更新时的 UI贡献撤销仍未接入。
- 共享 Core 已作为 `vault.*` provider 接入，Notes/Vocab 不再各自打开数据库；迁移和安全边界见 [ADR 0039](0039-single-core-provider.md)。
- 用户覆盖层、bundle 安装、UI贡献发现和 profile 写入尚未实现；“当前嵌入 profile 可校验”不等于完整插件管理器。

## 验证

已执行：

```sh
cargo test -p capability-composition --offline
cargo test --manifest-path apps/qusor/native/Cargo.toml --lib --offline
cargo test --workspace --exclude runtime-process --locked --offline
```

覆盖缺失/重复/不兼容 provider、必需项禁用、依赖环、层覆盖、激活顺序、profile 解析、未知 Host 启用门、RuntimeScope 挂载/释放和迁移后的真实 Notes/Vocab Host 初始化。专项结果为纯内核 10 项、应用 9 项；排除既有 macOS 内核启动身份用例的根 Workspace 为 293 项通过。未排除时 `runtime-process` 的两个 macOS 身份测试因当前环境返回 `kernel boot identity unavailable` 失败，与本切片无关。

未验证：真实 NSH 窗口中的插件启用/停用、动态 bundle 安装、用户 profile 写入、UI贡献卸载、动态重载、reader/library、其他平台。
