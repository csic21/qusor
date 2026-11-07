# 基础设施与原生适配器

当前只有目录约定，尚无生产存储/运行时/NSH adapter。规划与门槛见 [目录架构](../docs/architecture/workspace-layout.md)。

按真实 ports 和已验证路线创建 SQLite/文件、插件运行、平台资源、NSH、PDF/EPUB 服务。adapter 负责具体技术，不拥有用户流程，不复制 Core 数据权威和事务规则。

Core 存储持有规范知识/审计/outbox；Host 插件管理存储持有包/授权/私有状态。包更新或卸载不回滚、删除 Core 内容。

插件经公开能力访问原生服务，不获得 NSH 节点、原生指针、数据库连接或任意路径。运行适配器保留身份、预算、取消、撤权和故障恢复约束。

先在 [experiments](../experiments/README.md) 留证再注册生产成员，不把实验作为生产 path dependency。
