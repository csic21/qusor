# 真实 PDF 导入与原生页图

日期：2026-11-13。状态：macOS 真窗口 UI 与 Android API 36 arm64 模拟器子项通过，含 Library 0.7 保存范围重开高亮；不是完整阅读器、Reader Surface、插件导入 wire 或发行资格。

## 本轮范围

- 资料库“导入 PDF”调用可信应用入口 `LibraryHost.importPdf`，用户选择的字节不进入插件。
- 原生层把原件写入私有 job 目录，调用固定 PDFium `chromium/8125` 生成有界报告，投影为 Core `PdfSnapshot`，再由 `KnowledgeApplication::import_document` 一次写入原件 CAS、映射、回执、审计、outbox 与索引任务。
- PDFium 调用集中在进程内单一线程 `qusor-pdf-service`，不阻塞 UI；这不是崩溃隔离或安全沙箱。
- 阅读视图从 Core 原件重新渲染当前页，经 `ImageData -> File -> URL.createObjectURL` 显示原生页图；文字层继续用于选文、复制和“选文 + 想法”保存。
- 原生 PDF 服务保留最多 3 页、32 MiB 编码页图的 LRU，并在页图显示后预取下一页；命中缓存时不重复运行整份 PDFium 报告。
- `PdfPageFile.geometry` 只对 `Rotate 0`、`text_status=mapped` 且几何预算内的页返回字符像素框。资料库用原生 Canvas 命中这些框、显示选区，并把同一 scalar range 接入现有“复制/保存摘录”Core 事务；旋转或不完整映射页不伪造页图选区，继续使用下方文字层。
- Library Host API 0.7 增加 `SavedSelections { document_id, page_index }`，返回该页最多 64 个已提交摘录的 scalar 范围。Core 投影按 `(start, end)` 去重、以最小 capture ID 标识，复合摘录展开同页 segment；不上传引文正文或想法，不做相似文本匹配。
- 应用 Canvas 先绘制 `pdf-saved-selection` 的持久范围，再绘制当前 `pdf-selection` 拖动范围。0.6 请求保持原义，只有 0.7 能调用保存范围 operation。
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

页图选区纯函数回归 5 项通过：尺寸绑定、最小包含字形命中、glyph 顺序到精确 scalar range/像素框的映射、持久范围到同一像素框的映射，以及重叠保存范围合并为一层高亮。应用 PDF 导入/读取回归 3 项通过，包含真实选文提交后 `savedSelections` 回读。Library 0.7 wire 回归还覆盖旧 0.6 拒绝、64 个范围预算和复合 segment 展开。

Android API 36 arm64 模拟器实际验证：

- 系统文件选择器选中 1,292 bytes、两页普通文本 PDF。
- 导入后显示“本机 Core · 原生页图 + 文字层”，第一页页图和文字层一致。
- 强制停止重开，同一 PDF 与“已继续上次阅读 · 第 2 页”恢复。
- 第二页为 `Rotate 90`，原生页图与文字层均显示；进程保持存活。
- 在第一页页图上真实拖动，选中 `QUSOR native PDF qualification`（30 字），Canvas 高亮、工具条与 scalar range 一致；点击“保存摘录”后界面显示“选文已保存到本机 Core。”
- 强制停止并重开应用，再次打开同一 PDF/页面；第一行保存范围仍从 Core 读取并以较浅的持久高亮显示。当前 APK 的该流程重新执行并留证，证明高亮不是内存选区的残留。

截图：

- [android-pdf-picker.png](android-pdf-picker.png)：Android 系统文件选择器。
- [android-page1.png](android-page1.png)：导入成功后的第一页。
- [android-restart-page2.png](android-restart-page2.png)：强制停止重开后的第二页与继续位置。
- [android-page2-rotated.png](android-page2-rotated.png)：旋转第二页。
- [android-library-with-pdf.png](android-library-with-pdf.png)：回列表后的 Core 资料元数据。
- [android-page-selection.png](android-page-selection.png)：页图上的原生选区高亮。
- [android-selection-tools.png](android-selection-tools.png)：选区进入复制/保存工具条，显示“已选 30 字”。
- [android-selection-saved.png](android-selection-saved.png)：选文成功提交到 Core。
- [android-saved-selection.png](android-saved-selection.png)：打开已保存摘录的页面，第一行显示持久范围。
- [android-saved-selection-restart.png](android-saved-selection-restart.png)：强制停止重开后，同一范围仍从 Core 恢复。

测试日志：

- [macos-nsh-test.txt](macos-nsh-test.txt)
- [native-cargo-test.txt](native-cargo-test.txt)
- [reading-selection-test.txt](reading-selection-test.txt)
- [library-saved-range-tests.txt](library-saved-range-tests.txt)：Library 0.7 wire 74 项、学习应用 7 项、Host 8 项、根 Workspace 排除既有 macOS `runtime-process` 后 317 项。
- [android-artifact.txt](android-artifact.txt)：APK 哈希、原生库条目、16 KB 对齐与 v3 签名核对。
- [android-saved-highlight-artifact.txt](android-saved-highlight-artifact.txt)：当前源码 APK 与两张截图哈希、设备序列。
- [android-saved-highlight-inject.log](android-saved-highlight-inject.log)：当前 APK 的 PDFium 注入与 v3 签名核对。

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
/private/tmp/qusor-android-saved-highlight-final/QUSOR.apk
sha256 018bf2440b3221e44a8c179093fb75484addff6d6e29cd3f117768e0d76c4c24
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
- 页图选文当前只支持未旋转且映射完整的页；尚未实现跨行有序多段、拖动把手、键盘扩选、任意临时选区持久化或源页精确回显。重开高亮只覆盖已提交摘录的同一页 scalar 范围。
- 未在 Android 真机、平板、折叠屏或商店签名包验证；APK 使用 debug key。
- Android 触摸选文、IME、摘录保存和从想法回源本轮未逐项截图。
- macOS/Linux/Windows/iOS 仍缺各自的运行库打包与签名流水线；当前只登记 macOS arm64 与 Android arm64。
- PDF 页图是当前渲染证据，不是 Core 永久内容；缩放、正式 Reader Surface、普通插件导入 wire 和完整阶段 D 旅程仍未完成。
