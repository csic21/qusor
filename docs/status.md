# QUSOR 实现与验证状态表

> 权威范围：本文件是全仓库按 Feature 与模块维度的**唯一实现与验证状态源**（[ADR 0048](architecture/adr/0048-doc-governance.md)）。`AGENTS.md`、ADR 与产品范围文档不再维护日期进度流水。

## 状态口径说明

源码存在 ≠ 构建通过 ≠ 本机运行 ≠ 模拟器 ≠ 真机；实验或原型验证通过不代表正式第三方/第一方插件已交付。状态表各列仅使用以下枚举：

- **源码**：`未开始` | `设计中` | `进行中` | `实验原型` | `已落位`
- **构建**：`未验证` | `通过` | `部分通过` | `失败` | `受限`
- **本机运行**：`未验证` | `无界面通过` | `实窗通过` | `实验通过` | `部分通过` | `不适用`
- **模拟器**：`未验证` | `通过` | `不适用`
- **真机**：`未验证` | `通过` | `不适用`

> 凡在 `本机运行`、`模拟器` 或 `真机` 标注 `通过`、`无界面通过`、`实窗通过`、`实验通过` 或 `部分通过` 的条目，必须在最后一列给出仓库内真实存在的证据文件链接。

## Feature 与模块状态

<!-- STATUS-TABLE-START -->
| Feature / 模块 | 源码 | 构建 | 本机运行 | 模拟器 | 真机 | 证据与关联 ADR |
|---|---|---|---|---|---|---|
| F-PLAT-01 六端跨平台交付（macOS/Win/Linux/iOS/Android/Web） | 进行中 | 部分通过 | 部分通过 | 通过 | 未验证 | macOS 预览与 Android arm64 APK/API 36 模拟器通过；Win/Linux/iOS/Web 产品端未验；[跨平台结果](../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) · [Android 记录](../experiments/nsh-reader/evidence/android-package-launch/README.md) · [ADR 0033](architecture/adr/0033-cross-platform-runtime.md) |
| F-PLAT-02 应用装配、打包与自动更新（`apps/qusor`） | 已落位 | 部分通过 | 实窗通过 | 通过 | 未验证 | macOS ad hoc `nsh package`、回环 0.1.0→0.1.1 更新与 GitHub Actions draft v0.1.0 通过；公证/商店签名、线上拉取、Win/Linux 替换未验；[发行与更新](product/release-and-update.md) · [更新记录](../experiments/nsh-reader/evidence/app-update/README.md) · [draft 验收](../experiments/nsh-reader/evidence/github-release/README.md) · [ADR 0037](architecture/adr/0037-production-app-assembly.md) |
| F-EXT-01/03/05/06 插件策略、签名包、Wasm 执行与安装内核（`extension-*` / `runtime-*`） | 已落位 | 部分通过 | 实验通过 | 未验证 | 未验证 | 无界面策略/Wasm/持久安装/Linux 监督本机通过；macOS 上 `runtime-process` controller 测试在干净树失败；生产安装 UI 与通用 Wasm 业务 UI 未完成；[策略规格](product/foundation.md) · [Wasm 结果](../experiments/extension-wasm/RESULTS.md) · [包结果](../experiments/extension-package/RESULTS.md) · [安装落位](../experiments/extension-package/INSTALLATION_PRODUCTION_RESULTS.md) · [ADR 0004](architecture/adr/0004-headless-extension-policy.md) · [ADR 0025](architecture/adr/0025-durable-installation-authority.md) · [ADR 0027](architecture/adr/0027-supervised-raw-process-runtime.md) |
| F-EXT-01/04 能力组合、Host 挂载与随包启停（`capability-composition` / `RuntimeScope`） | 已落位 | 通过 | 实窗通过 | 未验证 | 未验证 | 桌面 profile、单 Core 写者迁移与随包启停 macOS 实窗通过；动态第三方安装/卸载与 UI slot 挂载未完成；[迁移记录](../experiments/nsh-reader/evidence/core-migration/README.md) · [启停记录](../experiments/nsh-reader/evidence/package-enable-lifecycle/README.md) · [ADR 0038](architecture/adr/0038-capability-composition.md) · [ADR 0040](architecture/adr/0040-runtime-mount-scope.md) · [ADR 0043](architecture/adr/0043-persistent-package-enable-intent.md) |
| F-EXT-02 / F-SRC-01 / F-DATA-01 Core 捕获、原子提交与导出恢复（`core-api` / `learning-*` / `storage-sqlite` / `blob-store`） | 已落位 | 部分通过 | 实验通过 | 未验证 | 未验证 | Linux/macOS 本机捕获事务与空库恢复通过；非 Unix `storage-sqlite` 缺 `StudyStore`、全库快照读放大待拆分；[捕获结果](../experiments/capture-core/RESULTS.md) · [持久结果](../experiments/capture-core/PERSISTENCE_RESULTS.md) · [复合结果](../experiments/capture-core/COMPOUND_RESULTS.md) · [ADR 0018](architecture/adr/0018-core-api-contract-source.md) · [ADR 0019](architecture/adr/0019-atomic-knowledge-persistence.md) |
| F-LEARN-01 词汇学习（`learning-domain` FSRS-5 / `plugins/vocab` / Host API 0.5） | 已落位 | 通过 | 实窗通过 | 通过 | 未验证 | macOS 实窗（含音标/系统语音/读音偏好，后两项属工作台样例能力）与 Android API 36 模拟器评分/重开恢复通过；UI 仍在宿主内，未从 Wasm 包运行；[词汇规格](product/vocab-plugin.md) · [实窗记录](../experiments/nsh-reader/evidence/vocab-workspace-live/README.md) · [Android 记录](../experiments/nsh-reader/evidence/android-package-launch/README.md) · [ADR 0036](architecture/adr/0036-vocabulary-study-capability.md) |
| F-NOTE-01 想法读取与工作区（Host API 0.4 / `plugins/notes`） | 已落位 | 通过 | 实窗通过 | 通过 | 未验证 | macOS 实窗与重启截图一致、Android 模拟器空态通过；正式独立 UI 插件与精确高亮回源未完成；[包记录](../experiments/capture-core/evidence/notes-package/README.md) · [实窗记录](../experiments/nsh-reader/evidence/notes-workspace-live/README.md) · [ADR 0035](architecture/adr/0035-notes-read-capability.md) |
| F-LIB-01 / F-READ-01/03 资料库列表、PDF 导入、页图与保存范围高亮（Host API 0.6/0.7） | 已落位 | 部分通过 | 实窗通过 | 通过 | 未验证 | macOS 真窗与 Android API 36 模拟器（导入、旋转页图选 30 字保存、强停重开恢复高亮）通过；PDFium 仍在应用串行线程，长书/OCR/EPUB/跨行多段/拖动把手/持久几何高亮/正式 Reader Surface 未完成；[页图证据](../experiments/nsh-reader/evidence/pdf-page-import/README.md) · [ADR 0046](architecture/adr/0046-library-host-api-0-6.md) · [ADR 0047](architecture/adr/0047-native-pdf-import-and-page-images.md) |
| F-UX-01/02 NSH 原生宿主、中文输入与编辑器生命周期（`experiments/nsh-reader`） | 实验原型 | 部分通过 | 实验通过 | 未验证 | 未验证 | Linux 隔离依赖前缀原生构建、实窗选文/编辑/保存、正常关闭及 Core 重开/空库恢复通过；C9 九场景本机通过；图像任务退休仍在整合，资源释放 ACK、B 原生替换、IME 候选窗未完成；[NSH 结果](../experiments/nsh-reader/RESULTS.md) · [默认界面资格](../experiments/nsh-reader/evidence/native-editor-shutdown-gui-default/README.md) · [进程回收资格](../experiments/nsh-reader/evidence/native-editor-shutdown/README.md) · [C9 结果](../experiments/capture-core/supervised-results.md) |
| F-EXT-04 插件原生 UI 边界与组合协议（`check_native_ui.py`） | 进行中 | 通过 | 无界面通过 | 不适用 | 不适用 | `check_native_ui.py` 阻断 Web/DOM 回归；已拍板 3A 声明式原生 UI 树路线，生产协议与宿主渲染器接入中；[组合规格](design/composition.md) · [ADR 0028](architecture/adr/0028-extensible-native-workspace.md) · [ADR 0042](architecture/adr/0042-native-only-plugin-ui.md) · [ADR 0052](architecture/adr/0052-plugin-ui-protocol.md) |
| F-AI-01–07 / F-CAN-01 / F-KG-01/02 / F-SYNC-01/02 / F-AGENT-01–05 远期能力 | 设计中 | 未验证 | 未验证 | 未验证 | 未验证 | 仅架构/产品设计预留，待首版范围、云默认行为与后端授权确认；[范围记录](product/scope.md) · [Agent 规格](product/agent-workbench.md) · [ADR 0024](architecture/adr/0024-cloud-storage-adapters.md) · [ADR 0029](architecture/adr/0029-local-agent-workbench.md) |
| P2-8 文档治理与自动化校验（`AGENTS.md` / `docs/status.md` / `check_docs.py`） | 已落位 | 通过 | 无界面通过 | 不适用 | 不适用 | 本机脚本校验与单测通过；[文档检查脚本](../scripts/check_docs.py) · [ADR 0048](architecture/adr/0048-doc-governance.md) |
| P0-2 Host API 累积兼容与正交演进（`extension-api` / `extension-host`） | 已落位 | 通过 | 无界面通过 | 不适用 | 不适用 | 已支持 0.1–0.7 累积向后兼容、跨域联合声明及 `open_negotiated_session`，累积与兼容单测及 `policy_walkthrough` 通过；ADR 0049 第二部分正交扩展点为下一阶段演进规划（未实现）；[cumulative 测试](../crates/extension-api/tests/cumulative.rs) · [Host 测试](../crates/extension-host/tests/cumulative.rs) · [ADR 0049](architecture/adr/0049-cumulative-host-api-and-capability-evolution.md) |
| P0-3 / P1-5 存储端口拆分与增量查询（`learning-ports` / `learning-application` / `storage-sqlite` / `blob-store`） | 已落位 | 通过 | 无界面通过 | 未验证 | 未验证 | 已拆分为 Query/Command/Archive 六端口、落地 SQLite schema v6 迁移与 12 项索引（2k 摘录+3MiB PDF 实测 CAS blob 读取为 0），非 Unix 补齐完整 trait 与无泄露结构化错误并过 `wasm32`/`windows-gnu` 编译检查；`apps/qusor` 经 deprecated 委派兼容，非 Unix 运行时未实测；[ADR 0050](architecture/adr/0050-split-storage-ports-and-incremental-transactions.md) · [SQLite 适配器](../adapters/storage-sqlite/README.md) · [规模与迁移测试](../adapters/storage-sqlite/tests/scale_and_migration.rs) |
| P0-1 应用壳止血边界与前端拆分（`apps/qusor` / `check_app_boundary.py`） | 已落位 | 通过 | 无界面通过 | 未验证 | 未验证 | 已完成 vault 绑定、manifest 驱动 profile/runtime、Core 服务门面收口（冻结 16 个 `legacy_plugin_bypass`）、`scripts/check_app_boundary.py` 基线守卫与前端 `features/*` + `shell/*` 拆分（`nsh check` 与无界面 reading 回归通过；真窗口 `pdf-import` 及按“词汇 → 笔记 → 资料库/阅读”迁出正式插件 UI 仍未完成）；[边界守卫](../scripts/check_app_boundary.py) · [ADR 0051](architecture/adr/0051-app-shell-boundary.md) |
| P1-4 插件 UI 协议（`experiments/plugin-ui`） | 实验原型 | 通过 | 无界面通过 | 未验证 | 未验证 | 用户已拍板选定候选 A（3A）；Schema/白名单/Token/预算/无障碍/原子 Patch/代际围栏/`capability-composition` 原型 7/7 无界面集成测试通过；六端真窗/真机未验证，待 `@uiprod` 晋升生产契约后更新；[实验结果](../experiments/plugin-ui/RESULTS.md) · [ADR 0052](architecture/adr/0052-plugin-ui-protocol.md) · [协议设计](architecture/plugin-ui-protocol.md) |
| P1-5 运行时预算配置化与长驻流式会话（`extension-runtime-api` / `runtime-wasm` / `extension-package` / `sdk`） | 已落位 | 通过 | 无界面通过 | 未验证 | 未验证 | 已支持 Host 注入 MiB 级预算（`DesktopStandard`/`MobileStandard`/`LegacyProbe` 与硬上限失败关闭）、`qusor.emit_chunk` 有界流式响应、`WasmiSession` 跨回合长驻会话及 debug-only 显式 `DevUnsignedOptions` 调试门禁，源码/构建/本机 debug 与 release 单测通过；WIT 契约（`sdk/wit/qusor-plugin.wit`）、`qusor-sdk`/`qusor dev` CLI 与 iOS/Web/Windows 平台适配仍为设计态/未实机交付；[WIT 契约](../sdk/wit/qusor-plugin.wit) · [SDK 设计](../sdk/DESIGN.md) · [平台矩阵](architecture/plugin-platforms.md) · [ADR 0053](architecture/adr/0053-runtime-budgets-abi-and-platforms.md) |
<!-- STATUS-TABLE-END -->

## 当前已知结构缺口与待决事项

- **架构缺口**：详见 [2026-11-14 架构评审](architecture/reviews/2026-11-14-architecture-review.md)（第一方功能尚在 `apps/qusor` 壳内按 `词汇 → 笔记 → 资料库/阅读` 迁出中；Host API 正交能力演进与 `qusor-sdk` 独立发行待阶段三推进；Windows/iOS/Web 存储与原生服务待补齐）。
- **已拍板架构决策（2026-11-14 用户确认，不列入待决）**：1A（第一方同构/内置交付但强制同公开契约并受脚本守卫，[ADR 0003](architecture/adr/0003-plugin-first-delivery.md)）、2A（桌面与 Android 动态 `.qpk`，iOS/Web 仅随包内置与纯声明式，[ADR 0053](architecture/adr/0053-runtime-budgets-abi-and-platforms.md)）、3A（声明式原生 UI 树协议 + `ControlledSurfaceRef` + `ui.slot`，迁出顺序 `词汇 → 笔记 → 资料库/阅读`，[ADR 0051](architecture/adr/0051-app-shell-boundary.md) · [ADR 0052](architecture/adr/0052-plugin-ui-protocol.md)）。
- **待用户拍板（仅保留产品范围待定项，未定前不自动执行或改写）**：
  1. **C-02 首版功能范围**：首版是否同时包含 Reader AI、画布、知识图谱或同步（见 [范围记录](product/scope.md)）。
  2. **C-03 云端默认行为**：严格离线、默认关同步+主动调用云 AI，或首版登录同步元数据（见 [范围记录](product/scope.md)）。
  3. **C-04 / F-PLAT-01 / C-13 首发平台门槛与 Agent 深度**：各端最低系统版本/渠道资格、首发平台范围及本地 Agent 工作台首版深度与默认授权（见 [范围记录](product/scope.md)）。
