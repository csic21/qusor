# NSH 原生 PDF Surface 探针

状态：本机原生页图与桌面鼠标选区子项通过；完整阅读旅程未完成。关联 P03/P05/P07/P08、F-READ-01/03、F-SRC-01 与 C-09。架构见 [ADR 0020](../../../docs/architecture/adr/0020-native-pdf-surface-probe.md)，体验与退出条件见 [选区规格](../../../docs/product/native-pdf-selection.md)。

复用固定 NSH 与本地依赖前缀，在 [native-host](../native-host/README.md) 独立 Workspace 增加一个原生模块和 host bin。准备阶段真实执行 PDFium provider，页面 PPM 与字符映射一同冻结；PNG只做显示编码转换，NSH Image/Canvas均为原生节点，非WebView。报告字节 SHA 构建进 bundle，原生启动拒绝不匹配的报告。准备产物与运行数据均在忽略的 target 中，显示 PNG 和绑定源文件由准备脚本生成。

```sh
python3 experiments/nsh-reader/native-host/prepare_pdf.py
python3 experiments/nsh-reader/native-host/run_pdf.py
```

前置条件同上级原生实验，并需要已校验的 [PDFium运行库](../../pdf-provider/README.md)。衍生锁仅接受固定 NSH 根锁与既有 provider 锁的 registry 版本/来源/checksum受控并集；未移除漂移检查，也不意味着依赖发布许可核查已完成。

当前只验证桌面鼠标局部DIP事件，原生 PointerSession负责generation/gesture与映射。候选引文显示“尚未保存”，不代表业务写入；本探针不是正式reader插件、SDK能力、持久化旅程或五端验收。中文输入、异步取消和移动触控仍分别处理。


## 实际结果（2026-11-08）

[结构化证据](../evidence/linux-native-pdf-surface.json) 包含当前源码、报告/PNG/bundle/可执行文件摘要、实际指针轨迹、完整日志与截图。固定 NSH tracked 源码保持干净，无 WebView/DOM。原件摘要沿用真实两页合成 MIT PDF，准备进程通过现有 PDFium provider 实际渲染；没有把预制截图代替 PDF 渲染。

- 1000×1000 原生窗口，60%第一页：真实 raw Touch/responder 拖动经过 `Dragging(QUS)`、`Dragging(QUSOR)`，物理鼠标释放后才 `Ready(QUSOR)`；[页图高亮与独立候选](../evidence/native-pdf-selected.png) 一致，文件名、页码及尚未保存可见。
- 70%第二页（源PDF带90°旋转）：真实竖向拖选得到 `Selection`，原文、高亮和候选一致，见 [旋转页](../evidence/native-pdf-rotated.png)。
- 实际点击空白得到 NoHit，拖动出纸页得到 Cancelled，均清空叠层/候选，无技术错误弹窗；实际缩放/换页/窗口变宽窄均更新generation并清选区。
- 异步 Image 加载以不可变generation作为节点key，旧节点回调不能放行后继frame；每次configure清空readyGeneration，当前Image onLoad后才允许responder领取输入。首次立即截图可能仍在加载，不能把白页或最终截图单独作为完整加载安全证据。
- [600×1000窄窗](../evidence/native-pdf-narrow.png) 自动约49%，候选移至下方，纸页及控件可见。只记录该尺寸，不声称已通过移动布局或所有窗口尺寸。
- 三项原生适配边界测试通过：报告读预算/构建摘要、换页缩放的旧generation/gesture与非所有者拒绝、非法结束点清理及close后拒绝。`cargo clippy --all-targets --locked --offline -- -D warnings` 通过。两者使用 `native_prefix.runtime_environment`，先执行上述prepare以准备真实provider测试输入。
- 最终窗口通过正常WM_DELETE退出，实际wait退出码0，无探针进程残留。

保留的失败解释了适配取舍：最初thread_local初始化位于main而JS在worker，实际窗口读不到状态，改用仅含中性report/session的Mutex；PressUp会在press移动阈值处提前发生，改用raw responder；helper解析combine4.6.9被摘要基线守卫拒绝，精确固定回已资格上游4.6.8；初次auto-fit的过渡小尺寸导致25%，修正为保留desiredZoom并等待稳定布局，25%图不计可读UX通过。完整失败日志与Mesa警告见证据。

这只是受控原生服务适配实验。模块直接由可信探针TSX调用，不是正式公开插件Surface协议；原生单实例状态也不等于多文档宿主。保存/重开/引用回源/完整导出尚未从此窗口贯通，中文输入、键盘、无障碍、异步取消、触控、EPUB和发布平台门槛保持未验证。
