# ADR 0016 原生 PDF 命中、手势与有序选段服务

日期：2026-11-08。状态：[独立本机子项通过](../../../experiments/pdf-provider/SELECTION_RESULTS.md)，NSH 适配和完整复合锚点未通过。关联 [选区规格](../../product/native-pdf-selection.md)。

## 决定

在现有 `experiments/pdf-provider` 新增选区模块，不引入 DOM/WebView，不复制 Core、PDFium 或新建空业务 crate。基于真实冻结报告的字形几何和索引进行命中，显式处理无命中、多候选及映射不完整。

服务接收带 generation 的 viewport 和带原件/页/frame hash 的版本绑定，将 DIP 点转换到该固定页图的像素空间。frame hash 是渲染帧的身份，不等于 C1 的语义提取版本；两者由可信适配器核验映射，持久记录继续仅保存文档语义来源及 PDF user space 几何。

指针状态机与命中算法分开：down/move/up/cancel、pointer ownership、源变化和 viewport 变化决定候选是否有效。失效或取消清除当前可提交预览；不复用最后有效点、不自动写 Core。NSH 平台捕获事件和窗口生命周期仍需真实界面验证，CLI 事件回放不代替它们。

普通 LTR 多行使用可解释的限制 profile：保留 engine 行/字符映射，以原始 PDF 几何验证行关系，再输出有序 segments。无法确认布局时拒绝，不将启发式排序升级为全格式 canonical text。各 segment 不要求 engine 索引全局单调，尤其旋转页；不得取 min/max 吞成一个连续范围。

现 C1 只支持一个连续 PdfDraft。单段结果可通过现有公开能力和批准链路保存；多段必须等待显式复合契约及单事务支持，不借多次独立保存绕过原子性。该边界是未完成工作，不缩减完整阅读旅程。

## 证据门槛

真实原生页图的命中、旋转、多行及变换检查和指针生命周期分别留证；合成异常只证明异常处理。旧渲染/字符映射测试必须保持。Core 不依赖本模块的屏幕坐标或手势对象，插件不获得 native/FFI 句柄。
