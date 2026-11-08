# Wasmi 执行适配器

通过 [extension-runtime-api](../../crates/extension-runtime-api/README.md) 实际实现 `ModuleExecutor`。生产managed/raw/private路径共用Wasmi配置，不创建Host/grant、不读包路径、不含合成选文或进程退出。typed入口只解析旧策略wire；实际授权/数据来自父broker。私有迁移无导入，预检业务调用必拒绝。

```sh
cargo test -p runtime-wasm --locked --offline
cargo clippy -p extension-runtime-api -p runtime-wasm --all-targets --locked --offline -- -D warnings
python3 experiments/extension-wasm/validate.py --output experiments/extension-wasm/target/production-runtime-regression.json
```

[直接端口测试](tests/port.rs) 验证真实VM、外部broker、范围/预算及私有状态，不以mock executor替代。旧 [Wasm实验](../../experiments/extension-wasm/README.md) 的32项Rust测试中7项managed/5项private直接消费生产实现。20个受监护场景仍验证实验Probe的Host/撤权/外层看门狗边界，使用共享的Wasmi配置而非生产managed调用链；后者还由实际包Store/监督回归验证。managed/private模块已改成兼容转发。详见 [ADR 0022](../../docs/architecture/adr/0022-authority-free-wasm-runtime.md)。

指令fuel和合作期限不抢占native编译/阻塞broker；父进程watchdog及回收/最终接纳仍是Host职责。生产进程监督/安装器、平台/渠道与正式阅读插件未因本crate落位而完成。实际本机成绩和源码摘要见 [生产运行器回归](../../experiments/extension-wasm/evidence/linux-production-runtime.json)。


2026-11-08 本机复验通过：直接端口3项、原实验32项和作者4项测试，20项受监护场景（含阻塞broker被watchdog终止及下一独立调用恢复）。完整入口35条检查命令通过，16项生产源/声明/锁指纹前后和当前一致，另101项实验/fixture摘要留证。测试显式单线程以保持fuel/deadline先后可复现，未修改250ms生产期限或原精确断言；[并行测试失败记录](../../experiments/extension-wasm/evidence/linux-production-runtime-parallel-failure.log) 保留两种安全拒绝的竞争事实。其他平台、OS沙箱和生产监督/安装器仍未因此通过。
