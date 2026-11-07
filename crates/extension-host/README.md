# extension-host

无 UI、无 I/O 的插件宿主策略库，使用 [extension-api](../extension-api/README.md) 的 0.1 JSON 协议。本轮关联 F-EXT-01/03/04/06，完整范围见 [功能规格](../../docs/product/foundation.md)。

从仓库根目录运行：

```sh
cargo run -p extension-host --example policy_walkthrough --locked --offline
cargo test -p extension-host --locked --offline
```

示例使用合成 manifest 和包摘要，打印注册、显式授权、跨库拒绝、撤权阻断旧许可、升级清空授权和卸载重装的结果。它不会读取资料、写入知识或执行插件。

## 集成边界

- 每个 Host 绑定一个 vault；安装 ID、Session 和 Permit 不能跨宿主使用。它们是仅供受信任宿主使用的进程内对象，不属于插件 wire 协议。
- register/update 接收受信任包验证器的元数据；当前仅检查摘要格式和身份声明一致性，不计算包摘要或认证发布者。
- replace_grants、enable 等管理操作由受信任宿主执行。必须先有用户授权，不应将这些方法暴露为插件可自行调用的 RPC。
- authorize 只产生当前策略许可。dispatch 消费 Permit，在受信任同步回调之前再检查会话和授权；回调不得执行插件、网络或把上下文当许可转交异步任务。
- 真正能力适配器仍需验证资源存在性、所属库、当前选区快照和内容版本；真正知识写入还需事务、审核、审计、outbox 与撤权的统一提交顺序。
- request_id 只关联请求，不是幂等键。重复 authorize 可以产生多个许可；本库没有防重业务回执。
- 所有状态仅在内存。重新启动需要重新注册和授权；这里不提供包安装、持久任务、恢复或安全沙箱。

## 当前能力范围

| 能力 | 可授予 / 请求的资源 | 包含规则 |
|---|---|---|
| selection.read | Selection（vault_id + selection_id） | 只匹配同一选区；选区变更须撤权或更换宿主管理的不可复用 ID |
| document.metadata.read | Document 或 Vault | 库级元数据授权覆盖同库 Document，单资料授权不能扩大为全库 |
| capture.propose | Vault | 只匹配目标库；不表示已接收或提交任何候选内容 |

声明与授权分开；不支持的必需能力拒绝注册，未授予的必需能力拒绝启用。可选能力不可用或未授权时，其依赖贡献不进入可用列表。贡献身份使用返回的 (InstallationId, contribution.id) 组合，不能只用字符串 ID 作为全局索引。

更改任意授权集合（包括同值替换）、停用、升级或注销都会使旧会话及许可失效。关闭单个会话只影响该会话。成功升级必须同包同发布者、更高版本且不同摘要，随后清空权限并禁用；失败验证保留旧状态。授权世代耗尽则禁用并关闭会话，需要注销重建安装实例。

本库最多保留 256 个安装、1024 个会话和每安装 256 项授权，以限制策略目录规模；这不是第三方代码的 CPU、内存或 I/O 运行预算。
