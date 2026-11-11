# 平台无关库

现有 [Core 数据契约](core-api/README.md)、[领域边界](learning-domain/src/lib.rs)、[插件协议](extension-api/README.md) 和 [宿主策略](extension-host/README.md)。learning-domain 已有 FSRS-5 词汇调度内核（零外部依赖），学习数据契约与用例在 core-api/learning-ports/learning-application。Host本身保持内存纯策略，durable模块经纯port接收新鲜持久材料；安装SQL在独立extension-store适配器，不把持久记录当作许可。

捕获/来源/锚点公开契约已按 ADR 0018 从实验落位；随后已落位 [知识用例](learning-application/README.md) 与 [原子持久化端口](learning-ports/README.md)，由实验普通插件真实消费生产SQLite/CAS；其他来源/定位算法及包管理策略按用例继续推进。创建时机见 [目录架构](../docs/architecture/workspace-layout.md)。不生成全套空 crate，不承载产品工作区或阅读/笔记交互流程。

domain 不依赖 UI/运行时/数据库；application 经 ports 访问适配器；wire 不暴露 NSH/FFI/数据库对象。插件不得路径依赖这些内部实现，公开工件见 [SDK 入口](../sdk/README.md)。

新成员及 normal/dev/build/target 依赖更新 [策略](../config/architecture.toml) 并运行 [开发检查](../docs/development.md)。
