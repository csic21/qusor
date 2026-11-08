# 长驻双插件监督实验

依据 [ADR 0011](../../docs/architecture/adr/0011-persistent-supervisor-probe.md)，关联 F-EXT-01/03/05/06、EXT-B03/B04、EXT-P04/P06/P11。本页描述实验契约，实际通过范围以 [结果](RESULTS.md) 和机器证据为准；不代表正式宿主或完整 B2/M-1 通过。

后续 [ADR 0012](../../docs/architecture/adr/0012-supervisor-crash-reconciliation.md) 已扩展持久执行排他与异常退出协调；新增 GuardReady、恢复字段、hold_guard/release_guard 及限制见 [ORPHAN_RECOVERY.md](ORPHAN_RECOVERY.md)。本页保留原长驻监督协议，不把未知 native 清理算作已完成。

## 边界

同一长期运行的父进程持有 Store 管理锁、当前 Host 授权和 worker 句柄；每次调用启动一个 worker。worker 只读父进程交付的冻结 Wasm，通过管道请求能力，不打开安装数据库或下载路径，不自行建立 Host/授权。父 broker 提供显式 synthetic 元数据，不读取真实资料或写 Core。

每个管道在父侧固定绑定运行标识。guest 0.1 请求不包含可信身份，父侧根据安装、完整包摘要、世代和新会话判权，并要求精确能力/资源。取消、撤权或超时使旧结果失效；迟到事件不允许进入替代实例。

逻辑失效、任务终态与资源回收分别处理。若 kill/wait 失败或无法确认退出，保留 Child 句柄并隔离对应安装，禁止替代；显式 reap 返回确认后才可启动新实例。普通任务失败不等于数据被删除，其他安装和管理入口应保持可用。

## 本机管理协议

已安装并明确授权普通签名包后启动：

```text
qusor-package-probe supervise STORE
```

stdin/stdout 使用有界 JSONL；管理请求均包含 request_id 和 op，回复携带相同 request_id 和 ok。worker IPC 为独立管道，不与管理输出混用。父进程先发 ready 事件，含 PID、boot 和预算。worker 的 Ready 仅确认收到启动数据，不代表 Wasm 已激活；status 在实际合法能力调用或成功结果前仍显示 starting。

```json
{"request_id":"s1","op":"start","package_id":"org.example.supervised-a","library":"synthetic-library","document":"synthetic-document","version":"version-1"}
{"request_id":"s2","op":"status"}
{"request_id":"s3","op":"cancel","run_id":"从启动回复取得"}
{"request_id":"s4","op":"revoke","package_id":"org.example.supervised-b","operation_id":"explicit-revoke","expected_revision":2}
{"request_id":"s5","op":"reap","package_id":"org.example.supervised-b"}
{"request_id":"s6","op":"shutdown"}
```

start 立即回复 run_id 和 worker PID；完成经异步 completed/failed/quarantined 事件报告。status 单独列 active、quarantined、launch_failures、needs_repair 和持久安装状态；持久启用意图不能被展示为运行成功。过期消息记录为 stale_event。

revoke 使用真实 Store 管理事务清授权并递增世代，停止关联运行；取消只关闭当前任务，不自动扩大或恢复授权。reap 的 reaped=false 表示尚未确认，不能把发送 kill 当成回收完成。连续三次启动失败后持久清授权、标记需要修复，不自动无限重启；计数也包括真实模块/入口加载失败，不能只计 spawn 错误。历史撤权回执只描述旧事实，回放不会取消后来重新授权的世代。

shutdown 回复只表示接受请求，随后必须等待 shutdown_complete 的 confirmed=true。若 300 ms 内仍无法确认回收，则 confirmed=false 并非零退出；不能显示为正常关闭。实验另以 /proc PID/starttime 核对已观测 worker 正常关闭后消失，不把此检查外推为异常崩溃恢复。

上述字段是实验协议，不是产品 UI 文案。正式管理体验需将“任务失败”“正在停止”“资源回收未确认，需要修复”“已禁用”分别表达，并提供能成功执行的恢复动作。

## 可信故障注入

start 可附 fault 字段，仅供显式合成实验：block_host_call、crash、delay_result、kill_failure、wait_failure、launch_failure。模块及 fault 均由可信父测试入口确定，不由 guest 申请这些能力。

- block_host_call 在父侧保持单次调用未完成，父事件循环继续处理其他安装和管理消息，600 ms 过程看门狗终止该任务。
- delay_result 延迟投递真实已读的 worker 结果 1000 ms，使其晚于原运行失效与替代调用，用来验证旧通道拒绝。
- kill_failure/wait_failure 保留真实子进程句柄、模拟进程控制调用返回不确定；不声称已经触发真实 OS 权限/内核失败。
- launch_failure 验证有界失败和持久禁用；crash 验证实际 worker 异常退出。

来宾仍受现有模块、无 start、内存、表/栈、fuel、请求/响应和 host-call 次数限制。原生编译和回调不被 fuel 抢占，父看门狗负责进程级时限；这些机制不等于 OS 权限沙箱。当前父服务只有有界合成元数据；block_host_call 模拟异步服务不回复、worker 等待 IPC，未证明可抢占任意同步父 provider I/O。正式 I/O 必须另有异步/可取消边界。

## 验证与未覆盖

[supervisor_scenarios.py](supervisor_scenarios.py) 由原 validate.py 调用：仓库外作者构建多个普通签名包，保持同一宿主字节，在同一 supervisor PID 中交错正常/故障调用，测试管理响应、撤权、隔离/显式回收、迟到结果及启动失败上限。结果记录全部管理命令和异步消息，不以另外重启的成功宿主替代恢复。另验证合法 8 次调用成功、第 9 次按 host_call_limit 拒绝。活跃 worker 的 /proc fd 检查未见管理库/写锁句柄，不等于 OS 沙箱。

本机预算：最多 16 个活跃/隔离 worker、每次 supervisor 最多 256 个 run；worker 帧 64 KiB、启动模块帧至多 4×256 KiB+4096、管理输入 64 KiB、管理回复 1 MiB。输入命令与 IPC 都用有界通道；启动和回复经写线程发送，父循环不阻塞于子进程管道。运行消息的授权门只判策略，不执行 guest 预检；初次启动仍执行完整配对预检。

本轮未证明 supervisor 自身崩溃后的残留 worker 身份核验/清理、真实 Core 提交与撤权交错、实际知识保留/导出、正式私有存储 SDK、跨调用保留来宾内存、原生其他平台/移动/商店渠道或 NSH 阅读。无外部账号、网络副作用或用户数据上传。
