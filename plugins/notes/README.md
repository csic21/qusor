# @qusor/notes 规划

状态：规划中，尚无实现、manifest 或可安装包。当前 `learning-application` 已有有界 PDF 想法查询，原生实验工作区也能列出已保存想法；这些都是后续 notes 包的读取/交互基础，不是正式插件交付。

目标是 F-NOTE-01 与 F-SRC-01：Markdown 想法、不可编辑的原文引用、按资料组织、编辑保存、从引用回源，以及卸载插件后规范内容仍可读取和导出。

实现边界：

- 只消费公开 SDK、贡献点、Core 查询与授权；
- 原文、用户想法、AI 推断保持分离；
- 引用保存版本化锚点，歧义、缺原件和版本不符明确失败；
- 不复制 Core/Host/application/storage/NSH 私有实现；
- 使用独立 Workspace/lockfile，仓库外只凭 SDK 工件构建。

当前可复用的读取模型见 [pdf_notes.rs](../../crates/learning-application/src/pdf_notes.rs)，实窗增量与未覆盖范围见 [想法列表证据](../../experiments/nsh-reader/evidence/notes-workspace/README.md)。
