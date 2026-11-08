# 生产 process controller / Core 消费者

状态：production controller/消费者已实施，统一冻结后的九场景本机资格通过。机器证据见 [linux-supervised-controller.json](evidence/linux-supervised-controller.json)：108份输入前后hash一致，消费者二进制前后hash一致，构建、Clippy及实际执行均exit0。关联C9持续授权、进程执行和原生优先边界。

`tests/supervised_consumer.rs`是独立`harness=false`测试可执行文件，现有普通main/CLI未变。`--managed-worker`在任何Core/Store初始化前调用production `worker_main(&WasmiRuntime)`；只有明确的可信实验profile会在guard前等待，供真实supervisor崩溃窗口验证。该profile没有进入生产Boot协议。

正常链从production Store签名包冻结模块，`ManagedRun::prepare`后持久prepare/mark，再交production Controller；Guarded对应OS身份持久bind后，`ManagedRun::release_boot`在同一授权门重查fresh source再立即排入Boot。每次metadata Call查询真实Core版本、走DurableHost.dispatch；ResultReceived经ManagedRun fresh accept；只有实际Controller Reaped事件后才record_reaped/finish，并在清lease成功后forget。消费者没有复制Controller、worker、协议解析或OS清理实现。

本轮统一资格已实际执行：两个普通签名0.1/0.2包读取真实Core，8/9次call预算，最坏64KiB全255 request与4096字节response传输；Boot前撤权不发Boot、收到结果后撤权拒接受；真实阻塞broker时独立watchdog及管理仍工作；非终止Wasm父deadline；真实SQLite写锁导致持久cleanup失败保持排他，释放锁后重试；实际Core另一来源版本绑定到Document1时，真实Core版本解析使Host拒metadata；真实supervisor在Spawned但未bind时exit(73)，新Store保持attempted/workerNone并拒替代。测试observer持有真实OS identity，仅用production pidfd cleanup回收测试孤儿，不将Terminated冒充原父reap，也不自动删除未知lease。

这些是Linux进程生命周期和本地Core资格，不是安全沙箱、NSH界面资格、移动平台支持或业务阅读器发布。

正常metadata0.1/0.2各1call、8次真实Core及9次预算拒绝、raw64KiB均使用默认250ms VM deadline。去除同一次Source读取内的重复签名验证后，一轮无重编诊断的metadata callback约13–15ms，8call从控制器启动到完成约222ms；这不是实时保证。快速transport-only 8/9call另独立验证预算，不伪造Core metadata。初期默认250ms超时与cleanup退出竞态失败日志保留在临时诊断路径；本轮机器资格已保留正式stdout/stderr、源与二进制hash及实际命令。

本轮复验命令为 `python3 experiments/capture-core/supervised_validate.py`。8次真实Core调用使用默认250ms期限，本次从启动到完成221ms；第9次以host_call_limit拒绝，broker只执行8次。阻塞broker期间管理操作可提交，独立watchdog317ms完成；非终止Wasm138ms超时。数值是本次观察，不能作为实时保证。旧包/Wasm/完整C3兼容链也已通过，分别见 [包回归](../extension-package/evidence/linux-native-workspace-package.json)、[Wasm回归](../extension-package/evidence/linux-native-workspace-wasm.json) 和 [C3回归](evidence/linux-native-workspace-compound.json)。

统一资格的108份源码已按机器证据逐项核对并封存，见 [源码归档校验](evidence/supervised-controller-source-archive.json)。归档位于本机实验target目录，逐项解压hash与资格输入一致；该备份不代表已提交或发布。
