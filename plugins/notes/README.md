# Notes 插件

状态：真实 Rust `wasm32` + 声明式原生 UI 插件已落位，支持 PDF/EPUB 想法列表、分页、筛选、搜索、详情、正文分块、编辑、删除和“打开原文”。Reader 的 `source_capture` Wasm→SQLite Core 回源用例已在测试源码落位，但本轮尚未执行、无结果；Notes 点击“打开原文”→宿主解析→Reader 的完整链仍未覆盖。v0.3.1 回源改动的构建与实窗/模拟器/真机验证待本轮回填。

目标是 F-NOTE-01 与 F-SRC-01：保存原文引用与个人想法，按资料组织、编辑和删除，并从已保存摘录回到来源节次。插件只消费公开 SDK、正交知识能力与 Host 授权，不直接访问 SQLite、Host 私有状态或 NSH 内部对象。

## 本轮已有

- `manifest.json`：Host API 0.8，required `notes.read`、`ui.slot`，贡献 `org.qusor.notes.workspace`。
- Rust UI slot：提供 PDF/EPUB 想法混排列表、范围筛选、关键词搜索、分页、详情、正文分块读取、编辑保存和两步删除确认。
- 打开原文：详情将选中想法编码为 `open_source:{format}:{document_id}:{section_index}:{capture_id}`，由宿主安全解析后向 Reader 发起一次性回源导航；无效格式、ID 或动作被拒绝。
- `build.py`：把公开 SDK 导出到仓库外临时目录，以 `wasm32-unknown-unknown --release --offline --locked` 构建真实 Wasm，并由普通签名包工具生成 `.qpk`。
- 既有验证：真实 SQLite Core 上的签名包安装/授权/执行/分页读取，以及 `extension-store` 持久安装、重开、停用与卸载后想法仍可读，见[包记录](../../experiments/capture-core/evidence/notes-package/README.md)。

## 未完成与边界

- 自动化测试覆盖列表、筛选、搜索、正文分块、编辑、错误提示与两步删除，并断言详情存在准确的“打开原文”动作 ID；未真正点击该动作，未覆盖从详情点击“打开原文”→宿主解析→Reader 的完整链、跨格式筛选回源、失败回滚及所有关闭/返回路径。Reader 的 `source_capture` 回源用例源码已落位，但本轮未执行、无结果。
- 原文、用户想法与 AI 推断保持分离；插件通过 Host 请求知识，不持有数据库连接。卸载插件后内容仍由 Core 保存并可读取。
- v0.3.1 构建待本轮回填；macOS 实窗、Android 模拟器与 Android 真机均待验证；当前源码落位与已落位用例不代表发布产物或回源链已通过验收。

## 构建

```sh
python3 plugins/notes/build.py
```

产物在 `plugins/notes/target/`（已被 gitignore）：`notes.qpk`、`author-proof.json`。固定测试签名种子只用于资格验证，不是生产发布密钥。

相关设计见 [ADR 0035](../../docs/architecture/adr/0035-notes-read-capability.md)、[功能规格](../../docs/product/notes-plugin.md)、[UI 设计](../../docs/design/notes-workspace.md) 与 [ADR 0056](../../docs/architecture/adr/0056-orthogonal-host-api-and-full-plugin-migration.md)。
