# 公开 SDK 入口

尚未发布独立 SDK。[extension-api](../crates/extension-api/README.md) 当前只有 0.1 策略协议，完整包格式、能力实现、贡献 UI 和平台契约仍须验证。

wire DTO/schema/契约测试以 [extension-api](../crates/extension-api/README.md) 为权威源。此目录用于生成配置、语言绑定、作者文档、打包/验证工具及发行入口，不维护第二套手写协议。

发行工件应包含协议/schema、manifest/能力/权限/错误规范、绑定、构建打包/本地验证说明、兼容策略和独立例子。官方包消费同一工件，作者不需要主仓库内部路径。实验包格式不是已冻结的生产 SDK。

有真实内容才创建源码目录。生成工件可追溯协议版本/hash，干净环境可再生成核对；包、API、manifest、格式、私有状态版本分别管理。

独立性验收是在仓库外只凭 SDK 工件/公开文档构建包，在未修改/未重编译 Host 中安装运行。详细边界见 [目录架构](../docs/architecture/workspace-layout.md)。
