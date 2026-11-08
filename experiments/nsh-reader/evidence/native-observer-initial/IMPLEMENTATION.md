# 固定913 Linux native owner observer：隔离实施规格（尚未实现）

日期2026-11-08。固定913fd76e1f8434ec6cdf248f4476c1ebe08af19f；本轮复用刚完成的官方上游核对，不升级。`source-scope.json`给出逐文件hash。没有改pin checkout、没有编译、没有运行GUI。本规格与现有GUI2正常退出、现有false ACK分别记录，不能当作observer已通过。

## 单一建议与严格承诺

在独立NSH worktree增加Linux adapter私有observer，继续使用stock Input和Image，先限定受控组件树为View/Text/Input/Image/Scroll及普通非导航outro；图像仅接受现用File/Blob的`Location::Memory`。拒绝Canvas、Shader、Web/Video、Modal/Stack/HeaderItem/自定义Native子组件、URL/asset/path图像、Image.prefetch/getSize等未覆盖资源路径。遇到这类路径标`Unprovable`，保留Closing，不降级为成功。

明确两种事实：
1. `NativeMountReleased`：该mount节点/子控件的最终native引用退出，关联queued/running任务和UI交付已实际结束，mount专属decoded cache已清空，mount-owned Picture/Lease/pixel容器已退出；不以主进程退出替代。
2. `SnapshotObserved`：特定FrameStamp确实进入指定Image的GTK snapshot，身份/像素尺寸/DIP bounds和本轮snapshot序号匹配。它不等于物理屏幕已经present、不等于GPU fence、纹理驱动内存回收。

物理compositor提交没有现成可靠信号，本切片不宣称它。若产品要求物理present，必须另接GDK/GSK backend可核的present/fence，此observer不得伪造。

## 已查到的真实持有链（相对NSH根）

- `crates/nsh-platform-linux/src/surface.rs:564`：Surface.apply依序执行MountBatch，最后Mounter.end_batch。只能在这里记录DeleteApplied，不能在这里ACK。
- `mount.rs:75` Create先建NshView、Input/Image state；`mount.rs:99` Insert决定真实parent；`mount.rs:131` Remove可启动outro；`mount.rs:140` Delete从views map移出，然后native::deleted/shader::deleted，仍可能动画保留；`mount.rs:389` detach才unparent。
- `transition.rs:129` removing发现已有dying会继续保留；`:233` tick在`:263`才unparent。回调弱引用NshView，但GTK父节点仍是强引用；祖先outro/任意Delete顺序均需覆盖，Remove还可能重新Insert。
- `view.rs:492` ObjectImpl.dispose仅unparent children，未清空ViewState。不能从dispose入口、Delete、组件Box Drop推断image/native state已释放。`:532` allocation临时clone natives；`:760` set_native同时由state.natives和GTK parent持widget；remove_native处理这两份，但其他GTK owner仍可持有。
- `native.rs:327` deleted先从hosts删除/drop组件Box；`:366` component factory创建widget并fill入NshView。Box Drop早于外壳outro，不是最后native owner。
- `image.rs:401`进程全局Images<Picture>；`:451` Load ticket/waiter；`:476` Drop仅删WAITING/PROGRESS，随后Ticket取消；`:497` fetch可同步cache hit，也可后台decode再glib invoke；`:432` on_main排队的result可在WAITING删除后仍持Picture/Lease。
- `image.rs:665` ImageState持主图/placeholder的Picture、Lease、Load与动画；`:869` set替换源释放state持有，`:922` deferred idle捕获弱view但需等待回调实际退出；`:943` start主图与placeholder均必须计数。
- `crates/nsh-core/src/image/decoded.rs:324` load：cached同步返回，否则read→decode；`:359` decode按key合并waiter，`:400`后台闭包持images/bytes/target；`:417` decoded即使waiter已消失仍可能把结果插入cache；`:446` give_up移除waiter/drop ticket，不能停止已进入decoder的函数。
- `image/loader.rs:375` Ticket Drop只cancel；`:686` read_loop read返回后才减running并分发；`:766` decode把闭包排队，`:786` worker只在开始前检查cancelled，运行中的decode仍执行完成。取消≠join。
- `image/memory.rs:496` Lease只管理cache in-use；最后Lease释放不会必然evict。全局cache保留Texture，所以不允许把Lease0写成Texture destroyed。`MemoryCache::clear:386`只清unused；需要先等所有关联lease退出再清，且验证cache entries/cost为0。

## 真正可信的owner绑定：无guest自报token

API全部放`nsh-platform-linux`新私有/实验feature模块`retirement.rs`，不进入QUSOR Core、extension-api、普通SDK或JS事件数据：

```rust
pub struct NativeOwner { /* private Arc identity + checked u64 incarnation */ }
pub struct OwnerRegistration { /* !Clone, one shot */ }
pub struct CloseObservation { /* private owner identity */ }
pub struct FrameStamp { /* private Arc identity + frame generation */ }
pub enum RetirementStatus { Open, Closing(Counts), Released(ReleaseEvidence), Unprovable(Reason) }
pub enum Observation { Bound, DeleteApplied, NativeFinalized, JobFinished,
                       SnapshotObserved(SnapshotEvidence), Released(ReleaseEvidence) }
pub fn register_region(surface: &SurfaceHandle, factory_name: &str,
                       limits: ObserverLimits) -> Result<OwnerRegistration>;
pub fn begin_close(owner: &NativeOwner) -> Result<CloseObservation>;
pub fn poll_close(close: &CloseObservation) -> RetirementStatus;
pub fn register_frame(owner: &NativeOwner, frame: TrustedFrameFacts) -> Result<FrameStamp>;
```

`surface: SurfaceHandle`为adapter真实Surface弱handle新增入口，不能用任意NodeId或guest字符串签署身份。可信Rust renderer在已经授权组件挂载时注册一次性boundary factory。factory名映射到Rust私有OwnerRegistration，Root Native boundary可为无业务空GTK容器；首次实际Component item消耗registration并绑定该NshView NodeId。重复创建、外来surface、世代复用拒绝；UI能调用factory名字不等于拿到token或ACK权，误用最多导致挂载失败。

**必须预扫整批MountBatch再apply**：在Surface.apply前根据Component{name}、Create与Insert构建本批临时parent/owner归属，验证root唯一、循环/跨owner移入拒绝。否则ImageSource在Insert前可能启动未归属decode，事后递归标签是漏账。已有节点若之前加载/分配过未追踪资源，禁止追溯认领。后续批次新子节点在属性应用前继承owner；outro移出后owner标签不清除；同owner reinsert保留，跨owner reparent变Unprovable。children不会因Delete map消失而从registry消失：registry只弱持GObject与不可伪造的owner，持续等最终退出。

Close必须先原子封闭该owner的新frame、load、事件与节点准入，再请求正常JS卸载。拒绝新准入不是提前抹掉旧资源；实际outro仍允许完成。Delete全节点已执行+registry实际清零才可能完成。仅Remove不算Delete；outro复活/迟到Insert不能把Closing恢复Open。

## 最小文件补丁清单与落点

1. `nsh-platform-linux/src/lib.rs`新增仅隔离feature `native-retirement-observer`和Rust observer导出；Cargo.toml feature无新依赖。
2. 新`retirement.rs`：有界registry、opaque identities、计数、状态机、诊断事件。每owner节点/对象/任务有限上限，溢出Unprovable。事件队列满只丢诊断并sticky标错，不丢事实计数或伪ACK；所有ID checked_add。observer不强持被观察GObject，也不调用业务Host。
3. `surface.rs:564`全batch预检/归属、end_batch记录DeleteApplied；surface shutdown关闭准入，不因clear或Drop直接发布Released。
4. `mount.rs:75/99/140/389`在Create前传owner；绑定native subtree；Delete只是标deleted，Remove/outro保持待回收。Insert拒跨owner和Closing新节点；clear逐项记录而非直接清观察表。
5. `view.rs`给ViewState/NshView挂owner；stock Input建立时列出其真实GTK objects（TextView/TextBuffer/ScrolledWindow/adjustments/controllers/IM context可见部分），后续set_native注册实际widget而非只wrapper；在view最终清理处释放state资源，取消自身source/timer，原生对象仍等下面的finalization observer。allocation临时clone不得绕过跟踪。
6. `transition.rs`补outro start/end事实与删除节点unparent时机；无动画不提前签发；动画时长/弱回调消失也不是替代finalization证据。
7. `image.rs`为tracked owner采用**owner独立Images<TrackedPicture>缓存**，共用既有进程Budget；绝不插入原全局Images。TrackedPicture克隆携带真实共享picture/像素owner guard，主图/placeholder/playback全覆盖。文件Blob data要求Memory路径，否则failclosed。fetch/on_main/deferred/progress/playback分别携带job/delivery guard，回调删除只是订阅结束，queued result guard一直持到实际执行/丢弃。Close不先清state破坏outro；真实detach/finalize后释放state，再等job结束、lease0、显式clear owner cache并核entries/cost0。private cache不能只Drop：目前Inner无Drop预算回收路径，必须显式clear_unused且验证无lease，避免虚增全局budget。
8. `nsh-core/src/image/decoded.rs`只加图像基础设施的可选trace hook（不是QUSOR业务Core）：每次**真正创建decode job**时取得guard并把它作为queued closure所有权的一部分；取消queued闭包真正被worker丢弃或running闭包返回且捕获资源释放后才结算。合并waiter不是新增物理job。新private Images可不跨mount合并。给Images加受限cache stats/clear_unused接口供平台聚合，正常构造无hook行为不变。
9. `nsh-core/src/image/loader.rs`首片Memory来源不创建物理read job；其同步回调仍由调用/delivery guard覆盖。decode队列运行/取消结算需保证guard和捕获bytes/images实际先释放再发结束通知，不能cancel标记即done。若后续允许URL/path，要另加ReadJob owner集合与运行结束hook（包括已运行后加入的waiter），本切片不冒充覆盖。
10. `image/memory.rs`增加只读stats（entries/in-use/bytes），只清指定owner cache；不清全局cache或其他mount，避免行为扩大。若用现有clear必须保留其预算扣减/on_evict顺序。
11. `text_input.rs`注册真实native input object与idle composition/change job的生命周期。现有Callbacks中多数Weak仍不能当job已结束；pending idle必须执行/取消并实际释放捕获后才能清job账。不能复刻/改变原IME行为以凑ACK。

这比“加一个Drop guard”大，因为任务取消、全局decoded cache与GTK outro三个独立持有链都真实存在。不能遗漏任何一项后宣称完成。

## 原生最后owner究竟如何观察

- GObject `weak_ref`通知是dispose通知，`g_object_run_dispose`可在仍有强引用时触发；**禁止单靠弱通知签发最终释放**。
- 对受控GTK对象安装observer独有、不删除不覆盖的`g_object_set_qdata_full` destroy-notify（私有quark，不传到JS）。它在对象finalization清data时报告`FinalizationReached`；install需仅一次，registration不持该GObject。禁止应用在关闭流程主动clear qdata；测试必须覆盖显式run_dispose但仍持strong引用，此时不ACK。
- 对GTK Widget限定UI线程生命周期；destroy-notify只排队事实，UI下一turn处理时该finalizer已返回，不在notify重入Host。NshView的Rust imp Drop也只诊断，不能先于其state字段销毁就归零。按实例注册实际child/widget/controller/buffer，父已消失而某个子仍被外部强持时，子registry非0。
- Texture允许decode线程创建/释放，不能把UI下一turn简单当跨线程finalizer完成。对`gdk::MemoryTexture`由自持`glib::Bytes::from_owned(PixelsAndGuard)`记录像素容器最后引用释放；decode job退役在所有结果/临时Picture已drop之后结算。GDK纹理对象另记qdata finalization reached；如果特定格式不能提供精确像素owner/finalizer退出界限则标Unprovable，首片把PNG reader frame转为可跟踪MemoryTexture，不覆盖所有格式。
- GSK snapshot/render node可能继续持Texture；owner缓存清零和view销毁后仍必须等Texture/pixel registry零。若GSK/renderer长期保留它，保持Closing并暴露原因，不把GSK缓存重新命名“共享”规避要求。驱动内部GPU缓存不属于可证明的GObject/像素引用释放，不宣称显存物理归零。
- source/result的计量guard放在真实共享容器里；job结束通知必须在其capture显式释放后，避免在闭包局部Drop顺序中先报告结束再drop像素。observer中的ReleaseEvidence必须包含该owner的closure/resources0证明，而非通用Sentinel Drop。

## snapshot事实的准确位置

`view.rs:580` WidgetImpl.snapshot及`:699` snapshot_children控制参与；`image.rs:1371` draw取实际Picture/animation texture；在真正append texture的draw_one路径记录FrameStamp+owner+NodeId+sourcegeneration+natural pixels+view DIP+surface scale+snapshot sequence。FrameStamp来自trusted ImageData/registry映射，不从JS props取approved。

只在本轮非空、可见、有效layout且准确frame实际append时记录。onLoad、layout ready、queue_draw、snapshot调用入口都不足以证明它；旧image/placeholder命中不算目标frame。Surface某轮绘制结束可补`SnapshotPassComplete`，仍只表示生成渲染树，不是物理present。Hidden/minimized无snapshot时不发InputReady。Retirement无须为了等新paint释放旧输入准入，先关准入即可。

## 完全自持GtkNativeComponent比较

可把所有输入/图像/解码/任务/缓存收进自有component，避开stock loader instrumentation，但将重新承担GTK输入/IME与渲染逻辑，不能继承现有NSH Input已验结论。组件Box在native::deleted就drop，outer NshView仍outro且state.natives仍持widget，故仍必须用上面的真实wrapper/child finalization registry；widget Drop哨兵一样不足。若自有component保留自己的root强引用直到close，再等待自己root销毁，会构成自我阻塞；必须释放自持资源后仅弱观测，不提前清GTK外壳冒ACK。

建议先做stock observer隔离worktree，因为当前真实UI正使用这些对象；不以换掉输入组件绕过已知持有链。此为技术验证切片，不自动升级主pin。

## 必须真实通过的最小验证（不是mock counter）

- 同一存活进程A→close→A循环；无outro/长outro/祖先outro/Delete乱序/Remove后reinsert。每次Closing配额仅在完整实际释放后下降。
- 实际额外strong clone某个NshView、TextBuffer或Texture：DeleteApplied及JS cleanup后仍不得ACK；放掉最后引用后才完成。显式run_dispose仍持strong也不ACK。
- 真实decode屏障：入队未运行、已进入decoder、完成但UI invoke未消费、placeholder/动画分别阻塞。cancel/ticket Drop后不得ACK；任务和result实际退出后才完成。不能用sleep推断完成。
- owner1/owner2相同图片bytes：各自private cache/FrameStamp隔离，close1不清2也不影响2输入；global预算账正确。缓存/closure/mount limits溢出、channel满、panic漏结算保持Closing/Unprovable。
- 图片已经load但尚未snapshot时拒InputReady；实际target snapshot后才发票；旧frame/placeholder/outro旧图不让新frame就绪。截图/真拖选另核，不把snapshot计数当物理屏幕证据。
- 正常关窗记录未收完owner而不伪ACK；存活宿主能确认release是本轮核心退出门槛，不能只跑process exit。

输出逐owner事件序列/零计数证据、native对象identity与最终化事实、每job结束、owner cache stats、Host cleanup ticket确认次序和真实截图。新增feature不开时旧pin行为及ABI应无变化；主树当前false ACK不改，待隔离实测通过后再决定整合。
