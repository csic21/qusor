# 签名包与持久安装事务实验

关联 F-EXT-01/03/05/06、EXT-B02/B04、EXT-P04/P08/P11/P13 的本机子项。依据 [生命周期规格](../../docs/product/plugin-lifecycle.md) 和 [ADR 0007](../../docs/architecture/adr/0007-signed-package-persistence-probe.md)，推进独立作者包、完整性/发布者绑定和安装事务；[实测结果](RESULTS.md) 单独记录。

## 当前格式权威源

[签名包格式](../../crates/extension-package/README.md) 已按字节迁入生产 crate；实验 `package` 模块仅兼容转发，Store、签名作者和 SDK 导出实际消费同一实现。纯格式14项测试移至根，运行 `cargo test -p extension-package --locked --offline`。包字节/签名domain未变；生产安装器仍未交付，见 [ADR 0021](../../docs/architecture/adr/0021-signed-package-authority.md)。

## 验证的问题

在不重编译宿主、不给作者内部宿主源码的情况下，能否独立构建两个签名 Wasm 包，拒绝完整包篡改与身份接管，并在安装/更新/卸载进程终止后恢复唯一的持久状态？

本实验使用单 JSON 容器和 Ed25519 完整签名，详见 [格式与固定向量](FORMAT.md)。解码文件只留在冻结内存中，不解包到主机路径。原始文件读取采用有界读取及 Linux O_NOFOLLOW/O_NONBLOCK，拒绝链接和非普通文件；校验后不重开下载路径。执行将验证所得模块交给既有 [Wasm 实验](../extension-wasm/README.md) 的解释器和合成能力 broker。

持久化采用 rusqlite/SQLite：完整包字节、package/publisher/key 绑定、当前安装引用、修订/世代、卸载 tombstone、幂等回执在同一事务提交。安装状态仍只有 disabled/uninstalled；新增持续元数据授权及策略会话恢复，见下文。没有后台自动启动或真实知识内容；显式恢复后的受限执行、私有状态成对迁移/回滚见下文。内容按完整摘要保存为 BLOB，不涉及第二个文件发布事务。原文件在安装后被替换，不改变保存字节。

## 环境与复现

Linux/WSL，本地文件系统；Rust 1.97.1、Python 3.11+、C 编译器（bundled SQLite）。两个独立 Cargo Workspace 均有锁文件。首次准备：

```sh
cargo fetch --manifest-path experiments/extension-package/Cargo.toml --locked
cargo fetch --manifest-path experiments/extension-package/author/Cargo.toml --locked
python3 experiments/extension-package/validate.py --output /tmp/qusor-package-validation.json
```

验证脚本执行两个 Workspace 的依赖图、fmt/test/clippy/build，先固定宿主二进制，再导出实验 SDK 到仓库外临时目录。导出内容只有权威包模块、extension-api、作者工具与合成 fixture；作者图不得引用仓库源码或带入宿主/SQLite/运行器。SDK 从当前权威实现生成，没有第二份手写协议。外部构建使用离线可用锁定依赖；工件和锁文件摘要留在结果中。这是发行工件可独立使用的实验，不是已发布稳定 SDK。

两版本均真实处理合成中文选区；坏包须在执行前拒绝。安装、更新、卸载分别在 before_transaction、after_content、before_commit、after_commit 退出进程（86，不执行析构），随后新进程重开 SQLite 并核验状态/回执。前三点不得留下新状态，提交后即使响应丢失也必须保留结果；相同操作可查询回执，不同输入重用 ID 则拒绝。

## 手工入口

构建后的 qusor-package-probe 支持：

```text
verify PACKAGE TRUSTED_PUBLIC_KEY_HEX
probe PACKAGE TRUSTED_PUBLIC_KEY_HEX
status STORE
install STORE OPERATION_ID EXPECTED_REVISION PACKAGE TRUSTED_PUBLIC_KEY_HEX [--crash POINT]
uninstall STORE OPERATION_ID EXPECTED_REVISION PACKAGE_ID [--crash POINT]
```

STORE 必须是用户指定的专用本地 0700 目录，父目录已存在；不存在时创建。首次安装及明确重装传 expected revision 0；更新/卸载使用 status 返回的当前修订。修订和世代跨重装保持递增，重装分配新安装 ID；旧命令不能因修订归零误中重装实例。重复命令返回历史回执，当前状态另查 status。

首次安装中的公钥是受信任宿主调用者的明确选择，不由包自报键自动决定；更新及卸载后重装都不能覆盖已有 package/publisher/key 绑定。没有用户信任 UI、现实身份认证、换钥/撤销服务或生产密钥管理。作者使用的常量 seed 是公开测试材料。

probe 是显式合成实验入口，每次新建内存宿主并授权合成选区，不激活持久安装。成功安装始终 disabled。包验证不保证 Wasm 完整结构/ABI 可启动；运行器继续验证模块并限制执行。此次不新增长驻多插件 supervisor，也不以直接 Probe 调用证明 worker 超时、OS 沙箱或完整恢复。

## 存储预算与边界

SQLite 使用 DELETE journal、synchronous=FULL，管理锁由 fs2 持有直到 Store 释放；重开时校验 schema、数据库和当前包签名/绑定。损坏状态关闭整个实验存储入口，保留文件供诊断，不提供自动修复/迁移。每个包遵守格式预算；读取 BLOB 也先限制长度。总内容 64 MiB、安装记录 1024、操作回执 8192；现有内容不会重复计预算，卸载不新增包/安装名额。达到回执上限会拒绝新操作，不自动清历史。

信任前提是宿主拥有的私有目录及本地账户可信；不防御账户所有者主动修改数据库/父目录。没有多主机文件系统、断电/存储设备故障验证或正式灾难恢复。内容暂不垃圾回收；卸载仅写 tombstone，不删除 Core 数据（此实验没有 Core 数据）。

异常退出协调的后续成绩见 [恢复协议](ORPHAN_RECOVERY.md)；下一项按相应门槛推进真实捕获存储/定位资格切片，再接生产适配器和入库。完整 EXT-B/P、NSH 阅读及五平台/渠道验收均未由本实验替代。


## 持续授权与新策略会话恢复

按 [ADR 0008](../../docs/architecture/adr/0008-durable-grant-recovery-probe.md) 扩展同一实验。管理库 schema 1 事务升级至 2（新增 grants 和 installation_library 表），已有包/安装/回执保留；这不是插件私有状态迁移。

```text
grant STORE OPERATION_ID EXPECTED_REVISION PACKAGE_ID LIBRARY DOCUMENT VERSION EXPIRES_AT [--crash POINT]
disable STORE OPERATION_ID EXPECTED_REVISION PACKAGE_ID [--crash POINT]
revoke STORE OPERATION_ID EXPECTED_REVISION PACKAGE_ID [--crash POINT]
resume STORE PACKAGE_ID LIBRARY DOCUMENT VERSION
check-session STORE SESSION_JSON LIBRARY DOCUMENT VERSION
```

EXPIRES_AT 为 Unix 秒。grant 只代表可信宿主管理调用对 `document.metadata.read` 的明确授权，绑定精确库/文档/版本、包摘要、安装 ID、世代和策略版本；不代表已有授权 UI。只有这项能力可持久化，selection.read 和 capture.propose 均拒绝。测试使用独立作者构建的元数据权限包，不靠提升原选区包权限启用。

resume 重验包与授权，并通过权威 extension-host 创建新策略会话，返回 `scope: policy-only`。这里没有实际读取 Core 元数据或调度 Wasm。CLI 调用结束即释放 Store；因此下一次 check-session 必须拒绝之前返回的 token。Rust 同 Store 回归另验证有效会话成功、范围错误/过期/撤权失效；不以永远拒绝的 CLI 替代正向策略测试。

安装 ID 首次授权时绑定库；禁用、撤权、更新均保留库归属，重新授权不能换库，重装新 ID 可重新绑定。禁用和撤权在此最小候选中均清授权及恢复意图，递增修订/世代。更新/卸载也清授权，只影响对应安装的会话。回执查询与当前授权有效性分开：授权已经过期，重放已提交操作仍返回原回执，但不能恢复执行。过期使用本地时钟，未验证恶意时钟回拨。

重开时检查包与授权绑定，未知策略版本、损坏记录或孤立授权拒绝打开；没有自动修复。验证脚本新增授权四个提交点，以及禁用/撤权提交前后的进程终止场景；精确数量和结果见 [证据](RESULTS.md)。


## 从持续授权恢复真实受限执行

[ADR 0009](../../docs/architecture/adr/0009-restored-wasm-execution-probe.md) 增加显式命令：

```text
execute STORE PACKAGE_ID LIBRARY DOCUMENT VERSION
```

新进程重开存储、复验当前包与有效授权、创建新会话后，消费该会话运行已存储的冻结 Wasm。它不调用会自行授予测试选区权限的 Probe::new，而将原 Host/Session 交给可信 from_authority 构造。执行过程每个 fuel slice、host call 与结果接受均复验 Host 和授权期限；guest 仍用公开 0.1 JSON 请求，响应必须精确匹配 capability/resource。

服务仅返回显式标记 synthetic 的库/文档/版本/标题元数据，不读取真实 Core 文档。普通作者的 metadata.wat 直接输出 broker 返回的实际字节；metadata-v2.wat 调用两次，验证更新后的模块确实生效。包安装、更新均不自动授予运行权限。API 层 Store.execute 消费一次会话，成功或 VM 失败后均不能复用旧 token。

复现仍为本页同一 validate.py；[执行恢复证据](evidence/linux-execution-recovery.json) 包含仓库外独立包、跨进程授权/执行/撤权/更新/卸载及签名故障包。受影响的旧运行器另通过 [原 Wasm 验证入口](../extension-wasm/validate.py)，[完整运行回归证据](evidence/linux-runtime-authority-regression.json) 保留原监督场景与新授权边界测试。

本命令是同步、有界、无外部副作用的单次调用；持单写者锁期间管理命令串行，不证明长驻 worker 或并行持久撤权。fuel 不抢占原生编译/宿主阻塞，原看门狗场景也不等于当前命令已集成长驻监督。正式知识写入、真实用户资源、NSH、私有迁移/回滚与发布渠道仍待验证。下一项具体设计见 [MIGRATION.md](MIGRATION.md)。


## 私有状态与包成对迁移/回滚

[ADR 0010](../../docs/architecture/adr/0010-paired-private-state-probe.md) 已实现 [MIGRATION.md](MIGRATION.md) 固定契约。初始化、升级、回滚和 private-status 的参数、结果、预算及崩溃点均在该文档，复现仍为同一 validate.py；新增的 [private_scenarios.py](private_scenarios.py) 验证仓库外签名作者和跨进程提交事实。

[本机证据](evidence/linux-private-lifecycle.json) 包含真实 Wasm 迁移保留偏好并添加新字段、预检拒绝能力、错误模块、普通 install 旁路拒绝、成对回滚、各切换点进程中止及损坏 pending 拒绝。操作返回 receipt 不自动表示升级成功，必须读取 outcome：succeeded、failed 或 interrupted。

管理库 schema 3 兼容既有 schema 1/2 的安装与回执。准备事务提交前校验失败不撤权；之后失败/中断保持旧配对且禁用。新进程不重跑 pending，先验证再终结为 interrupted。普通与私有操作 ID 共域，重装新安装 ID 不继承旧 head。没有正式私有存储 SDK、Core 知识内容、断电/移动或长驻 worker 结论；[运行回归](evidence/linux-private-runtime-regression.json) 仍不能替代双插件监督。


## 同一长驻宿主双插件监督

[ADR 0011](../../docs/architecture/adr/0011-persistent-supervisor-probe.md) 已连通父端真实授权、隔离 worker 与有界 IPC，完整 CLI/状态/故障说明见 [SUPERVISION.md](SUPERVISION.md)。同一 validate.py 现追加 [supervisor_scenarios.py](supervisor_scenarios.py)，从仓库外作者构建普通包，保持宿主字节不变，在同一 PID 交错正常/故障插件与管理命令。

[长驻证据](evidence/linux-persistent-supervisor.json) 包含 62 条管理命令、14 项监督场景：A 在 B 等待时完成、管理响应、循环/trap/异常退出、8/9 调用预算、取消/持久撤权、kill/wait 注入隔离和显式回收、真实迟到结果、真实加载失败上限及带待处理 worker 的确认关闭。本轮独立 Rust 回归另覆盖旧撤权回执不会取消新世代。已观测 worker 的 /proc fd 无管理库/锁，正常关闭后按 PID/starttime 核对不再存在；不外推成 OS 沙箱或异常崩溃恢复。

worker 无 Host/Store/授权；父端运行消息门只做共享策略检查，不执行插件预检，初次启动仍完成配对预检。原运行器另完整通过 [managed runtime 回归](evidence/linux-managed-runtime-regression.json)。父阻塞场景为异步不回复的受控故障，未验证任意同步 provider I/O 抢占。下一项仍是 supervisor 自身崩溃/残留协调，完整生命周期、正式 Core/阅读及平台/渠道尚未通过。


## 宿主异常退出与持久执行排他

[ADR 0012](../../docs/architecture/adr/0012-supervisor-crash-reconciliation.md) 将管理库升级至 schema 4，新增有摘要的 worker lease、spawn 意图、GuardReady 身份屏障和绑定后 Boot 前的当前授权门。完整协议、恢复结果及同内核无身份窗口的隔离限制见 [ORPHAN_RECOVERY.md](ORPHAN_RECOVERY.md)。

同一 validate.py 已追加 [orphan_scenarios.py](orphan_scenarios.py)。最新 [linux-supervisor-crash-recovery.json](evidence/linux-supervisor-crash-recovery.json) 通过59项Rust、543次CLI；保留既有40次事务进程终止，另实测10次宿主异常退出及14项恢复场景。已死确认、默认父死守卫和仍活残留实际 pidfd 清理分别验证，不互相代替；完整但身份不匹配记录不会杀掉无关进程。两种握手释放路径均检查当前授权，已过期则 boot_queued=false。

新增 rustix 1.1.4 仅在独立实验，SDK 导出仍排除该进程实现。未增加生产依赖，未实现正式 Core/NSH/完整平台交付；不将未知窗口隔离写成自动清理成功。下一项资格切片见 [计划](../../docs/product/next-phase.md)。


当前持久SQL与进程身份已迁唯一生产实现，原Store/identity模块是兼容消费者；完整来源、回归和仍未完成的安装/监督范围见 [C7结果](INSTALLATION_PRODUCTION_RESULTS.md)。历史各轮记录保留，不以旧摘要证明当前代码。
