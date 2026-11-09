# 通用 Agent 基座的组合机制研究

日期：2026-11-09。关联 C-11/C-12/C-13、F-EXT-04/06/07、F-AGENT-01–05。用户否定前稿作为整体产品的 IDE 式布局，明确要求通过插件适配各种场景的 Agent。本轮据此修正 [上一轮结论](10-product-design-synthesis.md) 的适用层次；先前开发稿仅保留为开发场景候选。

## 研究判断

需要优先研究的是宿主如何接入执行者、能力与交互，并保持它们可组合。视觉规范继续用于一致性，整体结构改为“任务 + 可替换 Agent + 工具/资源 + 可替换交互 + 场景组合”。对话、阅读、代码和表单都是可能的主交互。

| 官方资料，2026-11-09核对 | 证据 | 对 QUSOR 的取舍 |
|---|---|---|
| [Raycast UI API](https://developers.raycast.com/api-reference/user-interface) | 扩展通过共同的 List/Grid/Detail/Form 与 ActionPanel 声明交互，支持元素渲染为原生 UI | 借鉴同权贡献、公共交互角色和原生呈现；不照搬其命令窗口作为所有复杂任务的布局 |
| [Claude 插件](https://support.claude.com/en/articles/13837440-use-plugins-in-claude) | 插件把技能、连接器与子 Agent 打包；不同客户端可用能力不同 | 场景可以组合行为与工具，不只提供一个面板；按平台/执行环境展示差异。QUSOR 仍支持替换完整 Agent 后端，不绑定单一厂商 |
| [MCP Apps 概览](https://apps.extensions.modelcontextprotocol.io/api/documents/overview.html) | 工具可关联交互界面；宿主、工具/资源与结果视图分工；富 UI 是可协商增强 | 借鉴结构化结果、适用视图与能力协商。其 HTML/iframe 路线不自动成为 QUSOR 原生渲染方案；Web 兼容另走缺口验证 |
| [ACP 介绍](https://agentclientprotocol.com/get-started/introduction) | 解耦编程 Agent 与客户端，面向编码场景 | 作为特定后端的候选适配通路；不能据此定义所有非编程 Agent，更不能把 QUSOR 外壳固定成 IDE。版本与后端能力仍需逐项固定验证 |

这是官方机制文档研究，没有登录或执行上述服务，也未实测互操作性。这里没有把这些协议视为同一件事：Agent 会话、工具能力和交互视图分别适配，协议本身不提供 QUSOR 的统一授权、恢复和知识事务。

## 得出的产品规则

1. 宿主固定任务/空间、身份、权限、恢复和组合规则，业务入口来自插件与用户固定项。
2. Agent 是可替换执行者；场景是指令、工具、资源与交互预设，不能把场景名称当作后端身份。
3. 界面随任务组合：研究可以并置证据，创作以文稿为主，事务显示表单/步骤，开发再显示代码与终端。
4. 插件可贡献工具、数据、结果视图或完整场景；单一包可贡献多个角色，但必须使用共同公开链路。
5. 人直接操作视图与 Agent 调用工具共享契约；任务进展与成果可见，不强迫一切回到聊天文本。
6. 任务、会话、运行和界面分离，换布局不重跑任务，换 Agent 不冒充原会话恢复。

具体状态、流程、数据归属、插件生命周期、手机和 AF-01–07 验收见 [基座设计](../design/agent-foundation.md)。这轮修正不把所有场景加入首版，也不撤回既有阅读与知识目标。

## NSH 更新核对

本轮已重新核对 [官方发布](https://github.com/longbridge/nsh/releases) 与近期提交：稳定 v0.4.0，nightly 目标 `36b91689c1e7c38379179a56381de7ddab16c6a5`；main 为 [87c7e8d](https://github.com/longbridge/nsh/commit/87c7e8da85d2f15a56107baa6f0fabb419b508b3)，前一提交 `cbf170762b7ffb8837dbb7e69a8df53ce0f6d1c3`，分别涉及发布播放器构建和翻译页锚点。重新读取 [同一 main 的官方设计指南](https://github.com/longbridge/nsh/blob/87c7e8da85d2f15a56107baa6f0fabb419b508b3/skills/nsh-design-guides/references/design-guides.md)。

影响：保持应用自己的设计系统、原生组件、明确焦点/区域滚动及手机导航；可组合职责不要求采用 Web 页面。没有据这些更新认定通用插件舞台或 Agent 接入已实现，也没有升级现有固定 NSH 运行基线。
