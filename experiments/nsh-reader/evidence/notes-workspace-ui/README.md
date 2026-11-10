# 想法工作区 UI v1 记录

日期：2026-11-10。关联 F-NOTE-01、F-SRC-01、F-UX-01/02、C-11/C-12/C-16。规格见[想法插件规格](../../../../docs/product/notes-plugin.md)，交互见[想法工作区设计](../../../../docs/design/notes-workspace.md)。

## 本轮范围

在 `native-workbench` 原生样例中把“想法”工作区按正式交互规格实现到可评审状态，数据仍是窗口内 fixture，不接 Core、不是安装包：

- 列表：范围导航（全部想法/当前资料）、搜索、选中/悬停/焦点三态、完整内容与原件缺失提示；
- 详情：引文与个人想法分区、可选取文本、复制引文/想法/引用、当前资料回源动作、其他资料与缺原件的明确提示；
- 右键菜单：原生 `Menu.popup`，行右键先选中再弹出；含打开所在页、三种复制、读取完整内容、编辑/删除（禁用并说明原因）；
- 键盘：↑/↓ 移动选择、Enter 打开、`mod+c` 复制、`mod+shift+c` 复制引用、`mod+f` 搜索、Shift+F10 菜单、Escape 返回；
- 完整正文：预览截断时“读取完整内容”，加载中/完成状态；失败保留预览（fixture 用延时模拟分块读取）；
- 窄窗（<1120pt）：列表/详情单列切换，返回保留筛选与选择；导航内可切范围；
- 复制反馈：详情状态行就地显示“已复制…”，不弹系统通知。

## UI 细节（2026-11-10 补）

- 统一控件刻度：compact 28 / regular 30 / large 34，图标 15/16/17、文本 12/13；输入框 32；菜单项 30、菜单弹层内边距 5/圆角 10。共享实现见 `ui.tsx` 的 `control()`、`Tool`、`TextButton`、`menuItem()`、`menuPopup()`。
- 修复引文强调线：固定 NSH 的 macOS 开发宿主会把单侧 `borderLeftWidth` 画成横线，改为独立 3pt `View` 竖条；列表与详情一致。
- 去掉详情页脚重复的“打开所在页”，只保留页头一个主操作；页脚只留来源状态说明。
- 横幅里的“读取完整内容”改为 raised 小按钮，避免与 `surface-inset` 背景融为一体。
- 侧栏行 34/圆角 8、搜索框 32、菜单行 30，与插件内控件同尺度。

## 实际验证

| 检查 | 结果 |
|---|---|
| 类型检查（固定 NSH 类型） | `nsh check App.tsx` 通过；同时修正了样例原有的两处无效属性（Tabs/Dialog 的 accessibilityLabel） |
| bundle 构建 | `prepare.py --build-only` 通过，bundle sha256 `c5de99ab…` |
| headless 交互测试 | `nsh test tests/notes-workspace.test.tsx` 6/6 通过，覆盖详情/返回、右键菜单项与禁用态、复制引文/想法/引用、缺原件、完整正文、键盘与窄窗；日志见 [tests.log](tests.log) |
| macOS 真实渲染 | 通过 `nsh test --target macos` 在 macOS 开发宿主窗口内驱动并截图（列表、详情、截断、完整正文）；见 [list.png](list.png)、[detail.png](detail.png)、[truncated.png](truncated.png)、[full-body.png](full-body.png)，日志 [visual.log](visual.log) |

截图与 headless 测试都针对最终构建；headless 测试运行在固定 NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f` 的 macOS 平台模拟上，截图来自同一固定的 macOS 开发宿主，不是真机输入法/辅助技术资格。

## 未覆盖

- 不是正式 `plugins/notes` 包：没有 manifest、签名、安装/授权/停用链路，数据为自写 fixture。
- 未接真实 Core、公开 `notes.read` 能力、分页游标与正文分块（读取模型已在应用层通过，见 ADR 0034）。
- 真实窗口中的右键菜单、文本拖选、剪贴板与 Shift+F10 未逐项人工复验；headless 已覆盖行为。
- 编辑/删除/标签/收藏等写入能力未实现；菜单中禁用并说明原因。
- 中文输入法、辅助技术、其他平台、性能与资源释放未验证。

## 复现

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --build-only
cd experiments/nsh-reader/native-workbench
../target/nsh/target/debug/nsh test tests/notes-workspace.test.tsx
../target/nsh/target/debug/nsh test tests/notes-workspace-visual.test.tsx --target macos
```
