# 基础设施与原生适配器

云存储按 [ADR 0024](../docs/architecture/adr/0024-cloud-storage-adapters.md) 预留可替换适配器：结构化同步、对象存储、身份会话分离，可分别选择供应商；Supabase 与 Cloudflare 为候选。实际用例和独立验证出现后再创建模块，不预建空实现。

已有真实 [SQLite](storage-sqlite/README.md)、[不可变CAS](blob-store/README.md)、[插件持久内核](extension-store/README.md)、[受限Wasmi](runtime-wasm/README.md) 与 [Linux进程身份](runtime-process/README.md) 生产成员。当前耐久与身份验证仅Linux本机，完整进程监督与NSH生产adapter仍待落位。规划与门槛见 [目录架构](../docs/architecture/workspace-layout.md)。

按真实 ports 和已验证路线创建 SQLite/文件、插件运行、平台资源、NSH、PDF/EPUB 服务。adapter 负责具体技术，不拥有用户流程，不复制 Core 数据权威和事务规则。

Core 存储持有规范知识/审计/outbox；Host 插件管理存储持有包/授权/私有状态。包更新或卸载不回滚、删除 Core 内容。

插件经公开能力访问原生服务，不获得 NSH 节点、原生指针、数据库连接或任意路径。运行适配器保留身份、预算、取消、撤权和故障恢复约束。

先在 [experiments](../experiments/README.md) 留证再注册生产成员，不把实验作为生产 path dependency。
