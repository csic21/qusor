# dsh 插件组合研究：能力接缝、Profile 与可逆注册

日期：2026-11-12。关联 C-07、C-11、C-12、F-EXT-01/03/04/06。用户提出 QUSOR 的插件化思路可参考 dsh。

## 范围与来源

本轮阅读 DeepSeek Harness（dsh）官方架构、能力接缝、插件解剖、扩展手册和应用启动文档。站点在核对时标注 DSH `v0.1.5-alpha.1`；未冻结上游提交，也未引入 dsh 代码或依赖。本文只提取与 QUSOR 插件基座相关的结构，不作为 dsh 完整能力、安全或平台结论。

主要来源：

- [dsh architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.zh.md)
- [dsh capability seams](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/capability-seams.zh.md)
- [dsh extension cookbook](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cookbook/extension-cookbook.zh.md)
- [dsh app boot](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/boot/app-boot/README.zh.md)

## dsh 的核心结构

### 能力接缝，而不是包名分支

dsh 把一个可替换能力拆成三种角色：

1. Service Definition 声明接口和词汇；
2. Service Provider 实现该能力；
3. Consumer 通过稳定服务使用能力。

一个包可以同时承担多个角色，但只定义接口、只消费接口或只实现后端都不自动构成完整接缝。替换后端时，消费方不必导入具体实现。

### Profile 与 Bundle

运行中的 dsh 由具名 profile 组合有序 bundle 层，再由 profile patch、机器 patch 和启动 overlay 覆盖。应用启动器只负责 profile 解析和插件树启动；不同运行面（Web、headless、SDK、ACP）是不同组合，而不是在各处增加模式分支。

Bundle 携带配置行和挂载代码，因此其插入内容仍可被后续层配置或禁用。`--dump-config` 是组合的有效诊断入口，而不是只读静态模板。

### 静态依赖声明与可逆副作用

插件在加载前声明自己注入哪些服务，加载器据此等待依赖、报告缺失并拒绝同一服务的冲突提供者。`inject` 只解决“能否访问服务”，不授予文件、进程、凭据或网络权限；权限与沙箱是独立轴。

所有工具、提示词片段、监听器、提供者和适配器注册都通过 effect 安装，并在插件卸载时撤销。启动失败策略区分 required 与 optional，失败插件的诊断保留到能说明原因的位置。

### 事件分层

dsh 区分持久会话事实、进程内 Agent 事件和能力事件，并明确 emit、waterfall、serial 等分发模式。策略门禁、变换、观察是不同契约，不能把一种 hook 语义套到所有扩展点。

## QUSOR 采用的部分

| dsh 机制 | QUSOR 采用方式 |
|---|---|
| 能力接缝三角色 | Host 服务定义、普通包/适配器实现、插件工作区消费；不以插件包名写业务分支 |
| Profile + ordered layers | 使用声明式 JSON/TOML profile；bundle、产品和用户层可覆盖完整行；运行前校验 |
| 静态 requires/provides | Manifest 或 profile 声明版本化服务依赖和提供；缺失、重复、不兼容在启动前失败 |
| 可逆注册 | Host 服务、UI贡献、监听器返回 disposer；禁用/更新时按所有权撤销 |
| 分层事件 | 宿主内部策略事件与持久 Core 事实分开；访客 wire 仍使用有界版本化请求 |
| 组合诊断 | 提供 composition dump，列出激活顺序、层来源、服务边沿和错误 |

QUSOR 第一轮采用纯 Rust 规划内核，不采用 Cordis 的运行时对象模型。插件执行仍经过签名包、Host 授权和受限运行器。

## 不直接采用的部分

- 不引入 Node/Cordis、任意进程内 TS 插件或 YAML `!!js` 可执行配置。
- 不把 Core 的知识事务、来源锚点、审计和 outbox 变成可被 profile 替换的服务。
- 不采用无 schema 的“整行最后写入覆盖”；QUSOR profile 和插件配置需要版本、结构校验与迁移规则。
- 不把 `requires` 当作权限。能力可达、资源授权、网络出站和文件访问仍是不同层次。
- 不为了“所有东西都是插件”创建没有真实多实现需求的抽象。

## 初始能力目录

第一轮只覆盖当前已存在的两个真实消费链：

- `notes.read` 0.4：Notes Host 提供，想法工作区消费。
- `vocab.read` 0.5：背单词 Host 提供，词汇工作区消费。
- `vocab.review` 0.5：背单词 Host 提供，词汇工作区消费。

后续按真实功能增加：

- `vault.read` / `vault.write`：共享 Core 服务（第一版由 `app.qusor.core` 提供）；
- `library.import` / `library.list`；
- `reader.surface` / `reader.selection`；
- `notes.write` / `source.resolve`；
- `ai.provider`、`agent.backend`、`export.archive`。

每一项都必须同时定义 provider、consumer、失败语义和停用行为，不能只增加一个名称。

## 本轮落地与未覆盖

已落地纯 `capability-composition` crate、桌面 profile、Core provider 和 Notes/Vocab Host 的启用门。内核检查层级覆盖、重复 provider、缺失 provider、版本不兼容、必需项禁用和依赖环，并输出确定性激活顺序；Core provider 已让两个 Host 共享一个 SQLite/CAS 写者，旧分库通过 staging 迁入。

已完成纯 RuntimeScope 的顺序挂载/逆序释放和应用统一 Host runtime；尚未完成动态重载/停用、用户 profile 覆盖写入、UI贡献过滤、正式 reader/library 包和动态 bundle 安装。当前结果证明组合规划、Host 启动门、共享 Core 所有权和统一运行时，不证明插件热插拔、安全沙箱或产品阅读旅程。
