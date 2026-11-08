# ADR 0026：持续阅读权限与原生动态 PDF 通路

日期：2026-11-08。状态：权限与原生适配实现已落位，动态窗口资格验证通过；完整产品旅程尚未完成。关联 C-08/C-09、F-EXT-01/03/05/06、F-LIB-01、F-READ-01/03、F-NOTE-01、F-SRC-01、F-DATA-01。继承 [ADR 0025](0025-durable-installation-authority.md) 与 [公开 Surface](0023-public-reader-surface.md)。

## 问题

原有持续授权只保存一项 metadata 能力，不能表达阅读 Surface、选区读取与收录提案所需的完整集合。固定两页图片的消费者也不能证明用户选择新 PDF 后，页图、字符映射、权限与 Core 来源仍对应同一份原件。

## 决定

在 extension-store 中增加显式 policy_version2 的 `GrantSet`，固定 API0.2、同一 vault 和共同有效期，最多32项、编码最多32KiB。阅读、选区与 metadata 权限绑定精确 Document 和提取版本；CapturePropose 绑定 vault。拒绝临时 Selection、lease、一次性批准、重复条目与同文档不一致版本。SQL先核对字节长度再加载记录；未知版本失败关闭。旧 profile1 字段、编码和操作 hash 保持，旧单项接口拒绝将 profile2 投影为首条权限。

保存集合前，由同一个实际 Host 无副作用预检；存储事实不是运行许可。统一 DurableHost 恢复全量集合并固定协商版本，每次 dispatch 或短作用域 `with_surface` 都重新读取持久事实。可信 Core 在同一 gate 内核对实际来源版本；guest、PDF provider和耗时I/O不在 gate 内执行。DurableSurface 不公开原始 Host/Session，私有帧、输入和手势票据首先核对当前安装归属，再由原 registry 核对世代、mount与frame。

同一持久安装反复激活时保留内部安装身份及 Closing 配额，只替换运行会话。旧上下文不能撤销或关闭后继会话；修订/世代倒退或同计数变更拒绝。真实卸载后，可信 Source 提供经过包/键重验的 tombstone，并从保留的安装绑定读取 vault，缺失即拒绝；Host 在 fence 前核对 vault、包与安装身份。只有更新的卸载计数且所有原生资源实际释放后才允许 `retire_removed`。禁用或关闭会话不构成退役依据，避免长期 Host 耗尽安装槽，也避免借激活绕过资源预算。

动态 PDF 继续使用 NSH native：Rust文件对话框→受控后台冻结原件与生成报告→同一Core导入→精确权限→原生Image显示。编码页图经宿主专用 native File/Blob 和原生对象URL传递给可信TSX，Canvas只画高亮；无需HTTP、DOM或WebView。路径、像素、File与NSH内部对象不进入普通Wasm协议。显示、命中和保存共用同一来源报告，异步完成重核来源、页码、frame和世代。

解析与渲染在受控同程序 worker 中执行，沿用 runtime-process 父死守卫和真实OS身份检查，GuardReady核对后才发送有界Boot。输出、时间、待执行队列与缓存有界；父端持有Child并确认回收，未知身份或残留marker保持隔离。此进程管理不构成PDF安全沙箱，也不代替通用插件supervisor。实验装配留在独立Workspace，Core不依赖NSH/PDFium。

## 交互与失败

文件选择取消或坏PDF保留当前文档和草稿；切换有未保存笔记时用原生确认。保存异步显示状态并避免重复提交。Core已导入但权限未准备好时明确报告导入已发生，不能伪装两个数据库共同回滚。重启仅恢复仍有效的精确授权，不延长有效期，不恢复临时选区；显式选择另一文档也不能扩大原有可选能力集合。

## 验证与边界

根回归覆盖集合解析、同Host预检、版本、撤权、跨安装票据、后继会话、配额及300个不同安装的安全退役；真实Core消费者补充实际卸载后的退役与真实跨库证明拒绝。最终根134项Rust、42项边界回归、完整包/Wasm/C3消费者与新二进制的三/四页动态打开、普通插件选文/笔记保存及完整恢复已通过，见 [C8结果与源码快照](../../../experiments/capture-core/READER_AUTHORITY_RESULTS.md)。原生窗口使用一次构建后新生成的PDF，保存不变工件摘要；首次异常状态资格和最终主链分别标明工件，不能互相冒充。

当前16MiB/16页预算与完整归档式原件读取仅用于预算内动态通路。普通长书仍需分块/页窗与格式版本设计；不能静默截断。正式library/reader/notes独立包、通用异步插件监督、中文IME、资源释放ack、EPUB及其他平台仍按原计划逐项完成。
