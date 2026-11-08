# 原生挂载与资源释放确认

2026-11-08 11:25 UTC，只读源码调查；未修改NSH、未编译、未运行窗口。关联 C-09/C-11/C-12、F-EXT-04/05/06、F-READ-02、F-UX-01/02，承接[ADR 0028](adr/0028-extensible-native-workspace.md)、[ADR 0030](adr/0030-readonly-paragraph-projection.md)与[编辑/替换交互](../design/paragraph-editor.md)。本文提出下一实验的信号与验收，不宣称已有native ack。

## 上游核对与固定基线

本次读取[官方发布](https://github.com/longbridge/nsh/releases)、[近期提交](https://github.com/longbridge/nsh/commits/main/)、[Linux后端文档](https://github.com/longbridge/nsh/blob/bba76ef739135dbdaa0147e2533c75e814beb61f/docs/dev/linux.md)。发布页显示stable v0.4.0、nightly f81b965；近期检索首项为 [bba76ef](https://github.com/longbridge/nsh/commit/bba76ef739135dbdaa0147e2533c75e814beb61f)（tooltip转发），另有 [df35720](https://github.com/longbridge/nsh/commit/df35720fede134fa98b080a4f19ec51360b725b1)（分平台运行组件安装）、[3b4f2ab](https://github.com/longbridge/nsh/commit/3b4f2abfe4dfc4f346b73877dd9036d04674ead4)（Android提交时结束composition）及 [0fd4712](https://github.com/longbridge/nsh/commit/0fd4712887c4154368a0c9abaed6571fd217bfe8)（超高聚焦view滚动）。这些提交摘要没有给出本项目所需的mount释放回执证明，不据此升级或关闭门槛。

下文实际追踪的本地NSH仍为固定 [913fd76e1f8434ec6cdf248f4476c1ebe08af19f](https://github.com/longbridge/nsh/tree/913fd76e1f8434ec6cdf248f4476c1ebe08af19f)，根目录为 `experiments/nsh-reader/target/nsh`。上游链接固定到该SHA；本地行号仅对应这份源码。后续若要增加backend observer，须在独立副本/工作树留存base SHA、补丁、锁与工件，不能悄悄修改已封存pin后沿用旧资格。

## 当前信号能证明什么

| 信号/源码位置 | 能证明 | 不能证明 |
|---|---|---|
| [DynamicReader.frame](../../experiments/nsh-reader/native-host/dynamic_module.rs)489–539，本地同路径 | 原生工作产生有界PNG、生成前后重新核对frame身份，返回ImageData | NSH已挂载/绘制，或所有字节/纹理引用已释放 |
| [ReaderViewport Image.onLoad](../../experiments/nsh-reader/dynamic-pdf/ReaderViewport.tsx)12、[App loaded](../../experiments/nsh-reader/dynamic-pdf/App.tsx)118 | JS收到图片加载事件并请求ReaderSurface.ready | 图片已参与本次原生snapshot/呈现；JS请求不构成原生事实 |
| [Linux image.loaded](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/image.rs#L1135)，本地`crates/nsh-platform-linux/src/image.rs:1135`、1191–1208 | ImageState接收Picture/Lease，调用view.changed后即发Load/LoadEnd | 没等待snapshot或after-paint，不证明当前可见frame |
| [ReaderSurface.ready](../../experiments/nsh-reader/native-host/plugin_surface.rs)935–949 | 核对frame/report，调用Host.native_frame_ready并取得input ticket | 当前只是JS触发的Rust校验，不是renderer提交回执 |
| [MountSink.submit](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/surface.rs#L81)，本地`surface.rs:81` | batch入UI队列；`Surface.apply`564–581逐项应用并end_batch | submit返回void，无公开applied/disposed回执；batch applied也不等于像素已显示 |
| [Mounter Delete](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/mount.rs#L140)，本地`mount.rs:140` | 从views表移除，调用native/shader删除路径 | outro动画可能保留view；Remove/地图中不存在不等于GObject及资源已销毁 |
| [Load Drop](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/image.rs#L474)、[Ticket Drop](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-core/src/image/loader.rs#L420) | 移除回调/等待订阅，最后等待者取消请求 | 不join正在运行的read/decode，不证明队列任务已物理退出 |
| [NshView dispose](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/view.rs#L491)，本地`view.rs:491` | unparent现有子widget | 目前没有向宿主发送mount-specific资源销毁确认 |

[FrameScheduler](../../experiments/nsh-reader/native-host/dynamic_module.rs)181–211丢弃spawn返回的JoinHandle；244–259的close只置stopped、取消pending。已经取出的work仍可做PNG读取/编码。`finish_frame`159–174还会把FrameFile放入Completion，可能等待Future/JS交付，不能以work返回或close返回替代全部owner释放。`shutdown`84–95等待parser_running最多2秒，既不join这个frame线程，也没有按frame的drain结果。

[plugin_surface.shutdown](../../experiments/nsh-reader/native-host/plugin_surface.rs)1236–1256已诚实记录`native_mount_release_ack:false`；关闭前从未返回给TSX、也没有render job的failed Open可局部清理，不能外推到已经mounted的frame。

## 明确三个不同事实

1. **已失效**：Host关闭/撤权后不再允许旧输入、frame请求或结果。它不证明任务已结束。
2. **已挂载并参与原生绘制**：可信renderer在UI线程观察正确owner/generation/frame内容、尺寸与布局，并在该snapshot/绘制轮次后回报。它不等于物理显示器或compositor已呈现每个像素。
3. **已释放**：该mount的工作已排空，挂载节点及其持有资源已真正退役，旧回调无法再取回这些owner。不能以JS unmount、Blob URL revoke、弱引用无法upgrade、Drop某个代理对象或等待固定时长替代。

[Surface cleanup ticket](../../crates/extension-host/src/surface.rs)710–736绑定Host owner/handle/generation/mount；[Component cleanup ticket](../../crates/extension-host/src/component.rs)380–408绑定Host owner及同一identity Arc。它们是可信协调器确认的门，不是现成native证明。没有真实证明时保持Closing和配额，不能为了切换成功直接调用confirm或删除map。

## 最小实施路线

### 先收敛Rust frame任务

给现FrameScheduler保存JoinHandle，并增加按不可复用`(mount owner, generation, frame_id)`记账的task/result lease；身份由可信NativeAdapter建立，不接受guest/JS传来的released布尔值。至少区分pending、running、completion/result等待交付，以及native接收后持有的frame数据。

- close-owner先失效输入及后续入队，取出并取消属于该owner的pending；在锁外完成错误回执/释放闭包。
- 运行任务用取消标记在安全边界检查。不能中断的PNG编码允许实际返回，再经退出guard归零running；取消请求本身不归零。
- Completion中的成功FrameFile、丢弃的Later结果及已经交给renderer的frame lease分别释放。转交时移动lease，不能只看Future完成。
- per-owner drain只在该owner所有计数归零后完成；全scheduler退出才join线程。不能在持有`plugin_surface.gate`/队列锁或UI线程必需回调时阻塞join，否则worker的fresh context读取可能死锁。
- 若超时、panic清理不明或计数不能闭合，保持Closing并报告原因；不产生成功ack。PDF导入子进程仍按其实际owner/lease/reap处理，不能把全局import与某个page-frame线程混为一项。

本步只证明Rust frame work/result退出，**尚不足以调用Host confirm_surface_closed/confirm_component_released**。

### Rust任务整合进度

后续[整合记录](../../experiments/nsh-reader/evidence/native-frame-retirement-integration/record.json)已把任务队列、独立线程退休服务、可信frame所有权映射与编码字节容器拆到原生实验私有模块；没有新增Core／SDK依赖。实际JoinHandle由独立服务持有，State／scheduler析构只请求退休，避免最后一个State引用在自身工作线程上析构时自join。排队、运行、未取结果与已交付本地结果分别记账；编码容器另由实际共享引用持有，已排空的历史任务记录可以退役，不引入累计64次翻页／缩放限制。

候选13项std线程／所有权测试已由主负责人对冻结测试程序[独立复跑](../../experiments/nsh-reader/evidence/native-frame-retirement-integration/root-std13.json)，包括130次顺序frame且保留旧容器引用；源码ZIP、日志与哈希独立保留。它不证明NSH或新关窗路径。当前已整合，默认feature的原生全目标检查已通过；Clippy、完整native库回归和新工件GUI仍待分别验证。现有默认GUI证据仍属于整合前工件。

关窗先封闭编辑与动态任务准入，再使旧State失效、等待实际parser工作结束与frame线程join；只有这些条件满足才允许移走阅读Service。若未确认则保留Service和持久状态并报告失败，不用进程退出代替存活宿主中的资源ACK。本切片只收敛Rust任务／编码容器，仍不能调用Host原生释放确认。

### 编码字节的实际所有权补充

后续固定pin的[本地源码核对](../../experiments/nsh-reader/evidence/native-frame-byte-owner-source.json)发现，`nsh-core::ImageData::new` 接受带所有权的 `AsRef<[u8]> + Send + Sync` 容器；`nsh-js::new_file` 经 `Blob::shared(data.clone())` 共享该容器。完整Blob转图片继续clone同一ImageData，slice路径的BlobBytes保留原Blob。因此可以把PNG字节与专属RAII lease放入同一容器，实际最后一个共享引用退出时结算编码字节owner，无需把每次bridge交付永久记为不可知。

进一步沿Blob URL解析核对：ImageSource.data会定位为 `Location::Memory`，Loader::read直接clone并返回，早于L3缓存查找；通用read完成路径也只向L3插入非内存源。因此当前完整File／Blob路径不应笼统算作L3持有。另一方面，宿主PNG服务返回的底层 `Arc<[u8]>` 仍可由独立的3页／32MiB缓存保留；专属ImageData容器最后Drop证明容器引用链退出，不能称底层字节分配已释放。容器计量、共享allocation与decoded/native owner须分别报告。

这只是一条已核对源码的候选路径，尚未完成主消费者运行验证。即使编码字节owner已经退出，解码器仍可能持有独立像素／纹理；它也不证明原生节点退役或图片参与绘制。不能从该Drop调用Host释放确认，也不能用无法释放的永久frame计数引入重复换页／缩放次数上限后宣布生命周期完成。后续实验须分别核对实际字节clone／slice／交付失败，以及decoded/native owner。

### 再接真实native mounter/renderer回执

固定pin的公开MountSink/Image事件没有所需ack。最小隔离实验应在Linux adapter接入可信lifecycle observer，而非让业务Core依赖GTK或让普通插件获得NSH节点。

1. 由宿主renderer把本次mount的真实node/image/load owner映射到NativeRetirement token。node IDs只留在adapter，不写Core/SDK或从guest组装。
2. UI线程`Surface.apply`的end_batch后可回报DeleteApplied，仅证明相关删除操作已执行。另需真实节点owner退役信号：在实际ViewState/native对象清理结束处释放专属RAII guard，并把outro保留、native子控件、text/image/shader owner纳入等待。
3. 为每个mount的image订阅、正在read/decode、UI结果交付和texture/picture lease建立退出计数。订阅Drop与后台任务完成是不同事件；复用共享loader时，必须有可观察的job结算，不能把其取消标记当完成。
4. 对frame-ready，在可信image/widget snapshot观察准确frame token与实际布局后，再关联一次UI绘制轮次的完成信号；由Rust observer送回协调器，替代把TSX onLoad直接当visible。隐藏/未绘制时不签发input-ready；取消仍可立即失效，不等待新paint来接受旧输入。
5. 最后聚合：Rust任务已drain、旧native节点实际退役、mount-owned load/交付/纹理持有者已释放，才允许用原cleanup ticket调用Host confirm。重复或迟到回执必须核owner/世代，不影响后继mount。

现有公开`NativeComponent` factory可建立一个受控native控件并用Rust Drop清理其自有资源，适合隔离探针；但[NativeComponent删除](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/native.rs#L327)会先drop组件Box，NSH外壳view可能仍在outro。**组件Drop只覆盖该组件明确持有的对象，不能据此宣布整个NSH子树、stock Image loader或GPU资源释放**。若采用自有GTK image控件，必须完整接管其decode/job/texture owner并仍等待容器退役；不能只加一个会Drop的哨兵。

### 共享缓存要独立实测

[原生PNG缓存](../../experiments/nsh-reader/native-host/dynamic_worker.rs)478–490有最多3页/32MiB的限制；[NSH image Lease](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-core/src/image/memory.rs#L491)释放的是cache in-use持有，缓存仍可保留条目。应分别记录mount-owned引用/任务及共享cache的entries/bytes/in-use，证明关闭后不再被该mount pin住且共享总量仍受实际预算限制。

这不等于“cache改叫共享就算释放”。若资源仍被旧mount持有、运行任务仍引用旧frame、cache charge未按既定预算核算，或无法区分owner，则不得确认该mount释放。GPU驱动/compositor的内部缓存与物理显存回收，本调查没有完成信号；只能按已证明的owner范围报告，不能宣传“全部显存已释放”。若验收明确要求实际native纹理对象销毁，还要增加相应对象最终销毁/渲染器结算证据。

## 独立Linux observer第一片实际结果

固定913的独立worktree已实施feature-gated批次归属、关闭准入、节点qdata最终化阶段观察和实际图片append记录。[初片证据](../../experiments/nsh-reader/evidence/native-observer-initial/record.json)保留patch／变化源码ZIP、固定lock／本地前缀／工具链、原check输出与GTK4后端三项真实测试。实际用例包括额外强引用及run_dispose不提前报告qdata退出、一次性注册／关闭与foreign节点拒绝，以及2×2合成PNG经stock Image实际append并匹配准确ImageData。Xvfb自动display失败单独留证，成功阶段实际显示进程已回收。

本片所有状态仍强制Unprovable，没有Host ACK接口；qdata只证明最终化阶段达到，snapshot只证明参与渲染树生成。native子对象、后台job、decoded缓存／纹理、outro、完整边界factory成功实例化及真正协调器接线尚未闭合。它不修改主pin，也不覆盖QUSOR正式组件替换；下一图片链实现保持独立worktree，不覆盖此冻结证据。

## 验证与退出证据

- **Rust排空**：用可控屏障阻塞running frame，同时有pending与尚未poll的Completion。close后pending取消，running退出和result lease释放前均无ack；放行后计数归零。覆盖panic、重复close、过期frame、关闭后入队与线程join失败/不明路径。
- **真实窗口删除**：分别有/无outro、缩放换页、隐藏/最小化、组件A→B→A、直接关窗。记录DeleteApplied、节点/资源owner退出、每项job结束及最终Host确认的顺序；只有组合条件成立才让Closing配额下降。保留一个旧owner，验证不能提前释放或抢额度。
- **挂载事实**：图片decoded但还没snapshot时onLoad不能解锁输入；旧frame晚Load/晚paint不能给新frame ready。实际原生snapshot与frame身份、像素尺寸、DIP布局一致后才发input ticket；截图/指针选段验证另记，不能只计counter。
- **后台decode**：删除view时仍在decode/交付，证明先取消订阅、后物理结算；旧任务结束不能重新挂图。缓冲/纹理/共享cache分别计量，不只观察RSS或弱引用。
- **恢复与故障**：原生节点退出信号缺失、worker未确认结束、observer返回错世代时保持Closing及限额；重试不会假确认。控制器OS worker的actual reap/lease finish与native view销毁分别取证。

以上是完整退出要求；初片实际执行范围单独列出，未测项不能据此签署通过。Linux局部测试不证明macOS／Windows／移动后端具备相同信号。

## Input composition类型的旁证

固定pin的[nsh-base Input](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-base/src/lib.rs#L93)是core host element；[Rust listener](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-core/src/element/host.rs#L861)与[Linux发送路径](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/text_input.rs#L517)已有composition事件。

本地实际生成声明 `experiments/nsh-reader/dynamic-pdf/.nsh/types/nsh/types.d.ts:1069` 为`onCompositionChange?: (e: { composing: boolean; text: string }) => void`；`nsh/base.d.ts:6066`的InputProps继承InputHostProps。这足以支持下一实现用公开事件跟踪组合状态，在composing期间避免Esc抢走IME，而不是猜DOM的isComposing。声明和源码不证明当前Linux输入法实机、Esc事件顺序、焦点/undo或其他平台已经通过；须保持[设计验收](../design/review.md)中的真实输入门槛。


## Rust任务与encoded容器接线的实际局部结果

默认features全目标check、Clippy、29项native lib，以及同源八项feature-only真实OS退休回归均通过；[主检查归档](../../experiments/nsh-reader/evidence/native-frame-retirement/maincheck-archive.json)保留失败attempt、实际日志和九份已测前后hash源码，[主负责人独立复跑](../../experiments/nsh-reader/evidence/native-frame-retirement/root-independent.json)验证冻结exe。九源ZIP不替代完整transitive构建输入封存，新默认GUI仍未在该记录中验收。

线程句柄由独立有界退休服务在spawn前预留、实际退出后join；State／scheduler Drop不自join，不在调用方Gate中等待。任务registry只回收已闭且Rust任务／本地结果已排空的记录，仍活encoded guard自己保留opaque owner和实际字节引用计数；130次顺序frame保留旧guard的模型测试不因历史64次handoff停住。真实pin ImageData的new／clone配合显式Blob／slice模型验证共享引用；独立缓存Arc和decoded copy仍可活着，容器Drop不证明PNG底层allocation或decoded/native资源消失。State／实际std Job屏障覆盖共享shutdown等待helper和closing拒新操作，不宣称已端到端触发main＋Service.take全部竞态。

这些检查不调用Host confirm、不归还native Closing配额，也不完成图片snapshot／节点退役／纹理释放或提供者切换ACK。普通B的原生视图、局部输入与最终A→B→A仍需后续源实现和串行窗口资格；ACK未知时必须保留关闭状态和输入，不能把已有headless算法或这些Rust检查当作替换通过。
