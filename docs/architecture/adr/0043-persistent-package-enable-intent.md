# ADR 0043：随应用包的持久启停与 Host 作用域重建

日期：2026-11-12

状态：已实现本机切片；动态安装、卸载 tombstone、第三方包目录和完整包管理界面不在本 ADR 内。

## 背景

Manifest 工作区目录已经能从已验签普通包生成原生入口，但启用状态仍完全来自静态 desktop profile。停用包必须同时撤下工作区贡献和 Host 会话；若只隐藏 UI，旧 Host 仍可继续接受调用，违反“撤权/停用后旧回调不得进入 broker”的生命周期边界。

## 决定

1. **应用本地偏好**：启用意图存放在应用数据目录的 `extensions/preferences.json`，协议版本为 1。它属于插件管理状态，不进入 Core、知识导出或同步；未知字段、未知包和未知版本 fail closed。
2. **包行过滤**：`org.qusor.notes`、`org.qusor.vocab` 的停用意图同时禁用其 Host entry 和 workspace consumer entry；Core provider 不可停用。解析仍在 `capability-composition` 内验证 provides/requires 与激活顺序。
3. **重建而非隐藏**：切换启用状态时，应用用现有 `VaultStore` 句柄重建 `AppRuntime`，旧 Host/Session 整体释放，新 Host 只为仍启用的包创建。Core 的 SQLite/CAS 句柄不关闭，已收录数据不迁移、不删除。
4. **先持久后切换**：新运行时先在锁内构建；偏好文件成功落盘后才替换旧运行时。保存失败保留旧运行时，避免“界面已停用、旧 Host 仍在运行”的分裂状态。
5. **原生管理入口**：功能目录列出随应用包及版本，可停用/重新启用；工作区目录随响应刷新。停用当前打开包时退出该工作区并返回原来源。

## 后果

- 每次启停会重建当前进程内的 Host 作用域；已有会话与许可失效。这是保守的一致边界，不是热卸载单个插件。
- 偏好文件的 Unix 替换使用同目录临时文件 + `rename`；Windows 目标存在时退化为删除后重命名，尚未完成跨平台原子替换资格。
- 当前仅管理随应用构建的两个普通包。用户取得、验证、信任、安装和卸载任意 `.qpk` 仍需持久包内容存储、安装 ID/tombstone、授权恢复和管理界面；不能把本 ADR 计作 F-EXT-03/05 完成。

## 验证

- 应用测试：停用 `org.qusor.vocab` 后工作区目录只剩 notes，`VocabHost` 返回“未挂载”，同一 Core 的 ideas 仍可读；重新启用后仍读出原 `Legacy deck`。
- 重启目录：新 `System` 使用同一 data directory，停用状态仍生效。
- Core 不可停用；未知包和损坏偏好返回错误。
- `nsh check App.tsx`、应用 12 项测试、架构守卫、原生 UI 边界和文档链接检查通过；隔离数据目录的真实窗口完成停用、重启保持、重新启用和原词书读取，见[窗口记录](../../../experiments/nsh-reader/evidence/package-enable-lifecycle/README.md)。
- 跨平台文件替换、任意用户包安装/卸载和并发调用仍未验证。
