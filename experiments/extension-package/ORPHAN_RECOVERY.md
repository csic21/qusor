# 宿主异常退出与残留 worker 协调

依据 [ADR 0012](../../docs/architecture/adr/0012-supervisor-crash-reconciliation.md)，关联 F-EXT-01/03/05/06、EXT-B03/B04、EXT-P04/P06/P13。本页是独立 Linux 实验协议；实际成绩见 [结果](RESULTS.md)，不签完整生命周期、平台或正式 Core 通过。

## 启动和记录

管理库 schema 4 增加每安装至多一个 worker_leases：安装/包摘要/世代、supervisor boot/run ID、父原生身份、spawn_attempted、可选 worker 身份和撤权 fence。record_hash 核对精确 canonical JSON 的 SHA-256，再验证内部绑定、UUID 和计数；偶然损坏不得被解释成内核重启。没有抵抗本地账户所有者重算摘要的承诺。

```text
授权和配对预检 → 提交 unstarted 意图
→ 提交 spawn_attempted → 创建仅持管道的 native worker
→ worker 安装父死守卫并复核父身份 → GuardReady
→ 父复核并可靠绑定真实身份 → 再次检查当前授权 → 才发送 Boot/模块
→ Boot Ready → 能力调用/结果继续经父策略门
```

GuardReady 解决刚 spawn 后读取 proc 命令行中间态的问题，不靠 sleep 猜启动时间。绑定完成后重新检查授权期限、世代及 Host 会话；授权在握手等待时过期，不交付模块。boot_queued 仅表示已排队，不能当成 worker 已消费；transport_ready 才表示收到 Boot。

Store 的写锁使用显式释放的 RAII guard，Connection 先关闭再解锁。活 Store 仍拒绝第二写者；释放所有权不依赖其他线程 fork/exec 暂时继承的锁描述符何时关闭。该行为有真实子进程继承模型回归，不通过 sleep/retry 放宽单写者检查。

## 恢复和隔离

新宿主先核对持久记录，按安装协调，不重放旧任务/旧会话：

| 持久事实 | 行为 |
|---|---|
| 确定从未尝试 spawn | 清 unstarted_intent；没有旧 native 执行者 |
| 同 kernel boot，已尝试 spawn 但未绑定身份 | 保留 unbound_spawn_uncertain，禁止替代或普通执行/身份变更；不猜 PID |
| 已绑定且进程缺失/已终止 | 确认后清账，按仍有效授权创建新会话 |
| 已绑定且仍活、身份严格匹配 | pidfd 固定对象并再次核验，发送信号并确认已终止后清账 |
| 身份错配、不可读、权限/终止结果不确定 | 只隔离该安装，保留记录；其他安装及管理仍可工作 |
| 完整有效记录来自此前 kernel boot | 原进程不跨该实例存活，可清账而不向当前 PID 发信号；真实系统重启尚未测 |

普通 resume/execute、grant、启停、更新/卸载和私有迁移受 lease 排他门约束；只有原父侧精确 run 控制可以管理自己的启动中 worker，broker 则必须在绑定后判权。撤权与 lease fence 同事务，提交后父崩溃不会恢复旧授权。

缺少 PID 的不确定窗口仍是限制：父死信号和未交付 Boot 可以证明没有旧 guest 权限，不能证明所有极早期、可能被 OS 暂停的 native 残留已清。实验不自动放行该安装，不声称完整 B2/M-1 通过。Terminated 可含 zombie，不等于新宿主已回收一个非子进程；测试器精确收养/waitpid 仅为测试卫生。

## 管理与确定性故障

原 supervise STORE 的 status 增加 worker_leases、orphaned，active 增加 guard_ready、identity_bound、transport_ready 和 boot_queued。显式恢复：

```json
{"request_id":"r1","op":"recover","package_id":"org.example.orphan-b"}
```

恢复成功才返回 recovered=true；false 时仍保留禁止替代，不把旧 PID 查找结果当作执行许可。恢复不自动重新授权或重演具有副作用的任务。

start 的可信 crash_point：before_lease、after_lease、after_spawn_intent、after_spawn、after_bind、after_boot。命中后输出 launch_checkpoint 再 exit(86)，不析构；after_boot 只表示排队，未声称 worker 已消费。revoke 另支持 after_revoke_commit，验证 fence 与授权提交事实。

可信故障 orphan_survivor 在正常守卫/Boot 检查后、Ready 前显式关闭父死信号并等待，专门验证父 SIGKILL 后仍活的目标实际经 pidfd 清理；默认 worker 不关闭守卫。hold_guard 使父收到 GuardReady 后暂不绑定/发 Boot，用来确定性验证 starting 期间取消、撤权和授权过期。

```json
{"request_id":"release","op":"release_guard","run_id":"由start返回"}
```

release_guard 只允许 hold_guard，和默认握手共用同一绑定/判权/Boot 顺序。授权到期返回 released=false、code=stale_authority，失败事件 boot_queued=false。该显式故障预算5秒，默认看门狗600毫秒不变，均有界；不是插件可申请的权限或产品开关。

## 复现与范围

同一验证入口从仓库外 SDK 构建两个普通签名来源身份、执行真实父 SIGKILL 和阶段 exit(86)，并保存所有管理消息：

```sh
python3 experiments/extension-package/validate.py --output /tmp/qusor-supervisor-crash-recovery.json
```

[orphan_scenarios.py](orphan_scenarios.py) 仅使当前测试器成为 subreaper，按自己创建/收养的确切 PID 清理并恢复原属性。生产进程身份逻辑使用 safe Rust pidfd/prctl，无裸 PID 清理退化。测试模拟身份错配与合法 UUID bitrot，不能将其描述为已经实际强制复用了内核 PID 或重启系统。

本机验证不覆盖任意同步父 provider I/O 抢占、OS 权限沙箱、断电、真实 Core 事务/知识保留/导出、NSH 或移动渠道。下一项转向有门槛的真实捕获存储/定位资格切片，不把 synthetic metadata 当作知识业务。
