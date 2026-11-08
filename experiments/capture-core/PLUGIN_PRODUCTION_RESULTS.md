# C5：生产包验证与执行端口的真实捕获集成

日期：2026-11-08。关联 C-08、F-EXT-01/03/05/06、F-SRC-01、F-DATA-01。包格式和执行适配器分别见 [ADR 0021](../../docs/architecture/adr/0021-signed-package-authority.md)、[ADR 0022](../../docs/architecture/adr/0022-authority-free-wasm-runtime.md)。

## 实际实现与消费者

extension-package 是完整签名包格式的唯一实现；实验 Store、两个作者工具和仓库外 SDK 直接消费它。迁移前、生产源与 SDK 的 package.rs 摘要完全一致，未修改签名domain、格式或规范JSON。

extension-runtime-api 提供无授权副作用的执行端口，runtime-wasm 实现有界 managed/raw/private 执行。实验 managed/private 只做兼容转发，普通捕获与持久插件实验实际调用生产适配器。合成 Probe 留在实验，其 Wasmi 配置与限额共用生产builder；生产接口不含自授权Host、合成选文、文件读包、阻塞故障或进程退出。

这两项没有把 SQLite/Wasmi/NSH 依赖带进 Core 应用或纯包/协议/策略层。SDK 生成器只导出公开契约与签名格式，拒绝新旧命名的 Host、应用/端口、存储与执行适配器进入作者依赖图。

## 统一实际检查

主任务在两项源码/锁/策略冻结后串行运行：

- 根格式、全目标Clippy、88项Rust测试、以 `RUSTDOCFLAGS=-D warnings` 构建公开文档，均退出0。
- 42项架构守卫回归通过；根测试包含原71项、迁入的14项包格式测试及3项直接执行端口测试。
- 完整 C3 再次通过314条命令、286项CLI场景、8项native场景，包含普通签名插件、真实原件/快照、复合摘录、撤权/生命周期、崩溃与空库恢复。
- C3仓库外SDK作者构建与Clippy通过，55个包中没有Host、SQL、私有应用/执行器或NSH依赖；83项完整源码指纹与开始/结束及当前文件匹配。
- C4的26项Core与旧0.1协议/策略源逐项保持不变；0.2 Surface仍只是待实施设计，没有直接修改HOST_API破坏旧包。

显式 `RUST_TEST_THREADS=1` 记录于本轮统一检查。它不改变测试内部的真实撤权竞争、进程监督或故障场景。

## 分项证据与限制

[包回归](../extension-package/PRODUCTION_RESULTS.md) 通过593次检查、543项CLI场景、40次真实退出86和45项实验Rust测试。两套仓库外SDK分别53/55个包，未把安装Store导给作者。

[执行适配器](../../adapters/runtime-wasm/README.md) 的3项直接端口测试及旧32项运行器、4项作者测试通过。20个受监护CLI场景保留 legacy Probe 的Host/外层watchdog资格；它们不冒充生产managed通路，后者由包回归和上述C3实际消费者验证。并发CPU竞争导致deadline先于fuel的失败日志保留，最终串行精确断言通过；未放宽250ms期限或原断言。

[统一证据](evidence/linux-plugin-production-integration.json) 引用根测试/文档检查、完整C3、包与运行器报告的摘要，并核对其当前源。旧C4迁移记录属于其当时的源码/根依赖基线；当前新增模块后的通路以本轮完整C3为准。

生产持久安装器/监督装配、正式SDK发行、0.2公开Surface、独立reader/library/notes、输入法与完整保存回源旅程仍未完成。此轮实测为Linux via WSL；平台分发、移动渠道和完整安全沙箱仍分别验收，签名和内存解释执行不单独构成完整沙箱。
