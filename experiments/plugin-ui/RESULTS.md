# `experiments/plugin-ui` 验证结果

## 1. 实验目标与关联

- **对应问题**：架构评审 P1-4「缺少插件 UI 协议」（[2026-11-14-architecture-review.md](../../docs/architecture/reviews/2026-11-14-architecture-review.md)）
- **关联文档**：[插件 UI 协议设计](../../docs/architecture/plugin-ui-protocol.md)、[ADR 0052](../../docs/architecture/adr/0052-plugin-ui-protocol.md)、[ADR 0023](../../docs/architecture/adr/0023-public-reader-surface.md)、[ADR 0028](../../docs/architecture/adr/0028-extensible-native-workspace.md)、[ADR 0042](../../docs/architecture/adr/0042-native-only-plugin-ui.md)
- **目标**：在不改动生产代码（`crates/`、`adapters/`、`apps/`）的前提下，对候选 A（声明式原生 UI 树协议 + 受控宿主槽位）构建最小可运行的独立 Rust 验证包，并结合 NSH 上游源码核对候选 B（NSH TSX/JS 受限运行时）的边界。

## 2. 验证环境与上游基线

- **核对与验证日期**：`2026-11-14`
- **工作区**：独立 Workspace `experiments/plugin-ui/Cargo.toml`（已在 [config/architecture.toml](../../config/architecture.toml) 登记；`unsafe_code = "forbid"`）
- **直接依赖**：`capability-composition`（path）、`extension-api`（path）、`serde =1.0.228`、`serde_json =1.0.149`（零新增第三方依赖）
- **NSH 固定基线**：`rev 913fd76e1f8434ec6cdf248f4476c1ebe08af19f`（[apps/qusor/native/Cargo.toml](../../apps/qusor/native/Cargo.toml)）
- **NSH 上游对照**：Release `v0.5.0`（`2026-11-11`）、`main` HEAD `3bf8affe15f1057ca251fb17af286e560e2b88aa`（`2026-11-14`）；本机当前未安装 `nsh` CLI

## 3. 实验包实现范围

- `src/schema.rs`：定义协议版本 `PLUGIN_UI_PROTOCOL = 1`、系统硬上限常量（`HARD_MAX_PAYLOAD_BYTES = 65536`、`HARD_MAX_NODES = 512`、`HARD_MAX_DEPTH = 16`、`HARD_MAX_CHILDREN = 128`、`HARD_MAX_TEXT_BYTES = 65536`、`HARD_MAX_PATCH_OPS = 256`、`HARD_MAX_EVENT_BINDINGS = 256`）、`UiBudget::clamp_to_ceiling`、`SemanticToken`（对齐 `apps/qusor/theme.json`）、`SpacingScale`、`TypographyRole`、`NodeStyle`、`AccessibilityProps`/`AccessibilityRole`、白名单组件 `UiComponent`（含 `ControlledSurfaceRef` 接缝，不含 `WebView`/HTML/脚本）、`UiTree`、`UiPatch`/`UiPatchOp`、`UiEventEnvelope`/`UiEventKind`、`SlotRegion`/`SlotDescriptor`。
- `src/validator.rs`：实现树与补丁全量校验，拦截超协议版本、超载荷/节点/深度/子数/文本/事件预算、未知组件或原始样式逃逸（`#[serde(deny_unknown_fields)]`）、未授权/未知图标名、交互节点缺失 `accessibility.label` 或 `accessibility.role`、非法节点 ID 或 `contribution_id`。
- `src/host.rs`：实现 `resolve_slot_composition`（通过 `capability-composition` 解析 `ui.slot@1.0` 及 `slot.<region>.<role>` 单提供者约束）、`MountedSlot` 状态机（初始树挂载、原子 `apply_patch` 失败全量回滚、generation 隔离的 `build_event`/`verify_inbound_event`、`replace_provider` 冻结与代际递增）、`resolve_theme_token`（明暗语义色值解析）。
- `tests/protocol_probe.rs`：7 个端到端无界面集成测试。

## 4. 实测命令与结果（分层记录）

### 4.1 源码与架构规范检查

| 检查项 | 命令 | 结果 |
| --- | --- | --- |
| 格式化检查 | `cargo fmt --manifest-path experiments/plugin-ui/Cargo.toml -- --check` | 通过（exit 0） |
| 静态检查 | `cargo clippy --manifest-path experiments/plugin-ui/Cargo.toml --all-targets --locked --offline -- -D warnings` | 通过（0 warnings） |
| 架构边界守卫 | `python3 scripts/check_architecture.py` | 通过（含 `experiments/plugin-ui` manifest、边界与依赖闭包检查） |
| 原生 UI 禁令检查 | `python3 scripts/check_native_ui.py` | 通过 |
| 文档索引检查 | `python3 scripts/check_docs.py` | 通过 |

### 4.2 构建与本机无界面运行

- **命令**：`cargo test --manifest-path experiments/plugin-ui/Cargo.toml --locked --offline`
- **结果**：`tests/protocol_probe.rs` **7 passed; 0 failed; 0 ignored**（本机无界面单元测试/集成测试通过；`src/lib.rs` 0 unit tests）。
- **覆盖用例**：
  1. `valid_tree_patch_and_event_roundtrip`：初始 `UiTree` 挂载、`UiPatch` 增量修改 props/insert/remove、语义 token 明暗解析、`UiEventEnvelope` 双向代际校验。
  2. `patch_failure_rolls_back_atomically`：同批次多条 `UiPatchOp` 中后一条引用不存在节点失败时，前一条写入完全回滚，`active_tree` 保持旧 `revision` 不变且 `fault_count` 递增。
  3. `provider_replacement_fences_stale_generation_events_and_patches`：ADR 0028 槽位替换后 `generation` 从 `1` 递增至 `2`，旧代 `UiPatch` 与旧代 `UiEventEnvelope` 均被 `StaleGeneration` 拒绝，新提供者可继续挂载并接收事件。
  4. `enforces_budgets_and_hard_ceiling_clamp`：验证超出 `UiBudget::clamp_to_ceiling()` 裁剪后上限的深层嵌套/超额节点树被拒绝。
  5. `rejects_unknown_component_and_raw_style_escape`：验证 `WebView` 节点及原始 CSS/hex 颜色字段在反序列化阶段即被 `deny_unknown_fields` / 枚举白名单拒绝。
  6. `rejects_missing_accessibility_label_and_invalid_icon`：验证交互组件缺失无障碍 `label`/`role` 或使用非白名单图标名时被阻断。
  7. `composition_bridge_and_controlled_surface_boundaries`：验证与 `capability-composition` 的 `ui.slot@1.0` + `slot.<region>.<role>` 组合解析、同 `(region, role)` 多提供者冲突检测、`extension-api::Capability::ReadLibrary` 与 `ControlledSurfaceRef` 接缝集成。

### 4.3 结论分层

- **源码实现**：完成候选 A 协议 Schema、校验器、Host 状态机与组合桥接原型。
- **构建通过**：`experiments/plugin-ui` 在 `--locked --offline` 下编译与 `clippy -D warnings` 通过。
- **本机无界面运行**：7 项协议/安全/组合集成测试通过。
- **实窗渲染 / 模拟器 / 真机**：**未验证**。

## 5. 未验证范围与限制

1. **候选 B 实机运行时探针未执行**：本机未安装 `nsh` CLI，且 `experiments/plugin-ui` 未链接 `nsh-core` / `nsh-engine` / `nsh-platform-*`；候选 B 对多 `AppView`、QuickJS `Limits`、内存与线程开销、`nsh/host` 跨上下文吞吐量的评估来自对上游 `913fd76` / `v0.5.0` / `main` 源码的核查，尚未形成实机运行证据。
2. **真实 NSH 渲染器与输入闭环未验证**：候选 A 输出尚未接入生产 `apps/qusor` 的 NSH 组件映射层，未在真窗口中验证布局性能、IME 组合输入、系统焦点切换、屏幕阅读器无障碍树映射与高频增量 diff 帧率。
3. **跨平台真机与 Web 未验证**：macOS 真窗、Windows、Linux、iOS、Android（模拟器和真机）及 Web（`nsh-platform-web`）均未实测。
4. **Wasm 宿主传输基准未测**：本次在进程内 Rust 结构体与 JSON 反序列化路径验证，未叠加 `wasmi`（`adapters/runtime-wasm`）宿主-客体序列化/调用开销基准。
