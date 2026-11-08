# ADR 0030：与编辑独立的只读段落投影

日期：2026-11-08。状态：wire、应用registry及Host挂载授权局部通过；完整查询协调器、guest与原生消费者尚未接通。关联 C-09/C-11/C-12、F-EXT-03/04/05/06、F-NOTE-01、F-UX-01/02。承接[ADR 0028](0028-extensible-native-workspace.md)与[段落提供者资格](../../../experiments/capture-core/PARAGRAPH_PROVIDER_RESULTS.md)，不扩大AI、云或平台范围。

## 问题

普通B已能在guest中编辑/移动正文，静态ParagraphEditor描述却不提供段落显示数据。Host自行切段会复制提供者语义；把滚动包装成编辑事件会覆盖Pending输入或争用上传/checkpoint。任意完整正文/段落表一次返回则突破现有预算，也不适合长段与大量空段。

## 决定

新增独立 `paragraph-projection/v1`，显式API0.3，沿用当前提供者的精确Draft临时许可；不扩充0.1/0.2或持久授权profile。查询由可信协调器创建，绑定mount世代、草稿、revision与不可复用query身份。guest通过公开自身context、固定revision的读取与投影续传，生成最多8行的ordinal/body spans；Host只验证身份、范围、UTF-8边界、数量和预算，通用renderer读取片段展示。

这是提供者视图，不是可信来源。Host不解析正文就不能独立证明ordinal语义或guest的EOF声明诚实；引用预览继续来自Core来源。投影不授予新读写权限，不绕过签名资产、安装世代和同一Host的fresh授权。

同一guest需要知道本次任务用途。新增显式API0.3 `component-invocation/v1` bootstrap，只返回 `edit_event` 或 `paragraph_projection`，再分别读取各自公开context。旧API0.2 InvocationRequest和旧0.3编辑Context保持原义；不把query generation伪装成编辑sequence，不以失败试探分支，也不由native传私有参数。新提供者须在签名资产中明确声明支持的协议，未知必需协议拒绝。bootstrap本身不授予身份、权限或查询状态；该入口wire及Host挂载鉴权已有局部实现；查询协调器、新B支持声明及真实分流仍待实现和验证。

投影registry在learning-application，仅保存有界查询、窗口/checkpoint与最后一次CAS回执，不依赖extension-api/Host/NSH/数据库/运行时。投影的推进没有文本effect。滚动更新或取消查询不触碰ProviderEvents、Draft revision、Pending输入或编辑upload。外层协调器负责每mount一个运行查询、一个latest待办，编辑优先，旧worker真实回收后再调度替代；关闭或撤权后旧结果失效。

## 状态与恢复

- `reached_eof`是guest声明已扫描至源EOF；`window_ready`由窗口行完整性及数量/EOF推导；`complete`只在窗口ready后封存查询。三者不混用。
- Active可以原子发布部分窗口与checkpoint。已完整的前缀行保持不变，末partial行只可扩展，不能以刷新覆盖用户输入。更换revision/query后旧句柄失效。
- checkpoint最多8KiB，读取块最多2048，完整响应最多4KiB。同CAS同payload返回最后回执，不重复发布；同CAS不同payload拒绝。query间不共享编辑checkpoint。
- 每registry最多8个mount，每mount一个当前查询。取消/过期/失效不再读取checkpoint；所有读取与发布仍核对草稿存在及精确revision。失效清理不改写草稿。
- 长段未完整加载时只读，提供全文入口。完整段编辑另需可信协调器确认完整范围和正文，不能把预览片段当整段提交；此registry不签发“可编辑”许可。

调用的运行状态与安装授权分别校验。Store lease在owned reap前仍存在，不能证明VM仍live。协调器在Call出队和实际应用前检查运行准入；已准入的原子操作可能完成后回复失效，应隔离、真实回收，再从同一checkpoint恢复。协议/身份/fuel错误不能当超时盲目重试；预算和重试有界，失败保留输入。此处不声称Controller状态检查消除了所有跨线程竞态。

## 交付门槛

先验证closed DTO、独立应用registry及编辑不受投影影响，再接同一Host/真实Store/普通签名guest和监督进程。随后用真实原生窗口验证逐段显示、partial长段只读、全文入口、焦点、中文IME/Esc、取消/替换及资源释放。1MiB单长段和大量空段继续有效；功能结果和实际输入延迟分别取证。

此决定不创建通用插件UI引擎、任意FFI或所有未来角色。具体签名和验收见[段落契约](../../product/paragraph-editor-contract.md)，交互见[编辑与切换](../../design/paragraph-editor.md)。

局部检查已通过全根209项Rust、42项架构守卫、Clippy/格式与仓库外SDK编译，见[机器记录](../../../experiments/capture-core/evidence/projection-foundation/record.json)。这不证明真实Store/query broker、普通投影guest或原生UI已完成。

bootstrap wire4项和Host fresh挂载1项新增检查后，全根214项Rust及Clippy通过，仓库外作者同时编译projection/bootstrap类型；[记录](../../../experiments/capture-core/evidence/projection-foundation/bootstrap-record.json)不包含真实任务分流或native资格。
