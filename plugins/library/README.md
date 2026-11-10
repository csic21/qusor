# @qusor/library 规划

状态：规划中，尚无实现、manifest 或可安装包。资料导入、列表和搜索目前只有实验内的动态 PDF 选择链，不是正式 library 插件。

目标是 F-LIB-01/02/03：保留原件、导入与重开、标签/分类、中文搜索。文件选择、原件冻结和持久化必须走公开 Host/Core 能力；插件不直接写 SQLite，不依赖 NSH 私有对象。

进入实现前必须具备独立 SDK/包格式、普通身份安装与恢复、受控文件服务和稳定 Core 查询契约。完整边界见 [目录架构](../../docs/architecture/workspace-layout.md) 与 [插件生命周期](../../docs/product/plugin-lifecycle.md)。
