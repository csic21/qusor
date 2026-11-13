# ADR 0047 原生 PDF 导入与页图阅读

日期：2026-11-13。状态：macOS 与 Android arm64 本机子项通过；崩溃隔离、其它平台发行和正式 Reader Surface 未完成。关联 F-LIB-01、F-READ-01/02/03、F-SRC-01、F-DATA-01。

## 背景

Library Host API 0.6 已能从 Core 读取 PDF 元数据和 `engine_text`，但产品应用没有用户导入 PDF 的原生入口，也没有页图。用户只能看到文字层，无法确认版面、图片、旋转和字体呈现；同时 `nsh package` 不会自动打包额外动态库。

PDF 字节、临时路径与 PDFium 句柄不能进入普通插件，页图不能让 UI 通过 DOM/WebView 生成，Core 也不能依赖 PDFium。Android 还暴露了两个平台事实：应用数据目录禁止硬链接，额外 `.so` 必须进入 APK 原生库目录才能被 linker 正常加载。

## 决策

1. **导入是可信应用动作。** `LibraryHost.importPdf(title, bytes)` 接收用户选择的字节；普通插件的 0.6 wire 不变，仍未获得原件、路径或可信映射提交能力。
2. **Core 只收版本化结果。** 原生 provider 在私有 job 目录写入冻结原件、生成 `DocumentReport`，应用把报告投影成 `PdfSnapshot`，再由 `KnowledgeApplication::import_document` 在一个 Core 事务中提交原件、提取身份、页/字符映射、回执、审计、outbox 和索引任务。
3. **PDFium 服务串行且离开 UI 线程。** 应用进程内只有一个 `qusor-pdf-service` 线程；导入与按页渲染都排队执行，主线程只等待 Promise。该线程隔离资源与阻塞，不是安全沙箱，也不等于独立 OS 进程。
4. **页图按需从 Core 原件重渲染。** `LibraryHost.pageImage(documentId, pageIndex)` 重新核对报告 hash 与 Core `engine_report_sha256`，把 PNG 作为 `ImageData`/native `File` 返回给可信 TSX。原生服务保留最多 3 页、32 MiB 编码页图的 LRU；`prefetchPage` 在后台预取下一页，命中时不重复运行整份 PDFium 报告。像素/File/URL/缓存是显示资源，不写 Core、不导出、不作为来源锚点。
5. **页图与文字层并存。** 页图负责版面确认，现有 `engine_text` 继续用于显式 scalar range 选文和摘录保存；不以页图像素或视图坐标替代 PDF user-space 字符几何。
6. **运行库按平台显式打包。** 固定 PDFium `chromium/8125`。macOS arm64 从应用资源加载 `.dylib`；Android arm64 由 `package_android_pdfium.py` 向 APK `lib/arm64-v8a/` 注入 `.so`，16 KB 页对齐并重新签名。未登记的平台明确返回不可用，不静默退回系统库。
7. **Android CAS 使用不可覆盖原子重命名。** Linux/Android 的 blob 发布由 `hard_link` 改为 `renameat2(RENAME_NOREPLACE)`；其它 Unix 保持硬链接。发布成功后的临时文件缺失不构成完整性错误。
8. **资料库视图使用声明式分支。** NSH 信号更新不会重新执行组件顶部的 `if (signal())`；列表/阅读切换使用 `<If>`，资料行整行可点击。

## 后果

- 用户可在 macOS 和 Android arm64 选择真实 PDF，看到 Core 持久化的页图与文字层；重开应用后仍可恢复。
- Core/插件边界不变：插件看不到 PDF 字节、路径、PDFium 句柄或页图像素；文档事实仍归 Core。
- 页面渲染失败不删除已导入原件；文字层仍可读，错误明确反馈。
- 进程内 PDFium 崩溃仍会终止应用。恶意文件、扫描件、OCR、复杂字体、长书、拖选高亮、缩放/页窗缓存和正式 Reader Surface 仍是后续门槛。
- 当前只登记 macOS arm64 与 Android arm64；Linux、Windows、iOS、Android x86_64 和商店/公证签名尚未完成。
- 未命中缓存时仍会重新执行有界 provider 报告；后续仍需要按文档缓存、取消、资源回收 ACK 和独立 worker。

## 验证

- macOS 真窗口 NSH 测试 3/3：真实导入、坏 PDF 拒绝、Library 工作区显示原生页图。
- 应用原生 17 项通过、1 项忽略：真实 PDF 导入、页快照、PNG 页图。
- Android API 36 arm64 模拟器：系统 picker、真实导入、第一页、强制停止重开、`Rotate 90` 第二页。
- 详见 [PDF 页图导入证据](../../../experiments/nsh-reader/evidence/pdf-page-import/README.md)。
