# QUSOR 本地优先与 Supabase 同步协议

日期：2026-11-07。状态：设计提案；未创建云项目、执行迁移、实现RPC或运行同步测试。所有指标为待验收目标。配合[Core Schema](./03-core-data-architecture.md)实施。

## 1 核心决定

SQLite是当前设备事实源；离线导入、阅读、标注、笔记、画布、复习不等待云端。Supabase负责认证、授权后的副本、设备协调和可选文件备份。它没有自动提供本方案的SQLite同步协议，协议由QUSOR实现。

采用本地原子提交 + durable outbox + 实体CAS版本 + 每库有序提交日志 + 一致快照。第一版不做多人协同，不引入通用CRDT，冲突保留三方内容。

排序流为(owner_id,vault_id)，不是全账户混合流。commit_rev、sync_epoch、日志、cursor、快照与保留边界按库独立。账户设置另设独立流；一次RPC只处理一个vault。这样多库并行且不会按vault过滤账户日志时跳过事件。

ID使用随机UUID。local_version、server_entity_version、stream commit_rev三个概念分开；手机时间只作事实时间/展示，不裁决覆盖顺序。Realtime仅提示pull，断线重连、前台恢复和周期pull负责补齐。

云同步默认关闭；开通账户不上传书籍、摘录、全文、向量、提示词或附件。同步和云模型调用分别授权。

## 2 当前官方变化

已检查[Changelog](https://supabase.com/changelog.md)和当前官方文档。

| 变化 | 设计影响 |
|---|---|
| [Data API表不再自动暴露](https://supabase.com/changelog/45329-breaking-change-tables-not-exposed-to-data-and-graphql-api-automatically)，2026-04-28公告，10-30覆盖现有项目的新表 | 显式schema/GRANT/EXECUTE/RLS；不假设建表后REST自动可用 |
| [Realtime schema锁定](https://supabase.com/changelog/realtime-schema-locked-down-against-modification)，2026-07-14 | 自建日志放应用私有schema，不改内部对象 |
| [旧TLS CBC停用公告](https://supabase.com/changelog/cbc-tls-ciphers-deprecated)，2026-10-12，最早11-10生效 | 五端验证TLS1.3或受支持TLS1.2 AEAD；不关闭证书校验 |
| [CLI v3](https://supabase.com/changelog/supabase-cli-v3)，2026-11-04 | 未来固定CLI/目标，读取--help；不套旧远端默认行为 |

## 3 本地表与事务

使用同一个SQLite事务保存业务、审计、outbox和索引作业。规范名称与Core一致。

| 表 | 关键字段/约束 | 用途 |
|---|---|---|
| sync_state | PK(account_subject,vault_id,stream)，sync_epoch、device_id/epoch、applied_rev、snapshot_id、protocol_version | 匿名库未绑定时以本地profile标识，绑定后明确迁移；管理cursor与世代 |
| sync_shadow | PK(vault_id,entity_type,entity_id)，server_version、server_commit_rev、base_payload/hash、tombstone | 服务端基线，供三方比较 |
| sync_outbox | operation_id PK、vault_id、command_ids、status、command_payload、sealed_payload/hash、client_seq?、attempts/retry_after、created_local_seq | draft→sealed→sending→accepted/conflict/rejected |
| sync_outbox_dependencies | PK(operation_id,depends_on_id)，两端FK | 命令依赖，无环；父未同步不当已存在 |
| sync_inbox | PK(vault_id,sync_epoch,commit_rev)，operation_id、applied_at | 本地去重/诊断，压缩不得影响连续cursor |
| conflicts | id PK、vault_id、operation_id、base/local/remote、remote_version、status | 保留冲突分支，解决产生新操作 |
| operations | Core定义的审计、forward/inverse、actor、affected_versions | 用户意图历史，不能用云回执替代 |

索引：outbox(vault_id,status,created_local_seq)、dependencies(depends_on_id)、conflicts(vault_id,status)。credentials独立于业务数据库，account/vault不能静默重绑。

```text
命令 → scope/引用/版本/审批检查
     → SQLite事务：业务 + local_version + operations + outbox草稿 + index_job
     → commit后通知UI“已保存”
     → 后台冻结网络请求、分配client_seq、发送
```

单设备每库一次发送一个冻结操作。client_seq在冻结时持久分配，尚未发送的连贯编辑可合并网络操作，但保留operations审计和关联。冻结后ID/hash/序号/payload不变，超时只能原样重试。

用户在等待ACK期间继续编辑形成pending overlay。ACK更新shadow、移除对应overlay，再重放后续命令；禁止用旧正文覆盖用户新输入。未发送命令在前序确认后绑定准确base_version；冻结请求的base不能自动改写。

## 4 云端 Schema

私有schema qusor_sync_private，api schema只暴露有限RPC。云端typed-envelope同步副本与本地规范化Schema不同；body受实体白名单、schema_version、大小和引用校验，不是任意JSON数据库。

| 云表 | 主键与字段 | 必要索引/约束 |
|---|---|---|
| sync_vaults | PK(owner_id,vault_id)，owner FK auth.users、status、created_at | vault归属入口；owner来自JWT |
| sync_heads | PK(owner_id,vault_id)，sync_epoch UUID、head_rev>=0、min_retained_rev>=1 | FK sync_vaults；写事务首先锁此行 |
| sync_devices | PK(owner_id,device_id,device_epoch)，status、last_seen、retired_at | 设备全账户注册；退休使其所有库进度失效 |
| sync_device_vaults | PK(owner_id,vault_id,device_id,device_epoch)，last_terminal_client_seq、acked_pull_rev、status | FK vault与device；索引(owner,vault,status,acked_pull_rev) |
| sync_entities | PK(owner,vault,entity_type,entity_id)，entity_version>0、last_commit_rev、body JSONB、is_tombstone、schema_version | 索引(owner,vault,last_commit_rev)，需要时加类型索引 |
| sync_references | PK(owner,vault,from_type,from_id,kind,to_type,to_id) | 两端复合FK含同owner/vault；反向索引(owner,vault,to_type,to_id) |
| sync_changes | PK(owner,vault,sync_epoch,commit_rev)，operation_id、changes JSONB、schema_version、committed_at | 每成功原子组一行，不能分页拆组 |
| sync_receipts | PK(owner,operation_id)，vault_id、device/epoch、client_seq、canonical_hash、outcome、commit_rev?、result | UNIQUE(owner,vault,device,device_epoch,client_seq)；请求hash含vault/epochs |
| sync_snapshots | PK(owner,vault,snapshot_id)，sync_epoch、base_rev、manifest/hash、expires_at、state | 索引(owner,vault,expires_at) |
| sync_snapshot_items | PK(owner,vault,snapshot_id,item_seq)，entity/version/payload/hash | FK snapshot；只读不可变分页 |
| sync_file_manifests | PK(owner,vault,file_id)，object_key、hash、size、encryption_version、status | 用户授权后才存在；private bucket路径含owner/vault |

所有用户数据表启用RLS，引用和归属均需server校验。日志、墓碑、文件对象不得跨vault。跨库复制/移动是显式可恢复作业或导出导入，不假设跨库事务或隐含引用。

## 5 Push v1

请求形状如下，UUID及正文为示意，整数版本/序号通过JSON十进制字符串传输，避免JS精度问题：

```json
{
  "protocol_version": 1,
  "vault_id": "<vault-uuid>",
  "sync_epoch": "<vault-epoch>",
  "device_id": "<device-uuid>",
  "device_epoch": "<installation-epoch>",
  "client_seq": "42",
  "operation_id": "<operation-uuid>",
  "changes": [{
    "entity_type": "note",
    "entity_id": "<note-uuid>",
    "action": "update",
    "base_version": "7",
    "schema_version": 1,
    "body": {"title": "因果与相关", "body_md": "已获准同步的笔记"}
  }]
}
```

单个sync_push_v1 RPC是一个原子变更组：

1. 从JWT auth.uid()得owner，验证vault归属、device/epoch、sync_epoch、协议、类型与大小。客户端不提交可覆盖owner字段。
2. 首先FOR UPDATE锁(owner,vault) head；所有后台修复/删除/GC写路径遵守同一协议。锁内不得HTTP/LLM/Storage操作；实体锁按固定顺序。
3. 查询receipt：同ID同规范化hash返回原终态，同ID不同payload/vault拒绝；另一ID重复device/vault/epoch/seq也拒绝。
4. 新操作必须client_seq=last_terminal+1。缺序返回sequence_gap恢复前一个请求。create要求base=0且ID从未出现；update/delete/restore匹配当前entity_version。
5. 任一冲突则整个业务组不写；正常返回并持久化conflict receipt，推进设备终态seq，不增加head。不能抛异常导致receipt一同回滚。
6. 全部通过：实体版本递增，head+1，写完整组后像/删除事件、references、receipt和设备seq，同事务提交。
7. 返回outcome/op/rev/entity_versions/canonical_changes。响应丢失原样重试；数据库错误整体回滚、seq不消耗。

初始限额128实体/256KiB规范payload。大导入拆明确依赖组；需要超过限额的原子语义时设计分块准备/发布协议，不能静默拆分。引用先对整个目标集合校验，不依赖数组顺序。

### 5.1 游标排序陷阱

不能以bigserial、UUIDv7或updated_at直接作提交游标：A拿41后挂起，B拿42先提交，客户端推进42，A后来提交41将永久漏拉。[PostgreSQL sequence](https://www.postgresql.org/docs/current/functions-sequence.html)不随事务回滚而撤销分配；[行锁](https://www.postgresql.org/docs/current/explicit-locking.html)可持续到事务结束。

本方案正确性来自同库head行串行事务：B等A提交或回滚后再分配下一rev。不同库/用户并行。不能旁路直写表破坏该不变量。

### 5.2 ACK 不能推进 pull cursor

本机cursor100，push得到105，101–104可能来自另一设备。ACK只能确认本操作和相应shadow，必须正常pull101–105。自身echo用op去重。ACK与pull交错时shadow只接受更高实体版本，较老后像不覆盖，但每个日志组仍计入已处理连续前缀。

## 6 Pull 快照 保留

pull_v1(epoch,after_rev,page_limit)按(owner,vault,epoch,rev) keyset返回完整变更组、next_cursor、has_more、head。客户端只能推进到已持久成功最后一组，不用head直接跳过。

一页内shadow、pending重放、conflicts、去重和cursor同SQLite事务提交；未知schema、引用不满足、解析错误整页回滚，禁止跳事件。随后ACK落盘cursor，可重复发送。空页不掩盖revision缺口。

服务端日志页/head/保留边界必须同快照读取；初期可短暂锁head，后续优化为经验证MVCC方案。GC不可夹在边界检查和读取间导致缺口。

### 一致快照

当after_rev < min_retained_rev-1、epoch变更、设备退休回来或本地损坏，返回snapshot_required。

1. 锁head，在同事务物化所有获准同步实体、必要墓碑和引用，固定base_rev=S。
2. 写不可变snapshot items及manifest/hash，提交后分页下载；后续写入为rev>S。
3. snapshot TTL内pin保护S之后日志；不跨HTTP保留数据库长事务，也不把多次REST读活动表当一致快照。
4. 客户端保全未确认outbox和conflict，在临时DB验证owner/vault/epoch/schema/数量/hash/FK后原子切换shadow、cursor=S。
5. 查询旧请求receipt、重评pending，再拉>S。不能用snapshot覆盖未同步笔记，不能盲重放已接受op。

物化锁时长和存储有预算；超限转服务器snapshot任务/版本化历史，不牺牲一致性。过期snapshot重新开始但保留本地pending。

### 保留与清除

- 初始日志保留90天为配置起点，考虑活跃设备min ACK、snapshot pin；清理和边界推进同head协议。
- 超期离线走快照；退休设备不能复活旧epoch，新安装登记新epoch。
- 删除先tombstone。日志压缩后保留最小ID/type/deleted version登记，避免旧操作复活ID；restore显式CAS。
- receipt正文可压缩，最小op/hash/vault/device/epoch/seq/outcome/rev账本保留到云库/账户清除；短期去重窗口不够。
- 永久云库清除覆盖历史、snapshot、Storage并轮换库sync_epoch；旧epoch不能重推。关闭同步不等于删除云内容。
- 旧备份/设备克隆需握手检查seq/receipt，序号不一致保留分支、注册新epoch、快照后重新评估；不能沿用复制的设备身份写新操作。

## 7 冲突策略

| 实体 | 第一版策略 |
|---|---|
| Note/RichText | base/local/remote三方；仅确定不相交的结构变更可自动合并，其余保留副本 |
| 标注 | 不同ID并存；同正文冲突留分支；anchor与文档版本整体检查 |
| 阅读位置 | 每设备独立，用户选择从另一设备继续；不取最大页码当当前位置 |
| 标签/成员 | 独立成员ID/CAS，不覆盖整个数组丢掉新成员 |
| 概念/观点/证据/关系 | 各实体CAS，同owner/vault FK；provenance完整 |
| 画布 | 节点/边独立同步；同节点位置先提交成功，另一候选可重放；几何不覆盖正文 |
| Flashcard | 题面按content_version，稳定card ID独立 |
| Review/FSRS | 不可变review事件去重；记录card_content_version、base_schedule_rev、算法/参数；schedule为可重建投影 |
| 设置 | 按项，设备偏好本地，敏感值不进普通settings |

同base_schedule_rev的双端复习都保存并标并发。M2确定用户确认顺序/重复事件策略再重算；不根据不可靠手机时间直接假定真实因果，不合并两个stability值。

知识图谱可有环；只有outbox依赖、分组父链和特定学习路径先修要DAG。删除来源保留墓碑/来源缺失状态，不级联抹掉论据。新关系不能绑定已删除端点。

## 8 AI 审核 撤销 隐私

Agent proposal经审核才通过正常Core命令成为实体/outbox。批准绑定payload和base versions；AI不得直写SQLite或Supabase。撤销是新补偿op，检查当前版本，记录undo_of；redo产生新ID，重发旧ID只会返回旧receipt。

审计默认本地；不要把before/after全量加入云日志导致隐私开关失效。SyncProjectionPolicy统一生成白名单副本：

| 数据 | 默认 |
|---|---|
| 原文件/附件 | 本地，单独授权后备份 |
| 标题/作者/hash/进度/图谱 | 新库不同步；启用范围后上传，hash/书名也可泄露特征 |
| quote/anchor上下文/卡片原文 | 独立内容许可，不能因藏在JSON就当无敏感元数据 |
| 笔记/AI内容 | 独立选择；可能包含复制原文 |
| 全文/FTS/embedding | 本地可重建，不默认上传 |
| API密钥/refresh token | 系统安全存储，禁止业务同步/普通导出 |

没有摘录许可时只同步获准的结构定位/hash；引用未授权父实体时先取得最小来源元数据授权，否则依赖对象留本地。另一设备缺文件显示source_unavailable，不能假装可回原文。

文件上传与数据库无法同ACID事务，采用requested→uploading→verified→linked，内容版本不可覆盖，宽限期回收孤儿，下载核验hash。private bucket路径限定owner/vault；不把有RLS/TLS表述为E2EE。云端不可读要求另行设计密钥恢复/轮换与加密检索。

## 9 认证与数据库权限

客户端只用publishable key+用户JWT，不带service_role、secret key、数据库密码。owner取auth.uid()，不信可编辑user_metadata。[RLS文档](https://supabase.com/docs/guides/database/postgres/row-level-security)。

读优先SECURITY INVOKER+RLS。写需禁止绕过日志直接DML，因此采用严格限定的privileged RPC边界：

- 私有表、专用NOLOGIN且无BYPASSRLS、非表owner执行角色。
- 角色只有所需表权限，owner/vault RLS和函数显式过滤同时检查。
- 内部definer helper放私有schema、空search_path、全限定对象名，不用用户动态SQL。
- api.sync_push_v1采用SECURITY INVOKER薄包装，调用私有sync_push_impl（SECURITY DEFINER，由专用执行角色拥有）。authenticated只获api/private schema USAGE及这两个指定函数的EXECUTE，不获私有表DML；私有schema不加入Data API暴露列表。
- 撤销PUBLIC/anon默认EXECUTE，按完整签名授权，禁止whole-schema函数授权。私有helper自身仍重复完整owner/vault/device校验；即便经其他合法通道直接调用helper也不能扩大能力。部署测试验证包装函数、helper及执行角色整条权限链。
- 绝不使用超级用户或表owner部署后宣称RLS仍保护全部调用。

这是为单一写入协议设计的权限边界，必须单独审查。[Functions文档](https://supabase.com/docs/guides/database/functions)。暴露视图PG15+使用security_invoker；直接UPDATE需SELECT、USING、WITH CHECK配套。GRANT与RLS是两层条件。

[Storage权限](https://supabase.com/docs/guides/storage/security/access-control)需要owner路径约束；upsert涉及INSERT/SELECT/UPDATE，优先不可变对象避免覆盖。

系统浏览器OAuth+PKCE，精确回调allowlist，绑定一次性state/verifier并校验；[PKCE](https://supabase.com/docs/guides/auth/sessions/pkce-flow)和[移动deep link](https://supabase.com/docs/guides/auth/native-mobile-deep-linking)是官方依据。NSH的系统浏览器/deep link/安全存储仍须M-1探针，不能假设TSX localStorage。

Windows Credential Manager/DPAPI、Apple Keychain、Android Keystore加密持久化、Linux Secret Service由platform-services提供。不可用时内存会话重新登录，不明文降级。refresh串行化；退出停止同步/清凭据但保留未同步资料。

切账户先停旧任务和回调，关闭旧profile，切session generation，再开独立vault；迟到响应绑定account/vault/generation拒绝。匿名绑定需明确归属，不能把旧owner outbox换名上传。

## 10 验收和故障注入

| ID | 阶段 | 场景 | 通过条件 |
|---|---|---|---|
| SY01 | M-1 | SQLite写入点强杀、低存储、耐久配置 | 全提交/全回滚；“已保存”仅commit后；掉电承诺另测 |
| SY02 | M-1 | 五端安全存储、PKCE、回调、TLS | 真机恢复登录、无凭据日志 |
| SY03 | M0 | 原子命令/audit/outbox/隐私投影 | 云关闭仍完整本地闭环，无自动上传 |
| SY04 | M1 | 同op重发100次、提交后丢ACK | 一次版本推进/一次事件；不同payload复用ID拒绝 |
| SY05 | M1 | A锁head暂停、B并发、A提交/回滚 | 只看连续已提交rev，无sequence跳漏 |
| SY06 | M1 | 双端离线改/删、节点拖动 | 三方可见，不静默丢内容 |
| SY07 | M1 | 迟到ACK、push领先cursor | 新输入保留，不漏另一设备事件 |
| SY08 | M1 | 父子随机顺序、跨owner/vault引用 | 原子整组，同库强约束 |
| SY09 | M1 | 丢/重复pull页、未知schema、强杀 | cursor和数据同事务；Realtime全部丢失仍收敛 |
| SY10 | M1 | 快照并发写、TTL过期 | S基线与后日志无缝，pending保留 |
| SY11 | M1 | 180天离线、退休、克隆/旧备份 | 新epoch重建，删除不复活、op不重复 |
| SY12 | M1 | A/B账号、匿名/过期JWT、直接REST DML | 跨账号/库拒绝，不绕日志 |
| SY13 | M1 | 推拉途中切账户 | 旧响应不入新库，旧outbox不泄漏 |
| SY14 | M1 | 未审核AI、撤销前他端已编辑 | 未审不应用，不覆盖后续修改 |
| SY15 | M1 | 原文隐藏于quote/anchor/audit/card | 统一出站白名单不泄漏 |
| SY16 | M2 | 双端review、改题面、算法升级 | 事件不丢重，可知作答题面，可重算 |
| SY17 | M1 | 100k元数据、10k补齐、50k快照 | 固定设备/网络；UI可用；普通push p95<1s、正常网络收敛<5s为起始目标 |

跨账户泄漏、数据不可恢复、cursor漏事件、删除复活、未审核AI应用、默认上传原文、重复review和快照覆盖pending均为发布阻断。性能优化不得降低上述正确性。

官方额外依据：[Postgres Changes](https://supabase.com/docs/guides/realtime/postgres-changes)、[Storage buckets](https://supabase.com/docs/guides/storage/buckets/fundamentals)。Realtime通知不承担本方案持久投递保证。
