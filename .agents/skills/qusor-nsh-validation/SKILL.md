---
name: qusor-nsh-validation
description: 规划或执行 QUSOR 的 NSH 平台、桥接、输入和阅读 Surface 验证；用于 M-1 技术实验，不用于业务功能开发。
---

# NSH 技术验证

先区分当前请求是研究、基座还是具体实验。保持既定NSH方向，规划请求不自动变成业务开发。

## 已有证据

按实验读取 [源码审查](../../../docs/research/01-nsh-gpui-audit.md)、[阅读与画布路线](../../../docs/research/02-reading-canvas-routes.md)、[M-1清单](../../../docs/research/06-milestones-risks.md) 和 [实验约定](../../../experiments/README.md)。

已有本机环境探针与 [结果](../../../experiments/nsh-reader/RESULTS.md)，复用其固定版本和失败证据；preflight 通过不等于窗口/阅读通过，WSLg 不代表 Windows 原生。已有证据未失效时不要重做广泛选型；需要最新事实时核对官方源码并固定提交。区分NSH原生TSX、Web target和内嵌WebView，不能从TSX语法推出浏览器DOM兼容。

## 定义实验

写出一个假设、通过条件和失败后的备选；关联M-1任务ID。固定NSH提交、lockfile、Rust/SDK、设备/系统、输入法/WebView、语料hash。

只创建当前实验所需的独立Workspace，不把NSH或renderer依赖加到主Core。实验代码不自动升级为生产adapter。

使用 [验证记录模板](../../../docs/templates/validation-record.md)。没有运行证据时保持未验证，不能填入目标数字充当成绩。

## 执行重点

- 分别记录编译、链接、安装、启动、交互、恢复，区分headless/模拟器/真机。
- Rust/TSX验证错误、事件、取消、关闭后回调和资源释放。
- 输入验证composition、光标、undo、emoji及UTF-8/scalar/UTF-16映射。
- 阅读验证离线资源、选区、锚点、重开定位和未可信文档桥接权限。
- 画布验证总量与可见量、命中/DPI/触控、后台布局和原生语义替代视图。
- 性能附设备、release/debug、样本、p50/p95及内存，不能用一次截图证明稳定。
- 优先公开许可或合成fixture；不用用户资料上传来替代本地验证。

例如WebView能打开远端网页，只证明该导航路径；离线EPUB字体/iframe/Worker和frame权限须另验。Core CI成功也不证明NSH移动端可用。

## 更新决定

保留失败日志和复现步骤，结果链接对应任务/ADR。硬门未通过时不推进依赖它的生产功能；使用已定义备选实验并重新评估成本。报告实际做了什么、结果、限制和下一项验证，不泛称“五平台支持完成”。
