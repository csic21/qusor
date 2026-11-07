# QUSOR 阅读 编辑 画布和知识图谱技术路线

研究日期：2026-11-07。NSH固定提交913fd76e1f8434ec6cdf248f4476c1ebe08af19f，Workspace 0.4.0。下文的源码事实与选型建议分开陈述，所有M-1指标均为待验证目标。本轮未编译阅读器或测试实体设备；其他候选库进入实现前再固定版本、二进制校验和及许可清单。

## 1 路线决定

| 模块 | M-1首选验证路线 | 条件性备选 | 开工门槛 |
|---|---|---|---|
| PDF | Rust PDFium adapter，分页/分块位图，自有选择/高亮层 | NSH WebView + PDF.js | 五端构建、位图显示成本、文字选择、精确锚点 |
| EPUB | 受控WebView + EPUB.js；Rust负责包结构、文件与来源 | Readium TS；移动必要时Readium Swift/Kotlin Navigator | 离线资源、中文重排、CFI、多章节标注、桥接隔离 |
| Markdown | 原始Markdown + 稳定块ID；NSH Editor/TextView | WebView编辑器 | 中文IME、源码到选择映射、持久化 |
| 富文本 | NSH原生RichEditor + QUSOR受限schema | WebView Tiptap/ProseMirror；M0可只做Markdown | 移动IME、undo、格式往返、块ID |
| Think Space | Rust canvas-engine + NSH Canvas/Painter | 经测量必要时NativeComponent专用Surface；WebView React Flow另备选 | 大小图、命中、DPI/触控、帧时间、无障碍替代 |
| Knowledge Graph | Rust/SQLite语义图 + 复用画布renderer | WebView Cytoscape.js；Sigma.js作概览 | 可编辑/搜索/回溯，语义独立于布局 |

这些是优先验证顺序，不是已有兼容保证。局部能力失败先换adapter，不自动更换用户已确定的NSH。

## 2 NSH 提供了什么

### WebView

[公共元素API](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/elements.md)已有onMessage、injectJavaScript、postMessage、load和加载事件；各端实际实现如下。

| 平台 | 源码 | 关键实现 |
|---|---|---|
| iOS | [web.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/web.rs) | WKWebView、loadHTMLString/baseURL、JS桥接 |
| macOS | [native.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-macos/src/native.rs) | WKWebView、嵌入、导航策略 |
| Windows | [web.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-windows/src/native/web.rs) | WebView2 composition、层叠/裁剪/DPI/输入 |
| Linux | [web.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/native/web.rs) | WebKitGTK6.0，需要webview feature与运行库 |
| Android | [WebBox.java](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/WebBox.java) | Android WebView、addJavascriptInterface、loadDataWithBaseURL |

原生TSX和WebView是两个环境。PDF.js、EPUB.js、浏览器Tiptap、React Flow依赖DOM/iframe/contenteditable/HTML Canvas等能力，不能因TSX或Canvas名字相同直接导入NSH原生树。

已发现的差异会直接影响阅读器：

- Android incognito只改变HTTP缓存/表单行为；cookies与DOM storage仍应用级共享，不能当成独立私密会话。
- Android beforeContentLoaded注入用onPageStarted/evaluateJavascript，不能假设与Apple document-start完全同序。
- 桥接以data为主，缺少统一可信frame/origin元信息；不是可直接授予数据库权限的RPC边界。
- Apple/Android导航检查偏顶层，Linux对iframe导航也应用capability；EPUB iframe/blob/internal URL须验证。
- load headers只作用于文档请求，不自动覆盖图片、字体、Worker。
- WebView2缺失、Linux未启用feature或运行库缺失，必须有安装/报错路径，不能显示空白后视为支持。

### RichEditor

[RichEditor设计](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/rich-editor.md)和[native view](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-base/src/rich_editor/view/mod.rs)已包含Rust实现的ProseMirror风格schema、node/mark/step/transaction/selection/history、commands、tables和Tiptap风格extension层，提供JSON/Markdown/HTML适配与原生输入。部分表格粘贴上游行为未完成，协同extension为future work。

这不意味着npm插件可直接运行。NSH自动节点ID和UTF-16临时编辑位置服务于渲染/事务，不是QUSOR永久block ID。即便内核不依赖view，业务Core也不应依赖nsh-base；转换留在adapter。

### Canvas 和 Shader

[Canvas文档](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/canvas.md)定义HTML Canvas2D子集，由平台重放：path、变换、填充、文本、位图drawImage、路径/笔画命中、动画参数。限制包括无getImageData/putImageData、filter不完整；JS measureText主要width；动画命中值按0处理，stroke命中存在joins/dashes限制。

Android/Web/GPUI的相应Canvas路径会重绘整个画布；GPUI使用tiny-skia栅格化，不等于全GPU矢量批次。CanvasNode语义子节点当前只在GPUI/测试模型暴露，其他backend不能据此宣布读屏可用。应提供原生节点列表与检查器。

[Shader契约](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/shaders.md)偏WGSL盒子内fragment效果、有限uniforms，不是通用图形引擎API。深空背景可用，但不据此推断可直接承载大图renderer。

## 3 PDF

### 3.1 原生主线

[PDFium官方说明](https://github.com/chromium/pdfium/blob/main/README.md)和[fpdf_text.h](https://github.com/chromium/pdfium/blob/main/public/fpdf_text.h)提供渲染、字符边界框和位置命中。FPDFText_GetCharBox返回PDF user space；选区UI、跨页拖动、手柄、复制、键盘选择与批注叠层需要QUSOR实现。

document-engine负责元数据、抽取文本和几何映射；PDF backend adapter封装FFI handle；reader-engine管理页面窗口/缩放/旋转；annotation-engine处理范围与quads；nsh-adapter处理坐标与呈现。

[pdfium-render](https://github.com/ajrcarey/pdfium-render/blob/master/README.md)可评估，但不包含PDFium二进制、不代为构建。动态/静态链接要自行选择；thread_safe用锁串行化调用，不能当成并行渲染能力。先使用专用PDF worker、前台页优先、取消过期任务。桌面多进程可后评估，移动不能假设同模型。

### 3.2 像素传输必须单独测量

不假设PDFium buffer到NSH drawImage已有零拷贝。M-1先用页图/分块PNG证明正确性，再分解编码、复制、解码、上传、缓存耗时。需要时用NativeComponent做专用页图view；不能把每帧写PNG读PNG未经测量带入生产，更不能用TSX JSON传全页像素。

缓存键包含content版本、页索引、旋转、zoom bucket、tile与render参数；LRU有硬预算，低内存/后台释放不可见页。

### 3.3 五端与许可

官方构建文档有Windows/Linux/macOS/Android说明，但不能证明QUSOR iOS构建可用。iOS须验证static link、device/simulator架构和签名；Android验证ABI、16KB page-size兼容，不使用系统私有libpdfium.so。

[PDFium LICENSE](https://github.com/chromium/pdfium/blob/main/LICENSE)包含主许可及其他条款，最终依赖需逐项notice；[wrapper manifest](https://github.com/ajrcarey/pdfium-render/blob/master/Cargo.toml)为MIT OR Apache-2.0。[MuPDF](https://github.com/ArtifexSoftware/mupdf/blob/master/README)为AGPL-3.0-or-later或商业授权，产品分发策略未定前不无条件选用。

### 3.4 PDF.js 备选

[PDF.js](https://github.com/mozilla/pdf.js/blob/master/README.md)面向HTML5/Web环境；需真实WebView、viewer/worker及所选版本实际依赖的字体、CMap、WASM等资源。断网、文字层/位图层DPI对齐、DOM选区→QLA锚点、大文件、重载恢复必须通过。文档内JS/动作禁用，外部链接显式处理。核心[许可](https://github.com/mozilla/pdf.js/blob/master/LICENSE)为Apache-2.0。

PDFium字符序列和PDF.js抽取顺序可能不同；char index、DOM offset不能直接互换。固定extractor revision及source_map，renderer切换跑锚点回归。

扫描PDF可阅读和区域标注，但无文本层时显示未索引。OCR独立版本、来源和confidence，不冒充内嵌文本。多栏/连字/旋转导致不可靠时保留page/quads/quote并显式标状态。

## 4 EPUB

### 4.1 受控 WebView 加 EPUB.js

[EPUB.js README](https://github.com/futurepress/epub.js/blob/master/README.md)和[iframe实现](https://github.com/futurepress/epub.js/blob/master/src/managers/views/iframe.js)证实浏览器DOM/Range/布局依赖。已有分页/滚动、CFI、选区/高亮能力，适合最短验证主线；[license](https://github.com/futurepress/epub.js/blob/master/license)为BSD-2-Clause形式。

Rust仍负责原文件hash、ZIP/OPF/spine/nav、资源清单、安全解包（路径、symlink、压缩比、大小上限）、索引文本与原内容映射。WebView仅展示，不维护权威数据库。

[CFI实现](https://github.com/futurepress/epub.js/blob/master/src/epubcfi.js)有范围能力及已注明的覆盖限制；保存CFI还必须带文档版本、spine href、quote/context、提取版本和DOM映射。净化、脚注改写、高亮wrapper会改变路径，须保留原始节点映射，注入节点可忽略策略需要回归。跨章节选择拆有序segments；字体/宽度改变不改变来源。

### 4.2 Readium 对照与备选

[Readium Kotlin](https://github.com/readium/kotlin-toolkit/blob/develop/README.md)和[Swift](https://github.com/readium/swift-toolkit/blob/develop/README.md)功能表涵盖reflow/fixed layout、RTL、搜索/highlight decoration，采用BSD-3-Clause许可，适合移动专业阅读对照。代价是Swift/Kotlin与NSH原生容器集成、Navigator/Locator与Core映射，且不自动覆盖三桌面。

[Readium Web](https://github.com/readium/web/blob/main/README.md)包含TS/Go组件，[TS Toolkit](https://github.com/readium/ts-toolkit/blob/develop/README.md)面向浏览器/embedded frame。QUSOR可用Rust适配publication资源/manifest，不能未经验证就认为与Go服务等价。[Locator模型](https://github.com/readium/architecture/blob/master/models/locators/README.md)含href/type/fragments/progression与text上下文，值得参考，但与QLA/CFI转换需定义覆盖范围。

原生Rust解析EPUB用于导入/检索合理，自写完整CSS/分页/bidi/ruby/竖排/SVG/数学排版不属于M0。[NSH TextView](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/text-view.md)是原生reader-mode/chat文档显示，不是完整网页布局引擎。纯文本降级必须标能力边界，不能替代EPUB基础验收；若WebView和Readium都失败，M0阅读闭环阻断。

M0支持范围拟为无DRM的reflow EPUB；复杂fixed layout、音视频、交互脚本、专业竖排独立分期。先通过相应语料再扩大承诺；不支持文件保留原件并明确告知。

## 5 阅读 WebView 的 QUSOR 协议

### 5.1 本地资源

优先验证应用内资源handler；NSH公共API若不能统一提供，可用原生adapter补足，或验证仅监听loopback的临时服务。图片asset scheme不自动证明WebView fetch/iframe/worker兼容。

资源只能按当前文档的规范ID访问；禁止任意路径、目录遍历、symlink越界。区分应用shell与未可信资料资源；明确MIME，按需range。loopback须验证Host/origin、会话授权、暂停恢复和平台网络政策，不使用开发服务器充当离线验证。书籍远程图片/CSS/字体默认禁出站。

### 5.2 窄桥接

typed envelope含protocol_version、request_id、reader_instance、document_revision与消息种类。允许ready、selectionChanged、locationChanged、openAnchor、当前文档有限资源、创建标注候选；不允许任意SQL、文件路径、工具名、模型key或Supabase凭据。

会话nonce帮助拒绝过期消息，不能替代frame/origin隔离。当前缺少统一可信origin元数据时，仅应用自有shell启用窄接口，未可信文档脚本关闭、HTML净化，验证iframe无法触发越权动作。EPUB.js默认关闭scripted content，开启会削弱sandbox，其README已说明。

原文不得因选区变化自动发给AI。AI请求必须由用户动作触发并受scope控制。

### 5.3 生命周期

桥接成功表示Core已持久提交，不只是JS内存变化。重建WebView/杀进程/关闭窗口后恢复版本、位置、高亮；operation_id避免重复批注；忽略旧reader instance迟到消息；关闭文档撤销资源授权；高亮重建不重复套wrapper。

## 6 Markdown 与富文本存储

M0可靠基线是Markdown源文+NSH Editor/TextView。必须维护原文revision、稳定block UUID、字节/scalar/UTF-16及显示范围映射。块移动保留ID，拆分/合并保留迁移关系；行号、AST数组下标、屏幕坐标都不能当永久位置。

原生RichEditor优先采用QUSOR受限schema：段落、标题、列表、代码、引用、链接、图片附件、来源引用节点；复杂表格后续扩展。

两种格式的权威性要明确：

- Markdown Note以原始Markdown为准；不得通过每次MD→JSON→MD保存而静默丢未知语法。
- 富文本Note以QUSOR versioned JSON为准，Markdown是导出视图；无法无损时保留完整JSON和附件，报告损失。
- 不保存NSH EditorState内部结构为长期格式；源引用节点只存QUSOR anchor/evidence ID和显示快照。

[ProseMirror view](https://github.com/ProseMirror/prosemirror-view/blob/master/README.md)使用浏览器contentEditable，README已指向新的官方托管源；[Tiptap](https://github.com/ueberdosis/tiptap/blob/main/README.md)的headless表示不固定UI，不表示无需DOM。备选方案只能在真实WebView运行。核心MIT与Pro extensions/云服务许可分开审查。

编辑器临时undo与持久operation历史分层；保存带base revision；冲突保留双方；WebView crash从已确认checkpoint恢复。

## 7 Think Space 与 Knowledge Graph

### 7.1 主线实现边界

Rust canvas-engine负责world坐标、相机、几何、空间索引、选择/吸附/分组、布局与命令；NSH Canvas画背景、边、卡片和非编辑文字。当前编辑卡片才挂原生Editor/RichEditor，避免每卡片常驻完整编辑器。TSX做工具条，不每帧传全图或在bridge上跑force simulation。

CanvasEdge是视觉线；Relationship是有类型/方向/证据的语义实体。靠近、自动布局、画线不自动生成因果/支持关系。同Concept可出现在多个Canvas，位置属于视图。

### 7.2 性能与无障碍

- 可见裁剪、空间索引、按zoom的LOD和边标签降级。
- 静态背景/边与拖动项分层，离屏暂停动画，支持reduce-motion。
- 布局后台执行，可取消且带graph revision；旧布局不得覆盖用户新拖动。
- 拖动结束提交一个operation，中间坐标只作临时交互。
- 图谱默认局部邻域，不全量显示整个知识库。
- 几何命中与绘制共享Rust模型，不能依赖动画值按0的path helper。
- 提供可搜索节点/关系列表与证据检查器，完成基本读屏编辑闭环。

若限定可见元素后仍不达预算，再根据布局、传输、文字度量、Canvas重放或合成的分解数据决定专用Surface。NativeComponent有[嵌入入口](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/embedding.md)，但任意wgpu surface的resize/DPI/焦点/遮罩/弹窗/设备丢失/后台行为仍需五端验证。

### 7.3 Web 图形库边界

| 库 | 价值 | 限制 |
|---|---|---|
| [React Flow/xyflow](https://github.com/xyflow/xyflow/blob/main/README.md) | 编辑式节点图与视口；MIT | React/Svelte浏览器UI，不能直接导入NSH TSX |
| [Cytoscape.js](https://github.com/cytoscape/cytoscape.js/blob/unstable/README.md) | 图模型/算法/布局，MIT | renderer需浏览器；headless算法能力不等于native可视化 |
| [Sigma.js](https://github.com/jacomyal/sigma.js/blob/main/README.md) | WebGL大图概览、Graphology | 不是完整知识编辑器，需WebView/WebGL |
| [tldraw](https://github.com/tldraw/tldraw/blob/main/LICENSE.md) | 白板交互 | 当前生产使用受许可条件限制，不能沿用旧MIT印象 |

上述库都不定义Core持久数据模型。

## 8 跨 renderer 来源契约

最终协议以[QLA v1](./03-core-data-architecture.md)为准。

| 格式 | 必须持久保存 | 不得单独作为锚点 |
|---|---|---|
| PDF | 版本、零基页索引、CropBox/旋转契约、quads、quote、extractor revision/source_map | 屏幕像素或瞬时glyph index |
| EPUB | 版本、spine href、CFI/range、quote/context、DOM映射版本 | 屏数/进度 |
| Markdown | 版本、稳定block ID、明示单位范围、quote | 行号/AST下标 |
| RichText | schema/document revision、块ID、quote、事务映射 | 临时ProseMirror线性位置 |
| Web | URL、抓取时间、快照hash、quote/context | URL本身 |

返回状态统一exact/recovered/ambiguous/orphaned/source_unavailable；新版本重定位创建新anchor并保留旧anchor。不能多个匹配取第一个。UTF-8字节、scalar、UTF-16、字素簇经显式映射转换。

## 9 M-1 探针与预算

语料至少PDF20份、EPUB20份，覆盖中文、多栏、旋转/CropBox、连字、扫描、字体、加密、超大页、脚注、嵌套目录、RTL、fixed layout、ruby/竖排、非法路径/外链/脚本。编辑语料含10万字、组合字符、表格/公式/代码/附件/未知语法；图集100/1000/10000节点。每份记录hash和预期支持级别。

| ID | 探针 | 通过条件 | 失败动作 |
|---|---|---|---|
| R01 | 五端离线shell的JS/CSS/font/image/iframe/worker | 已打包应用断网可读，桥接可用 | 补资源adapter，否则阻断Web路线 |
| R02 | PDFium五端构建链接安装 | 记录二进制来源/hash/ABI/许可，设备运行 | 对照PDF.js；不以交叉编译代替运行 |
| R03 | 页图呈现 | DPI/旋转/裁剪正确、缓存有界、耗时分解 | 优化图像adapter/页图view |
| R04 | PDF选区/锚点 | 100人工样本准确恢复，复制顺序正确 | 错源/错位阻断 |
| R05 | EPUB重排/CFI | 字体/宽度/方向/重开后100锚点相同 | 修DOM映射，对照Readium |
| R06 | 五端中文输入 | composition/undo不丢字不重复 | 验证备选；可降级Markdown |
| R07 | 编辑持久往返 | 结构/块ID/附件/引用保持 | 收紧schema，不静默丢失 |
| R08 | 画布 | 桌面1000节点/2000边、约200可见p95<=33ms；移动约100可见独立测 | 先裁剪/LOD，再评估Surface |
| R09 | 输入/DPI | 100/150/200%，鼠标框选/双指缩放/菜单命中 | 修坐标契约 |
| R10 | 无障碍 | 列表/检查器可查找编辑节点及来源 | 增加原生语义视图 |
| R11 | 资源/桥接 | 不读库外路径、不自动联网、不越权、不泄密 | 阻断不可信资料导入 |
| R12 | 恢复 | 强杀/重建/锁屏/重开无已确认数据丢失或重复 | 修持久确认/幂等 |

普通文本PDF首屏和EPUB首章p95<=2秒；交互反馈p95<=100ms为初始目标。大型扫描件单独预算并显示进度。PDF页图缓存桌面128MiB/移动64MiB起测，同时记录NSH cache/WebView/RSS，不能把一个cache上限当总内存。连续打开关闭50次后内存稳定或回落。

60fps是流畅目标，33ms是早期最低门槛，不能以此宣称60fps。10000节点用来验证存储、过滤和裁剪，不要求手机同时画全部文字。

## 10 进入后续里程碑的条件

M0必须完成原件保留、版本/章节、阅读/选择/标注、笔记/引用、Reader Agent候选审核和重开准确回跳；高级富文本可由R06/R07决定延后。

M1交付可恢复画布、可编辑语义图、证据检查、AI候选人审、撤销/重做和可访问替代操作，不把漂亮图形当验收。

出现静默错源、IME丢字、只在开发服务器可读、资源越权、无法安装启动、许可不兼容、无界内存或长期阻塞时停止依赖功能开工。记录补丁/工期，再选择修adapter、明确缩小非核心范围或延后里程碑，不能悄悄取消PDF/EPUB核心能力后算通过。
