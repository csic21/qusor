# ADR 0027：独立监督的原始字节执行通道

日期：2026-11-08。状态：已实施，Linux真实消费者九场景及完整兼容回归通过；正式应用装配与其他平台仍待完成。关联 C-08/C-09、F-EXT-01/03/05/06、EXT-B02/B03/B04；服务于最终正式资料库/阅读/笔记插件旅程，不扩大AI、云或平台范围。继承 [ADR 0025](0025-durable-installation-authority.md)、[ADR 0026](0026-durable-reader-native-dynamic-pdf.md) 和 [包生命周期](../plugin-lifecycle.md)。

## 问题

生产已具备持久安装、单一Host授权、无授权副作用的Wasmi及Linux身份原语，但通用异步进程装配仍留实验。原supervisor的Boot包含Fault，Call反序列化为固定策略请求，Result为Probe Report，父poll同步调用模拟服务；原样迁移会把实验协议与阻塞模型带入生产，也不能证明真实宿主服务阻塞时管理与watchdog继续工作。

## 决定与依赖

在现有runtime-process适配器实现私有版本化IPC、后台传输、独立控制线程及worker入口，不新增平行执行crate。只依赖extension-runtime-api、固定serde/serde_json及原Linux rustix；无Host、Store、Wasmi、Core、Probe或UI依赖。serde只编码有界私有传输；依赖清单显式登记，锁文件最小更新，不升级已有注册表包。

worker接收注入的ModuleExecutor，可信可执行程序在正常应用装配之前识别精确managed-worker参数。守卫成功并发送GuardReady后，父端核对同exe、OS身份和run绑定，外层先持久bind_worker_lease，再在同一管理gate内重核当前Host事实并排队Boot。无Boot前不运行模块；manifest不提供执行配置。受限Wasmi仍保持原燃料、调用次数、响应和期限约束。

传输Call/Reply是原始字节，策略/Surface解码与真正服务属于上层broker。64KiB原始请求预算不能被64KiB JSON帧错误压缩；编码上限按最坏字节数组膨胀加信封计算。Boot单独按256KiB模块预算计算，响应保持4KiB，配置、队列、标准错误及结果均有界。只允许一个待回复调用，校验连续序号、run、握手阶段和终态；拒绝重复/迟到帧与回复，不接受来宾自报身份。

独立控制线程创建并持有Child、处理启动/执行期限和真实回收；应用不poll或业务broker阻塞也不能暂停watchdog。读写线程与有限队列不得反向阻塞监督；背压失败关闭，不无限积压。控制线程不调用业务服务，provider I/O不进入Host/管理锁。worker终止不意味着已经开始的宿主I/O取消，知识写入继续使用既有版本、幂等回执、事务与撤权提交门。

## 生命周期与授权

DurableHost增加只返回单元值的validate_current，用于Boot与结果接纳前的新鲜生命周期检查，不生成能力permit，也不代替后续每次dispatch和可信Core版本核对。生产纯策略ManagedRun保留Prepared、BootReleased、ResultAccepted、Quarantined、Finished阶段；重复Boot/Result拒绝。send/accept闭包在同一外层管理gate即时执行，不能持有许可去异步执行。

ResultReceived、实际owned Child reap与持久lease清理是三个不同事实。只有真实回收或确切未spawn的证明后，才调用清理闭包并进入Finished；清理失败保持Quarantined，已确认停止事实保留以便可信重试。helper析构不清持久lease，不能因界面关闭或收到结果自动开放替代执行。当前journal负责安装级排他，controller负责OS事实；Host不直接依赖进程类型。

spawn前必须提交attempted。活controller持有Child且确认其reap，可以报告真实回收；崩溃重开时仅有attempted/workerNone的记录仍是未知状态，不能因没发Boot、PID消失或猜测父死守卫生效自动清理。已知身份恢复沿原pidfd规则，Terminated只证明内核对象退出，不冒称原父已reap。无法核对身份、回收或持久清理时保持隔离。

运行ID延续旧32位小写hex启动nonce加十进制序号格式。Store将原实验1—256限制扩为规范化非零u64，旧记录/JSON/hash/schema不变，拒绝前导零、符号及溢出。可信协调者每次启动生成新nonce，序号用checked_add且不可复用，耗尽时拒绝；该格式解析不自行制造随机性或提供授权。

## 真实消费者与验收

capture-core独立新增supervised消费者，不改原CLI。早期worker分支注入真实Wasmi，两个普通签名0.1/0.2包经同一持久Store/Host读取实际Core保留记录，替换直接同步VM执行。原始字节边界与Core业务场景分开记录，不将transport回显当成正式metadata服务。需要补真实阻塞broker期间watchdog/管理运行、撤权后迟到回复/结果拒绝、未放行Boot、8/9次调用、最坏编码、真实回收与SQLite清理失败保持排他。

旧包543场景、Wasm20旧监督场景和完整C3仍保留，不能以它们替代新生产controller消费者。本轮源码、构建、实际进程、崩溃与原生窗口证据分别记录；尚未迁移的native消费者与完整安装UI继续明确标为未完成。Linux同程序guard与Wasm边界不是通用OS沙箱，其他平台和分发渠道仍需独立验证。

## 本轮验证结果

[统一消费者资格](../../../experiments/capture-core/supervised-results.md) 已通过九场景，108份源码输入及可执行文件前后hash一致；真实Core8次调用在默认250ms预算内完成，第9次调用被预算拒绝。完整包/Wasm/C3兼容链同时通过。源码归档逐项hash已复核；这些成绩不扩大到正式插件工作区、其他平台或native释放ack。
