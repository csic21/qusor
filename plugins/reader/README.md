# @qusor/reader 规划

状态：规划中，尚无实现、manifest 或可安装包。当前原生阅读资格位于 [动态 PDF 实验](../../experiments/nsh-reader/dynamic-pdf/README.md)，不能直接当作正式 reader 插件。

职责范围与公开能力见 [目录架构](../../docs/architecture/workspace-layout.md)、[阅读 Surface 验收](../../docs/product/reader-surface-capability.md) 和 [插件生命周期](../../docs/product/plugin-lifecycle.md)。

进入实现前必须具备：

- 独立 SDK 工件与版本化 manifest；
- 普通签名包安装、授权、更新、停用和卸载链路；
- PDF/EPUB 受控文档服务、精确选区、阅读位置和 Core 回源；
- 仓库外构建，以及至少一个目标平台的真实窗口资格。
