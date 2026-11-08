# ADR 0012 宿主异常退出后的 worker 身份与执行排他恢复

日期：2026-11-08。状态：独立 Linux 实验增量，生产集成未通过。关联 C-08、F-EXT-01/03/05/06、EXT-B03/B04、EXT-P04/P06/P13 的本机子项。

## 问题

[ADR 0011](0011-persistent-supervisor-probe.md) 只验证长驻父进程仍活着时的监督。父进程被强制终止后，内存 Child/会话消失，旧 native worker 可能仍存活。重启只检查包和授权会遗漏旧执行者；按旧数字 PID 直接发送信号又可能误伤已复用该 PID 的其他进程。

## 决定

在同一独立包实验中新增 Linux 身份原语和 schema 4 的 worker_leases；不接入 Core。每安装至多一个未结束记录，绑定安装 ID、包摘要、世代、父 boot/run ID、父原生身份、是否已尝试 spawn、可选 worker 身份和可选撤权 fence。记录有有界 canonical 编码和 SHA-256 摘要；防止偶然损坏被解释为新内核启动等恢复证据，不声称防御能重算摘要的本地账户所有者。

启动顺序固定为：

1. 校验当前包/私有配对/授权，创建新内存会话并冻结模块。
2. 可靠提交尚未尝试 spawn 的意图。
3. 可靠提交 spawn_attempted，再创建只持管道的 native worker。
4. worker 在读取 Boot 前设置父死亡信号并复核父 PID/启动时间/UID/映像，然后发送 GuardReady。
5. 父收到该通道的 GuardReady 后才读取稳定身份，复核 PID、内核 boot、启动时间、UID、原生映像及精确命令行 nonce，可靠绑定记录。
6. 绑定提交后再次复核当前授权和期限，通过后才交付 Boot 和模块。后续能力和结果继续经过父端权威策略门。

GuardReady 是身份检查前的启动屏障，Ready 仍只表示接收 Boot；两者都不是产品层“插件已启用”。回归曾在 spawn 后立即核验处出现命令行身份不匹配；此顺序用明确握手规定核验时点，不通过重试猜时间或放松检查处理，也不据此推断其他启动断连均有同一根因。

## 崩溃协调与排他边界

普通 resume/execute、授权/启停/升级/卸载/私有迁移均受未协调记录阻止；原父进程仅可通过精确 run ID 的内部控制门管理它自己的 worker。未绑定时只允许可信父取消/撤权等控制，broker 的能力调用仍要求身份已绑定。撤权和记录 fence 在同一管理事务中提交，父在该点崩溃也不能让旧世代重新匹配。

新宿主先检查持久记录，再逐安装协调，绝不重放旧任务或旧会话：

- 意图证明从未尝试 spawn：清除未开始的意图。
- 同一内核 boot 已尝试 spawn，但没有可靠身份：保留 unbound_spawn_uncertain，禁止替代；其他安装和管理仍可工作。不猜 PID，不把未交付 Boot 等同于全部 native 资源已回收。
- 已绑定：打开 pidfd 固定内核进程对象，复核身份后才发送信号，观察已退出/缺失后清记录。错配、不可读、权限或终止不确定均保留隔离。
- 完整、有摘要且格式有效的记录来自此前内核 boot：旧进程不能跨该内核实例存活，可清账，不向当前 PID 发信号。本机测试不能冒充真实机器重启成绩。

未结束记录在确认终止后才删除。已终止可包含 zombie，不等于新宿主已 waitpid 回收一个非子进程。原生映像变化时，对仍存活的旧映像保守隔离；缺失/已退出确认不被当前映像匹配阻挡。

## Linux 原语与限制

使用固定 rustix 1.1.4 的 safe process/event API：pidfd_open、pidfd_send_signal、poll 和父死亡信号，无裸 PID 清理退化。依赖只在实验，生产 Workspace 不变。身份观察使用有界 procfs 读取；完整身份不能确认时不放行。

[Linux pidfd_open](https://man7.org/linux/man-pages/man2/pidfd_open.2.html) 与 [pidfd_send_signal](https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html) 说明通过句柄定位任务；[父死亡信号文档](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html) 明确其针对创建线程、设置前父已死不会补发，因此必须设置后再次核对父身份。信号机制不替代持久排他记录或 OS 权限沙箱。

极早期已尝试 spawn、尚未绑定的 native 不确定窗口仍需修复/更强平台监督方案；本轮不把它自动放行。没有任意同步父 provider I/O 抢占、真实 Core 提交/导出、正式私有存储 SDK、NSH、多平台/移动渠道成绩，也没有用普通 app 崩溃测试证明断电或真实系统重启。

## 验证

[包实验结果](../../../experiments/extension-package/RESULTS.md) 留存独立签名作者、启动各阶段 exit(86)、真实 SIGKILL 父进程、默认父死守卫和显式关闭守卫后的活残留 pidfd 清理、启动中取消/撤权、撤权提交后崩溃、完整但身份不匹配记录与合法 UUID bitrot 等证据。

测试器会成为仅当前进程生效的 subreaper，按确切已创建/收养 PID 回收，避免测试留下 zombie；不把测试器回收外推为新宿主可回收任意非子进程。普通 worker 守卫默认不变，orphan_survivor/hold_guard 仅是可信实验入口，不是插件能力。
