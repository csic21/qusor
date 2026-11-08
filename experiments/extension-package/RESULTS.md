# 最新增量：纯签名包格式生产落位

2026-11-08：[ADR 0021](../../docs/architecture/adr/0021-signed-package-authority.md) 将纯格式按字节迁入生产权威源；旧实验仅转发，普通作者/Store及两份SDK直接消费同一实现。14项格式测试移入根、实验45项保持通过；543项CLI场景、40次CLI真实退出86、14项监督及14项异常恢复均已复验，见 [迁移结果](PRODUCTION_RESULTS.md)。生产安装器和稳定SDK仍未交付。以下59项等为原轮次覆盖分布，不代表当前所有测试仍在同一Workspace。

# 签名包与持久安装事务实验结果

更新：2026-11-08。最新本机结果为 **59 项 Rust、543 次 CLI 检查；另有同宿主14项监督及14项崩溃/恢复场景**，见文末 [异常恢复协议](ORPHAN_RECOVERY.md) 对应增量。本文按轮次保留原始结果，较早的20/84等数字仅属于对应历史轮次。

独立作者、完整签名、禁用安装、授权/受限执行、私有迁移和监督均有本机子项。生产安装器/稳定 SDK、未知未绑定 native 窗口自动修复、真实 Core/NSH/完整平台仍未完成。

## 假设、固定环境和证据

验证同一宿主能否消费仓库外作者工件，并在篡改/接管或安装进程终止时保持可判定状态。关联 F-EXT-01/03/05/06、EXT-B02/B04、EXT-P04/P08/P11/P13 的本机子项；[ADR 0007](../../docs/architecture/adr/0007-signed-package-persistence-probe.md) 规定实验范围。

完整命令、返回值、环境、源/SDK/包摘要与场景见 [JSON 证据](evidence/linux-validation.json)，复现入口为 [validate.py](validate.py)。

| 项目 | 实际记录 |
|---|---|
| 系统 | Ubuntu 26.04.1 LTS / WSL2，x86_64 Linux，本地文件系统 |
| Rust | rustc 1.97.1；debug/unoptimized |
| 容器/签名 | 实验格式 v1，固定规范 JSON；ed25519-dalek 2.2.0，SHA-256 0.10.9 |
| 持久化 | rusqlite 0.32.1 / bundled SQLite，DELETE journal、synchronous=FULL；fs2 0.4.3 单写者锁 |
| 输入 | 公开测试 seed、合成 Wasm/中文选区/静态资产，无用户资料 |
| 依赖 | [宿主锁文件](Cargo.lock)、[开发作者锁文件](author/Cargo.lock)，外部发行作者的实际锁摘要亦存于证据 |
| 宿主 SHA-256（作者构建前/所有场景后相同） | eef0e0f4381c4cbb1ad15808c5cd3e4e69361226c6e309a0d31bea5ad8582849 |
| 外部作者锁 SHA-256 | 5300b134b077485bfde8a94e1bca64b07bbec993759392400b76d4e0ef6e49a6 |

没有吞并实验到生产 Workspace。签名包模块只依赖权威 extension-api；SQLite/锁/既有 Wasm 复用属于实验宿主。SDK 导出剔除 store/Host/runtime，作者解析路径不回指 QUSOR 源码，也不带入 SQLite/wasmi/extension-host。

## 实际执行

| 场景 | 实际结果 | 证据界限 |
|---|---|---|
| 仓库外仅用导出格式/API/作者工件构建两个版本 | 成功；不同包和模块；两版本均处理宿主合成选区，输出前缀符合版本 | 同一宿主摘要不变；未发布稳定 SDK |
| 完整签名与显式键 | 正常包接受；manifest 权限、模块/资产（同时重算摘要）、MIME、签名改变均 InvalidSignature | 不把摘要校验当发布者认证 |
| 路径、重复/未知字段、编码歧义与预算 | 14 项格式测试通过；含遍历、大小写/祖先冲突、Windows 路径别名、弱公钥/伪签名、单文件/总量/数量 | 容器不解包，不证明任意归档解包器安全 |
| 包内自报陌生键 | 原信任键拒绝；明确传陌生包自己的正确键可单独验签 | 自签名不证明现实身份 |
| 持久发布者绑定 | 用陌生包自己的正确键更新，以及卸载后重装接管，均被拒绝 | 键轮换/撤销未实现 |
| 首装/更新持久化 | 内容、绑定、当前引用与回执同事务；重启仍 disabled；更新版本/修订/世代前进 | 没有权限继承或正式激活 |
| 操作回放/冲突 | 相同操作返回同一终态回执；相同 ID 的不同输入、过期修订及降级拒绝 | 当前状态使用 status 单独查询 |
| 卸载/重装 | tombstone 保留；重装新 ID，修订/世代不回退；旧修订的卸载/更新拒绝 | 无真实知识，因此未验知识保留/导出 |
| 冻结输入与存储完整性 | 验证后替换原文件不改变安装内容；重新打开校验内容/信任；损坏内容、缺内容、未知 schema 拒绝 | 不防御掌控本地账户的攻击者 |
| 单写者/文件边界 | 第二 Store 拒绝；链接输入/目录拒绝；合法大于 4 MiB 编码包可读取 | 本地 Linux 文件系统；未验网络文件系统 |
| 工程检查 | 两个 Workspace fmt/test/clippy 与依赖图通过；20 项 Rust 测试（14 包验证、6 存储） | 不代表正式业务测试 |

84 是 CLI 调用/检查次数，包含状态查询和回放，不是 84 项不同产品功能。细节保留在机器证据，不以测试数量代替完整 EXT 门槛。

## 进程终止与恢复

每个安装、更新、卸载操作都在下列 4 点由宿主进程直接 exit(86)，共 12 次；未走 Rust 析构清理。新进程重开同一目录，实际触发 SQLite 日志恢复与包复验。

| 中断点 | 首装 | 更新 | 卸载 |
|---|---|---|---|
| before_transaction | 无安装 | 原包/修订保持 | 仍为原安装 |
| after_content | 无安装，未提交内容回滚 | 原包/修订保持 | 仍为原安装 |
| before_commit | 无安装，引用/回执均回滚 | 原包/修订保持 | 仍为原安装 |
| after_commit（成功响应丢失） | 完整 disabled 安装 | 新包 disabled | 保持 uninstalled tombstone |

提交后重复相同操作得到原回执，不二次增加安装/修订；提交前重试则完成一次操作。卸载的 after_content 是事务开始后、tombstone 写入前的对齐注入点，不表示卸载复制内容。

本候选将小型完整包保存为 SQLite BLOB，因此没有第二个内容文件发布事务或孤立文件清理阶段。进程故障恢复不能外推断电、磁盘硬件异常、独立文件 CAS 原子发布或并发恶意账户替换。

## 回归、限制与下一步

本轮还通过生产目录/依赖守卫、42 项 Python 架构回归，以及受影响原 Wasm 实验的 16 项运行回归、fmt/clippy。生产 Rust 行为和依赖未改，不重复无关 Core 检查；本轮未执行远端 CI。

以下保持未验证：完整稳定 SDK/正式安装 UI、发布者身份与安全密钥管理、换钥/撤销、持续授权与新会话恢复、私有状态迁移/回滚、长驻多插件/worker 故障恢复、真实 Core 写入/卸载保留/导出恢复、官方阅读同权、NSH 阅读 Surface、原生 Windows/macOS/移动和分发渠道。probe 每次只显式运行合成能力，不从 disabled 安装自动取得执行权。

下一步延续 B2 的授权/执行恢复与私有状态迁移实验；B1 的 [NSH 结果](../nsh-reader/RESULTS.md) 已从缺 Cargo 缓存推进到真实编译缺 GStreamer 开发库，窗口尚未启动。通过对应门槛后才开始真实入库和正式功能包。


## 同日增量：持续授权与新策略会话恢复

依据 [ADR 0008](../../docs/architecture/adr/0008-durable-grant-recovery-probe.md)，在原有独立实验中新增持续元数据读取授权；原先 [linux-validation.json](evidence/linux-validation.json) 保留为禁用安装轮次证据。新结果单独存于 [linux-grant-recovery.json](evidence/linux-grant-recovery.json)，包含完整命令、输出、源/测试/工件摘要及环境。

持续授权绑定安装 ID、包摘要、世代、库/文档/版本、策略版本与到期时间。安装 ID 首次授权绑定一个库；禁用/撤权/更新保留库归属，重装才创建可重新绑定的新实例。临时选区和写入批准不持久化。恢复复验包与记录，再经过权威 extension-host 创建新策略会话；无第二套能力授权判定。

最终完整复现通过：29 项 Rust 测试（14 包格式、15 存储/恢复）、156 次 CLI 检查、20 次真实进程中止；两个实验 Workspace 的 fmt/test/clippy、独立与生产依赖图均通过。新增 72 次 CLI 检查及 8 次中止，其余为既有签名/安装回归。宿主 SHA-256 在仓库外构建前后均为 `4715b505ef44a72b4c6d38bf6a7e20b17240012c2e50ecf431994d65c92ce50a`，12 份源/测试/manifest/锁文件摘要与当前文件逐一核对一致。

本轮实际覆盖：

- 同一 Store 的合法会话通过真实 Host authorize/dispatch 策略检查；重开后旧 token 拒绝，新会话可用。
- 范围/版本不匹配、过期、临时能力持久化、损坏授权绑定拒绝；重新授权、禁用/撤权、更新、卸载后旧会话失效。
- 过期授权的已提交操作仍能查询原回执，但不能恢复会话；无重复提交或隐式续期。
- 授权在 before_transaction、after_content、before_commit、after_commit 四点进程终止；禁用/撤权在 before_commit/after_commit 终止。新进程依据提交事实恢复、重试只提交一次。
- schema 1 升至 2 保留安装/回执；同安装重复恢复替换旧会话，更新其他安装不打断当前会话。

CLI resume 返回 policy-only，会话随该 CLI Store 销毁；跨进程 check-session 的拒绝检查必须是 stale session，而非 JSON 解析错误。正向会话和隔离行为由 Rust 同实例回归另行证明。本轮没有从恢复授权实际调度 Wasm、读取真实文档或执行知识写入。

限制：本地时钟过期检查未验证恶意回拨；授权 UI、实际执行恢复、真实 Core 提交/撤权交错、私有状态迁移/回滚及长驻多插件仍待验证。原有完整生命周期与平台门槛保持。


## 同日增量：持久授权后的真实受限 Wasm 执行

[ADR 0009](../../docs/architecture/adr/0009-restored-wasm-execution-probe.md) 连接已保存完整包、持续授权、新会话和实际运行器。最终 [linux-execution-recovery.json](evidence/linux-execution-recovery.json) 通过 30 项包实验 Rust 测试（14 包格式、16 存储/恢复）、194 次 CLI 检查，包含既有 20 次进程终止；两个实验 Workspace 的 fmt/test/clippy、仓库外独立作者和依赖图均通过。38 次新增 CLI 检查用于实际执行路径，不是新增 38 个产品功能。

宿主 SHA-256 在独立构建前后均为 `cff1ca9c2fe786cef34c344f444b14e87e7273f74133b0e1bd3c0b661fd519b8`。20 份源/测试/fixture/manifest/锁文件摘要与当前文件逐一核对一致；未覆盖所有生产源码版本的签名构建证明。

| 场景 | 实际结果 | 限制 |
|---|---|---|
| 安装后无授权执行 | 拒绝 | manifest/签名不自动授予权限 |
| 授权进程结束后两次新进程执行 | 输出精确的合成库/文档/版本/标题 JSON，真实 host call 与 fuel 消耗 | 不证明真实资料存在或 Core 读取 |
| 资源/版本错误、撤权、卸载 | 执行前拒绝 | 本轮串行管理与执行 |
| 更新清授权，再授予后执行 | 未重新授权时拒绝；重新授权后 v2 模块发出两次 broker call | 不沿用 v1 代码或授权 |
| 独立签名的循环、未知 imports、超量内存、start、越权选区、错误文档、trap | 分别在预期运行/权限边界拒绝，未将解析错误当作越权成绩 | 没有新增 OS 权限沙箱 |
| 成功或 VM 失败后复用恢复 token | Rust 回归均拒绝 | 新调用需新恢复会话 |
| ExistingAuthority 安装/包摘要/会话不匹配及过期 | 运行器新增回归拒绝，不新增 grant | 可信宿主适配器入口，不是 guest 自报身份 |
| 响应精确资源 | 即使 Host 授予两资源，也不把当前响应错返回给另一资源 | 额外 adapter 范围检查已独立命中 |

原 Wasm 实验重新完整执行，见 [linux-runtime-authority-regression.json](evidence/linux-runtime-authority-regression.json)：20 项 Rust 运行回归（原 16 + 新 4）、20 个受监护场景、独立作者构建和 fmt/clippy/依赖图均通过。保留阻塞原生 host call 被看门狗终止、随后新受监护调用成功的成绩；84 份源/测试/锁/fixture/包摘要已核对。该旧监督入口每次启动新进程，**不能替代长驻双插件验收**。

生产架构守卫、42 项 Python 架构回归、本地文档目标和 diff 空白检查通过；没有改生产 Rust 行为或依赖，不重复无变更的 Core 测试。未执行远端 CI、NSH 新构建/窗口、原生其他平台或移动渠道。

尚待验证：私有状态与包成对迁移/回滚、长驻 supervisor、并行持久撤权/正式 Core 提交、真实阅读与知识保留/导出。下一项 [迁移设计](MIGRATION.md) 明确普通 install 更新旁路必须封堵，设计尚未计为实现。


## 同日增量：真实私有迁移与包/状态成对回滚

依据 [ADR 0010](../../docs/architecture/adr/0010-paired-private-state-probe.md)，[linux-private-lifecycle.json](evidence/linux-private-lifecycle.json) 为最终完整包复现：37 项 Rust 测试（14 包格式、16 既有存储/授权/执行、7 私有存储），432 次 CLI 检查，其中 40 次真实进程中止。相对前轮新增 238 次 CLI 检查与 20 次中止；次数包含准备、查询和重试，不代表同等数量的产品功能。

两个实验 Workspace fmt/test/clippy、依赖图及仓库外独立作者均通过。宿主在作者构建前后 SHA-256 同为 `4de86ad11e03176f32136771bae8d285555e89f77d46b08c2fc8f1b1bf3214af`，38 份源/测试/fixture/锁文件摘要已与当前文件核对。SDK 仍只提供公开格式/API和作者工具；迁移描述、模块和运行入口均由普通作者签名，不导出数据库/宿主实现。

| 场景 | 实际结果 | 证据范围 |
|---|---|---|
| v1→v2 私有偏好 | 作者 Wasm 复制 theme=night 原字节并追加换行和 layout=wide，新运行入口检查新版 Schema/后缀 | 宿主不转换字段，不读用户资料 |
| 迁移失败 | trap、总 fuel 耗尽、非法指针、超量输出、imports、错误来源 Schema 均命中预期运行错误；旧包/旧快照保留，授权失效 | 失败终态重试只回原回执 |
| 运行预检 | 错误 ABI、试图调用业务 broker 均失败 | 修正样例使拒绝业务调用真正命中该边界，不把 Schema 拒绝当能力拒绝 |
| 验证前拒绝 | 不接受的来源 Schema、未知协议在停止前拒绝，旧授权可用 | 与进入切换后失败明确区分 |
| 普通 install 旁路 | 有私有 head 时拒绝直接换包 | 私有包不能绕过迁移路径 |
| 成对回滚 | 重新验证历史包和快照，恢复原字节；修订/世代只前进、无旧授权 | 不执行反向转换，不包含 Core |
| 初始化中止 | before_prepare/after_snapshot/before_commit 未产生新 head；after_commit 完整初始化，重试不重复 | 4 次真实进程中止 |
| 升级/回滚中止 | 各 6 点：准备前保持原配对/授权；准备后切换前中断为 interrupted，旧配对且禁用；最终提交后完整新配对 | 12 次真实进程中止；同 ID 终态幂等 |
| 损坏 pending | 孤立所有者、非法摘要、错误目标包、陈旧世代均拒绝打开，并保留待修复记录 | 另 4 次 after_prepare 中止后实际修改合成数据库 |
| 重装/回放 | 新安装 ID 无旧私有 head，旧初始化回执不改变新实例 | 非自动继承私有设置 |
| 存储损坏/预算 | 缺 head、坏/缺 snapshot、孤立快照、64 MiB 快照总量、统一 8192 操作预算重开拒绝 | Rust 直接故障回归；不声称断电测试 |

另完整复现受影响 Wasm 运行器，见 [linux-private-runtime-regression.json](evidence/linux-private-runtime-regression.json)：25 项运行测试（既有 20 + 私有运行器 5）、4 项作者测试、20 个受监护场景及 fmt/clippy/独立依赖图通过；99 份源/测试/包/fixture 摘要与当前文件核对。新增测试明确覆盖 alloc 与 migrate 共用总预算，不能每个阶段重置。原看门狗场景仍为独立进程，不是长驻双插件。

新增依赖仅为实验 dev-only wat 1.228.0，直接编译正反例；生产 Workspace/依赖不变。Wasm 验证器在冻结宿主之前构建 all-targets，避免 dev WAT 与 Wasmi 的 wasmparser feature 合并在后续测试中重建 CLI；随后仍严格检查独立作者构建期间宿主字节不变，没有放宽 hash 门槛。

生产架构守卫、42 项 Python 架构回归、本地 Markdown 目标及 diff 空白检查通过。未重复无变更的生产 Rust 用例；没有远端 CI、提交/推送或 NSH/其他原生平台/移动新成绩。

剩余门槛：长驻 supervisor 双插件、kill/wait/旧回调故障与有界重试，真实 Core 提交/撤权、知识保留/完整导出，以及 NSH 阅读/输入与承诺平台/渠道。私有管理快照不等于正式知识，也没有正式插件私有存储 SDK；本机成功不能宣布完整生命周期或阶段 C/D 完成。


## 后续增量：同一长驻宿主中的双插件监督（2026-11-08）

按 [ADR 0011](../../docs/architecture/adr/0011-persistent-supervisor-probe.md) 执行最终 [linux-persistent-supervisor.json](evidence/linux-persistent-supervisor.json)：43 项包实验 Rust 测试（此前 37 + 新增 6 项真实多进程）、454 次 CLI 检查，包含此前 40 次管理进程中止；另在同一 supervisor PID 中执行 62 条 JSONL 管理命令和 14 项监督场景。454 包含准备/查询/回放，不代表454个独立业务验收，也不把其与管理命令数量合并成产品功能数。

宿主 SHA-256 在独立作者构建前后均为 `5541461e06845a92c4052f07299435daf58156fb4c60243ea4dd4ab8c71b0899`，46 份源/测试/fixture/锁文件摘要已与当前文件逐一核对。两个实验 Workspace 的 fmt/test/clippy、依赖图、仓库外仅公开 SDK 构建均通过，无生产依赖或 Workspace 变化。

| 场景 | 实际结果 | 范围 |
|---|---|---|
| B 的能力请求等待期间调用 A、查询管理状态 | 同一父 PID 内 A 先完成，B 后被 watchdog 终止；A 后续仍可调用 | 父异步故障不回复，不是抢占任意同步父 I/O |
| guest 循环/trap、越权文档、worker 异常退出 | 命中 fuel_exhausted/vm_trap/permission_denied/worker_exit，A 和管理仍工作 | worker 仅取冻结模块，父逐次判权 |
| 8 次和第 9 次调用 | 合法 8 次返回实际 broker 字节，第 9 次 host_call_limit | 补严 Ready+Call+Result+EOF 帧预算 |
| 取消与持久撤权 | 当前任务关闭；撤权递增世代、重启仍不可执行；A 不受影响 | 无真实 Core 提交 |
| kill/wait 控制错误 | 注入 worker_kill_failed/worker_wait_failed，真实 Child 保留并隔离；新 start 拒绝，reap 确认退出后才替代 | 不是实际 OS 权限/内核错误成绩 |
| 迟到旧结果 | 延迟投递真实已读结果，原任务超时；替代调用已完成后旧事件仍拒绝 | 不是构造一条假旧 JSON 代替真实通道 |
| 连续启动失败 | 注入 spawn 错误与真实无 run 导出包均三次后持久清授权，A 继续可用 | 无无限自动重启；无 supervisor 崩溃恢复结论 |
| 历史撤权回执 | Rust 多进程回归确认回放不取消重新授权后的新世代 | 查询历史事实不成为新撤权 |
| Ready/状态 | Ready 只表示接收启动数据，starting 不误报为激活；运行消息门不执行 guest 预检 | 初次启动仍完整预检 |
| 正常关闭 | A 仍有待处理 worker 时请求关闭；shutdown_complete confirmed=true，再核对已观测41个 PID/starttime 对应实例消失 | 接收关闭请求不等于已回收，未知则非零退出 |
| 文件描述符 | 存活 blocked worker 只见 stdin/stdout 管道和 /dev/null，未继承管理数据库/写锁 | Linux/WSL /proc 本机证据，不是 OS 沙箱 |

受影响 Wasm 验证器另完整运行，见 [linux-managed-runtime-regression.json](evidence/linux-managed-runtime-regression.json)：31 项运行器回归（此前 25 + managed runtime 6）、4 项作者测试、20 个旧受监护场景均通过；101 份源/测试/包/fixture 摘要与当前文件核对。旧单次 supervisor 的结果继续保留，不与新的同 PID 多插件场景混淆。

未新增依赖，生产架构守卫、42 项 Python 架构回归、本地文档目标与 diff 空白检查通过。没有远端 CI、提交/推送、NSH 构建/窗口、其他原生平台或移动渠道新成绩。

仍未完成：supervisor 自身异常退出后的残留 worker 身份核验和协调、任意同步父 provider I/O 的可取消执行边界、真实 Core 提交/撤权与知识保留/导出、正式私有存储 SDK、完整安全安装 UI/生产运行器、NSH 阅读和平台/渠道验收。当前长驻的是管理进程、worker 按调用新建；不能宣称跨调用保留来宾内存或完整 B2/M-1 已通过。


## 后续增量：宿主异常退出后的身份与执行排他（2026-11-08）

[ADR 0012](../../docs/architecture/adr/0012-supervisor-crash-reconciliation.md) 的正式完整复现见 [linux-supervisor-crash-recovery.json](evidence/linux-supervisor-crash-recovery.json)。本轮59项Rust（4身份/锁单元、14包格式、8私有存储、1身份多进程、16既有存储/授权/执行、16监督多进程）均通过，两个 Workspace fmt/test/clippy、完整依赖图及仓库外 SDK 作者构建通过。

543次CLI检查含既有40次管理事务中止，不是543个不同功能。另记录同一正常长驻宿主14项场景/本次72条管理命令，以及异常恢复14项场景/本次97条管理命令、10次真实父进程异常退出（阶段exit86和SIGKILL）。管理查询数量随调度变化，不能作为吞吐或性能目标。50份源/测试/fixture/manifest/锁文件摘要与当前文件核对；作者构建前后宿主SHA-256均为 `15d8bcb0a8666508e0a5c6df18ffd28cfe26b272ef1935019a534b528a061065`。

| 场景 | 实际结果 | 不作的外推 |
|---|---|---|
| 启动6阶段中断 | before_lease/after_lease证明未创建；after_spawn_intent/after_spawn保持无身份隔离；after_bind/after_boot经确认后允许新会话 | 不把stdout checkpoint PID作为生产猜杀依据；after_boot只排队 |
| 默认守卫+父SIGKILL | 新宿主介入前，测试器已确认worker由SIGKILL结束并回收 | 与新宿主主动清理明确分开 |
| 显式存活残留 | orphan_survivor只在测试关闭守卫；父SIGKILL后确认仍活，新宿主实际经pidfd清理，再启动新run | 不是只测已死/Absent分支 |
| starting取消/撤权 | GuardReady已到、身份未绑定/Boot未发时，可关闭精确run；撤权持久且无后续执行 | 普通Store不能获得此父控制旁路 |
| 授权等待过期 | hold_guard确定性暂停，真实短TTL到期后release：stale_authority且boot_queued=false；有效释放实际调用broker一次 | 测试故障预算5秒，默认600ms未放宽 |
| 撤权提交后父崩溃 | fence和新世代同事务，恢复清lease但不恢复旧grant | 仍无Core提交事务 |
| 完整但身份错配 | 合成管理记录指向另一个仍活的sleep进程，保持隔离；目标未被发信号，A仍完成；恢复原测试记录后再清真实worker | 未实际强制复用内核PID |
| 合法UUID bitrot | 不重算hash的损坏记录在任何进程操作前拒绝；原存活worker仍活 | hash不是对本地账户所有者的认证 |
| 锁生命周期 | 活Store拒绝第二写者；Connection先关再显式unlock，继承clone的测试子仍活也不延长已结束Store租约 | 未验证NFS/断电 |

启动回归曾报告 `managed worker command mismatch`；现规定GuardReady后才进行身份绑定，而非放松检查或sleep猜时机。另一次启动断连已捕获真实stderr `store has another writer`，增加显式解锁及可复现的描述符继承模型；未直接追踪当次具体持锁PID，不将模型解释冒充该次完整因果定位。最终整套与独立CLI重验通过。

新依赖为实验 rustix=1.1.4 的safe process/event API。依赖策略覆盖normal/dev/build/平台声明及解析闭包；缺失的windows-link/windows-sys公开锁定缓存已补齐，仅支持元数据检查，不是Windows构建成绩。发行SDK仍仅公开包格式/API/作者工具，不包含rustix、Host或SQLite。生产Workspace与依赖不变，原Wasm运行器源未修改，不重复无变更的31项完整运行器回归；本轮实际包调用/监督/恢复仍走真实受限运行器。

根架构守卫、42项Python架构回归、本地Markdown目标和diff空白检查通过。没有远端CI、提交/推送、真实用户文件/账号或云上传。NSH原生前置再次只读检查，6个库组仍缺，见 [本日记录](../nsh-reader/evidence/linux-20261108-preflight.json)，无新编译/窗口/阅读成绩。

仍未完成：同内核已尝试spawn而无身份的native不确定窗口自动修复（当前隔离）、真实系统重启/断电、任意同步父provider I/O抢占、OS权限沙箱、正式私有存储SDK、真实Core事务/来源定位/知识保留/导出，以及NSH和其他平台/渠道。下一项按 [计划](../../docs/product/next-phase.md) 推进真实捕获资格切片；现有synthetic metadata和包BLOB不证明该切片。
