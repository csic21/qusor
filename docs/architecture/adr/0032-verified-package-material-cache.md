# ADR 0032：完整材料相等时复用验包结果

日期：2026-11-08。状态：实现已落地，本机Store/投影进程验证通过，本机完整兼容回归通过。关联 F-EXT-01/04/05/06、F-UX-01/02，承接[签名包权威源](0021-signed-package-authority.md)与[只读投影](0030-readonly-paragraph-projection.md)。不改变权限、协议或执行预算。

## 问题与证据

新投影在真实Store/Host/Wasmi下可以输出正确窗口，但六次宿主调用的VM接近250ms上限，OS消费者在同一checkpoint耗尽有界重试。独立小样测得每VM约188–201ms花在六次宿主调用中，其余约55–61ms；不能把它归因于算法或CPU负载之一而忽略另一部分。

Store的component事实路径每call重新读取SQL材料，再完整执行纯验包：JSON解析/canonical重编码、文件解码/哈希和Ed25519严格验证。只读源码审查确认这是可重复计算。尝试的Base64范围判断候选没有解决超时；复核时发现当前函数已经是范围判断，候选不纳入正式源码。

## 决定

仅缓存成功的纯`VerifiedPackage`结果；不缓存ComponentInstallation、授权、grant、worker状态或私有配对结果。缓存为Store实例内私有对象，重开为空。公开`current_package()`的签名与原行为保留；component热路径可使用私有shared结果，避免复制整包或开放未验证构造。

每次调用仍从当前SQL实际读取完整原始包bytes、当前trust key/publisher及安装事实。命中必须原始bytes逐字相等、key文本相等、完整supported_apis相等、VERIFIER_POLICY相等；digest只能辅助查找，不代替字节比较，也不使用mtime。任一材料变化即完整验证，失败直接拒绝，不回退旧结果。复用后仍核当前Receipt的digest/package_id及fresh publisher绑定。

安装status/revision/generation、vault亲和、own-worker lease、pending事务与private pair检查全部继续按原路径执行。借用/锁不得覆盖SQL查询、验证器或业务handler。缓存只是复用对同一材料已经成立的验包证明，不是把旧授权当成当前授权。

先限2项、保守计费合计最多32MiB；计入原始bytes、解析/解码结果及元数据。超预算不缓存，仍完整验证。不可失败缓存，不依赖手动失效保证安全，也不改变磁盘Schema。当前运行中的冻结模块与旧证据不被替换。

## 验证与交付

先暖缓存，再测试当前key/publisher改变、同长度坏bytes、同长度有效替包但Receipt不匹配、验证配置隔离、撤权/卸载、private状态变化及容量淘汰。现有篡改/恢复/私有配对测试必须原样通过。不得通过减少fresh检查、扩大250ms/100万fuel/8calls或缩小1MiB功能通过资格。

随后实测冷/热读取与完整新投影OS八场景，区分cache命中、真实超时及恢复；继续完整包/Wasm/Core兼容回归。吞吐改进不等于UI/IME/Esc或native资源释放通过；若仍有延迟问题，继续按测量结果优化，不把重试当最终交互方案。

本机根Workspace223项Rust测试及Clippy通过；含暖缓存后private pair/pending/worker身份变更拒绝。原签名投影包八场景通过76 worker/412 calls，0 deadline，250ms/100万fuel/8calls不变。另有同fixture的4VM诊断：[记录](../../../experiments/capture-core/evidence/projection-cache-profile/record.json)；metadata可预热缓存，不能称冷读取基准。原生UI与完整兼容回归未据此签署。

完整[包543场景](../../../experiments/extension-package/evidence/linux-projection-cache-package.json)与[Wasm20场景](../../../experiments/extension-wasm/evidence/linux-projection-cache-wasm.json)已终态通过。首次包回归因/tmp tmpfs空间不足在编译阶段失败，保留[原失败记录](../../../experiments/extension-package/evidence/linux-projection-cache-package-tmpfs-failure.json)，转磁盘临时目录后重跑；不将环境编译失败记为行为通过。C3完整回归已在源码冻结窗口终态通过：314 commands/286 CLI/8 native，203份输入哈希前后一致并归档，见[结果](../../../experiments/capture-core/evidence/linux-projection-cache-compound.json)与[源归档](../../../experiments/capture-core/evidence/projection-cache-compound-source-archive.json)。新共享编辑服务的4项局部测试也包含在本次检查，但这不构成其真实guest执行或原生接线证明。
