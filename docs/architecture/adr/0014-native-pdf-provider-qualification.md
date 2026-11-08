# ADR 0014 原生优先与 PDF 服务资格验证

日期：2026-11-08。状态：[独立本机渲染/几何子项通过](../../../experiments/pdf-provider/RESULTS.md)，可靠阅读顺序与生产集成未通过。关联 C-08/C-09、F-LIB-01、F-READ-01/02、F-SRC-01、F-EXT-04/06。

## 问题与决定

用户明确采用 NSH 后尽量 native、少用 Web。宿主优先 NSH 原生控件与原生绘制/编辑服务，功能仍通过普通插件贡献点和公开能力接口接入；原生优先不构成第一方直接访问 NSH 内部对象或数据库的特权。Web 路线仅在具体原生缺口下重新评估，不能因网页库现成而默认嵌入。

PDF 沿已规划的原生 PDFium 路线，新增独立 `experiments/pdf-provider`。本轮资格问题是：真实 PDF 是否能在原生 worker 中渲染、提取逐字符来源映射，并在页旋转及 CropBox 偏移后按相同版本准确定位选段。没有 NSH 窗口时只记录 provider 结果，不记录阅读 UI 通过。

使用已发布的固定 `pdfium-render 0.9.5` safe wrapper，关闭默认 image/其他 ABI 特性，显式选择 `pdfium_8125`。上游 master 当时为未发布 0.9.6，不能把 master 版本号当可下载 crate。运行库固定社区构建的 PDFium 158.0.8125.0 Linux x64，无 V8/XFA；归档和实际动态库分别留 SHA-256、版本、构建参数及许可指纹。该构建来源与 Google 官方源码来源分开记录，不声称为官方二进制或已完成产品分发许可审计。

## 数据与定位边界

原件 hash、提取 profile/revision、逐页文本 hash、零基页索引、PDF 字符索引、Unicode scalar、UTF-8 byte 与 UTF-16 单位分别保留。PDFium 生成的空格/换行及未知 Unicode 不静默跳过；精确定位不能用字符串相似度修复。字符几何的 PDF user space 与渲染像素坐标分别标明，通过实际旋转页图核对。

安全 wrapper 没有公开全部映射错误检测时，只证明实际检查过的映射，不将合法 Unicode scalar 推断为可靠字体映射。尚未验证的中文、连字、多栏和复杂字体保持未通过。

C0 的文本快照不能替代 PDF 二进制原件及页映射。provider 不直接写 C0 数据库、不将几何塞进 source.reference，不将其结果冒充正式 PDF 保存。下一步 Core 版本化文档契约须显式保存原件、提取版本及锚点，并在同一事务建立引用；卸载后保留和完整恢复仍是后续验收。

## 隔离与限制

独立 Workspace 与固定 lockfile 登记在依赖策略，生产根不加 PDF/NSH 依赖。单次 worker 串行使用 PDFium，父测试器限制输入、运行时间与输出；该限制不等于 OS 沙箱。动态库加载仅接受可信实验者提供的固定运行库，不接受文档或插件指定任意库。

英文合成两页普通文本 PDF 验证 CropBox/旋转/重复句；image-only 表明“可显示而无文本层”，不启用 OCR 或捏造文本。损坏文件明确失败。夹具自建 MIT，不读取或上传用户资料。
