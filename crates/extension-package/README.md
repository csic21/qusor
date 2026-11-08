# 插件签名包格式与验证

此 crate 是格式 v1、完整文件清单、Ed25519 签名和显式发布者键绑定验证的唯一权威实现，承接 [ADR 0021](../../docs/architecture/adr/0021-signed-package-authority.md)。初次实现从已验证实验按字节迁入；后续增加显式宿主版本集合入口，旧0.1固定向量、签名domain、规范JSON与格式保持不变。模块开头的实验描述保留其来源。

验证仅消费有界的不可变字节，返回冻结的已验证内容，不打开路径、不解包、不安装、不授予能力、不执行代码。发布者 ID 自报不等于可信身份；验证者必须提供宿主已绑定的公钥。签名或 manifest 本身不是运行沙箱。crate 版本、容器格式版本、manifest 版本、作者包版本与 Host API 版本分别管理。

[15项包格式/兼容测试](tests/package.rs) 覆盖固定向量、完整元数据/文件篡改、弱身份键、未知/重复字段、规范编码、路径别名与预算。实验 Store 通过兼容转发消费此实现；两份实验 SDK 导出器也直接复制这里的公开源，仓库外作者不会获得 Host、SQLite、文件锁或运行器源码。

本地 author 是资格打包工具，可通过 Cargo 依赖别名使用公开契约；正式功能插件仍必须使用独立发行 SDK 并验证仓库外构建。生产持久安装器、稳定 SDK 发行、公开 Surface 与正式 reader/library/notes 工作区仍未完成。


`verify` / `build_signed` 的默认宿主支持集始终只有旧0.1。`verify_for_host(bytes,key,supported_apis)` 与 `build_signed_for_host(...,seed,supported_apis)` 必须由可信装配显式传入非空、无重复且已实现的版本集合；未知协议不会因manifest数值范围宽而启用。带reader.surface/ReaderWorkspace的新包须声明最低0.2，旧入口明确拒绝；同一旧包在显式双版本入口验证/构建时字节仍保持相同。兼容判断只选择可运行版本，不授予能力或会话身份。
