# 原生 PDF 指针选区本机记录

后续 C3 已用版本2复合契约补齐实验内多段原子收录，见 [完整记录](../capture-core/COMPOUND_RESULTS.md)。下文保留 C2 当时的资格和未接入状态，不将其旧代码指纹当作 C3 当前证据。

日期：2026-11-08。状态：原生命中、受限 LTR 分段及单段收录子项通过；NSH 指针捕获、复合原子收录与完整阅读顺序未通过。关联 [规格](../../docs/product/native-pdf-selection.md) 与 [ADR 0016](../../docs/architecture/adr/0016-native-pdf-selection-service.md)。

## 固定环境与复现

沿用固定 PDFium 8125 Linux x64 运行库、pdfium-render 0.9.5、Rust 1.97.1、锁定 debug 离线构建。使用现有两页英文 PDF、image-only PDF，新增标准库生成的真实两栏和重叠字形负例，均为自建 MIT 夹具。没有 WebView、网页选择机制或用户文档输入。

```sh
python3 experiments/pdf-provider/validate_selection.py
```

需先按 [运行库准备](README.md) 取得固定库。[机器证据](evidence/linux-native-selection.json) 留存 native/Core 二进制及运行库 hash、开始时固定并在结束时复核的源码指纹、仓库外 SDK 作者和所有结果。较早 PDF/C1 JSON 保留为当时基线，不当作新代码的当前指纹。

## 实际结果

- 原生 provider 的 11 项 Rust 测试通过：原有 6，加选区 5 项综合测试。格式与全目标 Clippy 零警告。选区测试包含真实两页/旋转页以及明确标注的合成坏映射、grapheme、未知 Unicode 等负例；合成测试不算真实中文 PDF 成绩。
- 37 项独立原生进程场景通过。输入点从真实页图字形内部的暗像素取得，不从预期字符索引直接冒充点击；分别测试普通页及 90° 旋转页、默认展示与另一组 zoom/DPI/原点/滚动变换、正反方向，得到相同的明确来源范围。
- 普通页和旋转页多行都得到正确的两个有序 segments。旋转页首段的 engine 索引大于后一段，实际输出保留该差异，未使用 min/max 吞成连续区间。结果标识 `qusor-plain-ltr-lines-v1` 及原始 PDF 行带排序依据，不宣称全文 canonical reading order。
- 空白不吸附邻字；原件、frame 报告或页文本 hash 不符均拒绝。真实两栏返回 UnsupportedLayout，真实重叠字形返回 Ambiguous，真实无文本层返回 NoText。
- 实际事件回放覆盖 Ready 只产生一次、重复 up、取消与迟到 up、第二 pointer 不夺取 anchor、无效 move 清预览、zoom/scroll/DPI/generation 变化取消活动手势。Rust 测试另验证旧 gesture token 带旧 context 的包不能取消新手势。
- 严格 JSON 入口拒绝未知批准字段、重复字段和超出 64 事件的脚本。原生指针与视口含非有限数值或超预算时拒绝，损坏映射不触发切片 panic。
- 12 项 Core 场景通过：可信适配器核对 native frame 与已保留文档的中立语义映射，单段转换为 C1 草稿；普通仓库外 SDK 作者包实际读取选段并构造提案，经明确批准保存。指针释放及预览均不写入知识，重启、统一导出/空库恢复后仍精确解析，并从恢复的原件重新原生渲染。
- 多段桥接实际返回“需要版本化原子契约”，没有构造单段草稿；检查 Core 只有用户确认的那一条摘录，未循环保存或产生半份笔记。

调试图：[普通页多行字形框](evidence/multiline-page-0.png)、[旋转页多行字形框](evidence/multiline-page-1.png)。它们仅显示原生页图上的段落几何，颜色不是已冻结的产品 UI。

## 设计与已发现问题

Native Window DIP、固定渲染图像素及 PDF user space 分开。公式中的 zoom 是固定页图的展示倍率，device_scale 是 px/DIP；frame 报告摘要含渲染信息，与 C1 semantic extraction revision 不混用。永久锚点不保存 viewport 或像素。

首次审查发现第二次 down 会重建 anchor，已改为活动手势拒绝第二/重复 down，保持原 pointer ownership。每次合法 down 分配原生进程内唯一 gesture token；move/up 先检查 ownership，再检查 context，迟到旧包不能取消新手势。无可靠 endpoint 时清预览，松手不复用旧位置。

原生 glyph 的句点框不与字母主体 y 带重叠，不能把所有标点直接当作文字行的共同高度。当前 profile 使用字母/数字建立行带，仍保留标点索引、字形与水平间距检查；遇不确定布局明确拒绝。

## 未通过范围

这是无界面的服务验证。没有真实 NSH pointer capture、窗口关闭回调、原生拖拽/滚动 UX、触控/键盘/读屏、像素传输性能或五平台成绩；事件回放不能代替它们。当前手势取消会清除临时候选，已保存 Core 知识不受影响。

多行采用受限的普通 LTR 行 profile；两栏、复杂缩进、方向、字形旋转、中文/复杂字体和跨页仍需验证。返回有序 segments 不等于完整可靠阅读顺序。C1 尚无复合锚点和单事务批量保存契约，多段目前不可收录，这是下一项未完成工作，不是产品范围缩减。

每次命中仍会验证整份有界报告，尚无生产级空间索引或高频指针性能承诺。父验证器限时和限资源不构成 OS 沙箱，也不证明各平台的原生线程可随时抢占。
