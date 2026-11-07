# 功能插件

当前尚无正式功能包。本目录明确代码归属，不代表功能已存在。全部 Feature 归属见 [目录架构](../docs/architecture/workspace-layout.md)，首条旅程见 [下一阶段计划](../docs/product/next-phase.md)。

资料库、阅读、笔记、AI、知识/画布、学习、同步等工作流以完整功能包交付。Core/Host 保证共享数据、事务、来源、授权、原生服务与恢复。官方和第三方用同一 SDK/贡献点/安装/执行链，预装只表示来源。

## 单个包结构

```text
<feature>/
├── README.md          Feature ID、作者说明、权限、停用行为
├── manifest.json      实际 SDK 定义的元数据与贡献点
├── src/               只依赖公开 SDK 的功能逻辑
├── tests/             公开契约及功能行为验证
├── fixtures/          有需要时才创建的公开/合成资料
└── 构建清单与锁文件   按实际语言/profile 确定
```

候选结构不冻结尚未验证的 SDK 字段/语言。Rust 包有独立 Workspace/lockfile；其他语言同样独立锁定构建。输出不提交。

## 独立性

- 不加入生产根 Workspace，不继承根依赖、版本或私有构建脚本。
- 不依赖主仓库 Core/Host/adapters 或另一插件的内部模块，开发时也只消费可导出 SDK 工件。
- 仓库外仅凭 SDK/公开文档能构建；安装/更新不重编译 Host。
- 协作通过公开能力发现与规范数据；缺提供者明确降级，不读取对方私有状态。
- 提交内容经 Core；停用/卸载后已存内容可通用读取、引用、导出。私有状态不能成为知识唯一副本。
- 不能伪装或绕过包管理、宿主授权、安全提示、AI 审核。完整要求见 [插件架构](../docs/architecture/extensions.md)。
