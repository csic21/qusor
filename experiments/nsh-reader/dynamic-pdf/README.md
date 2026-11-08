# 下一门槛：原生动态 PDF 消费者

状态：原生动态探针已实施，Rust all-lib 11项（动态4项）与Clippy、真实picker/第三页保存/换源/预算拒绝/取消/空库恢复首轮资格已通过；根Host跨vault卸载修复后最终native主链与空库恢复复验也已通过，见[RESULTS](RESULTS.md)。关联 C-08/C-09、F-LIB-01、F-READ-01/03、F-NOTE-01、F-SRC-01、M-1 NSH阅读/输入任务。沿用 [C7真实链路](../evidence/installation-production/RESULTS.md)，不将固定fixture资格当正式阅读器。

## 原生路线

固定NSH commit `913fd76e1f8434ec6cdf248f4476c1ebe08af19f` 已有下列真实接口，不能从TSX语法推断浏览器能力：

- `nsh-core/src/image.rs::ImageData` 以Arc持有编码图片；`JsHost::new_file` 经 `nsh-js/src/classes.rs::Blob::shared` 可将其作为宿主专用native File交给可信TSX，`runtime/picker.rs::PickedDocument::to_js` 是既有返回模式。
- `URL.createObjectURL(file)` 在该native runtime中解析为ImageData；`element/value.rs::ImageSrc::from_js` 与 `image/loader.rs::Context::locate` 的Memory路径可供原生Image使用。无需HTTP、DOM、WebView或把PNG/base64塞进Wasm JSON。
- `CanvasImage` 当前只有URI，Linux `load_bitmap`不解析这个Blob；继续用Image呈现页图、Canvas仅画高亮，避免假设Canvas支持同一路径。
- Rust-only `pick_paths(OpenDialog)` 和GTK实现已存在。路径只留native；真实文件选择之后执行QUSOR授权、预算读取/冻结和Core导入。NSH对话框不是QUSOR权限许可，默认Picker读取上限也不是本项目导入预算。
- `Later` 可承接异步结果；PDF解析/渲染在受控串行worker执行，有父端独立预算/中止，不能放在UI线程或Wasm broker，也不能用Wasm期限冒充native工作中止。

接口核对基于上述固定checkout的 `crates/nsh-{core,js,platform-linux}`；源码位置不构成新能力已经实测。

## 动态来源绑定

此前固定页探针把page0/page1、reportDigest和标题编入App；动态消费者已改为按Core document_id与来源版本读取原件、report/frame/PNG及资源票据。不能只换Image.src而继续用旧字符报告命中。所有事件/异步完成/onLoad均须重核handle、generation、frame与来源和当前授权。guest只拿公开opaque Surface和有界元数据，File/Blob、路径、NSH对象与像素留可信宿主。

Core版本解析、保存和Host撤权继续使用共享门；PDF worker和guest执行在门外。Blob URL撤销不证明已挂载NSH资源释放，原有Closing限制不能通过删map或revokeObjectURL虚报解除；需独立解决真实unmount/job完成ack。

## 首条动态验收

一次构建无PNG/frame imports的通用bundle；构建后再生成/选择内容不同的至少三页许可PDF，经真实原生文件对话框导入，在同一进程显示第三页、真实选文、非空笔记、普通签名guest提案和明确保存。binary/bundle hash全过程不变，重开后读取Core记录、完整导出空库恢复并精确回源。另核对取消文件选择、坏PDF/缺原件、切文档旧帧迟到、换页/缩放/关闭与保存失败的真实状态，不用手工替换report或重编资源证明动态行为。

当前Core PDF契约与provider均限制16MiB/16页；original_pdf还经过完整归档读取。这只能先验证预算内新文件的动态通路。普通长书/大资料库仍需分块、页窗和格式版本设计与实测，不能静默只导入前16页或取消长书要求。正式library/reader/notes的导入/列表/来源能力、通用持续权限及生产监督按原计划继续补齐。

## 当前实现与验证入口

`native-host/dynamic_module.rs`复用共享ReaderSurface服务及同一Core实例；不会在切资料时另开SQLite writer。后台完成预算、冻结、provider报告后才提交Core导入，随后切换Host的持久profile2授权。PNG File仅交可信TSX，普通签名Wasm继续经Open/Configure/完整Read/Propose协议工作。当前页图队列最多一个执行中、一个最新待执行；每份原件PNG缓存最多三页、32MiB。

`native-host/dynamic_worker.rs`以同一native executable的managed-worker profile运行PDFium。guard成功并发回run绑定的GuardReady后，父端核对真实OS身份，才发送至多4KiB路径Boot；control、report及stderr分别有界。父端持有Child，以try_wait确认reap后才清除落盘marker。发生未知身份、无法确认清理或崩溃遗留marker时阻止新worker，不将pidfd的Terminated观察当reap。该profile提供父进程死亡保护和已验证身份清理，**不是PDF安全沙箱**。普通退出会请求取消并有界等待guardian；强杀恢复仍可能需要显式人工清理quarantine，不能宣称自动恢复已完成。

本目录的`prepare.py`构建通用binary/bundle并记录hash，`generate_fixture.py`随后生成MIT合成的三页、四页及超预算十七页文件。`run.py --data <private-dir> --picker-dir <fixture-dir>`启动真实GTK原生窗口；`dialog_keys.py`/`ui_driver.py`仅向已核对PID、class和焦点的实验窗口发送XTest事件，不能据此声称物理键盘或IME已验证。当前Rust回归覆盖FIFO/符号链接快速拒绝、owned Child确认回收、未绑定身份保留持久quarantine，以及frame待执行队列合并；完整原生UI资格结果须以新证据目录的实际执行记录为准。

## 原生阅读工作区界面

本轮在相同普通Wasm/Core链上完成按需辅助区、语义主题与应用控件层、真实选文动作、草稿/保存状态和宽窄窗焦点处理；不是公开组件替换SDK。最终GUI、源/工件、恢复与未验证范围见[WORKSPACE_UI](WORKSPACE_UI.md)及[机器指纹](../evidence/reading-workspace/final-focus/qualification-fingerprints.json)。历史C8动态文档资格保持独立。
