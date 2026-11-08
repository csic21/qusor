# 普通组件 A：原子续传与真实进程编辑／保存资格

日期：2026-11-08。关联C-09/C-12及F-EXT-01/03/04/05/06、F-NOTE-01。本轮已完成普通文本提供者A的真实进程链，尚未完成提供者B、A/B替换或原生组件视图集成。

## 实现与权限

`ManagedRun<A = DurableSession>`仅按authority类型选择新鲜授权入口，旧session与组件mount共用同一RunPhase、Boot/Result迁移、真实reap和清lease实现；没有第二个controller或假Session。

Host为own-context与continuation提供独立gate，安装事实逐次重核；应用层ProviderEvents持有有界输入、event cursor及checkpoint。AdvanceEvent把event自有upload的效果与checkpoint原子推进，只缓存最后一次有界hash/回执；回复丢失后fresh VM读取同一结果，不重复Begin/Append/Commit。失效sequence、错CAS、错registry、已删除Draft、错误完成状态均拒绝；完成回执可在原base revision过期后恢复。未处理输入不会被段落操作静默覆盖。

续传checkpoint采用canonical base64：最多8192原始bytes，分块读取2048；请求仍≤64KiB、响应≤4KiB。新增依赖仅固定base64 0.22.1并登记实际闭包，其他包版本未升级，见 [锁更新](evidence/continuation-lock-refresh.json)。

## 作者与真实消费者

提供者A从 [公开作者源码](author/src/bin/component_author.rs) 及 [WAT](author/src/component_text.wat) 构建，仓库外仅消费导出SDK，签名模块/descriptor并安装。每次VM公开读取Context和续传metadata，Wasm复制input的完整JSON字符串片段构造Append，parent只转换DTO，不替插件复制正文或变换文本。作者构建/签名证据见 [记录](evidence/component-author-a/record.json)。

[真实消费者](tests/component_text_consumer.rs)使用生产Store、同一个DurableHost、Controller/worker_main与Wasmi。每次运行先prepare/attempt，GuardReady后绑定真实身份，再fresh授权Boot；只在真实owned Child reap后清lease和forget。每次VM保持默认250ms、100万fuel、8calls上限。

来源是明确的合成Core PDF snapshot，用实际SQLite导入；不代表PDF解析、原生手势或有效Surface lease。保存身份来自另一个真实普通API0.2 CapturePropose签名包和profile2授权，那个capture fixture模块未执行，不能称普通读者提案链通过。编辑器没有知识提交权限；可信保存fixture经原capture身份fresh dispatch后才freeze并提交。

## 实际结果

| 验证 | 结果 |
|---|---|
| Workspace | 191项Rust及四组真实controller场景通过，架构／42守卫／格式／workspace Clippy通过 |
| 应用草稿 | 整包20项测试，包含1MiB分块、最坏checkpoint、旧CAS／sequence／scope、超时与时钟、删除Draft拒读 |
| 小流 | 10个fresh VM、35calls；中文、emoji、CRLF、NUL与空文本逐字节一致 |
| 回复丢失 | Begin/Append/Commit成功后各真实terminate一次worker、不回reply；fresh VM续传成功；这是故障fixture，不冒称实际OS丢包 |
| 迟到结果 | 真实替换native事件后拒绝旧run结果；原capture撤权时在freeze前拒绝保存 |
| 小流持久化 | 两条正式知识记录，SQLite关闭重开、完整导出／空库恢复与来源／note／provenance一致 |
| 1MiB最坏转义 | 1,048,576个NUL，2050个fresh VM、8198calls全部完成；正式保存、SQLite重开／空库恢复一致 |
| 回收与冻结 | 所有owned worker实际reap；large二进制与源码前后hash一致，实际安装摘要和稳定签名fixture一致 |

小流机器证据：[linux-component-text-small.json](evidence/linux-component-text-small.json)。大流机器证据：[linux-component-text-large.json](evidence/linux-component-text-large.json)，fixture核对见 [安装摘要](evidence/component-text-large-fixtures.json)。大流耗时369.467秒，最大单broker调用23.488ms；这是Linux/WSL debug单次功能资格，不是性能保证或可交付编辑延迟。

源码／schemas／作者等144份已逐项hash封存，见 [归档核对](evidence/event-continuation-source-archive.json)。本机target内保留对应consumer二进制。旧0.3接线与原生UI成绩继续按各自已封存工件记录，不能替代本轮。

## 尚未完成

最新源码完整兼容回归已通过：[包543场景](../extension-package/evidence/linux-event-continuation-package.json)、[Wasm20场景](../extension-package/evidence/linux-event-continuation-wasm.json)、[C3完整捕获](evidence/linux-event-continuation-compound.json)及[旧生产controller九场景](evidence/linux-event-continuation-supervised.json)。旧C9原始证据已恢复保存，新结果使用独立名称。提供者B的逐段编辑／移动、A→B→A、原生用户选择与草稿移交、真实native释放ack尚未完成。已有NSH想法面板尚未接本链；中文IME/Esc、长书/EPUB、正式library/reader/notes包和平台门槛仍保留。

后续性能改进不得以降低1MiB容量、扩大旧calls/fuel/deadline或把业务算法移到Host替代。先保持真实插件与权限／恢复证据，再在真实界面验证交互与延迟。
