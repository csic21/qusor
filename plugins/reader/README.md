# Reader 插件

状态：真实 Rust `wasm32` + 声明式原生 UI 插件已落位，提供 PDF/EPUB 书架、目录与分节阅读；源码已接入 `source_capture` 回源。`source_capture` 的 Wasm→SQLite Core 回源用例已在测试源码落位，但本轮未执行、无结果；v0.3.1 的构建与实窗/模拟器/真机验证待本轮回填。

目标是 F-LIB-01 与 F-READ-01/03：在受控原生 Surface 中阅读 PDF/EPUB，并从 Notes 的已保存摘录精确回到来源节次。插件只消费公开 SDK 与正交知识能力，不直接访问 SQLite、Host 私有状态或 NSH 内部对象。

## 本轮已有

- `manifest.json`：Host API 0.8，required `library.read`、`ui.slot`，贡献 `org.qusor.reader.preview.workspace`。
- Rust UI slot：提供资料库书架、PDF/EPUB 筛选、打开文档、目录跳转、节次前进/后退，以及普通 `reader-doc:{format}:{document_id}:{section_index}` 受控 Surface 引用。
- 回源导航：接受 Host 透传的 `SlotNavigation { route, launch_token, payload }`；仅处理 `route = source_capture`，校验非空 `launch_token`、object payload、PDF/EPUB 格式、正 document ID、u32 section index、节次范围，并要求同一 `capture_id` 同时存在于该节 saved selections 与该文档笔记中。无效目标明确报错；相同 token 去重。
- 焦点 Surface：回源成功时生成 `reader-doc:focus-{capture_id}:{format}:{document_id}:{section_index}`，界面标注所选摘录及 PDF 几何定位或 EPUB 字符区间信息。该标记是交给宿主的定位提示；宿主尚未解析焦点编号，PDF 可见几何高亮未实现，EPUB 也不提供原生几何高亮。
- `build.py`：把公开 SDK 导出到仓库外临时目录，以 `wasm32-unknown-unknown --release --offline --locked` 构建真实 Wasm，并由普通签名包工具生成 `.qpk`。

## 未完成与边界

- Host 的 `ControlledDocumentSurface.tsx` 仍按 Surface ID 末尾三段解析文档与节次，并固定页图显示尺寸；尚未解析 `focus-*` 或绘制可见几何高亮，页面宽高比与严格请求 fence 也待补齐。因此不能把“焦点 Surface ID/提示”当作 PDF 可见高亮已经验收。
- 既有 Reader 测试源码覆盖书架、打开文档、受控 Surface 与节次导航；新增的 `source_capture` 回源用例源码覆盖 PDF/EPUB 焦点 Surface、缺失 capture、节次越界及非法 navigation JSON/非 object，但本轮尚未执行、无结果。Notes E2E 只断言“打开原文”动作 ID 存在，未真正点击，故点击→宿主解析→Reader 完整链、跨格式筛选回源、去重、失败回滚及所有关闭/返回路径仍未覆盖。
- EPUB 只支持字符区间定位，不能做原生几何高亮；长书、缩放、OCR、跨行拖动把手与辅助技术未完成。
- v0.3.1 构建待本轮回填；macOS 实窗、Android 模拟器与真机均待验证；当前源码落位和已落位用例不代表发布产物或回源链已通过验收。

## 构建

```sh
python3 plugins/reader/build.py
```

产物在 `plugins/reader/target/`（已被 gitignore）：`reader.qpk`、`author-proof.json`。固定测试签名种子只用于资格验证，不是生产发布密钥。

相关设计见 [ADR 0046](../../docs/architecture/adr/0046-library-host-api-0-6.md)、[ADR 0047](../../docs/architecture/adr/0047-native-pdf-import-and-page-images.md)、[受控 Surface 规格](../../docs/product/reader-surface-capability.md) 与 [ADR 0056](../../docs/architecture/adr/0056-orthogonal-host-api-and-full-plugin-migration.md)。
