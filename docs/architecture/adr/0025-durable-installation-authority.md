# ADR 0025：持久安装内核、统一授权恢复与进程身份边界

日期：2026-11-08。状态：持久内核、统一恢复策略和Linux身份已落位并经实际消费者验证；本记录不表示生产安装 UI、完整进程监督或正式阅读工作区完成。关联 C-08/C-09、F-EXT-01/03/05/06、EXT-B02/B03/B04；沿用 [生命周期](../plugin-lifecycle.md)、[包权威源](0021-signed-package-authority.md)、[执行端口](0022-authority-free-wasm-runtime.md) 和 [公开 Surface](0023-public-reader-surface.md)。云适配器决策独立见 ADR 0024，不由此新增云出站或服务接入。

## 问题

纯签名包、受限 Wasm 和公开 Surface 已有真实消费者，但持久安装仍在实验 Store 中。它同时拥有 schema1—4 的 SQL、每次恢复独立创建的 Host、VM 私有迁移、合成 metadata 和故障退出。整文件移入生产会把实验协调错误地当成产品安装器；只复制 install/uninstall 又会让授权、私有状态和 worker lease 留下第二套事务权威。

## 决定

完整持久 SQL 内核迁入 `adapters/extension-store`：不可变包内容、本地发布者绑定、安装/操作回执、持续授权材料、私有成对快照/迁移记录和 worker lease 的 schema/预算/事务均只保留一份。连接、事务和 writer lock 私有；安装仍先禁用，无授权。旧实验 Store 是组合包装器，不再实现另一套 SQL。原记录字段顺序、操作输入 hash、schema 和故障窗口保持兼容。

持久记录不是能力许可。`extension-host::durable` 定义纯 `AuthoritySource` port、快照和统一 `DurableHost`；适配器对该 Host 的依赖仅实现 port，不在存储中创建 Host/Session。一个库只有一个实际策略 Host，恢复多个安装时创建全新本机会话；每次 dispatch 重新读取持久包、修订/世代、启用意图、授权范围/有效期及 worker 排他事实。状态损坏、不可读、更新、撤权或重装使旧会话失效，恢复原记录也不能复活已关闭会话。

选定 API 的能力/资源语义必须匹配。持久授权的精确文档版本保留进 `DurableGrant`，可信 Core resolver 提供当前 `VersionedRequest`；缺版本、错版本或同文档新版本不自动继承原授权。Core 当前版本解析、实际读取/提交、Host 授权与持久管理变更由同一外层 gate 或一致快照串行化。guest 不执行在该 gate 内，也不自报版本成为来源事实。存储与知识仍是不同事务，未知业务提交结果查询原 Core 回执。

授权预检查复用实际 Host 的 package/grant/required 校验，不创建临时策略世界，不提前产生可用会话。授权材料落盘成功后才激活。旧实验的元数据恢复路径仍保留作为兼容 harness，不据此宣称它已经变成统一生产协调器。

私有 VM 调用经可信 `PrivateStateExecutor` 注入，并在 SQL 事务外执行；默认缺少验证器/执行器时失败关闭。逐次 dispatch 的 Current 读取只重验持久关系、配对和摘要，不再次执行 guest。grant validator 不接受来宾自报布尔批准。类型化 checkpoint observer 只提供可信检测点；故障标签、`exit(86)`、Probe Report和合成服务留实验。

`runtime-process` 接管同源 Linux `/proc` 身份、父死守卫和 pidfd 清理，纯身份/清理结果 DTO 属于 `extension-runtime-api`。保留父子同一 executable、启动时间、内核 boot ID 与精确 worker 命令检查；PID 不是发信号权限。其他平台明确 Unsupported。进程身份原语不等于完整监督器；GuardReady→持久绑定→新鲜授权→Boot、异步有界 IPC、独立 watchdog 和退出确认仍需后续装配。未确认 spawn 身份、kill/wait 或 lease 清理的安装保持隔离。

## 实际消费者与验证

原包/捕获/原生实验通过兼容包装实际消费同一生产 SQL 与进程身份实现。另以两个普通签名 Wasm 包消费持久授权和统一 Host，从真实 Core 保留记录读取元数据；验证安装不自动授权、0.1/0.2会话、撤权范围、更新清授权、重开新会话和知识保留。受控 fixture 的接口/事务验证不冒充 PDF 解析、原生 UI 或完整监督。

退出证据包括根检查、现有543包场景/监督及异常恢复、Wasm/C3实际消费者、迁移后的原生保存/恢复；分别记录当前指纹和失败。C6 证据属于迁移前历史，不直接用于证明新二进制。正式安装/授权 UI、通用持续权限 profile、完整进程监督与 reader/library/notes 仍按原门槛推进。


本轮 [C7结果](../../../experiments/extension-package/INSTALLATION_PRODUCTION_RESULTS.md) 已保存根121项Rust、42项边界回归、包543场景、Wasm20旧监督场景、完整C3和迁移后新二进制原生选文/笔记保存/恢复。详细源码快照和摘要随结果归档，未以迁移前证据替代当前消费者。
