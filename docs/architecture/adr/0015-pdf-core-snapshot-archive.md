# ADR 0015 PDF 原件与统一 Core 归档资格

日期：2026-11-08。状态：[独立本机子项通过](../../../experiments/capture-core/PDF_RESULTS.md)，生产集成未通过。验收见 [PDF 资格切片](../../product/pdf-capture-qualification.md)。

## 决定

沿 `experiments/capture-core` 扩展 schema2，不另建复制 Host/SQLite 的 PDF 知识库。原件进入 Core 二进制 CAS，公开 `pdf_contract` 仅定义版本化快照、页映射和锚点，不依赖 PDFium、NSH、数据库或运行器。原生 provider 通过可信适配器投影成契约；插件只提交选段提案，不能自报提取结果成为事实。

原件版本、语义提取版本、逐页文本 hash 和全局提交 revision 分开。语义提取版本包含原件、固定提取器身份和页映射；不包含页图/DPI 等渲染证据摘要，避免缩放改变来源版本。原始 provider 报告摘要另行保留为证据。

schema2 的提交日志将原文本记录回填到统一序列，文本和 PDF 的局部实体 ID 不充当全局 commit revision。原件导入和摘录保存分别是明确用户动作，各自在一个 SQLite 事务提交全部引用、凭据、操作事实、outbox 和索引任务；先可靠发布原件和映射所需 blob，允许无引用残留，不允许成功记录引用缺失 blob。

统一 V2 archive 包含文本、PDF 原件/映射/摘录及操作关系，二进制以严格 base64 编码。新增固定 base64 0.22.1 仅用于归档实验，不进入生产根或公开契约 SDK。空库恢复先验证全部引用并发布 blob，再单事务建立正式记录；兼容 V1 文本导出在混合库中明确拒绝，不默默缺失 PDF。

原生 PDF Host 复用既有 Host 策略与包验证：普通插件在动作内先取得受控 SelectionRead，再构造 CapturePropose；最终批准绑定启动、安装身份、包、授权世代、文档和选段 hash。撤权与复核后的 Core 提交共享原生门，包管理锁覆盖动作期间。批准不能从 JSON 恢复。

## 限制

仍是独立资格实现，不将实验 Core 自动提升为生产模块。原生服务导入入口不等于 library 插件完成。PDFium 字符流保留明确 engine profile，不把尚未可靠的行序冒充 canonical reading text。Core 对持久关系负责，原生提取器对实际 PDF 到映射的可信供给负责；不允许插件绕过该供给链。

已有原生 provider 和文本捕获回归必须保持；新增证据须包含真实 PDF、仓库外签名作者、混合事务、崩溃及恢复后再次渲染，而不仅是手写映射单元测试。
