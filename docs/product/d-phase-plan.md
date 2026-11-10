# D 阶段实施计划：正式本地阅读闭环

更新：2026-11-10。状态：D 阶段未完成；D0 的应用层读取契约（分页游标、正文分块、SQLite 重开）已通过，公开 wire、窗口回源与正式包未交付；`plugins/library`、`plugins/reader`、`plugins/notes` 目前均为规划，没有可安装功能包。关联 F-LIB-01、F-READ-01/02/03、F-NOTE-01、F-SRC-01、F-DATA-01、F-EXT-01/03/04/05/06。

## 目标旅程

```text
导入 PDF
→ 阅读并选文
→ 保存原文快照与个人想法
→ 关闭重开
→ 从想法列表打开来源
→ 完整导出到空库并恢复
```

第一条链先做普通文本 PDF；EPUB、长书分页、正式检索和其他平台另有门槛。实验工作区、Core 测试或签名包校验都只证明各自范围，不能代替普通身份安装后的完整旅程。

## 已完成的本轮前置

| 增量 | 实际结果 | 未覆盖 |
|---|---|---|
| 想法工作区 UI v1 | 原生工作台按[交互规格](../design/notes-workspace.md)实现列表/详情/范围/搜索/选取/右键原生菜单/复制/完整正文状态/窄窗；6 项 headless 交互测试、类型检查与 bundle 构建通过，实窗四张截图见[UI v1 记录](../../experiments/nsh-reader/evidence/notes-workspace-ui/README.md) | 仍是窗口内 fixture；正式包、公开 `notes.read` 能力、真实 Core 与编辑/删除未接 |
| 分页游标与正文分块 | 列表改为 keyset 分页（单页最多 200 条，游标绑定 vault/document 精确范围，分页期间新增想法不重排/不跳项）；正文按 Quote/Note 分块读取（4–16 KiB，UTF-8 边界安全，1 MiB 笔记可逐块还原）；应用层与真实 SQLite 重开通过，见 [ADR 0034](../architecture/adr/0034-note-read-model.md) | 公开 SDK wire、插件授权、分页 UI、精确高亮；每次查询仍读整库快照 |
| 有界想法读取模型 | `learning-application` 可按 vault/document 倒序查询 PDF 想法；引文和想法各返回最多 512 UTF-8 bytes 预览，完整内容保持不可变 | 公开 SDK 能力、撤权/版本错误分类 |
| SQLite 重开 | 真实 SQLite 保存长中文想法后关闭重开，列表恢复 capture、标题、页码与预览，完整 note 未截断 | 多平台耐久、超大资料库性能 |
| 原生想法列表 | macOS AppKit 实验窗口真实列出 Core 中两条旧中文想法，并区分当前资料与其他资料 | 正式 notes 包、编辑旧想法、精确高亮、当前资料新建后的回源实窗 |
| 统一工作台下钻 | 想法作为 `PluginWorkspace` 进入 `native-workbench`，宿主返回、插件自有导航、列表/详情和插件内返回已实窗通过 | 仍为窗口内 fixture；尚未接真实 Core/公开插件契约 |

Core 读取证据见 [想法列表实窗记录](../../experiments/nsh-reader/evidence/notes-workspace/README.md)，布局集成证据见 [想法插件工作区记录](../../experiments/nsh-reader/evidence/native-notes-workspace/README.md)。

## 阶段与退出条件

### D0：读取模型和可替换交互

- 稳定公开笔记查询契约：vault/document 过滤、稳定分页、完整正文按块读取、明确缺失/版本/权限错误。当前应用层已具备过滤、keyset 分页、正文分块与缺失/范围错误；公开 wire 与权限/版本错误分类仍待 D1。
- 想法列表只读取 Core，不持有完整知识副本；预览截断不改变持久内容。
- 当前资料可打开所在页；其他资料明确提示需先打开，不假装回源。
- 目标引用恢复为受控 Surface 定位和高亮；不能只落到页首冒充精确回源。

退出证据：两个不同 vault/document 的隔离、重启恢复、缺失原件、版本不符、撤权和迟到结果均明确；真实原生窗口完成列表与来源切换。

### D1：正式 SDK 和普通包身份

- 从权威契约导出 SDK：manifest/schema、读取能力、错误/版本规则、作者构建和验包工具。
- library、reader、notes 使用独立 Workspace/lockfile，仓库外只凭 SDK 工件构建。
- 普通签名包完成安装、授权、启用、更新失败恢复、停用和卸载；宿主二进制不重编译。
- 第一方包与第三方使用同一贡献、权限和生命周期，不按包 ID 写业务分支。

退出证据：至少两个仓库外普通包消费同一契约；旧协议回归、包撤销和卸载后知识保留通过。

### D2：正式功能包与宿主装配

- `plugins/library`：导入、列表、标签/搜索、原件冻结与重开。
- `plugins/reader`：PDF/EPUB 受控 Surface、目录、缩放、位置恢复、选区和标注。
- `plugins/notes`：想法的创建、编辑、组织、引用与回源。
- `apps/`：只做 composition root、插件安装/管理、可信权限/恢复和平台资源，不含阅读或笔记业务。
- Host 提供受控文件、文档、Surface、选择、持久化授权和资源回收；Core 保存规范内容与事务。

退出证据：官方包普通安装运行、撤权与停用后规范内容仍可读取/导出；宿主重开不复活旧选区、旧回调或旧运行许可。

### D3：完整旅程与发行资格

- 导入→阅读→选文→保存→关闭重开→从想法精确回源→完整导出/空库恢复。
- 保存失败、歧义、缺原件、权限撤销、版本变化和结果未知都有可观察状态；只有 Core 回执算保存成功。
- 至少一个桌面平台完成真实窗口、键盘/输入法、正常关闭、资源回收和发行包资格；其他声明平台分别留证。

退出证据：同一冻结应用/插件工件完成整条旅程，输入与恢复记录可复现；卸载 plugins/library/reader/notes 后，已保存规范内容仍可通过宿主通用入口查看和导出。

## 紧接的下一片

1. 将 `native-workbench` 的想法 fixture 换成真实 Core 查询：先落公开 `notes.read` 能力与 Host 预算（协议 0.4），再由普通 notes 包消费；UI v1 已按规格就绪，只差数据与包身份。
2. 在真实窗口完成“当前资料新保存一条想法→关闭辅助区→列表刷新→打开所在页”，再补精确高亮与缺失来源状态。
3. 之后才创建正式 library/reader/notes 包 manifest 与构建入口；不提前用未验证 TS 草案冒充 SDK。

### 公开读取能力的协议约束（2026-11-10 核对）

现有公开 wire 的单次请求/响应预算为 4 KiB，manifest 能力上限 5 个且已被 `SelectionRead`、`DocumentMetadataRead`、`CapturePropose`、`ReaderSurface`、`NoteDraftEdit` 占满。想法列表（最多 200 条、每条 512 bytes 预览）与正文块都远超 4 KiB，切片 2 不能直接复用 0.2/0.3 wire。进入实现前先确定：

- 新增 `notes.read` 能力并提升协议版本（旧 0.1–0.3 保持原义），还是把有界想法数据注入 0.3 component context；
- 列表 wire 的页大小与预览长度（例如每页 ≤8 条、每条预览 ≤128 bytes）及正文块预算；
- vault/document 范围由 Host 绑定、guest 不自报；Host 只做策略与预算，Core 查询由可信后端实现。

该决定影响公开插件契约与兼容性，先留设计记录再实现；应用层分页与正文契约已由 [ADR 0034](../architecture/adr/0034-note-read-model.md) 固定。

## 非目标与约束

- 不把内存文档列表、假 Provider 或实验 TSX import 当作正式包。
- 不让插件直写 SQLite、持有 NSH/DOM/原生指针或数据库连接。
- 不以安装存在自动授予阅读、选区、写入或网络权限。
- 不用预览、页首跳转或相似文本猜测替代精确版本化锚点。
