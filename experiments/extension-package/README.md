# 签名包与持久安装事务实验

关联 F-EXT-01/03/05/06、EXT-B02/B04、EXT-P04/P08/P11/P13 的本机子项。依据 [生命周期规格](../../docs/product/plugin-lifecycle.md) 和 [ADR 0007](../../docs/architecture/adr/0007-signed-package-persistence-probe.md)，推进独立作者包、完整性/发布者绑定和安装事务；[实测结果](RESULTS.md) 单独记录。

## 验证的问题

在不重编译宿主、不给作者内部宿主源码的情况下，能否独立构建两个签名 Wasm 包，拒绝完整包篡改与身份接管，并在安装/更新/卸载进程终止后恢复唯一的持久状态？

本实验使用单 JSON 容器和 Ed25519 完整签名，详见 [格式与固定向量](FORMAT.md)。解码文件只留在冻结内存中，不解包到主机路径。原始文件读取采用有界读取及 Linux O_NOFOLLOW/O_NONBLOCK，拒绝链接和非普通文件；校验后不重开下载路径。执行将验证所得模块交给既有 [Wasm 实验](../extension-wasm/README.md) 的解释器和合成能力 broker。

持久化采用 rusqlite/SQLite：完整包字节、package/publisher/key 绑定、当前安装引用、修订/世代、卸载 tombstone、幂等回执在同一事务提交。当前只有 disabled/uninstalled 状态；没有授权记录、自动启用、私有状态迁移、回滚或真实知识内容。内容按完整摘要保存为 BLOB，不涉及第二个文件发布事务。原文件在安装后被替换，不改变保存字节。

## 环境与复现

Linux/WSL，本地文件系统；Rust 1.97.1、Python 3.11+、C 编译器（bundled SQLite）。两个独立 Cargo Workspace 均有锁文件。首次准备：

```sh
cargo fetch --manifest-path experiments/extension-package/Cargo.toml --locked
cargo fetch --manifest-path experiments/extension-package/author/Cargo.toml --locked
python3 experiments/extension-package/validate.py --output /tmp/qusor-package-validation.json
```

验证脚本执行两个 Workspace 的依赖图、fmt/test/clippy/build，先固定宿主二进制，再导出实验 SDK 到仓库外临时目录。导出内容只有权威包模块、extension-api、作者工具与合成 fixture；作者图不得引用仓库源码或带入宿主/SQLite/运行器。SDK 从当前权威实现生成，没有第二份手写协议。外部构建使用离线可用锁定依赖；工件和锁文件摘要留在结果中。这是发行工件可独立使用的实验，不是已发布稳定 SDK。

两版本均真实处理合成中文选区；坏包须在执行前拒绝。安装、更新、卸载分别在 before_transaction、after_content、before_commit、after_commit 退出进程（86，不执行析构），随后新进程重开 SQLite 并核验状态/回执。前三点不得留下新状态，提交后即使响应丢失也必须保留结果；相同操作可查询回执，不同输入重用 ID 则拒绝。

## 手工入口

构建后的 qusor-package-probe 支持：

```text
verify PACKAGE TRUSTED_PUBLIC_KEY_HEX
probe PACKAGE TRUSTED_PUBLIC_KEY_HEX
status STORE
install STORE OPERATION_ID EXPECTED_REVISION PACKAGE TRUSTED_PUBLIC_KEY_HEX [--crash POINT]
uninstall STORE OPERATION_ID EXPECTED_REVISION PACKAGE_ID [--crash POINT]
```

STORE 必须是用户指定的专用本地 0700 目录，父目录已存在；不存在时创建。首次安装及明确重装传 expected revision 0；更新/卸载使用 status 返回的当前修订。修订和世代跨重装保持递增，重装分配新安装 ID；旧命令不能因修订归零误中重装实例。重复命令返回历史回执，当前状态另查 status。

首次安装中的公钥是受信任宿主调用者的明确选择，不由包自报键自动决定；更新及卸载后重装都不能覆盖已有 package/publisher/key 绑定。没有用户信任 UI、现实身份认证、换钥/撤销服务或生产密钥管理。作者使用的常量 seed 是公开测试材料。

probe 是显式合成实验入口，每次新建内存宿主并授权合成选区，不激活持久安装。成功安装始终 disabled。包验证不保证 Wasm 完整结构/ABI 可启动；运行器继续验证模块并限制执行。此次不新增长驻多插件 supervisor，也不以直接 Probe 调用证明 worker 超时、OS 沙箱或完整恢复。

## 存储预算与边界

SQLite 使用 DELETE journal、synchronous=FULL，管理锁由 fs2 持有直到 Store 释放；重开时校验 schema、数据库和当前包签名/绑定。损坏状态关闭整个实验存储入口，保留文件供诊断，不提供自动修复/迁移。每个包遵守格式预算；读取 BLOB 也先限制长度。总内容 64 MiB、安装记录 1024、操作回执 8192；现有内容不会重复计预算，卸载不新增包/安装名额。达到回执上限会拒绝新操作，不自动清历史。

信任前提是宿主拥有的私有目录及本地账户可信；不防御账户所有者主动修改数据库/父目录。没有多主机文件系统、断电/存储设备故障验证或正式灾难恢复。内容暂不垃圾回收；卸载仅写 tombstone，不删除 Core 数据（此实验没有 Core 数据）。

下一步先验证持续授权与新会话恢复、受限私有状态迁移/回滚及长驻双插件隔离，再按相应门槛接生产适配器和真实入库。完整 EXT-B/P、NSH 阅读及五平台/渠道验收均未由本实验替代。
