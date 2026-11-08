# 私有状态迁移与成对回滚实验

状态：独立本机实现，逐项实际验证见 [结果](RESULTS.md) 与 [ADR 0010](../../docs/architecture/adr/0010-paired-private-state-probe.md)。不代表正式安装器、Core 知识回滚隔离或长驻监督完成。关联 F-EXT-03/05/06、EXT-B04、EXT-P08/P13，依据 [生命周期规格](../../docs/product/plugin-lifecycle.md) 和 [架构](../../docs/architecture/plugin-lifecycle.md)。

## 包与私有状态契约

沿用完整签名容器，在固定资产 `assets/lifecycle.json` 中声明独立协议与私有 Schema，不用包版本猜测 Schema：

```json
{"protocol":1,"private_schema":2,"migration_module":"migration.wasm","accepted_from_schemas":[1]}
```

描述和模块都必须是签名清单中的文件；无描述的既有包为无私有状态包。真实迁移模块禁止任何 imports、WASI、网络、正式知识及 UI 能力，限制输入/输出至 64 KiB，并沿用内存、指令及有界执行预算。本轮固定 ABI：

```text
alloc(len: i32) -> i32
migrate(from_schema: i32, to_schema: i32, ptr: i32, len: i32) -> i64
```

结果高 32 位为指针、低 32 位为长度，分配/迁移均计入预算，所有内存范围由宿主校验。宿主只复制旧快照和执行作者模块，不硬编码字段转换。新运行入口另提供 `qusor_abi_version()` 和 `qusor_state_check(schema, ptr, len)`；预检使用拒绝业务调用的 broker，验证真实 exports 和匹配状态，不能通过临时授权绕过预检。

## 持久状态和流程

管理库 schema 3 新增不可变 private_snapshots、private_heads、retained_pairs 和每安装至多一条 private_pending，并单独存储私有操作终态。快照绑定安装 ID、包摘要、Schema、状态摘要和字节。包摘要与当前状态 head 必须在同一 SQLite 最终事务切换。

可信实验管理命令：

```text
initialize-private STORE OP REV PACKAGE_ID STATE_FILE [--crash POINT]
upgrade-private STORE OP REV PACKAGE KEY [--crash POINT]
rollback-private STORE OP REV PACKAGE_ID RETAINED_PAIR_ID [--crash POINT]
private-status STORE PACKAGE_ID
```

命令成功处理返回 receipt，outcome 区分 succeeded、failed 和 interrupted；后两项是可查询终态，不能显示为升级成功。private-status 返回当前包/Schema/状态摘要和十六进制字节，以及可选择的历史 pair_id。初始化只允许无当前私有 head、未授权且禁用的安装，并通过对应包真实预检；这不是正式插件私有存储 API。

**必须封堵普通 install 更新旁路：一旦安装有私有 head，就拒绝直接只切包版本，要求走成对迁移。**

升级流程：

1. 验证新包签名、信任绑定、版本及声明；失败不动旧授权。
2. 准备事务保存内容和 pending，清授权、递增修订/世代；提交后撤销该安装会话。
3. 在事务外复制旧快照，执行受限迁移和新入口预检。
4. 最终事务选择新包和新状态、保留旧配对、记录终态回执、清 pending，再次推进修订/世代，保持禁用。

准备提交后失败：旧包与旧状态仍为权威，保持禁用，记录失败回执，不能复活旧会话。重开遇到 pending 不自动重跑插件迁移；协调器终结为“中断，保留旧配对，禁用”，同操作 ID 查询该终态，显式新操作才可再试。

显式回滚只接受保留的历史配对，重新验证签名/信任/兼容、快照和历史运行预检，通过相同准备/完成流程恢复；不做反向字段转换，不把新状态交给旧代码。重装产生新安装 ID，旧私有状态不能自动命中它。Core 不在管理库中，本实验不能声称已验证真实知识不回滚。

## 验证要求

- 仓库外作者构建两个版本；真实 Wasm 迁移保留原偏好并添加新版字段。
- trap、指令耗尽、非法指针、超量输出、错误 Schema/ABI、模块导入能力均失败，旧配对字节保持。
- 准备提交前后、迁移后、新快照写入后及最终提交前后逐点终止进程；恢复只能得到完整旧配对（切换前）或新配对（切换后）。
- 失败后仍禁用，旧授权拒绝；回滚匹配旧快照且世代单调，幂等回执不重复。
- 普通 install、初始化重放、卸载/重装均不能绕过配对绑定；损坏/缺失快照、孤立 pending 和未知协议 fail closed。

## 预算、作者与复现

迁移模块不超过 256 KiB，状态输入/输出至多 64 KiB；运行器共享 128 KiB 内存、表/栈/递归限制。单次迁移中 alloc 和 migrate 共用 1,000,000 fuel、10,000 指令片段及 250 ms deadline；预检中 ABI/alloc/state_check 也共用预算。deadline 不抢占原生编译，尚无长驻 worker 回收成绩。

快照最多 8192 个、总字节 64 MiB。普通操作、私有终态和 pending 共用 8192 预算；重开核对总量、快照/包/所有者、head/历史引用和 pending，损坏或孤立记录拒绝打开，保留文件供诊断。没有自动猜测恢复、GC 或生产灾难恢复。

升级/回滚中止点为 before_prepare、after_prepare、after_migration、after_snapshot、before_commit、after_commit；初始化没有持久准备阶段，只接受 before_prepare、after_snapshot、before_commit、after_commit。after_prepare 之后至最终提交之前，重开终结为 interrupted，原配对保持禁用；before_prepare 没有新持久事实，可显式重试。

作者工具仍以前五个参数构建普通包，可追加 lifecycle.json 和 migration.wat（v1 无迁移时传 `-`）。描述固定包内 assets/lifecycle.json，模块固定 migration.wasm，均参与完整签名。测试偏好 theme=night 由真实作者 Wasm 复制并追加换行与 layout=wide，宿主不转换字段。

运行包实验入口即可复现全部跨进程场景：

```sh
python3 experiments/extension-package/validate.py --output /tmp/qusor-private-lifecycle.json
```

证据必须分别报告作者独立构建、Rust 回归、真实进程终止与平台范围；本页不把上述要求自动计为通过。正式插件私有读写 API、通用 Schema 迁移框架及实际知识保存尚未实现。
