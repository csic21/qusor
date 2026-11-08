# C7：持久安装权威源与统一 Host 恢复

日期：2026-11-08。关联 C-08/C-09、F-EXT-01/03/05/06、EXT-B02/B03/B04、F-DATA-01 和 [ADR 0025](../../docs/architecture/adr/0025-durable-installation-authority.md)。这是生产持久内核与实际消费者的落位结果，不等于完整生产安装器、监督器或正式功能插件完成。

## 实际改变

`extension-store`成为 schema1—4、包内容/发布者绑定、安装/授权材料、私有成对状态和worker lease的唯一SQL实现。实验Store组合该适配器，保留原CLI、会话和合成服务；生产不构造Host/Session、不绑定具体VM、不含故障退出。默认缺少验证器/私有执行器时失败关闭，VM回调在SQL事务外，Current读取不执行guest。连接与事务不公开。

`extension-host::durable`接通真实持久源，在一个库的同一Host恢复多个普通安装，并按选定API校验授权语义。每次dispatch重读当前包/键、修订/世代、有效期、范围和排他状态；坏数据、撤权、更新和重装关闭旧上下文，不靠成功回调清理才保证安全。精确文档版本保留在持久grant中，可信Core当前版本必须匹配，不能把guest自报版本当来源事实。

两个普通签名Wasm包通过同一个Host，在0.1/0.2会话读取真实Core保留文本记录的metadata；安装不自动授权、撤权互不串扰、更新后禁用/清授权、重开新会话和知识保留均实测。这里使用受控文本fixture的真实Core存储，不使用synthetic metadata响应，也不冒充PDF解析或原生产品工作区。版本解析与handler读取在测试独占的一致上下文中完成，不将它称为完整应用并发提交验证。

`runtime-process`实际接管原Linux进程身份、父死守卫和pidfd确认清理；实验仅转发，DTO归`extension-runtime-api`。保留同executable/精确命令/启动时间/内核boot等检查，不降级为裸PID kill；其他平台返回Unsupported。该模块不包含worker IPC/controller/watchdog或Host授权。

## 本机实际验证

Linux via WSL、Rust1.97.1、锁定缓存离线执行。完整 [统一机器记录](../capture-core/evidence/linux-installation-production.json) 保存命令、源码和工件摘要；[167份合格源码快照](../capture-core/evidence/installation-production-sources-c7.zip) 已逐文件核对，防止后续共享文件刷新后失去本轮源记录。

| 检查 | 实际结果 |
|---|---|
| 根工作区 | fmt、all-target Clippy、121项Rust、无警告rustdoc通过 |
| 架构 | 守卫与42项Python回归通过；normal/dev/build/目标平台及解析闭包检查 |
| 包实验 | 46项Rust、543个CLI场景、593项检查；含40次真实exit86事务中止、原长驻监督与异常恢复 |
| Wasm | 32项运行器、4项作者、3项生产执行端口、20项旧受监护场景通过 |
| 完整C3 | 42项Rust、314命令、286 CLI场景、8个native案例通过；含新增统一Host/Core真实消费者 |
| 迁移后原生窗口 | 重建普通签名包/宿主、3项native测试、Clippy、3次真实GTK/XTest选文/非空笔记、单段/跨行保存和完整空库恢复通过 |

原包Rust测试49→46是4项底层测试迁到根成员（writer锁1项、进程身份3项），并新增1项兼容重放测试，未删故障覆盖。根新增持久服务/Host测试另计入121。一个重要兼容错误已由真实监督回归发现并修复：历史install/grant/revoke回执重放不能误清后来建立的新session，wrapper改为按当前持久绑定清理，而不是收到任意成功回执都撤会话。

精确燃料测试设置 `RUST_TEST_THREADS=1`，保留250ms期限；测试内部的真实并发/撤权/监督没有改成串行实现。20项旧受监护场景包含Legacy Probe，不全部记作生产managed supervisor。

本次 [包详情](evidence/linux-installation-production-package.json)、[Wasm详情](evidence/linux-installation-production-wasm.json)、[固定C3详情](../capture-core/evidence/linux-compound-pdf-installation-c7.json)、[根输出](../capture-core/evidence/linux-installation-production-root.log) 分别归档。新增registry记录按既有C6包与Core作者锁逐项核对，未知漂移为空，见 [不可变基线](evidence/installation-production-baselines/review.json)。

## 原生消费者与限制

[迁移后原生复验](../nsh-reader/evidence/installation-production/RESULTS.md) 使用新二进制和全新数据库，三条摘录/笔记（含protocol2两段）经过同一普通guest及原生明确保存，退出后查询、完整导出/空库恢复及三次精确解析一致。最终binary `1fdf5ded1b069b5a558e16ad823be6afeb0e70e378f5152a1f1bf6aae0b82130`；普通包仍为`895b6b116c114dba2f12a67ae6e07b38f37fc99f152e070d86964ed65fcf0dff`。100源/6工件/21证据指纹匹配；ELF无未解析库、无WebKit/JavaScriptCore/libsoup。

C6原生证据没有覆盖，旧工件备份；补非Unix unused-variable问题前后的两次真实成功run分开保留。当前键盘证据为Xvfb真实GTK/XTest，不证明当前桌面焦点、物理键盘、中文IME、触控或无障碍。NSH mount缺少释放ack，Closing仍保留到实际进程退出。

持续权限profile仍仅旧metadata精确授权，不等于reader.surface/selection.read/capture.propose通用持久恢复。原生消费者仍是资格relay，页图仍由固定fixture生成；未交付任意本地PDF动态导入/页图、正式library/reader/notes、安装/信任UI、完整异步进程监督、EPUB或其他平台。Core知识与包管理存储分开，原始文件未上传；后续继续完成原定阅读旅程。
