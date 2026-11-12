# ADR 0041：普通包 Manifest 驱动的 Workspace 贡献目录

- 日期：2026-11-12
- 状态：本机实现并通过真实窗口；随应用包停用/启用后的目录刷新已完成，任意用户包动态安装/卸载未完成
- 关联：C-07、C-11、C-12、F-EXT-01/03/04/06
- 承接：[ADR 0038](0038-capability-composition.md)、[ADR 0040](0040-runtime-mount-scope.md)

## 背景

生产 shell 的工作区入口仍来自 `contributions.tsx` 的固定数组：`fixture.notes`、`fixture.vocab` 和阅读样例被写在同一列表里。普通包 manifest 已经声明 `notes_workspace`/`vocab_workspace`，但 UI 不消费这些声明。结果是：

- 包存在/启用与 UI 入口没有同一来源；
- 工作区 ID 仍是 fixture 名，不能对应真实贡献；
- 未来 reader/library 加入时又要修改 shell 业务数组；
- “界面样例”和“已验签插件贡献”在视觉上无法区分。

## 决策

1. **应用内建立 `package_catalog`**：集中嵌入 notes/vocab 的 `.qpk`、发布者公钥和 package ID；Host runtime 与贡献目录都通过它验包，避免各模块重复常量。
2. **从验证后的 Manifest 提取 Workspace 描述**：只读取 `notes_workspace`、`vocab_workspace`（以及预留的 `reader_workspace`）贡献；entry 必须同时存在于 resolved profile。不会因为包里声明了贡献就自动启用。
3. **`CompositionHost.workspaces()` 是 shell 的发现入口**：返回已验证、已启用的 contribution id、package、title、kind；不返回包路径、授权或 Core 内容。
4. **TSX 只按贡献 `kind` 选择渲染器**：notes/vocab 工作区的标题来自 manifest，图标和说明来自通用 renderer；不再按 package ID 写 shell 分支。
5. **界面样例单独列出**：阅读 fixture 暂时保留为“界面样例”，不混入“扩展工作区”。样例不消费 Core，也不冒充安装功能。
6. **profile entry id 与 contribution id 对齐**：workspace entries 改为 `org.qusor.notes.workspace`、`org.qusor.vocab.workspace`；Host/provider entry 继续使用应用实例 id。

## 后果

- 真实窗口侧栏现在显示“扩展工作区：想法、背单词”和“界面样例：阅读”。
- 打开真实工作区仍由 NotesHost/VocabHost 的普通包授权链路执行；manifest 仅提供发现元数据。
- 新增 workspace 包时，需要加入 catalog、提供已验证 manifest/profile row 和对应通用 renderer；无需修改 shell 的包名判断。
- 随应用包停用/启用后的目录刷新与整体 Host 作用域重建已实现；任意用户包动态安装/卸载、权限变化、版本切换和独立 UI 作用域卸载仍未实现。

## 验证

- `package_catalog` 单测：从两个普通包 manifest 得到 2 个启用 workspace，标题分别为“想法”“背单词”。
- `nsh check App.tsx` 通过。
- 隔离 HOME 的真实临时包窗口：侧栏出现“扩展工作区/界面样例”分组；打开想法与背单词均读取共享 Core。

## 未验证

- 任意用户包动态安装/卸载后的目录更新；
- 非预装包的 catalog 加载；
- UI contribution 的运行时作用域/释放；
- reader/library 正式包。
