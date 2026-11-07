# M-1 技术验证约定

当前没有实验实现或通过结果。本目录为后续具体 NSH 与插件基座验证保留入口。

插件边界实验采用 [EXT-P01–P13](../docs/architecture/extensions.md#10-可执行验证计划)，当前均未实现；具体来源接入验证与合成契约测试分别记录。

每个实验解决一个可证伪问题，并引用 [M-1 任务 ID](../docs/research/06-milestones-risks.md)；例如 Rust/TSX 取消是否可靠、离线 EPUB 子资源是否可加载。功能规划不自动触发实验实现。

获授权执行具体实验后创建独立目录，具有自己的 Cargo Workspace/lockfile，不加入根 Workspace，不把 NSH/PDF/WebView 依赖带进 learning-domain。实验产物不是生产 adapter，转入业务前重新检查范围、数据契约和运行证据。

采用 [验证记录模板](../docs/templates/validation-record.md)，记录版本、设备、语料 hash、步骤、预期、实际结果和失败路径。编译、链接、安装、启动、交互、模拟器与真机分别标记。

优先公开许可或合成 fixture，记录来源；不能为验证便利上传用户书籍或把缓存/大二进制提交进仓库。
