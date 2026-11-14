2026-11-09 增量：macOS profile 使用 libproc 的微秒创建时间、内核 boot session、UID 与 image inode 记录观测；父进程守卫通过受信线程检测重父化/原父身份消失，仅退出自身。终止只允许仍持有的 `Child`，恢复记录的裸 PID 不发送信号；未知恢复进程继续隔离。四组真实控制器场景与当前身份/拒绝伪造的测试在 macOS 通过。父死延迟与完整 Core/GUI 另行留证；Windows、移动及 Web 执行 profile 尚未实现。

# runtime-process：进程身份、受控传输与独立监督

> 职责边界（[ADR 0055](../../docs/architecture/adr/0055-lifecycle-and-port-decoupling.md)）：`runtime-process` 仅用于桌面原生 helper 与外部 MCP 进程监督，不承担普通插件生命周期；普通跨平台插件统一走进程内 `runtime-wasm` 沙箱。

关联 F-EXT-01/03/05/06。Linux `/proc` 身份、父死守卫与 pidfd 清理从已有监督实验迁入，实验兼容模块转发同一实现；现已增加有界原始字节传输、独立controller/watchdog及注入执行器的worker入口；不包含Host授权、数据库、具体guest执行器或完整产品supervisor。

`extension-runtime-api::ProcessIdentity` 只是可持久化观测，不能直接授权 signal。回收先打开 pidfd 固定内核对象，再核对启动时间、kernel boot、UID、可执行文件设备/inode及带 nonce 的 managed-worker 精确命令。不退回裸 PID kill。已经退出的对象可以先于旧 executable 不匹配得到退出确认；`Terminated` 仍可能是待父进程 reap 的 zombie。

当前受控 worker profile 要求父子使用同一可信可执行文件，参数为 `--managed-worker <parent-pid> <parent-start> <run-id>`。`guard_worker` 必须在 worker 主线程、读取 Boot 或运行 guest 之前调用。跨 executable worker 不由此实现允许；以后扩展需显式身份协议。

Linux 以外的平台入口返回 `io::ErrorKind::Unsupported`；Linux rustix 依赖限制在目标声明内。源码可在其他平台编译不等于原生身份/清理已通过。

局部检查：

```sh
cargo test -p runtime-process --locked --offline
cargo clippy -p runtime-process --all-targets --locked --offline -- -D warnings
cargo test --manifest-path experiments/extension-package/Cargo.toml --test process_identity --locked --offline
```

三项迁入 unit tests 覆盖 `/proc/stat` 解析、旧 image 的 exited/absent 区别、nonce 与非 worker 拒绝；实验真实进程测试经兼容模块验证外部 image 不被杀、pidfd 退出观测和真实父死守卫。完整双插件/异常恢复由实验监督回归单独验证，不能把这四项检查当作完整生产安装/监督或 OS 沙箱通过。

## 独立控制器

`Controller`通过有界队列在专用控制线程创建并持有Child；应用poll或业务broker阻塞时，watchdog仍独立运行。`start`只启动守卫，收到`ProcessEvent::Guarded`后由上层持久绑定身份并经同一Host核对当前授权，才能调用`release_boot`。`worker_main`注入`ModuleExecutor`，适配器自身不依赖Wasmi或Core。

Call/Reply传原始字节，业务协议由外层解析。请求64KiB、响应4KiB、Boot模块256KiB，JSON信封按最坏膨胀独立设限。Result仅在输出干净结束且子进程正常退出后接纳；真实回收事件与持久lease清理仍是不同步骤。`forget`不重置启动nonce下的运行序号，不能以清记录复用run_id。

统一消费者资格见 [九场景结果](../../experiments/capture-core/supervised-results.md)：真实普通签名0.1/0.2包、Core查询、撤权、独立超时、SQLite清理失败及未绑定身份的父进程崩溃均留证。它不证明NSH、其他平台、完整安装UI或安全沙箱已完成。

2026-11-09，macOS 另通过五类真实控制器场景。父进程跳过析构退出后，已消费 stdin 并进入五秒测试睡眠的子进程在本机约 30 ms 结束继承管道；这是实际父死终止的观察，不是所有设备的最大延迟保证。当前 Mac 原生阅读的具体范围见 [本机记录](../../experiments/nsh-reader/evidence/macos-native-reader-20261109/README.md)。
