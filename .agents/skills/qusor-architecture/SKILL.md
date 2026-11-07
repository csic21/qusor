---
name: qusor-architecture
description: 设计或审查 QUSOR 的 Rust 模块边界、接口、持久化与架构决策；适用于架构基座或跨模块变更，不用于普通文案修改。
---

# QUSOR 架构工作流

先读仓库 [AGENTS](../../../AGENTS.md)、[当前范围](../../../docs/product/scope.md) 和 [架构入口](../../../docs/architecture/README.md)。用户最新指令优先，研究建议不自动等于已确认功能。

## 按问题读取

- 目录、功能归属、独立插件 Workspace 与 SDK 来源：读 [目录架构](../../../docs/architecture/workspace-layout.md) 和 [ADR 0006](../../../docs/architecture/adr/0006-plugin-workspace-layout.md)；包安装/恢复读 [生命周期设计](../../../docs/architecture/plugin-lifecycle.md)。

- 插件、外部来源、扩展UI、运行隔离与生命周期：读 [插件架构](../../../docs/architecture/extensions.md) 和 [ADR 0002](../../../docs/architecture/adr/0002-extension-boundary.md)、[ADR 0003](../../../docs/architecture/adr/0003-plugin-first-delivery.md)。
- 模块、线程、Schema、定位和导出：读 [Core 设计](../../../docs/research/03-core-data-architecture.md) 的相关段落。
- 工具调用、引用、审批：读 [Agent 设计](../../../docs/research/04-agent-retrieval.md)。
- 同步版本、游标、快照和隐私：读 [同步设计](../../../docs/research/05-local-first-sync.md)。
- NSH能力与renderer：读 [源码审查](../../../docs/research/01-nsh-gpui-audit.md) 和 [技术路线](../../../docs/research/02-reading-canvas-routes.md)。

只读本次变更需要的细节；不要复制研究报告进新技能。

## 形成决策

说明问题、选项、依赖方向、数据所有者、版本/错误/恢复行为和验证证据。已有充分授权的工程细节自行决定；改变产品范围时更新范围记录并确认真正缺失的选择。

Core不依赖NSH/GPUI/DOM或平台view。来源锚点独立于渲染位置；local_version、server_entity_version和commit_rev不可混用。画布几何与语义关系分开。

插件通过版本化能力接口提交，不直接写SQLite；规范知识和来源快照属于Core。区分本地摘录定位与外部回跳，卸载仍可读/导出。manifest、签名、独立进程不等于沙箱；运行支持与商店分发分别留证。

第一阶段开放第三方已确认，官方功能必须通过相同公开SDK实现。区分Core不变量、Host原生服务与功能插件；不能用内置特权或私有直连填补SDK缺口。独立包构建/安装/更新及隔离是M0功能前置，移动未验不能自动降级为仅内置或缩减平台。

当前有无业务类型的 learning-domain、已实现的 extension-api/extension-host 策略内核及独立实验；具体完成状态查入口与结果记录。新增模块须有真实职责，不生成全套空 crate、假后端或占位业务方法。

新增依赖或成员时更新 [依赖策略](../../../config/architecture.toml)，写明用途和验证范围；不能通过放宽检查掩盖架构倒置。架构决定记录在 [ADR目录](../../../docs/architecture/adr/)。

## 例子

“需要PDF展示”应先评估adapter、像素通道与定位契约；不能把PDFium句柄或NSH节点类型加入永久Document模型。

“接入RSS或外部阅读选段”应复用CaptureDraft与来源模型，不新增服务名领域分支，不假设公开API或将插件私有数据当知识事实源。

“优化同步游标”应证明连续已提交前缀与ACK/pull交错；不能仅把updated_at或自增sequence当作提交顺序。

## 交付

按 [开发指南](../../../docs/development.md) 运行相关检查，区分设计、构建和实际运行。结果说明改变了哪条边界、为何、如何验证及未验证范围；基座可编译不代表M-1或业务验收通过。
