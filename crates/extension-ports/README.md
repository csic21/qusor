# extension-ports

纯扩展持久化与恢复端口契约 crate（仅依赖 `extension-api`）。

- **职责**：定义安装/授权/恢复所需的纯数据结构与端口 trait（`Grant`、`DurableGrant`、`AuthoritySnapshot`、`AuthorityRead`、`AuthoritySource`、`RemovedInstallation`、`VersionedRequest`、`ComponentInstallation`、`ComponentAuthoritySource`）以及同权升级保留授权纯策略判定（`GrantUpgradeDecision`、`preserve_grants`）。
- **边界**：不包含 `Host`、`Session`、SQLite、进程身份或 Wasm 运行时实现；由 `crates/extension-host` 消费与对外兼容 re-export，并由 `adapters/extension-store` 实现，从而消除存储适配器对宿主策略层的依赖倒置（[ADR 0055](../../docs/architecture/adr/0055-lifecycle-and-port-decoupling.md)）。
