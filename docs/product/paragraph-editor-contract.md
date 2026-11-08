# 普通段落提供者 B：文本规则与流式实施契约

状态：B真实guest及无界面A→B→A已有本机资格，见[实际结果](../../experiments/capture-core/PARAGRAPH_PROVIDER_RESULTS.md)；1MiB B算法在冻结Wasmi构建中已通过；真实进程1MiB、原生投影/界面及交互延迟尚未通过。关联 C-09/C-12、F-EXT-01/03/04/05/06、F-READ-03、F-NOTE-01、F-UX-01/02，承接[组件契约](component-provider-contract.md)与[现有A续传证据](../../experiments/capture-core/EVENT_CONTINUATION_RESULTS.md)。A已通过的真实大文本链保持原样，不因本设计重写。B目标是普通签名包经相同SDK、权限和native renderer完成实际段落输入/移动，不以换皮肤代替功能。

## Plain UTF-8语法

文本上限仍为1MiB UTF-8 bytes。仅LF字节构成边界；LF紧前的一个CR与它组成CRLF，其他孤立CR是正文，不规范化。扫描产生正文列表P、ordinal边界槽列表S和EOF终止符T：

- 若最后字节为LF，将最后一个LF或CRLF作为锚定EOF的T；T不额外产生不可编辑的尾sentinel。
- 去掉T后，按剩余LF/CRLF分割正文；每个边界的原始字节依次进入S。空正文是合法可编辑段落。
- 始终有至少一个P；空文本是一个空段。文本精确等于 `P0 + S0 + P1 + ... + Pn + T`。
- 段落索引从0开始，按正文列表计数。CRLF最多向前消费一个CR，例如`a\r\r\n`的正文是`a\r`，终止符是`\r\n`。

| 原始字节（转义表示） | P | S | T |
|---|---|---|---|
| `""` | `[""]` | `[]` | `""` |
| `"a\n"` | `["a"]` | `[]` | `"\n"` |
| `"\n"` | `[""]` | `[]` | `"\n"` |
| `"a\n\n"` | `["a", ""]` | `["\n"]` | `"\n"` |
| `"\n\n"` | `["", ""]` | `["\n"]` | `"\n"` |
| `"a\r\nb\nc\r\n"` | `["a", "b", "c"]` | `["\r\n", "\n"]` | `"\r\n"` |
| `"a\rb"` | `["a\rb"]` | `[]` | `""` |

## 三种公开动作

B必须同时支持现有 `InputChanged { total_bytes }` 与 `MoveParagraph { from, to }`，并新增typed `ReplaceParagraph { index, input_bytes }`；不能把rowtext在native parent拼成整文。新事件仍通过公开InputChunk取得输入字节，绑定当前mount/sequence/base revision，权限仍是精确Draft的NoteDraftEdit，不扩大0.2。Host仅校验和传递身份、索引及有界输入，不切段或拼正文。

`InputChanged`与A语义相同：整文输入逐字节复制到事件上传，不按段落重写，允许保留尚未处理输入后明确重发给新提供者。切换不转移旧mount许可或旧事件；新提供者须获用户批准并创建新sequence。

Pending输入属于明确的动作及base revision。同一行、同一base的完整row input可合并为最新输入；另一行替换或段落移动必须等待当前动作完成，或由用户明确取消后再进行。拒绝新动作时保留原Pending输入并提供复制/重试。全文输入是完整快照，可替换Pending动作；UI切换到全文模式时须先取得用户尚未处理的输入，不能用旧Core正文覆盖它。新revision后重新解析并更新行索引；未完成时保留当前行焦点，其他行编辑/移动入口暂不可用并解释正在处理，避免输入表面可用但提交被静默拒绝。

B必须提供公开的“全文编辑”切换入口，使用整文TextEditor fallback及InputChanged通路。逐段操作保留T，因而无法单独删除最后终止换行；全文编辑允许修改/删除这些字节，角色整体仍能表达全部合法UTF-8修改。尾部空行在全文模式可见且可编辑，不把终止符藏成不可操作暗数据；UI无需暴露CRLF等技术字样。这个fallback不替代B的真正Replace/Move算法。

`MoveParagraph`的to是**移除from后列表中的最终插入索引**，索引范围均为原列表`0..len-1`，但范围内的移动仍可能因边界不可保持而拒绝。候选输出仅重排P，S的ordinal槽及T原字节不变；**重新按本规范解析候选输出，必须得到预期重排后的P及完全相同的S/T，否则拒绝并保留原文**。不得依靠提供者私有parse metadata记住纯文本无法表达的边界。尤其正文以CR结尾、随后槽位S或T为单LF时，CR会被重新解释为CRLF的一部分，必须拒绝。from==to直接成功无变更，使用无effect的complete结束事件，不递增Draft revision、不建立空上传。空末段属于P，可在满足边界条件时移动。只有成功的移动才保证反向`Move(to, from)`逐字节恢复原文；拒绝时提示用户使用全文编辑表达所需文本。

`ReplaceParagraph`只用输入替换目标P的原始正文span；原S及T不变。row input允许LF、CRLF、空文本、NUL、中文和emoji，不trim、不规范化。输入包含边界后，下一次动作重新解析新正文列表，不能沿用旧row索引或旧base revision。Replace是显式文本替换，允许改变paragraph parse，包括输入末CR与原LF合成CRLF；不要求逆操作恢复旧段落分解。超1MiB先拒绝并保留当前正文、Pending输入及复制/修改重试入口；不得以取消事件方式丢弃用户输入。

| 操作 | 输入 | 预期输出 |
|---|---|---|
| Move(0,2) | `a\r\nb\nc\r\n` | `b\r\nc\na\r\n` |
| 逆Move(2,0) | 上行输出 | `a\r\nb\nc\r\n` |
| Move(1,0) | `a\n\n` | `\na\n` |
| Move(0,0) | `\n` | 同原字节，revision不变 |
| Move(0,1) | `a\r\r\nb\n` | 拒绝，原文不变：候选末尾正文CR会被T的LF吞入CRLF |
| Move(1,0) | `a\nb\r` | 拒绝，原文不变：候选首段正文CR会被S的LF吞入CRLF |
| Move(0,1) | `\na` | 拒绝，原文不变：空段移至无T的末尾使原S被重新解析为T |
| Replace(1, `x\ny`) | `a\r\nb\n` | `a\r\nx\ny\n` |
| Replace(1, `""`) | `a\r\nb\n` | `a\r\n\n` |
| Replace(0, `z\n`) | `a\n` | `z\n\n` |
| Replace(0, `🙂\0`) | 空文本 | `🙂\0` |

## Wasm流式算法及常量checkpoint

每次VM fresh，Default fuel/250ms/8calls不扩大；每次最多消费有限512byte source/input块，用已有公开continuation的原子AdvanceEvent保存扫描/输出进度。不得要求一次VM读完1MiB，也不允许parent先算段落或完整输出。

扫描按固定base revision读取Draft；LF是ASCII，可按byte处理，其余正文只沿已验证UTF-8 scalar边界分块。每块保留是否上一byte为CR；遇LF时依据前一byte判CRLF，结合总字节数识别EOF终止符。记录from段及to段（或replace目标段）的起止位置、总段数、T位置即可，不保存全部段落表。边界跨块、连续LF和末尾CRLF必须有独立测试。

Move可描述为至多四个原正文区间的排列：未受影响前缀、跨越部分、被移动段、未受影响后缀；区间内依然逐段识别正文，不能把附着在正文后的旧separator一起移动。独立ordinal separator流从原文顺序取得S，输出每个P后交替发出原S槽，最终发出原T。

这里采用**三个有界cursor**：正常正文流、被移动段范围流、ordinal separator流。各流在自己的已知范围中单调前进；预扫描到输出阶段会重读原文，移动后段落可能回读较早的已知span。混合LF/CRLF且separator槽固定时，不承诺两个全局单调cursor即可完成。增加一个常量cursor比隐藏回读或存O(n)ranges更清楚；源段再大也不进入checkpoint。

Replace跳过一个目标正文span，输出该位置的公开input流；正文/ordinal separator流加input cursor同样是常量状态。预扫描后先计算 `source_bytes - replaced_body_bytes + input_bytes`，checked算术及1MiB校验在Begin之前完成。Move长度保持不变，但必须在Begin之前验证候选重新解析的P/S/T边界。原始P本身不含LF，重排时需要检查所有受影响的正文末byte与后续原separator/tail首byte：末CR遇单LF即拒绝；若T为空且段落数大于1，移动后的末段为空也须拒绝，因为末个S会被重解析成T，段落数随之减少。末空段与EOF终止符均按重新解析规则核实。可在预扫描时记录常量个受影响段的末byte及相关槽位类型，无需暂存候选全文；若不能证明边界保持就fail closed，不提交后再检查。InputChanged直接用输入总长。

以下JSON只示意常量状态的逻辑字段，不是公开wire或实际存储格式。当前guest采用544byte固定结构，版本化零run/literal编码后最坏552bytes；三个缓存各128bytes，每次最多64scalar让出，默认执行预算不变。checkpoint不得包含完整正文或O(n)段落表：

```json
{
  "v": 1,
  "phase": "scan",
  "action": "move",
  "scan": {"offset": 0, "paragraph": 0, "body_start": 0, "previous_cr": false},
  "source_bytes": 0,
  "paragraph_count": 0,
  "from": 0,
  "to": 0,
  "selected": {"start": 0, "end": 0, "found": false},
  "target": {"start": 0, "end": 0, "found": false},
  "tail": {"start": 0, "bytes": 0},
  "emit": {
    "segment": 0,
    "body_offset": 0,
    "moved_offset": 0,
    "separator_offset": 0,
    "input_offset": 0,
    "output_offset": 0,
    "paragraph": 0,
    "part": "body"
  }
}
```

offset/长度为有界u32，索引<=1MiB，布尔found区分合法零span与未发现。phase仅scan/start/emit/done，action仅copy/move/replace；所有字段严格解析，拒未知版本/越界/与trusted事件action不符，不从checkpoint恢复权限。源身份、mount、sequence、base revision和checkpoint revision由公开continuation外层绑定，不接受checkpoint替代。实际checkpoint为O(1)、应控制在2048bytes内（公开上限8KiB）；每VM最多一次checkpoint chunk即可恢复本算法。Begin/Append/Commit与checkpoint进度必须同一个AdvanceEvent原子执行；输出offset由last_effect确认，响应丢失从回执恢复，不能独立先写上传再写checkpoint。

输出拼接到至多512byte的临时guest缓冲，尊重UTF-8边界；每次输出可跨若干body/separator片段，但不能为填满buffer扫描无界数据；每VM为扫描byte数和空段处理数量分别设置固定步数上限，耗尽即checkpoint让出，不能在全空段文本上空转至超时。separator流若需越过长正文，应推进cursor并checkpoint，允许本VM无Append的Advance。最后CommitReplace complete=true；Completed保留结果回执，旧base可读回执但不得再次写。错误保留用户输入；超时/撤权/新事件按既有continuation状态机处理，不自行解锁Saving。

## 构建路线与退出门槛

2026-11-08本机只读工具调查并做/tmp工具链冒烟：Ubuntu clang21.1.8注册wasm32目标，可生成freestanding C对象；rust1.97.1自带的wasm-ld可链接。LLVM wasm assembly的 `.functype run () -> (i32, i32)` 包装C `work()->i64` 已成功汇编/链接为两页memory的Wasm。这里只证明工具链可用，未证明B算法或runtime执行。可采用无libc/no-WASI C guest + 小型multivalue assembly wrapper，再由导出SDK Rust作者签名，避免新增Host业务或大量手写WAT；构建须固定工具版本/参数/产物hash并验证仓库外可重复。

独立B作者、guest C及wrapper、公开typed ReplaceParagraph/schema、应用输入转发与真实普通包consumer现已实施；原生逐段输入路由和公开投影仍待完成。A旧工件与0.1/0.2语义保留。

B退出要求：普通签名manifest/ParagraphEditor asset可安装，A→B→A保留文本；InputChanged、ReplaceParagraph、MoveParagraph均由真实Wasm变换；表中期望值、混合分隔符、空段、空末段、尾终止符、跨512byte边界、Unicode/NUL及1MiB长段实测；成功移动的逆操作逐字节恢复；孤CR与LF重新结合的两项反例必须拒绝且原文不变；错index/from==to/超限输入/旧revision有明确行为。覆盖扫描、Begin、Append、Commit的真实lost reply恢复与撤权竞争，维持8calls/250ms/fuel默认预算；真实Core保存、SQLite重开/receipt及卸载保留知识另行取证。只有图形换位按钮或native parent预先拼好文本均不算B完成。

首批 [验收数据](../../experiments/capture-core/fixtures/paragraph-cases.json)由dsh生成16项正例，经主负责人独立核对后纳入，并补4项歧义拒绝用例；[委派记录](../../experiments/capture-core/evidence/dsh-paragraph-fixtures.json)保留输入／输出hash。它们是后续B实际Wasm验收的固定预期，这些数据现已由真实B及A→B→A消费者验证；原生界面与完整退出门槛仍按上述结果分别处理。

## 下一切片：独立只读段落投影

wire、独立应用registry与Host挂载授权已完成局部实现和检查，见[投影与bootstrap证据](../../experiments/capture-core/evidence/projection-foundation/bootstrap-record.json)；只读查询协调器和带签名协议声明的新guest已实现，八例实际Wasmi验证通过，但早期出现过真实deadline；精确材料验包缓存后原包八项真实OS投影已通过（76 worker/412 calls，0 deadline）。原生窗口与共享编辑服务的真实运行仍待验证。目标是让同一普通提供者生成有界段落视图，Host只呈现而不切段。投影是提供者的视图，不能冒充可信原文/来源。Host不解析正文便不能独立证明ordinal的语义正确；它只验证版本、范围、UTF-8边界、长度和句柄归属。提供者已有精确Draft的NoteDraftEdit，投影不扩大能力或引入持久授权。

采用独立的 `paragraph-projection/v1` 查询：可信协调器生成不可复用query identity，固定绑定当前mount/epoch、Draft、revision与查询generation；用户滚动请求首段ordinal及最多8行。guest通过自身公开context取得查询，再固定该revision读取Draft，输出row ordinal/body byte spans。不得把滚动实现成InputChanged、MoveParagraph或无effect编辑事件；这些做法会覆盖待处理输入或争用编辑checkpoint。

投影续传仅允许读取metadata/checkpoint以及CAS推进checkpoint/投影窗口，无文本effect字段。metadata包括运行/完成/取消/失效状态与版本化checkpoint进度；窗口有最多8个按序、不重叠的row span及明确的扫描是否完成。未扫描完的长段可以只给已知span并标不完整，不虚构总段数或下一段。原始正文片段最多512 UTF-8 bytes，窗口/回复预算仍≤4KiB，checkpoint≤8KiB且分块≤2048，身份和权限不能从checkpoint恢复。请求与返回采用closed DTO并独立协议版本；尚未发布SDK，不向0.1/0.2扩词汇。

应用层独立只读registry只持有有界查询状态/窗口/checkpoint，不依赖Host/NSH/运行时，不修改AnnotationDrafts或ProviderEvents。版本读取、投影发布和安装/挂载fresh权限校验由可信协调器在短序列化门中完成；旧revision、旧query、关闭/撤权后的call及迟到result均拒绝。mutating Draft/Continuation请求在只读任务中拒绝，即使提供者另有编辑任务。取消投影仅结束查询，不取消编辑upload或改变草稿修订。

每mount最多一个运行查询、一个latest待办；缓存最多三个窗口和有界正文片段。编辑优先于滚动，滚动不抢占原生组合输入。替换查询时作废旧generation，所有worker仍须真实reap/lease清理，不能删除记录假释放。持有待处理输入的row按原身份和base保留；投影刷新不得覆盖其内容或悄悄按新ordinal重绑。

长段片段只读并提供全文入口；完整正文尚未加载、段落span尚不完整或revision变化时，不允许把预览片段作为ReplaceParagraph提交。显式加载完整段时仍保留1MiB能力，校验完整范围后才签发当前版本/当前query的row handle；不完整时不把编辑控件伪装成完整可保存段。全文编辑继续使用InputChanged，不通过Host拼接部分输入绕过guest。

最小退出证据：普通签名B在真实进程中投影8行，混合换行/空段/Unicode/NUL/块边界/大量空段无丢失；1MiB单长段有明确不完整状态与全文入口。滚动/取消不改变编辑输入、checkpoint、upload或revision；修订/撤权后迟到投影拒绝，只读运行中的mutating call拒绝。随后在真实原生窗口验证可见范围、焦点、长段只读及全文入口。交互延迟单独测量，不以静态描述或无界创建段落控件代替。

同一guest的用途入口采用独立API0.3 `component-invocation/v1`，closed请求只读取本次purpose，响应只有edit_event/paragraph_projection，不包含身份或批准。wire及同Host fresh挂载方法已实现，旧API0.2 InvocationRequest和旧0.3编辑Context不变。新签名包仍须声明支持并实际按purpose分流；未完成前不把现B当作已支持投影，也不靠失败试探或假sequence启动。
