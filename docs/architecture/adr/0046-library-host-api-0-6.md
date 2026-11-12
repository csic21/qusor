# ADR 0046：Library Host API 0.6

日期：2026-11-12

状态：协议、Host 授权、应用 Core 后端适配和初始原生资料库列表已实现；PDF 打开、Reader Surface 与完整 Library 旅程未完成。

## 背景

[ADR 0045](0045-bounded-pdf-library-listing.md) 已在应用层提供 vault 内 PDF 元数据分页，但普通插件不能直接 import 应用层或读取 Core。若 UI 走私有调用，会破坏第一方/第三方同权与停用授权边界。因此需要独立的版本化 Library wire。

## 决定

1. 新增 Host API 0.6、`library.read` 能力和 `library_workspace` 贡献类型。0.1–0.5 语义保持不变。
2. `extension-api::library` 提供 `library-metadata-v1`：
   - 请求只有最新优先分页、opaque cursor 和 `limit`；
   - 单页最多 6 条，整响应仍受 4 KiB 预算；
   - 响应仅含文档 ID、提交修订、标题、原件摘要、原文字节数、提取修订和页数；
   - 不返回原件、CAS blob、文件路径、摘录、想法或权限。
3. 请求不自报 vault。`Host::authorize_library` 把请求绑定到 Host 自己的 vault，并要求安装实例的有效 `library.read` grant。
4. 0.6 manifest 必须声明 `library.read`；`library_workspace` 贡献必须声明同一能力；不能把它降级到 0.5。
5. 每个 permit 在 dispatch 前重新检查安装状态、世代、vault 与 grant；禁用、撤权或 session 关闭后失效。

## 后果

- Library 插件后续只通过公开 wire 消费元数据，不能直接持有 SQLite/Core。
- “打开文档”仍需另一条受控 Reader Surface/原件路径，不能从 Library 元数据响应夹带 PDF 字节。
- 应用已嵌入普通签名 `org.qusor.library` 包，`LibraryHost.page` 通过 0.6 permit 和 `LibraryReadBridge` 读取真实 Core；初始资料库工作区只显示元数据。
- Reader preview fixture 仍是独立入口，尚未从资料库打开。

## 验证

- `extension-api`/`extension-host`/`extension-store`/`extension-package` 合计 179 项测试通过。
- 根 Workspace 排除既有 macOS `runtime-process` 后 308 项测试；应用 12 项测试、`nsh check App.tsx`、架构守卫和原生 UI 边界通过。
- 应用集成测试从迁移 Core 中读取 `Legacy note` 元数据；停用/重启用仍沿用统一包作用域。
- 新增 `policy-v0.6.schema.json`；旧 0.1–0.5 schema 未修改。
