# ADR 0045：有界 PDF Library 文档目录

日期：2026-11-12

状态：应用层只读查询、Host API 0.6 与初始 Library UI 已实现；PDF 打开和 Reader 链路未完成。

## 背景

Core 已保存 PDF 文档记录和原件 blob，但应用层只有按 ID 的 `query_document`。Library 若直接 `export_all` 或按 ID 扫描，会把完整归档和 blob 暴露给 UI，也没有稳定分页语义。需要先建立只返回元数据的有界查询，再谈宿主/插件能力。

## 决定

1. `learning-application::pdf_library` 新增 `pdf_documents_page(vault, after, limit)`。
2. 单页最多 200 条，按 `commit_rev`、`document_id` 倒序稳定排序；返回标题、文档 ID、提交修订、原件摘要、原文字节数、页数和提取修订。
3. 不返回原件字节、CAS blob、文件路径、摘录、想法、授权或插件私有数据。
4. 游标是 `l1:<commit_rev>:<document_id>` 的 opaque keyset；查询先按 vault 过滤，再定位游标。其他 vault 或损坏/任意构造的游标拒绝，不跳到相似文档。
5. 查询复用一次 validated Core snapshot，不写 Core，不解析 PDF，不创建 Host 会话。

## 后果

- Library 插件通过版本化 Host wire 消费这一读模型，而不是直接拿 Core 归档。
- 正式打开阅读仍需按 `document_id` 取受控原件/Surface；本 ADR 不提供该通路。
- 文档标题来自导入时的 Core 记录；排序不使用文件名或文件系统时间。

## 验证

- `learning-application` 46 项测试通过，包含 vault 隔离、倒序、分页、跨 vault 游标拒绝、limit/vault 校验。
- 新增契约不改变 SQLite schema、包 wire、Host 授权或 NSH UI。
