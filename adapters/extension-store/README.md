# extension-store

关联 F-EXT-01/03/05/06、EXT-B02/B03/B04 与 [包生命周期](../../docs/architecture/plugin-lifecycle.md)。本模块是持久安装记录、不可变包字节、持续授权材料、私有状态成对切换和启动租约的 SQLite 权威实现；不保存正式知识，不创建 Host 或 Session，不执行具体 VM，也不持有 NSH 类型。

原实验 schema1—4 的表、JSON 字段顺序、操作输入 hash、提交算法和预算迁入同一实现。实验 Store 仅组合本适配器与其原会话/模拟 metadata 协调器，故障标签和退出注入留在实验；生产只提供类型化 `CheckpointObserver`。`Connection`、事务、writer lock 均不公开。Linux 配置沿用私有目录、单 writer、SQLite DELETE/FULL；其他平台的持久 profile 尚未验证，`open` 明确拒绝。

`open_for_host` 接收可信宿主版本集合，默认只支持0.1。安装始终禁用且不保存授权，每次 frozen action/恢复读取均重验同一批持久字节、完整摘要、发布者键与命名空间。更新和卸载清除授权材料并推进 revision/generation；重装分配新的安装 ID。操作重放只查询原回执，不重复写入或重新启用。

## 可信应用端口

- `GrantValidator` 由实际 Host 的无副作用策略预检实现；保存意图时不能用 `validated=true`、签名或 grant 记录代替当前 Host 校验。
- `PrivateStateExecutor` 只处理有界 module/state；私有更新先持久撤销旧授权和记录 prepare，再在 SQL 事务外迁移/预检，最后同一事务切换 package+snapshot。具体运行器与其预算由可信装配提供。
- `CheckpointObserver` 是提交边界观测，默认无操作；不属于 guest 协议，也不提供授权。

默认 services 拒绝缺失的授权/私有执行服务，不自动提供模拟后端。应用必须将持久管理变更与当前 Host 授权/正式提交的门串行协调；本适配器不声称两个数据库具有分布式事务。

`AuthoritySource` 向唯一的 `extension_host::durable::DurableHost` 提供新鲜持久事实：Activation 可执行启动前私有预检，Current 只重读当前安装、包/键、授权、来源版本、私有 pair 与 worker lease，不运行 guest。`resource_version` 保留旧持续授权的精确版本语义；实际 broker 用可信 Core resolver 的当前版本复核。快照与原始 grant 都不是能力许可，不能绕过 Host。

## 验证

```sh
cargo test -p extension-store --locked --offline -- --test-threads=1
cargo clippy -p extension-store --all-targets --locked --offline -- -D warnings
cargo test --manifest-path experiments/extension-package/Cargo.toml --locked --offline -- --test-threads=1
python3 experiments/extension-package/validate.py --output /tmp/qusor-package-validation.json
```

局部测试覆盖锁释放后活子进程不延长 writer lease、缺少服务时不写授权/私有状态、迁移回调在持久撤权之后且 SQL 事务之外、Current 读取不执行 guest、损坏发布者记录及撤权后拒绝。事务测试中的注入 executor 仅核对回调位置，不代替真实 Wasm 资格；真实迁移/回滚及进程中止由原实验消费者继续验证。

此迁移不等于生产安装/信任 UI、完整 supervisor、正式功能插件或多平台通过。用户知识仍由 Core 保留和导出，卸载不会取得知识库删除能力。

`AuthoritySource::removed_installation` 只为真实卸载、没有未完成私有事务/worker租约且包与发布者仍可重验的记录提供退役事实；同时读取卸载后保留的vault绑定，缺失失败关闭。DurableHost在停会话前核对vault、包、安装ID及更新的计数，所有Surface实际释放后才回收注册槽。另一资料库的有效卸载证明不能退役本库同编号安装；真实双Store负例见capture-core的durable_installation测试。

## 阅读持续授权 profile 2

`grant_set` 是与旧 `grant` 分开的显式管理命令。`GrantSet` 固定Host API0.2、一个vault与共同有效期，保存1—32个 `PersistentGrant`；ReaderSurface、SelectionRead与DocumentMetadataRead只允许精确Document及非空版本，CapturePropose只允许同vault且无文档版本。临时Selection、selection lease、一次性批准不能进入该记录。重复能力/资源和同一文档跨能力版本不一致拒绝。

`SavedGrantSet` 使用policy_version2，仍在原grants表与同一管理事务保存；单条记录编码最多32KiB，SQL先限制字节长度再加载/反序列化。`SavedAuthorization::{V1,V2}` 显式读取两种记录，未知版本失败关闭。旧SavedGrant/GrantScope及其JSON顺序、命令hash和profile1约束不变；旧 `saved_grant`、`execution_snapshot` 对profile2拒绝，不投影成首条授权。

`GrantValidator::validate_set` 默认拒绝，必须由可信同一Host的实际能力/资源/required预检实现。安装不自动创建任何集合，更新/撤权/卸载清除所有profile；重放旧回执不重演授权写入。Source保留集合全量和required_api0.2，旧0.1入口不能隐式恢复或降级。Current仅核对持久事实，不恢复旧Surface/lease/批准；资源版本仍需可信Core在同一gate内复核。

`tests/grant_sets.rs` 使用真实SQLite和真实Host纯策略验证集合、重开、32条正边界/超限、版本/资源/有效期/重复拒绝、坏行预算、旧profile序列化、幂等/CAS、更新/重装清授权及未绑定spawn排他。测试中的进程身份仅为持久排他fixture，不证明OS身份或实际清理；这些仍由runtime-process与监督实验留证。profile2本身也不是信任/授权界面或正式阅读插件。


组件0.3采用 `bind_component_vault` 只持久化库亲和绑定，保留disabled且不写任何Draft grant。`ComponentAuthoritySource`逐次校验安装、签名composition资产、库与private/worker事实；没有假造enable布尔或profile3。临时启用和Draft访问来自同Host当前用户mount意图，重启不恢复。普通签名包的绑定/重开/停用拒旧mount及缺asset测试见 [结果](../../experiments/capture-core/COMPONENT_CONTRACT_RESULTS.md)。
