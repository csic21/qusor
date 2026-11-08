# ADR 0022：无授权副作用的执行端口与 Wasmi 适配器

日期：2026-11-08。状态：采用；实际验证见下文。关联 C-08、F-EXT-01/03/05/06、EXT-B01/P04/P06/P11。

## 问题与决定

原实验的 managed/private 路径已实际服务捕获、私有迁移与监督，但其代码位于同时包含合成 Host、文件加载和故障注入的实验包。生产运行器不能据此自行创建授权、返回合成选文或读取插件提供的路径。

新增 `extension-runtime-api` 定义可信 `ExecutionLimits`、稳定错误/结果和 `CapabilityBroker`/`ModuleExecutor` 端口。`runtime-wasm` 实现真实 Wasmi 2.0.0 引擎；端口不含 Wasmi、Host、窗口或文件类型。实验 managed/private 模块只转发到生产实现，并真实通过 `ModuleExecutor` 执行；原 Probe 的合成服务、文件读取、控制/测试故障仍留实验，不成为生产 API。Probe 与生产路径共用适配器的具体 Wasmi 配置/实例限制 builder，避免资源策略分叉；这些具体类型仅位于适配器，未进入执行端口。

依赖为 runtime-wasm → extension-runtime-api/extension-api；Core/domain/application/ports 与 Host 策略不依赖运行器。旧实验错误名 `ProbeError`、结果名 `RunResult` 仅作为兼容别名；正式接口使用 `RuntimeError`、`ExecutionResult` 和不含 `block_host_call` 的 `ExecutionLimits`。

## 权限与数据边界

运行器只消费调用者已冻结的模块字节，不打开包或任意路径，不认证发布者，也不创建 Host/grant/session。每个 `qusor.call` 先验证 guest 输入/输出范围及调用预算，再把复制出的有界请求交给外部 broker；仅外部 broker 返回的数据可写回 guest。拒绝原样保留，不产生 fallback 服务数据。broker 的身份、授权、撤权竞争及返回结果最终接纳仍由父 Host/监督负责。

默认 typed 入口解析原有4KiB策略请求；raw 入口上限64KiB，供已有捕获等独立协议消费，不放大旧 wire 的预算。回复和 guest 最终 UTF-8 输出均不超过4KiB。不存在 WASI、环境文件/网络导入或隐式服务。

## 预算与取消的实际范围

保留已验证默认总 fuel 1,000,000、分片10,000、合作 deadline250ms和最多8次host调用。模块不超过256KiB，实例内存128KiB、单 memory/table，table元素16，递归128；禁止start，使用严格编译限制。可信宿主可通过端口配置 fuel/deadline/调用数，零预算及超出raw请求硬上限的配置在执行前拒绝。非整除fuel和total小于slice仍按剩余量夹取，不超额授予。

私有迁移/预检沿用原64KiB状态和共享迁移指令/期限预算。迁移拒绝全部导入；预检只连接必拒绝的业务broker，验证实际ABI及matching state，不激活run，不获得业务授权。

fuel只限制guest指令；合作deadline不能抢占正在编译的原生代码或阻塞的native broker。managed执行没有凭空增加Host撤权或切片取消语义；父进程仍需独立watchdog/worker回收与最终结果重核。旧合成Probe的取消/撤权实验也不能替代生产父监督。

## 验证与限制

新增直接端口测试实际调用trait对象，覆盖外部数据/拒绝、请求/输出范围在broker前拒绝、自定义/非法预算、非整除/短fuel和无授权私有迁移。原32项Wasm与4项作者测试继续运行，其中7项managed与5项private测试直接执行同一生产实现。20项受监护场景仍使用实验Probe验证合成Host、共享的Wasmi限制与外层watchdog，不能单独作为生产managed调用链的证明；生产managed/private的真实包Store/监督消费另由包回归核对。实际命令/源码指纹见 [运行记录](../../../adapters/runtime-wasm/README.md) 和 [Wasm结果](../../../experiments/extension-wasm/RESULTS.md)。

根验证不再错误地禁止整个Workspace存在已资格的Wasmi适配器，而逐一遍历domain/Core/API/application/ports/Host/包格式的实际normal/dev/build/target闭包，拒绝Wasmi、runtime-wasm或NSH倒灌。新增成员和声明/传递依赖单独登记。

本步不交付完整生产安装器、进程监督适配器、插件SDK发布或NSH阅读旅程；源码与本机运行成绩不代表五端分发或OS沙箱认证。


本机完整复验已通过35条检查命令、20项受监护场景、3项直接端口/32项原运行器/4项作者测试。16项生产源指纹与101项原实验/fixture摘要已记录，合作期限与原错误断言保持不变；详见 [结构化结果](../../../experiments/extension-wasm/evidence/linux-production-runtime.json)。串行单元测试只控制独立预算的CPU竞争，不关闭真实监护或多插件并发路径。
