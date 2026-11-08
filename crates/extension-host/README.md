# extension-host

无 UI、无 I/O 的插件宿主策略库，使用 [extension-api](../extension-api/README.md) 的版本化 JSON 协议，保留0.1兼容入口并支持显式0.2会话。本轮关联 F-EXT-01/03/04/06，完整范围见 [功能规格](../../docs/product/foundation.md)。

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
| selection.read（0.1） | Selection（vault_id + selection_id） | 只匹配同一选区；不获得0.2的Document语义 |
| selection.read（0.2） | Selection 或精确 Document | Document范围只允许读取同一Surface当前native selection lease，不能任意读全文 |
| reader.surface（0.2） | 精确 Document | 同一安装/会话/epoch的受控原生阅读呈现；不隐含selection.read或知识写入 |
| document.metadata.read | Document 或 Vault | 库级元数据授权覆盖同库 Document，单资料授权不能扩大为全库 |
| capture.propose | Vault | 只匹配目标库；不表示已接收或提交任何候选内容 |

声明与授权分开；不支持的必需能力拒绝注册，未授予的必需能力拒绝启用。可选能力不可用或未授权时，其依赖贡献不进入可用列表。贡献身份使用返回的 (InstallationId, contribution.id) 组合，不能只用字符串 ID 作为全局索引。

更改任意授权集合（包括同值替换）、停用、升级或注销都会使旧会话及许可失效。关闭单个会话只影响该会话。成功升级必须同包同发布者、更高版本且不同摘要，随后清空权限并禁用；失败验证保留旧状态。授权世代耗尽则禁用并关闭会话，需要注销重建安装实例。

本库最多保留 256 个安装、1024 个会话和每安装 256 项授权，以限制策略目录规模；这不是第三方代码的 CPU、内存或 I/O 运行预算。


## 公开阅读Surface的Host边界

[ADR 0023](../../docs/architecture/adr/0023-public-reader-surface.md) 的会话与registry实现位于 [surface.rs](src/surface.rs)。旧 `open_session` 明确使用0.1，新 `open_session_for_api` 按manifest支持选择已实现版本并固定进连接；请求不能跨会话版本使用。声明新能力/阅读贡献的包必须使用0.2，不能在旧默认入口隐式升级。

可信装配层以操作系统启动熵调用一次 `configure_surface_identity`，不能由guest设定或重新置零。Host不做OS I/O。Surface handle同时绑定Host身份、安装实例、session、授权epoch、package digest、vault/document和不可变来源；native frame/input/gesture/cleanup票据不实现Deserialize且私有字段不能由guest构造。每次native回调重新验证会话/授权及frame job/mount/世代，实际PDF内容与Core草案校验属于可信文档适配器。

Open必须属于实际安装的ReaderWorkspace贡献，并对贡献每项所需capability核对当前Doc（capture.propose映射目标Vault）范围，不能只凭另一个文档的同名授权。Pending/Failed帧不能接输入；同一frame的新gesture使旧gesture失效，旧回调必须携带旧gesture ticket，不可用通用clear删除后继候选。选区lease发布不替换grants，不使自己的session失效；完整响应超4KiB则拒绝而不截断。

停用、撤权、代码更新、卸载、断开会话和可信Core的文档失效通知统一将关联记录置为Closing，并立即阻止输入/读取。`pending_surface_cleanup`向可信后端提供释放票据，只有实际释放后才调用 `confirm_surface_closed`；未确认的资源持续占用容量。单Host至多64个、单安装16个Surface（含Closing），这是当前有界策略实现的保护上限，不是最终产品规模承诺。终止的frame job可报告Failed并显式新世代重试；它不代表已释放资源。

[13项Surface边界测试](tests/surface.rs) 与原14项Host测试实际通过，覆盖协议隔离、scope/源/贡献/nonce、pending与旧frame、跨安装/会话/Host、完整预算、旧gesture、各失效路径及释放前容量不可复用。真实NSH/普通Wasm/Core旅程由上层资格集成继续验证，单独Host测试不算产品阅读通过。


可信Invocation broker在向guest提供本次参数前调用 `preflight_surface_request`，Open与actual open复用同一贡献和scope规则，其他操作按真实handle/世代/lease/page验证；这是纯检查，不排队或替代实际操作的最终判权。VM输出不能仅与缓存描述比较：活跃State再次读取 `surface_state`，Selection再次读取完整当前lease。Close结果在native释放确认前可通过 `surface_result_state` 读取Closing描述（仍检查session/epoch/grant且无frame/selection），随后才确认释放；撤权后该接纳路径同样拒绝。结果还必须匹配本次已执行操作，不能由guest自报成功。


## 持久恢复与单一Host

[durable.rs](src/durable.rs) 定义Host侧 `AuthoritySource` 和 `DurableHost`：多个安装恢复到同一个实际Host，每次派发重读持久材料，重启只建新会话，不复活旧Session。`validate_activation_intent` 在同一Host无副作用地预检用户授权意图，持久成功后才激活。选定API下逐项检查资源语义，不能用0.2的Document选区授权激活0.1会话。

`DurableGrant.resource_version` 保留精确文档版本；`VersionedRequest` 必须来自可信Core当前解析，不是guest自报。文档版本解析、Host dispatch、实际Core读取/提交与管理变更处于同一外层gate/一致快照；guest执行位于门外。状态变化、不可读或过期时停掉当前会话并保留安装身份及未释放资源的配额，旧会话不能因记录恢复而复活。Current只核对持久事实，不运行guest；Activation才允许启动前私有预检。

生产存储另有显式profile2阅读能力集合，完整恢复且固定0.2；原profile1保持。`with_surface` 每次重核新鲜持久事实，再提供不暴露原始Host/Session的短作用域 `DurableSurface`，私有回调同时核对票据安装归属与来源版本。同安装重新激活沿用内部身份，不能重置Closing配额；旧会话不能关闭后继会话。

`retire_removed` 仅接受可信Source的真实卸载证明，在停会话前核对vault、包、安装ID及更新的revision/generation；所有Surface实际释放后才退役注册，避免长期耗尽256槽。禁用不等于卸载，删除map/revoke URL不等于释放。真实双Store负例验证跨库同编号卸载不能停止当前会话，300包退役与Surface配额另有回归。边界见 [ADR 0026](../../docs/architecture/adr/0026-durable-reader-native-dynamic-pdf.md)；生产supervisor、正式reader和通用信任/授权UI仍未完成。


## 临时组件挂载

同一Host安装目录新增0.3组件mount，不创建普通Session、不把安装标为Enabled、不修改持久grants。用户明确仅本会话启用并挂载到精确Draft；每次请求核owner、epoch、mount及资源，DurableHost再读当前安装事实。普通grant入口明确拒NoteDraftEdit。

统一persistent安装映射同时服务reader和component；更新、停止与安装身份变化使旧输入失效，Closing计入64总/16每安装直到可信native实际释放确认。换persistentID前必须完成旧ID的真实卸载证明与Surface/组件双资源释放退休。策略层的释放ticket测试不是native释放实测，见 [接线结果](../../experiments/capture-core/COMPONENT_CONTRACT_RESULTS.md)。
