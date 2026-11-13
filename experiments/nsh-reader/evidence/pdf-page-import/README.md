# 真实 PDF 导入与原生页图

日期：2026-11-13。状态：macOS 真窗口 UI 与 Android API 36 arm64 模拟器子项通过；不是完整阅读器、Reader Surface、插件导入 wire 或发行资格。

## 本轮范围

- 资料库“导入 PDF”调用可信应用入口 `LibraryHost.importPdf`，用户选择的字节不进入插件。
- 原生层把原件写入私有 job 目录，调用固定 PDFium `chromium/8125` 生成有界报告，投影为 Core `PdfSnapshot`，再由 `KnowledgeApplication::import_document` 一次写入原件 CAS、映射、回执、审计、outbox 与索引任务。
- PDFium 调用集中在进程内单一线程 `qusor-pdf-service`，不阻塞 UI；这不是崩溃隔离或安全沙箱。
- 阅读视图从 Core 原件重新渲染当前页，经 `ImageData -> File -> URL.createObjectURL` 显示原生页图；文字层继续用于选文、复制和“选文 + 想法”保存。
- 原生 PDF 服务保留最多 3 页、32 MiB 编码页图的 LRU，并在页图显示后预取下一页；命中缓存时不重复运行整份 PDFium 报告。
- 页图、临时路径、PDFium 句柄和像素坐标不进入 Core、不导出，也不成为永久来源地址。

## 验收结果

macOS 真窗口测试 3/3 通过：

```text
✓ 真实 PDF 进入 Core 并渲染第一页
✓ 损坏 PDF 被拒绝，没有导入回执
✓ 资料库工作区从 Core 打开 PDF 并显示原生页图
3 tests: 3 passed, 0 failed
```

应用原生回归 19 项通过、1 项忽略，包括真实 PDF 导入、Core 重读、页图 PNG 生成和页缓存有界/LRU：

```text
cargo test --manifest-path apps/qusor/native/Cargo.toml --locked --offline
cargo test: 19 passed, 1 ignored
```

Android API 36 arm64 模拟器实际验证：

- 系统文件选择器选中 1,292 bytes、两页普通文本 PDF。
- 导入后显示“本机 Core · 原生页图 + 文字层”，第一页页图和文字层一致。
- 强制停止重开，同一 PDF 与“已继续上次阅读 · 第 2 页”恢复。
- 第二页为 `Rotate 90`，原生页图与文字层均显示；进程保持存活。

截图：

- [android-pdf-picker.png](android-pdf-picker.png)：Android 系统文件选择器。
- [android-page1.png](android-page1.png)：导入成功后的第一页。
- [android-restart-page2.png](android-restart-page2.png)：强制停止重开后的第二页与继续位置。
- [android-page2-rotated.png](android-page2-rotated.png)：旋转第二页。
- [android-library-with-pdf.png](android-library-with-pdf.png)：回列表后的 Core 资料元数据。

测试日志：

- [macos-nsh-test.txt](macos-nsh-test.txt)
- [native-cargo-test.txt](native-cargo-test.txt)
- [android-artifact.txt](android-artifact.txt)：APK 哈希、原生库条目、16 KB 对齐与 v3 签名核对。

## 构建与工件

PDFium 固定发行版本：

```text
chromium/8125
macOS arm64  libpdfium.dylib  sha256 a45c44645523968525c710462691dd974f4efb1013ce1c249919a0939cfb373a
Android arm64 libpdfium.so    sha256 de19db1c0fb33579bca6ace903af7ccb9d03ad1d3f6c5202970d8d79d7538303
```

PDFium 的 `LICENSE` 与对应第三方 `licenses/` 随应用资源保留。

Android APK：

```text
/private/tmp/qusor-android-cache/QUSOR.apk
sha256 a2188a93a023d4752dd8af1280f98776af8067ec557b173fc52251219f79f2b4
debug key, arm64-v8a
```

`nsh package` 只写 `lib/arm64-v8a/libnsh.so`。本轮新增 `apps/qusor/package_android_pdfium.py`，在包生成后注入 `lib/arm64-v8a/libpdfium.so`、以 16 KB 页面要求重新对齐并重新签名。Android 运行库按 soname `libpdfium.so` 从 APK 原生库目录加载。

## 发现并修复的问题

1. Android 的 Core CAS 发布原使用 `hard_link`，SELinux 拒绝应用数据目录中的硬链接；改为 Linux/Android 的 `renameat2(RENAME_NOREPLACE)`，保持目标不可覆盖。
2. `rename` 成功后临时文件已不存在，旧清理逻辑把 `NotFound` 误报成完整性失败；发布成功后缺失临时文件现在明确视为成功。
3. NSH 信号会更新 JSX 文本绑定，但不会因组件顶部 `if (signal())` 重新执行组件函数；资料库视图切换改为声明式 `<If>`，修复“点击后状态写了但页面不切换”。
4. 资料行原先只有右侧小按钮可点；改为整行可点击，改善触控与无障碍命中范围。

## 未验证与边界

- PDFium 仍未在独立 OS 进程运行；恶意/损坏 PDF 崩溃应用的风险未消除，不构成安全沙箱。
- 仅验证普通文本 PDF，两页、`Rotate 90`；未验证长书、扫描件、OCR、复杂字体、表单、注释、中文排版或页数/字节超预算拒绝的 UI。
- 未在 Android 真机、平板、折叠屏或商店签名包验证；APK 使用 debug key。
- Android 触摸选文、IME、摘录保存和从想法回源本轮未逐项截图。
- macOS/Linux/Windows/iOS 仍缺各自的运行库打包与签名流水线；当前只登记 macOS arm64 与 Android arm64。
- PDF 页图是当前渲染证据，不是 Core 永久内容；缩放、几何高亮、Reader Surface、普通插件导入 wire 和完整阶段 D 旅程仍未完成。
