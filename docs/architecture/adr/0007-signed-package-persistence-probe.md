# ADR 0007 独立签名包与持久安装事务实验

日期：2026-11-07。状态：采用实验方案，生产集成尚未通过。关联 C-08、F-EXT-01/03/05/06、EXT-B02/B04、EXT-P04/P08/P11/P13。

## 问题与授权

用户要求在已完成的包生命周期设计和 NSH 构建阻塞记录基础上继续开发。既有 Wasm 实验只校验模块摘要，不能认证 manifest/资产/发布者，也没有可靠安装提交。按 [下一阶段](../../product/next-phase.md) 并行推进 B1/B2，不因 NSH 环境阻塞改用占位阅读 UI。

## 决定

- 新增 [独立包实验](../../../experiments/extension-package/README.md) 及独立作者 Workspace；生产三个 crate、运行时和业务依赖保持原有边界。
- 用带固定规范编码的单 JSON 容器验证完整内容，Ed25519 严格校验绑定 manifest、权限、profile、入口和全部资产。无压缩/解包，预算及路径/重复字段约束见 [格式](../../../experiments/extension-package/FORMAT.md)。成熟密码实现用于候选验证，未宣布生产密码库定型。
- 从包格式模块和权威 extension-api 导出最小实验 SDK，仓库外作者只取得这些工件，构建两版本；保持宿主二进制不变验证实际合成选区执行。SDK 不复制内部 Host 或数据库源码。
- 首次信任由宿主明确传入公钥；持久绑定 package ID、publisher ID 与键。普通更新或卸载后重装不能改键；换钥/撤销与首次信任 UI 留在后续验证。
- 使用 SQLite BLOB 保存完整包和管理记录，使内容、当前引用与回执在同一事务提交。相较 [生命周期设计](../plugin-lifecycle.md) 的独立内容文件方案，这个有界候选避免两个资源提交，但不验证将来流式大资产和文件发布协议。原子性只覆盖本实验管理数据库，不包含 Core。
- 安装/更新后仅 disabled，卸载为 tombstone；重装新 ID，修订/世代跨重装单调。expected revision 与操作 ID/输入摘要校验防止旧命令命中重装实例，重复请求查询持久回执。
- 每个 Store 持单写者锁，重开校验当前内容。用真实进程退出测试提交边界；FULL 同步配置和进程恢复不等于断电验证。发现损坏拒绝打开，不猜测或自动回退。

此实验没有新增生产授权状态机；正式启用及权限判定仍由 extension-host 负责。持久授权、新启动会话、私有状态迁移/回滚、长驻双插件恢复仍未实现。禁止把“安装记录已持久化”写成“完整插件生命周期完成”。

## 依赖方向和代价

实验宿主 → 包验证 + SQLite/锁适配 + 既有 Wasm 探针 → extension-api/extension-host。权威协议不依赖实验、NSH 或 SQLite；作者发行图只有公开格式/API、签名及 WAT 工具。

[依赖策略](../../../config/architecture.toml) 登记两个独立 Workspace 和 normal/dev/build/target 声明及解析闭包：ed25519-dalek 用于完整签名，sha2 用于内容/键摘要，rusqlite bundled 用于本机事务，fs2 用于锁，libc 只提供 O_NOFOLLOW/O_NONBLOCK 标志，serde/serde_json 用于有界容器。Wasm 运行预算复用既有实验，未选定生产运行时。

十六进制容器体积较大，不适合直接作为无限资产分发格式；保守预算使原型可审查。保留全部内容/回执，暂不做垃圾回收。只验证 Linux/WSL，本地锁和 SQLite 行为不能外推其他平台/网络文件系统。

## 证据与下一步

[本机结果](../../../experiments/extension-package/RESULTS.md) 记录源/工件/宿主摘要、外部构建、篡改与接管拒绝、安装/更新/卸载崩溃恢复。既有 Wasm 冻结字节构造入口仅为验证器传递已经取得的字节，不自行认证发布者。

后续按 [生命周期规格](../../product/plugin-lifecycle.md) 完成授权与执行恢复，再连通真实 Core 事务、知识保留和官方插件同权。NSH、正式 SDK/安装器、阅读功能及平台/分发分别留证；本 ADR 不改变首发平台、AI 或云范围。
