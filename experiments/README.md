# M-1 技术验证约定

独立插件执行实验已有本机证据，具体实现与实测结果分开记录。本目录包含实验性依赖，不进入生产 Cargo Workspace。

## 当前实验

| 实验 | 关联任务 | 当前范围与状态 |
|---|---|---|
| [原生 PDF provider](pdf-provider/README.md) | C-08/C-09、F-LIB-01、F-READ-01/02、F-SRC-01 | 实际页图、字符几何、裁剪/旋转与版本化范围定位；[本机子项](pdf-provider/RESULTS.md) 通过，可靠阅读顺序/Core PDF/NSH 未通过 |
| [真实捕获与快照定位](capture-core/README.md) | C-08、F-EXT-02、F-SRC-01、F-DATA-01 | 普通签名来源插件、明确批准、真实 CAS/SQLite、撤权竞争、故障与生命周期后保留、导出恢复；[本机通过](capture-core/RESULTS.md)，生产 Core/阅读 UI 未集成 |
| [受限 Wasm 插件执行](extension-wasm/README.md) | EXT-B01、EXT-P04/P06/P11 的本机部分 | 固定 Wasmi 2.0.0，独立作者构建目录包，合成选区、资源预算、权限与故障测试；本机执行已验证，覆盖与限制见 [实验结果](extension-wasm/RESULTS.md) |
| [签名包与持久安装事务](extension-package/README.md) | EXT-B02/B04、EXT-P04/P08/P11/P13 本机子项 | 完整签名、仓库外作者、显式键绑定及禁用安装/更新/卸载进程恢复；见 [结果](extension-package/RESULTS.md) |
| [NSH 最小阅读宿主](nsh-reader/README.md) | P01/P02，后续 P03/P05/P07/P08/P09 | 固定 NSH 源码、环境 preflight 与离线构建尝试；本地前缀解除 native 编译阻断，独立原生窗口及PDF页图/真实拖选已有证据；完整打包/输入/产品阅读旅程未通过，见 [结果](nsh-reader/RESULTS.md) |

用户授权从纯策略基座推进到 M-1 独立执行验证，决策见 [ADR 0005](../docs/architecture/adr/0005-wasm-runtime-probe.md)。它不决定生产运行时，也不代表 SDK 发布、完整第三方安全安装或移动分发通过。[EXT-P01–P13](../docs/architecture/extensions.md#10-可执行验证计划) 仍逐项留证，具体来源接入与合成契约测试分别记录。最初 Wasm CLI 每次新建监督父进程和 worker；后续包实验已增加长驻监督及异常退出协调，本机子项与残留限制见 [包实验结果](extension-package/RESULTS.md)。

用户已同意 [下一阶段顺序](../docs/product/next-phase.md)；目录与插件边界见 [ADR 0006](../docs/architecture/adr/0006-plugin-workspace-layout.md)。包生命周期按 [规格](../docs/product/plugin-lifecycle.md) 推进；签名包、持续授权、私有迁移和长驻恢复已有本机实验子项，生产安装器和全平台门槛仍未通过。C0 真实捕获独立验证知识边界。NSH 上游缓存位于实验 target 目录，不作为 QUSOR Cargo member。

## 实验规则

每个实验解决一个可证伪问题，并引用 [M-1 任务 ID](../docs/research/06-milestones-risks.md)；例如 Rust/TSX 取消是否可靠、离线 EPUB 子资源是否可加载。功能规划不自动触发实验实现，已明确授权的本轮执行实验无需重复审批。

每个实验使用自己的 Cargo Workspace/lockfile，不加入根 Workspace，不把 NSH/PDF/WebView/插件运行时依赖带进 learning-domain 或生产模块。实验宿主可复用生产协议与策略库；独立插件作者工程不得通过 QUSOR 源码路径依赖获得内部能力。实验产物不是生产 adapter，转入业务前重新检查范围、数据契约和运行证据。

采用 [验证记录模板](../docs/templates/validation-record.md)，记录版本、设备、语料 hash、步骤、预期、实际结果和失败路径。编译、链接、安装、启动、交互、模拟器与真机分别标记；权限拒绝、VM 预算、父进程终止和 OS 沙箱分别说明，不能相互替代。

优先公开许可或合成 fixture，记录来源；不能为验证便利上传用户书籍或把缓存/大二进制提交进仓库。首发平台仍由范围记录中的 C-04 确认，本机 WSL 结果不替代原生桌面、移动或渠道验收。
