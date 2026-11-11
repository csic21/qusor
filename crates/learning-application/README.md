# 知识应用用例

`KnowledgeApplication<S: KnowledgeStore>` 实现真实文本/PDF/复合捕获、PDF导入、查询、精确定位、完整导出与空库恢复。写用例从一致快照生成整体计划，保留独立意图、幂等 hash 和全局提交顺序；不依赖 SQL、文件系统、Host、插件运行时或 NSH。

现由捕获实验的普通插件宿主实际消费，SQL/CAS 不再复制在实验内。旧 schema/归档兼容与完整性校验共享 [core-api](../core-api/README.md)。稳定错误分类供后续 UI 区分未找到、预算、冲突、损坏和存储故障。

当前是有界完整快照路径，尚未完成大资料库的增量查询/性能、生产平台适配或正式功能插件旅程。见 [ADR 0019](../../docs/architecture/adr/0019-atomic-knowledge-persistence.md)。

`study::StudyApplication<S: StudyStore>` 提供词汇学习用例：幂等播种、词书计数、到期优先队列、四档评分与 FSRS 安排、导出/空库恢复。调度数学来自零依赖的 `learning-domain`；应用层固定每日新词预算 10、日起点由可信调用方传入。见 [ADR 0036](../../docs/architecture/adr/0036-vocabulary-study-capability.md)。

## 会话草稿基础

`draft_text`提供独立于renderer的会话文本、1MiB上限、UTF-8分块读取、暂存上传/原子替换、修订CAS、mount owner与超时、声明内存预留，以及Saving/Saved和未知结果锁定。`annotation_source::FrozenPdfSource`核对Core中的不可变PDF引用，保留来源和provenance；其只读receipt核对完整payload，不把查询当成新提交。

这些应用对象不授予插件权限。创建前的选区/安装核对和提交前的fresh capture授权仍由同一Host管理gate承担；来源冻结不保存Session/lease/permit。当前尚未接公开0.3协议、普通提供者或原生界面，不能据此宣称组件替换交付。具体契约见 [实施规格](../../docs/product/component-provider-contract.md)。

`annotation_draft::AnnotationDrafts`将DraftId、冻结来源、原capture generation及保存token不可变绑定，统一执行来源数/字节预算和文本预算。未知提交保持Saving，reconcile只读核对原payload，未发现回执不自动解锁；两份不同文档的输入/保存不会串用。组合层仍不代替Host授权。

`provider_events`封闭持有本次编辑事件、输入与event-owned upload，AdvanceEvent在同一应用gate原子更新文本effect和checkpoint／最后回执。完成恢复、取消、30秒活动超时及跨registry检查独立于Host，调用者仍须串行fresh授权和定时tick。真实普通提供者A的1MiB／中断恢复／SQLite保存验证见 [结果](../../experiments/capture-core/EVENT_CONTINUATION_RESULTS.md)，尚未接原生视图或完成B替换。

只读`provider_projections`使用独立opaque mount/query token、CAS/checkpoint与单调窗口，不创建编辑owner、不修改草稿或ProviderEvents。取消/过期不影响编辑输入、上传或时钟；完整row span也不自动构成已完整加载、可编辑的native row handle。Host/guest/renderer装配仍待逐项验证，见[投影决定](../../docs/architecture/adr/0030-readonly-paragraph-projection.md)。
