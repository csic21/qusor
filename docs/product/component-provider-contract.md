# 可替换想法输入提供者：最小实施契约

状态：应用层草稿、0.3 wire、同Host临时mount和真实Store安装事实端口已实施并有局部验证；尚未发布SDK或完成普通提供者替换，最新代码已通过完整兼容回归，见 [本轮结果](../../experiments/capture-core/COMPONENT_CONTRACT_RESULTS.md)。关联 C-09/C-12、F-EXT-01/03/04/05/06、F-READ-03、F-NOTE-01、F-UX-01/02；承接 [ADR 0028](../architecture/adr/0028-extensible-native-workspace.md) 与[组合设计](../design/composition.md)。本切片仅支持当前阅读选文产生的**新想法草稿**，不开放既有笔记编辑、Markdown、富文本或跨退出草稿恢复。

## 版本和权限边界

新增显式 Host API 0.3、贡献类型 `component_provider`、能力 `note.draft.edit`、资源 `Draft { vault_id, draft_id }`。贡献必须声明该能力，且 manifest 的最小 API 为0.3；首角色固定 `annotation.plain-text-editor/v1`。普通签名资产 `assets/ui-composition.json` 声明协议 `ui-composition/v1`、contribution_id、该角色和有界 native view tree；引用的贡献必须真实存在并为新类型。包格式、签名域和既有 manifest 字段不变，不向旧 schema 塞新字段。

新能力只在0.3、只对精确 Draft 有效。0.1/0.2的 capability/resource/贡献可用性表逐项保持；尤其不能把 `version == 0.2` 改成 `version >= 0.2`，令 ReaderSurface 或 SelectionRead 自动延伸到0.3。0.3首切片仅启用新提供者所需能力；阅读者仍用自己显式0.2会话。组合资产不是权限，也不把旧 Command 自动解释成组件。

`GrantScope`/profile1、`GrantSet`/profile2和旧hash/serde完全不变。profile2继续固定API0.2，Draft和新能力一律拒绝，无新持久授权profile。安装与默认提供者偏好均不授予草稿访问。首切片采用可信用户明确“仅本会话启用并挂载”：精确到草稿、提供者安装及mount，关闭/重启失效。现Store没有独立启用布尔值，旧grants本身就是auto-enable意图；新组件不伪造该记录，也不把安装存在推导为启用。`bind_component_vault`只持久化库亲和绑定，不授予执行或Draft许可。

## 可落代码的接口切片

以下为需实现的签名，不表示仓库已有。wire类型采用 closed serde、显式协议版本及受限标识符；Session、MountHandle、UserMountIntent、FrozenSave均不可反序列化或由guest构造。

```rust
// extension-api/src/draft.rs；api_version必须0.3，draft_protocol必须1。
pub struct DraftRequest {
    pub api_version: ApiVersion,
    pub draft_protocol: u16,
    pub request_id: String,           // 最多32 ASCII bytes
    pub resource: Resource,           // 必须Draft
    pub mount_id: String,             // 最多32 ASCII hex bytes
    pub operation: DraftOperation,
}
pub enum DraftOperation {
    ReadChunk { revision: Option<u64>, offset: u32, max_bytes: u16 },
    BeginReplace { expected_revision: u64, total_bytes: u32 },
    AppendChunk { upload_id: String, offset: u32, text: String },
    CommitReplace { upload_id: String },
    CancelReplace { upload_id: String },
}
pub struct DraftSnapshot {
    pub draft_protocol: u16,
    pub request_id: String,
    pub draft_id: String,             // 最多32 ASCII hex bytes
    pub mount_id: String,
    pub revision: u64,
    pub offset: u32,
    pub total_bytes: u32,
    pub text: String,                 // 最多512 UTF-8 bytes的一块
}
```

revision从1开始，checked_add溢出拒绝。ReadChunk首次revision=None固定当前修订，后续必须给同一revision；修改后旧revision读取返回Conflict，调用者丢弃不完整读取并重试，不能拼接不同修订。offset按UTF-8字节且必须字符边界；max_bytes为1..512，返回不超过预算的完整字符前缀、总字节数及offset，仅offset==total_bytes时空块表示结束。若max_bytes小于下一完整UTF-8字符，明确返回预算错误，不得用空块伪装结束或让调用者无进展循环。snapshot不含原文/来源。成功响应采用固定tag的Snapshot/UploadStarted/ChunkAccepted/Replaced/Cancelled，所有ID最多32ASCIIhex、request_id最多32ASCII，禁止任意额外元数据；完整序列化响应必须<=4096 bytes，最坏512bytes控制字符JSON转义也须实测。错误沿现有有界ApiError，原始请求仍<=64KiB。

**文本上限沿用Core的1MiB UTF-8预算，不截断、不trim、不规范化**。BeginReplace校验expected_revision与total_bytes<=1MiB，返回新upload_id；每mount最多一个upload，每Draft也最多一个upload，重复begin拒绝。AppendChunk严格连续offset、每块最多512 UTF-8 bytes、每次有效UTF-8，重发/跳号/超total拒绝，不改变当前文本。CommitReplace须收到恰好total_bytes且原expected_revision仍当前，再原子替换并递增revision；冲突保留当前文本并销毁upload。Cancel、mount关闭、撤权或最后活动30秒timeout均清除upload；timeout由监督tick执行，不依赖guest继续poll。所有分块请求都独立fresh授权，VM单次8hostcalls预算不限制文档生命周期，可多次独立受监督调用继续同一upload；guest不得携带执行许可跨调用。文本读/上传与native输入遵循相同SDK，不经私有状态注入绕过。

会话最多8个草稿、每草稿当前文本<=1MiB、每草稿最多一个<=1MiB暂存upload，文本总预算16MiB；来源快照另计且限制为8份、合计<=8MiB，超过在创建前明确拒绝。每次Append先检查声明剩余及全局预算，缓冲不得无界增长。upload所有权绑定mount，不允许B接续A的半成品；切换前明确提交、取消或保留当前提供者。读取重试上限由调用者显示冲突，不得无休止阻塞UI。既有大于1MiB文本继续由Core原校验拒绝，本切片仍只创建新想法，不扩大到历史笔记编辑。

```rust
// extension-host/src/durable.rs：安装事实与持久授权记录分开。
pub trait ComponentAuthoritySource {
    fn component_installation(
        &self, package_id: &str, read: AuthorityRead<'_>,
    ) -> Result<ComponentInstallation, ApiError>;
}
// ComponentInstallation是可信源返回的当前事实：installation_id/revision/
// generation/manifest/digest/vault_id及已验证composition资产/hash；未卸载。
// 无grants、无伪造required_api；Activation可做preflight，Current不可执行VM。

impl DurableHost {
    pub fn mount_component(
        &mut self, source: &dyn ComponentAuthoritySource,
        intent: UserMountIntent, now: u64,
    ) -> Result<MountedComponent, ApiError>;
    pub fn validate_component_current(
        &mut self, source: &dyn ComponentAuthoritySource,
        mount: &MountHandle, now: u64, run_id: Option<&str>,
    ) -> Result<(), ApiError>;
    pub fn dispatch_draft<T>(
        &mut self, source: &dyn ComponentAuthoritySource,
        mount: &MountHandle, now: u64, run_id: Option<&str>,
        request: DraftRequest,
        handler: impl FnOnce(&AuthorizedDraftContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError>;
    pub fn close_component(&mut self, mount: &MountHandle) -> Result<(), ApiError>;
}
```

MountedComponent包含opaque mount handle、显式API0.3的执行会话和冻结的签名模块/资产身份，不暴露可变Host。UserMountIntent由公开的可信原生“选择提供者”入口创建，绑定workspace/slot/role/draft_id及预期安装digest/generation；不接受guest自报approved。纯Host同一实例内新增mount临时授权表及单独会话路径，不调用 `replace_grants` 重置阅读会话，不把临时grant塞进durable `AuthoritySnapshot`。现有 `AuthoritySource`、activate/dispatch和profile2 reader路径无需改语义。Source缺少新实现即默认拒绝，不能从缺失saved_grant推导组件许可。

```rust
// learning-application/src/annotation_draft.rs：不依赖Host/NSH/SQL。
pub struct AnnotationDrafts { /* 会话内有界registry */ }
impl AnnotationDrafts {
    pub fn create_pdf<S: KnowledgeStore>(
        &mut self, app: &KnowledgeApplication<S>, id: DraftId,
        document_id: i64, selection: PdfDraft, origin: Provenance,
    ) -> Result<DraftRevision>;
    pub fn read_chunk(&self, id: &DraftId, revision: Option<u64>,
        offset: u32, max_bytes: u16) -> Result<DraftChunk>;
    pub fn begin_replace(&mut self, owner: MountOwner, id: &DraftId,
        expected: u64, total_bytes: u32, now: u64) -> Result<UploadId>;
    pub fn append_chunk(&mut self, owner: MountOwner, upload: &UploadId,
        offset: u32, text: &str, now: u64) -> Result<u32>;
    pub fn commit_replace(&mut self, owner: MountOwner, upload: &UploadId,
        now: u64) -> Result<DraftRevision>;
    pub fn cancel_replace(&mut self, owner: MountOwner, upload: &UploadId) -> Result<()>;
    pub fn expire_uploads(&mut self, now: u64);
    pub fn freeze_save(&mut self, id: &DraftId, expected: u64,
        operation_id: &str) -> Result<FrozenSave>;
    pub fn commit_pdf<S: KnowledgeStore>(
        &mut self, app: &mut KnowledgeApplication<S>,
        frozen: &FrozenSave,
    ) -> Result<PdfReceipt>;
}
```

DraftId是应用生成的不可复用32hex身份，DraftRevision为u64包装；DraftChunk仅修订/总长/offset/text，FrozenSave字段私有、绑定registry所有者、draft/revision、operation_id及完整内容hash。freeze_save要求无活跃upload，将草稿从Editing转Saving，固定正文和操作ID，Saving拒绝编辑和新upload。结果未知保留Saving，只允许查同operation的receipt或用同FrozenSave重试；可信只读receipt查询绑定FrozenSave的operation与内容hash，不先执行保存重新授权或再次freeze，不能让撤权后无法核对已发生的提交；重新提交仍须fresh保存授权；不得因切换provider生成新operation重复保存。确定未提交的失败可回Editing，已成功转Saved只读，历史note修改不属本切片。registry最多8个草稿，关闭后销毁；无磁盘恢复承诺。首切片只接单PdfDraft，复合选区仍走原流程，不隐式丢成首段。create_pdf先通过 `query_document(document_id)` 获取真实Core文档，再调用 `core_api::pdf_contract::resolve_pdf(&document.snapshot, &selection)` 校验，持有不可变选择器/精确引文/来源版本及Provenance；原件与完整快照继续归Core所有，保存个人text与来源分离；不要从renderer画面或provider回报拼装原文。commit_pdf复验冻结修订未变和当前Core文档/来源版本，调用现有 `capture_pdf(..., user_note)` 原子事务及receipt。失败保留草稿且按确定失败/未知结果处理状态，成功才标记已保存；同operation不同内容仍冲突。Core的1MiB不因此缩小，512只约束单次传输块。

## 当前性顺序与保存冻结

1. 普通签名包安装默认禁用；验证全部asset和manifest并绑定库，用户明确仅本会话启用并挂载具体提供者。可信源读取当前安装事实，核API0.3、角色和asset，再在同一Host建立精确临时mount授权。启用不自动接管slot。
2. 创建草稿由公开可信原生“写想法”动作触发。仍用原reader自己的0.2会话，在同一管理gate内fresh查安装/lease、SelectionRead和CapturePropose、解析真实Core版本并create_pdf。该动作对任意合格普通ReaderWorkspace均可用，无第一方ID特判。冻结来源后，窗口变化导致原selection lease失效不删除草稿；provider无权改引用。
3. 启动provider沿现process路径：准备lease/mark_spawn，GuardReady核OS身份，先持久bind，再fresh组件安装+mount授权，才release Boot。每个Call在同一短gate内核当前安装/运行lease、mount代数、精确Draft、revision，然后handler读写registry；gate内不得执行guest或阻塞broker。Result接纳重新fresh核对，真正Reaped才finish lease。
4. 用户切换A→B需独立确认B访问当前草稿。先使A mount失效，保留应用草稿数据；原native组件进入Closing，真实释放确认前不返还配额，不销毁后假报关闭。B获新mount，读取同一revision/text；旧回调即便revision碰巧相同也拒绝。未获许可或B挂载失败保留草稿并显示恢复入口。
5. 保存按钮属于可信工作区动作，不让guest JSON声明用户批准。在管理gate内重新核原capture提供者installation_id/digest/generation当前性、原vault/document/version和CapturePropose，并核当前Core版本，冻结当前草稿revision/内容hash/操作ID，再由应用commit。capture授权撤销、升级、卸载或文档版本变化均拒绝；绝不为了让草稿可保存而复活失效selection lease。保存采用先前已验证冻结的来源，不再次依赖活选区；冻结草稿只持有不可变candidate/source/provenance事实，不持有permit或活lease。Host另外持有原capture安装的generation与digest绑定，不能以Core Provenance替代；现有Provenance不包含generation。可为原capture安装建立新的显式会话，但不恢复旧selection lease；更新digest/generation不匹配、撤权或卸载均保留草稿拒保存，不借editor/Host身份或伪造provenance绕过。Core自身不验证插件安装权限，此检查属于提交前可信gate。原文展示在provider外的可信区域。提供者的编辑权限不是保存知识的权限。

## 修改位置及match影响

实施时需改而本文件未改：

- `crates/extension-api/src/lib.rs`：新版本、cap/resource/contribution及显式可用性表；新增 `draft.rs`/`composition.rs` 和独立schema，更新wire测试。`capability_resource_valid`、Resource::vault_id/validate、Manifest::validate/supports_api均需穷尽处理；旧0.1/0.2 fixtures字节及拒绝集保留。
- `crates/extension-host/src/lib.rs`、新增 `component.rs`、`durable.rs`：同Host临时mount授权、来源事实端口、fresh gates；覆盖grant验证/covers和contribution查询。Vault grant不得覆盖Draft；不通过空grants启用旧activate。`durable_surface.rs`/surface协议保持显式0.2。
- `adapters/extension-store/src/component.rs`：安装事实新端口重用现有验包、库绑定、current/run lease和privatePending检查；不读取伪造profile3或enabled字段。`grant_set.rs`和旧profile1验证明确拒绝新cap/resource，policy版本不变；无需新表/新数据库。
- `crates/learning-application/src/lib.rs`及新增 `annotation_draft.rs`：会话草稿用例、保存冻结和真实Core事务测试。wire DTO在extension-api，知识来源类型仍core-api所有，不向learning-application加入Host依赖。
- `experiments/nsh-reader/native-host/plugin_surface.rs`及对应native UI入口：消费公开组件facade、可信写想法/切换/保存入口、native primitives与release确认；不加包ID分支。`experiments/capture-core/export_sdk.py`、SDK版本文档和独立作者fixtures：发行新模块/schema并验证仓库外构建，不发布Host/Core私有实现。

新增枚举variant会使API/Host及SDK消费者的穷尽match失编译；还需检查runtime调用路由、Store profile1 scope验证、测试fixture/示例和独立SDK作者。不能用全局通配allow通过编译；未知版本/角色/动作默认拒绝。runtime-process仍只承载raw字节，无需新业务分支；不新增crate/依赖。实施者须以实际rg/编译结果补齐遗漏，而非认为上表穷尽所有调用点。

## 普通包退出验收

- 两个仓库外普通签名包仅用发行SDK：A多行输入，B段落编辑及换位（guest执行真实文本变换），同一native renderer。不得仅换主题，也不得复制业务到Host。实际安装/启用/选择/替换，无重编译宿主，无第一方特权。
- 新建想法，中文/emoji/空段/CRLF/LF/NUL逐字节往返A→B→A，1MiB成功、1MiB+1拒绝不丢原文；跨多次VM分块读取/写入、最坏转义chunk真实经过process的4096响应预算。追加乱序/重复、上传超时/取消、修订竞争、切换中断、全局预算耗尽均不暴露部分替换。坏UTF-8/超帧/未知角色拒绝。
- 旧mount、错Draft、跨vault、错revision、错安装generation、撤权/禁用/卸载、Boot前/Call中/Result前竞争均fail closed；升级不转移临时许可。旧profile2读写/重开、0.2reader和Surface全回归，0.2明确拒绝新能力。
- resize使旧选区lease失效后仍能编辑冻结草稿；另有当前capture授权才允许保存。真实Core查询核对引用/来源/用户想法分离、receipt幂等、版本冲突拒绝，导出恢复后相同；卸载提供者不影响已保存知识。退出前未保存草稿提示会丢失，不虚构恢复。
- 正常/失败/切换均证明native实际release后配额归还；独立watchdog、真实reap及unknown spawn隔离不回退。NSH平台成绩分别记录，不用Core测试替代真实窗口输入、键盘焦点与中文组合输入验证。

完成上述真实证据前，仅可称已完成契约设计或局部实现，不能称公开组件替换已交付。

## 当前代码与验证

应用层已落位 [draft_text](../../crates/learning-application/src/session/draft_text.rs)、[冻结来源](../../crates/learning-application/src/knowledge/annotation_source.rs) 和 [绑定草稿](../../crates/learning-application/src/session/annotation_draft.rs)。AnnotationDrafts封闭持有文本、来源及原capture generation，FrozenAnnotationSave只能由该registry创建，不暴露任意重新配对来源的入口。来源最多8份/8MiB逻辑序列化事实，文本及上传声明预留合计16MiB。

包内14项测试通过：1MiB完整分块往返、UTF-8边界、修订/连续上传、超时和owner撤销、跨registry拒绝、未知保存锁定、Core已提交但回复丢失后只读核对，以及两个文档/原插件身份/文本不串用。这里的丢回复由内存KnowledgeStore故障fixture注入，验证的是应用状态和真实Core用例逻辑，不冒充实际OS/SQLite故障。整个Workspace155项Rust测试与四组真实process controller测试通过；架构守卫、42项守卫测试、格式、workspace Clippy及本地文档链接检查通过。

应用代码不实施插件身份授权；同Host临时mount、安装新鲜性、公开wire/schema、SDK作者包及native组件替换仍待继续。此前119源的原生UI资格属于其封存工件，不包含本次新增应用模块；本轮新增代码尚未接入那个native消费者。

### Wire与Host接线增量

新增0.3的Draft/Composition/own-invocation Context wire与四份schema；普通静态descriptor只允许两种有限原生编辑root，不接受任意URL/属性/系统界面。Context为新输入和段落操作提供公开有界事件读取，module不依赖私有native入参。旧0.1/0.2请求明确拒绝Draft词汇，旧普通授权入口拒绝NoteDraftEdit，profile1/2不增加新权限。

同一Host使用唯一persistent安装到native身份映射；临时mount不创建普通Session或修改安装Enabled状态。Closing在真实释放确认前保留64总/16每安装的配额；换persistent安装ID必须先通过旧ID卸载证明及双资源释放退休，不能自动注销换身份。Store已实现签名资产校验、无权限副作用的库绑定及每次fresh安装事实读取，真实Store+Host测试覆盖停止后旧mount拒绝。

上述尚不包括provider事件缓冲、共享进程生命周期接线及实际Wasm提供者。尤其多VM续传必须让guest通过公开接口取得自身upload进度与有界临时状态；现fresh VM不能凭说明文字记住此前的段落扫描状态。该入口及事件取消/修订语义须补齐，再运行1MiB文本的A/B真实消费者；不能降低文本容量、放大旧8calls预算或把段落变换搬到Host来通过验收。

本轮177项Rust/完整包Wasm捕获与新Controller资格、仓库外0.3作者及133份源码封存见上述结果。后续明确采用“草稿效果与checkpoint原子推进”的续传方向；单独checkpoint写入不足以覆盖Begin/Append回复丢失，完成回执须在原事件revision过期后仍可只读恢复。该续传接口尚未实现，不将预算可编码误称为1MiB真实提供者已可运行。


### 原子续传与普通提供者A实施

[本轮真实结果](../../experiments/capture-core/EVENT_CONTINUATION_RESULTS.md)记录新增continuation wire、独立Host gate、ProviderEvents和共享泛型ManagedRun。AdvanceEvent现在使用event自有upload及独立EventEffect，文本效果与checkpoint/最后回执原子推进；不再要求fresh VM记住外部upload_id。completed事件允许只读恢复最终revision，Expired保留输入供明确恢复但不复活上传。

普通签名A已通过真实worker的小流3次丢回复恢复／迟到结果拒绝，以及最坏NUL1MiB的2050个fresh VM；正式保存均经原capture身份fresh授权，SQLite重开和空库恢复一致。来源为合成Core snapshot，capture身份包未执行，不冒充原生手势或完整reader。191项Rust及全兼容回归通过。作者B、真实A/B替换与NSH视图接线仍待实现，不能由此宣布组件替换交付。


## B与无界面替换的后续资格

B普通SDK签包、真实段落算法、逐段输入及默认稳定包的20份A→B→A已通过，见[实际记录](../../experiments/capture-core/PARAGRAPH_PROVIDER_RESULTS.md)。真实进程链183worker/681calls，正式保存20条并重开/完整恢复，卸载B后知识保留。1MiB B、扫描/撤权故障、公开段落投影和原生UI仍分别处理；既有A大文本资格或此前UI截图均不替代这些门槛。
