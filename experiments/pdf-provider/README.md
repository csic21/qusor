# 原生 PDF 服务资格实验

2026-11-08 增量：原生点命中、手势生命周期、受限 LTR 有序分段与单段 C1 收录见 [选区本机记录](SELECTION_RESULTS.md)。复现命令为 `python3 experiments/pdf-provider/validate_selection.py`。早期 JSON 是当时渲染/映射的历史基线，选区增量另存 `linux-native-selection.json`。

独立验证 PDFium 的真实页图、逐字符映射与选段几何，不接生产 Workspace。关联 C-08/C-09、F-LIB-01、F-READ-01/02、F-SRC-01、F-EXT-04/06；决定见 [ADR 0014](../../docs/architecture/adr/0014-native-pdf-provider-qualification.md)。原生 UI 优先，当前不使用 WebView/PDF.js。

## 准备固定运行库

采用社区发行的 [PDFium 158.0.8125.0 Linux x64](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F8125)，不是 Google 官方二进制。下载只需一次；不安装到系统目录：

```sh
curl -fL 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-linux-x64.tgz' -o /tmp/qusor-pdfium-linux-x64-8125.tgz
python3 experiments/pdf-provider/prepare_runtime.py /tmp/qusor-pdfium-linux-x64-8125.tgz
```

准备器要求目标为空，严格核对归档 hash、成员路径/类型/总量及动态库 hash。归档 SHA-256 为 `e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659`，库为 `1c251efce460d67437a4c76d4be9033ae2f17bf5603f5bff1af10209b94ca723`。下载失败不退回可变 latest 或系统同名库。

`pdfium-render 0.9.5` 只启用 `pdfium_8125` ABI，默认 image/其他 ABI 特性关闭；运行库构建参数禁用 V8/XFA。首次用 `cargo fetch --manifest-path experiments/pdf-provider/Cargo.toml --locked` 准备缓存，随后验证离线运行：

```sh
python3 experiments/pdf-provider/validate.py
```

## 输入、输出与边界

夹具由 [标准库生成器](fixtures/generate.py) 创建，采用 MIT 许可，不使用用户资料：真实两页普通文本 PDF，CropBox 偏移及旋转不同；无文本层的图像 PDF；损坏 PDF。原件 hash 和文字构造预期见 [manifest](fixtures/manifest.json)。未验证中文或嵌入字体，不能从英文结果外推。

provider CLI 仅由可信实验宿主调用：

```text
inspect LIBRARY PDF OUTPUTDIR
resolve LIBRARY PDF PAGE START END ORIGINAL_HASH EXTRACTION_HASH PAGE_TEXT_HASH UNIT EXACT_QUOTE OUTPUTDIR
select LIBRARY PDF OUTPUTDIR SCRIPT_JSON
```

`OUTPUTDIR` 中页图文件必须尚不存在。worker 一次冻结并读取有界 PDF，在同一串行 PDFium 实例中渲染与提取；每页保留 engine char index、Unicode scalar/UTF-8/UTF-16 映射、原始 PDF 坐标、未旋转裁剪框归一化坐标及实际像素 quad。原件、提取版本和页文本 hash 分开核验。未知映射用显式占位状态保留位置并禁止精确选段，不静默丢字。

`select` 脚本由 binding、viewport 和 operation 组成，operation 为 hit/drag/trace。frame_report_sha256 对应本原生报告的 extraction_sha256，不等于 Core 的 semantic extraction_revision。指针坐标是 DIP，逆变换为 `(point + scroll - origin) * device_scale / zoom`。trace 仅作可信测试回放：省略 gesture_id 时引用最近 down 的返回 token；正式异步 NSH 适配器必须携带实际 native gesture token，不能使用该简写。源/视口失效、取消或无有效 endpoint 不生成 Ready，更不写 Core。

父验证器限制 worker 时间、内存、CPU 和单文件输出，以真实页图暗像素独立核对字符几何。限制不构成 OS 沙箱。库路径只由可信实验者提供，文档/插件无权指定；本轮不声称防御同 UID 对运行库的恶意替换。

## 与完整阅读闭环的关系

本实验只验证原生解析/渲染服务。它不保存 Core 的 PDF 原件或正式摘录，也未接入签名阅读插件和 NSH Surface。完整旅程还需版本化的 PDF 原件/提取/锚点契约、Core 原子入库和导出恢复，以及原生选区、高亮、缩放、进度和重开 UI 验收。

PDFium 字符流不必等于可靠阅读顺序；旋转页的实际提取行序必须留存，不得按预期文本重排后宣称一致。safe wrapper 当前只检查 scalar 合法性，不能代替完整 Unicode-map-error 检测。相关限制与实际结果见 [本机记录](RESULTS.md)。
