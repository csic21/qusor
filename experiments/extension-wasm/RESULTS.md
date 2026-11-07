# M-1 独立 Wasm 执行：本机验证记录

日期：2026-11-07。结论：本机实验用例通过，保留为候选执行方案；未批准生产运行时。
关联 F-EXT-01/03/05/06、EXT-B01 和 EXT-P04/P06/P11 的本机子集。完整门槛不因本记录通过而关闭，边界见 [ADR 0005](../../docs/architecture/adr/0005-wasm-runtime-probe.md)。

## 假设与门槛

独立作者可只根据实验协议构建 Wasm 包，同一个宿主可执行文件读取受控合成选区并执行不同版本的插件；越权、耗尽预算和故障必须得到明确拒绝或终止结果。真实知识、网络、凭据、NSH 与移动渠道不在本轮输入中。若权限、预算或中止用例失败，保留反例并停止生产集成，先修正边界或比较其他解释执行方案。

## 固定环境与输入

- 基座提交：b1d78b6d41ee966126755b754b6d2be565236112（本轮实验为该提交后的工作区实现）。
- 本机 WSL2，Ubuntu 26.04.1 LTS，Linux 6.18.33.2-microsoft-standard-WSL2，x86_64。
- AMD Ryzen 5 5600，12 个逻辑 CPU；WSL 可见 RAM 7.7 GiB。不是 CPU/RSS 性能基准。
- Rust 1.97.1，编译器提交 8bab26f4f68e0e26f0bb7960be334d5b520ea452，LLVM 22.1.6；debug/unoptimized 构建。
- Wasmi = 2.0.0，作者 wat = 1.228.0，sha2 = 0.10.9，serde = 1.0.228，serde_json = 1.0.149。
- 作者和执行器独立 Workspace/lockfile；锁定依赖准备完成后，全流程使用 --locked --offline。
- [fixtures](fixtures/) 为本仓库新写的 MIT 合成样例，共 18 个 WAT 模块。选区正文只在宿主中定义为“本地选区: Rust & Wasm.”，正常插件按 host call 返回的真实字节拼接前缀。
- 无 NSH/renderer、WebView、输入法、GPU、DPI、模拟器或移动真机交互；这些项目不适用本次实验。

| 字节对象 | SHA-256 |
|---|---|
| 两版运行前后的同一个宿主可执行文件 | 2074e6253e43d1be2de42fbc36c49394394ec18b35aa988e63efb6e4ed2ac40b |
| 执行器 Cargo.lock | 0795baf73e278bc6f8fa0c41947cb2c2fa6f030e3c11879428e9da63c7f9e7cf |
| 作者 Cargo.lock | 4b95ea7a95c21aed2973db70b9aafbc3e1a7fea1e25f1b377e7212f49b80ae67 |
| v1 module.wasm | be8fbc9dadc350fcd0593bd03512ba06a139e9fa11b33346f9625a00e7b2bf2d |
| v1 manifest.json | fd0165335ad903e301796eb0345306c94266c1cc5ea3fdd689dd98e64b149cae |
| v1 package.json | ce3ff4616813bdb3d213c69acdec7472b067b5ad1d929b05b433a272688e9a08 |
| v2 module.wasm | 3d764d39051773d4bbfe92f875b702d903182432bc515b43d5011aefcf64900f |
| v2 manifest.json | c4f74dc411513af6657960c1e739f5f7cf72f606d9cbc55f8e56cdb7397bd55a |
| v2 package.json | 4493938995e9ff31d04d608a6b6ae7fbdae6f4a3242bb6a7425ffc1db36c1bf9 |

宿主 hash 用于证明本次两版试验之间没有更换宿主，不保证不同机器/构建路径的二进制可重现。package.json 的摘要只绑定 module.wasm，未认证作者或 manifest。

## 复现与实际执行

从仓库根目录运行：

```sh
python3 experiments/extension-wasm/validate.py --output /tmp/qusor-wasm-results.json
```

[验证脚本](validate.py) 的完整运行退出 0。它先构建宿主并取 hash，再将作者工程及 fixtures 复制到仓库外临时目录重新编译，检查外部作者的 Cargo metadata 不含 QUSOR 路径，生成全部 18 个包。随后执行两个工程的格式、测试和 Clippy 检查、20 个监督进程场景，并核对宿主 hash。依赖与测试命令的标准输出、退出值、全部 fixture/包文件 hash 和场景 JSON 可由上述命令生成到指定临时文件；生成包和构建缓存不入库。

| 检查 | 实际结果 |
|---|---|
| 作者在仓库外构建全部包 | 通过；没有 QUSOR 源码依赖 |
| 执行器构建、格式、Clippy（-D warnings） | 通过 |
| 作者构建、格式、Clippy（-D warnings） | 通过 |
| 执行器集成测试 | 16 通过、0 失败、0 忽略 |
| 作者 CLI 集成测试 | 4 通过、0 失败、0 忽略 |
| 监督进程场景 | 20/20 与预期状态、退出码和成功输出相符 |
| 三个 Workspace 的依赖声明/已解析闭包 | 通过；生产 Workspace 没有 Wasmi、wat 或 sha2 |

依赖检查复用 [现有架构检查](../../scripts/check_architecture.py) 的 normal/dev/build/target 声明与已解析闭包验证，实验允许清单及理由在 [依赖策略](../../config/architecture.toml)。这不等于审计每个第三方库实现或未解析可选分支。

## 场景结果

下表均实际通过；拒绝场景以退出 1 及匹配错误码为通过，正常场景退出 0。执行与恢复均为无界面命令行，不表示产品安装、UI 交互或持久恢复通过。

| 输入/操作 | 实际结果与证据 |
|---|---|
| v1 正常包 | 输出“[excerpt] 本地选区: Rust & Wasm.”，1 次 host call |
| v2 正常包 | 输出“[excerpt v2] 本地选区: Rust & Wasm.”，1 次 host call；模块不同，宿主 hash 不变 |
| 跨库、同库其他选区、未授予 capture.propose | 三个独立模块均返回 permission_denied |
| 死循环 | 默认预算返回 fuel_exhausted；测试另用 5,000 总 fuel、100 每片，确认经过多片且不需要 host call |
| 超大 fuel 下的 deadline | 独立测试返回 deadline_exceeded；不是误把 fuel 耗尽当超时 |
| 已运行循环的取消、撤权 | 观察到执行切片后再操作；分别返回 cancelled、stale_context，不依赖来宾再次调用宿主 |
| 运行前取消、撤权 | 拒绝进入来宾，host call 计数为 0 |
| 初始内存超过 2 页 | instantiate_failed |
| 内存增长、表增长、深递归 | 三个模块均 vm_trap |
| 输入指针、写回指针越界 | 两个模块均 invalid_pointer |
| 请求超过 4 KiB、最终输出超过 4 KiB | 分别 host_buffer_limit、output_size_limit |
| 4 KiB 全 NUL 合法输出 | lib 与 CLI 均成功；JSON 转义膨胀后仍完整返回，未被错误报告上限截断 |
| 未知 ambient import、start 函数 | 分别 unknown_import、invalid_module |
| 连续两次合法调用 | 默认预算成功；测试将预算降至 1 后第二次返回 host_call_limit |
| 包/manifest/Wasm 超大、摘要篡改、坏 manifest/非 Wasm | loader 测试在编译前拒绝 |
| 入口路径穿越、绝对路径、未知字段/版本、非法摘要、目录或静态 symlink 文件 | loader 测试拒绝；不据此承诺并发替换时的路径安全 |
| 注入 60 秒 native host call 阻塞 | 默认监督入口约 1.004 秒返回 watchdog_timeout，退出 1；worker 经 kill/wait 回收 |
| 阻塞之后再次调用正常包 | 下一次监督调用成功返回 v1 输出；未测试同一长驻插件管理进程的恢复 |

各场景为本轮一次回归的功能证据，不发布 p50/p95 或性能 SLA。1000 ms 是看门狗触发阈值，实际终止还受 OS 调度、kill/wait 影响；未注入 OS 终止失败或不可中断内核故障。

## 修正及结论边界

审查与回归中修正了两处问题：取消与短同步读取/最终结果验收使用同一宿主锁排序，避免检查后再取消的窗口；监督结果缓冲按最坏 JSON 转义预留容量，并增加全 NUL 样例。取消在锁内检查点生效，已经先完成的读取或已验收结果不能追回。

本轮可以证明独立代码加载、现有资源授权链、受限 VM 执行、合作式取消/撤权及本机 worker 故障处理具备可复现证据。fuel、StoreLimits 与 start 控制的配置依据 [Wasmi 2.0.0 API](https://docs.rs/wasmi/2.0.0/wasmi/) 和 [固定版本源码](https://github.com/wasmi-labs/wasmi/tree/v2.0.0)，本记录的结果来自上述实测而非上游能力推断。

尚未验证：OS 权限沙箱、总进程 RSS/CPU 硬限制、解释器漏洞逃逸、并发替换文件的防护、发布者信任和签名、原子安装/升级回滚、长驻多插件宿主恢复、公开 SDK 发行、真实知识写入/卸载保留/导出、NSH、原生 Windows/macOS、Android/iOS 与移动分发。没有网络/账号/凭据 broker；没有产品业务事务副作用可回滚。

下一步可在既有协议上验证持久化安装与授权恢复、包升级/禁用的崩溃一致性，同时按平台分开验证运行限制；本次通过不自动把 Wasmi 选为生产方案，也不关闭 EXT-B01 或 EXT-P04/P06/P11。