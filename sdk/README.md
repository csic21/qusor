# 公开 SDK 入口

尚未发布独立 SDK。[extension-api](../crates/extension-api/README.md) 当前保留0.1默认、显式0.2阅读Surface并新增待完整提供者验收的0.3草稿/组件协议；签名包格式已有生产权威源和本机验证，贡献 UI、完整能力与平台契约仍须分别验证。

- **阶段 2 ABI 与工具链设计**：见 [WIT 契约](wit/qusor-plugin.wit) 与 [SDK / ABI / `qusor dev` 设计说明](DESIGN.md)（含 `wasmi 2.0.0` Component Model 评估、MiB 级预算、Rust-first `qusor-sdk` 与开发期未签名加载开关）。

包格式/完整文件签名与显式键绑定以 [extension-package](../crates/extension-package/README.md) 为唯一权威源；实验 SDK 直接导出该纯实现，作者不会获得安装 Store 或运行器。

捕获/来源/文本及 PDF 锚点以 [core-api](../crates/core-api/README.md) 为唯一权威源，实验 SDK 导出直接使用该实现；仍未正式发布。Host 策略 wire DTO/schema/契约测试以 [extension-api](../crates/extension-api/README.md) 为权威源。此目录用于生成配置、语言绑定、作者文档、打包/验证工具及发行入口，不维护第二套手写协议。

发行工件应包含协议/schema、manifest/能力/权限/错误规范、绑定、构建打包/本地验证说明、兼容策略和独立例子。官方包消费同一工件，作者不需要主仓库内部路径。格式v1的验证不等于整个SDK已经稳定发布；Host执行端口和适配器不属于作者SDK。

有真实内容才创建源码目录。生成工件可追溯协议版本/hash，干净环境可再生成核对；包、API、manifest、格式、私有状态版本分别管理。

独立性验收是在仓库外只凭 SDK 工件/公开文档构建包，在未修改/未重编译 Host 中安装运行。详细边界见 [目录架构](../docs/architecture/workspace-layout.md)。

0.3契约已在仓库外只用导出SDK完成作者编译、签名asset和包校验，见 [机器证据](../experiments/capture-core/evidence/linux-component-sdk-author.json)。它仅证明独立编译/签名与旧Host拒绝新包，不证明普通Wasm提供者已运行或原生组件替换通过。复现：`python3 experiments/capture-core/component_sdk_validate.py`。
