# 段落提供者 B 与无界面 A→B→A 资格

2026-11-08，关联 C-09/C-11/C-12、F-NOTE-01、F-EXT-01/03/04/05/06。实现遵守[段落语义](../../docs/product/paragraph-editor-contract.md)与[编辑/切换交互](../../docs/design/paragraph-editor.md)。本记录证明普通包与真实进程的数据链，不宣称原生界面已接通。

B 的[独立作者](author/src/bin/component_paragraph_author.rs)用仓库外新导出的公开SDK构建、签名。正文算法位于[freestanding C guest](author/src/component_paragraph.c)，[multivalue入口](author/src/component_paragraph_entry.s)只包装返回值；不依赖libc、WASI或内部Host/Core。B与A使用相同API0.3、精确草稿的临时挂载授权和公开续传能力，不享有第一方特权。工具链、manifest、模块和包摘要见[作者记录](evidence/component-author-b/record.json)。

## 数据和执行边界

InputChanged逐字复制用户全文；ReplaceParagraph只替换目标正文span；MoveParagraph只重排正文，保留原分隔符槽及尾终止符。不能无损表达边界时在Begin之前拒绝。same-index移动不上传、不增加修订。未处理的局部输入仅能在同一行、同一base合并；其他动作拒绝且保留原上传、checkpoint、输入和时钟。

guest使用三个128byte缓存与每步至多64个scalar；checkpoint是544byte固定结构的版本化有界压缩，最坏552bytes。guest校验解包长度、版本、cursor/phase/action及可信事件的一致性；不从checkpoint恢复权限。每次fresh VM最多一次source/input读取，最多5次实际hostcalls，原100万fuel/250ms/8calls不变。扫描与输出均由guest执行；broker只做DTO、权限与事件身份核对。

先前512byte缓存实现曾在跨块压力中耗尽fuel，保留[失败日志](evidence/paragraph-guest-first-pressure-failure.log)。修复没有放宽预算或缩小1MiB文本要求；1MiB B实际执行与延迟仍在独立取证，不能从64KiB结果外推通过。

## 实际证据

| 验证 | 结果与边界 |
|---|---|
| 公开SDK作者 | 仓库外构建/签包/Clippy通过；Linux本机clang21+固定Rust linker，不宣称其他作者平台可用 |
| 固定20项算法 | exact签包模块真实Wasmi执行：66VM/293calls，含4项歧义拒绝 |
| 损坏checkpoint | phase/action/cache长度/span/base64/压缩溢出六项拒绝；正文和待处理输入保留 |
| 流式压力 | 跨512边界45VM、64KiB长段3077VM、4096空段194VM、NUL交替101VM、全文fallback26VM；压力模块与签包模块可执行sections逐字相同，custom name段因链接输出basename不同，详见作者记录 |
| 独立oracle | [200组](evidence/paragraph-root-oracle/record.json)中文/emoji/NUL/空段/混合换行；Python正则分解与重解析独立计算预期，exact签包模块712VM/3214calls/maxfuel630002全部符合，含8项拒绝 |
| 真实组件替换 | [默认稳定包复验](evidence/linux-paragraph-switch.json)：20份独立草稿各A→B→A，183实际worker/681calls/全部owned reap；guest执行Move/Replace，父组件不计算输出 |
| 故障与权限 | Begin/Append/Commit后各真实终止worker且不回reply，freshVM续传；直接Draft写拒绝；后继挂载后旧回调仍拒绝且不影响新挂载 |
| 正式保存和卸载 | 20条记录分别经原capture身份fresh授权后冻结/提交；SQLite重开、完整导出空库恢复；真实卸载B后知识archive不变 |

真实消费者的source仍是合成Core PDF snapshot，不是解析器、原生手势或Surface lease证明。capture身份来自真实安装的普通签名包及profile2，fixture模块未执行。编辑器仅有草稿能力，不能自行正式保存。没有原生widget，因此本轮headless任务结束后的释放确认不构成原生view/decode/render释放ack。

[149份实现输入](evidence/paragraph-switch-source-archive.json)和对应二进制摘要已封存；A的大流旧资格继续按旧工件记录。当前源码的[A小链复验](evidence/linux-paragraph-ingress-a.json)为10VM/35calls、3次实际丢回复恢复、2条保存与重开恢复；没有重跑旧1MiB A链。本轮193→194项根Rust增量及边界/格式/Clippy记录见[输入协议检查](evidence/paragraph-input-ingress.json)。

复验默认稳定包：

```sh
cargo test --manifest-path experiments/capture-core/Cargo.toml --test component_paragraph_consumer --locked --offline
```

## 继续推进

早期七项故障消费者的主负责人复验曾遇过期reply，失败保留。现消费者加入Call准入、严格错误分类、有界恢复及真实reap后保留输入的失败路径；[独立八场景复验](evidence/linux-paragraph-recovery-faults.json)通过104worker/440calls，包含故意制造的stale_process_reply恢复、扫描checkpoint丢回复与六次关闭/撤权。唯一自动恢复明确标记planned，非计划watchdog/runtime deadline均为0；不能据此声称实际压力超时已完成恢复。原默认20项在同一冻结恢复版本[重跑通过](evidence/linux-paragraph-recovery-default.json)。复制验证是可信应用读取fixture，不是实际原生剪贴板。1MiB B首次尝试在VM索引11423处真实deadline失败，逆向尚未启动，见[失败记录](evidence/paragraph-b-one-mib-attempt-1/record.json)。原harness panic使该次内存checkpoint丢失，不能声称仍可接续。[新attempt2](evidence/paragraph-b-one-mib-attempt-2/record.json)已用冻结binary/源码快照实际启动，保留同一应用状态并按有界策略处理真实deadline；guest和预算不变，已终态通过：正向49,153 VM/221,188 calls，逆向32,770 VM/147,465 calls，共81,923 VM/368,653 calls，deadline为0。冻结开发构建累计约60分35秒，仅证明实际Wasmi与生产应用的1MiB算法及逐字节结果，不证明真实OS进程、Core保存或原生UI资格；本次未触发deadline恢复，不能声称实测过恢复。当前算法的freshVM数量较大，不能作为最终交互延迟方案。需实测并优化扫描、编译/进程启动及投影开销，保持取消、复制与界面可用。

原生ParagraphEditor还缺公开有界段落投影、用户选择/授权、待处理输入移交及真实资源释放；不能由native parent偷偷切段或拼正文补齐。随后接入既有NSH工作区，按宽窄窗、焦点、IME/Esc及真实Core结果验收。正式library/reader/notes独立包、长书/EPUB、完整本地旅程和其他平台门槛继续有效。

完整兼容已按本轮冻结源码通过：[包543](../extension-package/evidence/linux-paragraph-provider-package.json)、[Wasm20](../extension-package/evidence/linux-paragraph-provider-wasm.json)、[C3 314命令/286CLI/8native](evidence/linux-paragraph-provider-compound.json)；C3内实际重跑当前默认20份A→B→A。186份C3输入与对应consumer工件另行[封存](evidence/paragraph-compat-source-archive.json)。这些证明该阶段兼容，不替代后续超时恢复修改的复验。独立故障失败的[原始日志与状态](evidence/paragraph-fault-reply-failure/record.json)保留；安装fresh授权与Store lease不等于本次OS invocation仍live，协调器必须结合Controller admission，并把已应用但回复失效的结果按checkpoint恢复。

只读投影的wire、独立应用registry及同Host挂载授权方法已有[局部证据](evidence/projection-foundation/record.json)：全根209项Rust、Clippy/格式/架构、42守卫及仓库外SDK编译通过。尚无bootstrap、只读查询协调器、投影guest或native窗口资格；不能将这些局部检查扩大为段落界面通过。该增量在旧完整兼容之后，尚未跑新投影链完整兼容。

随后新增purpose-only bootstrap wire及fresh挂载方法，4项wire/1项Host增量后全根214项Rust与Clippy通过，见[bootstrap记录](evidence/projection-foundation/bootstrap-record.json)。这仍是局部协议/授权检查，未把普通B的真实projection分流或原生视图记作通过。
