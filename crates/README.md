# 平台无关库

现有 [领域边界](learning-domain/src/lib.rs)、[插件协议](extension-api/README.md) 和 [宿主策略](extension-host/README.md)。领域尚无业务实现，安装/授权仍是内存策略。

未来按真实用例增加 Core API、ports、应用事务、来源/定位算法及包管理策略。创建时机见 [目录架构](../docs/architecture/workspace-layout.md)。不生成全套空 crate，不承载产品工作区或阅读/笔记交互流程。

domain 不依赖 UI/运行时/数据库；application 经 ports 访问适配器；wire 不暴露 NSH/FFI/数据库对象。插件不得路径依赖这些内部实现，公开工件见 [SDK 入口](../sdk/README.md)。

新成员及 normal/dev/build/target 依赖更新 [策略](../config/architecture.toml) 并运行 [开发检查](../docs/development.md)。
