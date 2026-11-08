# 原生 PDF provider 本机记录

日期：2026-11-08。状态：原生渲染、字符几何及显式范围定位子项通过；可靠阅读顺序、正式存储和阅读 UI 未通过。关联 C-08/C-09、F-LIB-01、F-READ-01/02、F-SRC-01、F-EXT-04/06。

## 固定环境

Linux/WSL x86_64、Rust 1.97.1、debug、固定 Cargo.lock，具体版本与运行输出见 [机器证据](evidence/linux-pdf-provider.json)。使用已发布 `pdfium-render 0.9.5`，只启用 PDFium 8125 ABI；社区 PDFium 158.0.8125.0 的原始归档、动态库、构建参数和 notices 逐文件 hash 留证。运行库不随仓库提交，无 V8/XFA；没有 WebView、PDF.js 或网页显示路径。

两页英文普通文本、图像无文本层和畸形 PDF 均为自建 MIT 夹具，原件 hash 见 [manifest](fixtures/manifest.json)。没有用户文档或账号输入，不访问网络。首次公开依赖/运行库准备与之后的离线验证分开执行。

## 实际验证

- 6 项 Rust 测试通过，覆盖真实渲染与页几何、三类版本核验、无文本层、损坏文件、输入大小/符号链接/已有输出拒绝，以及损坏映射不得 panic。测试中的组合字符和 emoji 用构造的映射检查边界，不是中文或 Unicode PDF 提取成绩。
- 验证器执行 10 条准备/静态/构建命令、17 项独立 worker 场景。每个真实 worker 受父端 30 秒时限及内存/CPU/文件限制；正常返回和拒绝路径均执行，未通过故障注入证明所有平台的挂起回收或 OS 沙箱。
- 原件一次冻结读取后加载；原件 hash、提取 profile/hash、页文本 hash 分离，重启另一个 worker 后报告与页图完全相等。错误原件/提取/页版本、单位、引文或页索引均拒绝，不采用相似文本回退。
- 页 0/1 分别有 CropBox 偏移及 Rotate 0/90。独立像素扫描验证 136/107 个字母字符框都命中真实墨迹，Rust 测试另检查所有有几何的字符。重复引文用两个明确范围得到不同位置，旋转页的实际选段也可解析。
- image-only PDF 成功渲染并报告 `no_text`，精确文字选择拒绝；没有自动 OCR 或捏造文本。
- 格式检查、全目标 Clippy 零警告及依赖图守卫通过。normal/build 本机声明许可去重清单见 [TSV](evidence/linux-native-dependencies.tsv)，不代替产品发布 SBOM 或许可审计。

可检查的调试图：[选段几何](evidence/selection-geometry.png)、[普通页](evidence/page-0.png)、[旋转页](evidence/page-1.png)。这些是 provider 原生页图及测试叠框，不是 NSH 应用界面。

## 发现与修复

最初使用 `set_fixed_size` 放大位图画布，但原生 matrix 渲染仍为原尺度，而坐标转换使用放大尺度，导致高亮偏移。实际页图扫描发现此问题，改用 `set_target_size` 将绘制与映射保持在同一实际尺度，并加入真实像素回归。没有通过扩大误差容限或缩小测试范围掩盖问题。

旋转页实际字符流按与源内容构造相反的行序返回。原件文字均存在、单字符几何准确，不等于阅读顺序可靠。报告保留引擎实际流和顺序差异，不重排文本冒充准确。原生选区拖动、多行次序和提取规范仍需专门验证，尚不能作为正式 canonical reading text。

## 未通过范围与下一步

safe wrapper 的检查仅为 Unicode scalar 合法性，未调用完整 UnicodeMapError API。中文/嵌入字体/连字/多栏/RTL/复杂 ToUnicode、密码/OCR、更多平台均未验；16MiB 原件、16 页、全篇 100000 字符、页图最长边 2048 的预算不代表生产规模性能。

动态库仅由可信实验者提供并由父验证器核对固定 hash，不证明防同 UID 恶意替换。输入 worker 的原生解析不等于安全沙箱。没有测 NSH 像素传输成本、缩放/选区 UI、IME、关闭重开或可访问性。

没有正式 Core PDF 二进制 CAS、版本化原件/页映射事务、签名阅读插件或 PDF 导出恢复。本轮不将提取文本塞入 C0 的来源字段冒充 PDF 收录。下一步需要完整的 PDF 原件/提取/锚点契约及存储资格，同时解决原生阅读顺序与 Surface 门槛。
