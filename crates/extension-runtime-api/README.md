# 执行端口与预算契约

`ExecutionLimits`、`RuntimeError`/`RuntimeErrorKind`、`ExecutionResult` 与 `ModuleExecutor`/`CapabilityBroker` 描述无授权副作用的执行边界。模块必须来自可信验证器冻结的字节；broker和最终结果接纳由外部Host负责。本crate不依赖Wasmi、Host、包安装器、文件系统或NSH。

生产 [Wasmi适配器](../../adapters/runtime-wasm/README.md) 实际实现端口，捕获/包实验经兼容模块消费；不是只有空trait。默认限额保留原资格值，正式配置没有实验阻塞故障字段。错误静态code保留兼容性，kind提供稳定分类，不传播guest异常文本。

合作deadline无法抢占阻塞native调用/编译；外部进程watchdog不可省略。详见 [ADR 0022](../../docs/architecture/adr/0022-authority-free-wasm-runtime.md)。这不是插件作者SDK或授权证明。
