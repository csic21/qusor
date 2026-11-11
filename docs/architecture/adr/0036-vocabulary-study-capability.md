# ADR 0036：词汇学习的公开能力（协议 0.5）

日期：2026-11-11。状态：Host API 0.5 wire、`vocab.read`/`vocab.review`、`vocab_workspace` 贡献、FSRS-5 调度内核、Core 词汇学习数据（SQLite schema 4）、`StudyStore` 用例、`plugins/vocab` 普通签名包与真实 SQLite 端到端测试已通过；原生工作台样例 UI（先回想后揭晓、四档评分、真实 Core、重开恢复）也已通过 headless 与实窗证据；生产安装界面、正式产品工作区、词书导入/随包数据与移动/渠道资格仍未完成。关联 F-LEARN-01、F-EXT-01/03/04/05/06，承接 [ADR 0035](0035-notes-read-capability.md)，规格见 [词汇插件](../../product/vocab-plugin.md)。

## 问题

用户于 2026-11-11 明确要求开发背单词插件。这是 F-LEARN-01（卡片、FSRS、复习记录）的第一个真实切片，但仓库此前没有任何学习业务实现：

1. 公开能力到 0.4 为止，0.4 词表只有 `notes.read`，能力集合封闭；插件无法读取或提交任何学习数据。
2. 学习进度（词书、词卡、复习历史、下次到期）属于用户数据。按项目不变量，它必须归 Core：插件停用/卸载后仍能读取、导出和恢复，插件私有状态不能成为唯一副本。`learning-domain` 此前刻意保持零业务实现。
3. 调度算法属于领域算法，学习入口与工作流属于功能插件。需要一条不依赖阅读器、词库导入或云服务的完整纵向切片，才能验证“官方功能也用普通插件身份”的要求。

阅读器、library 与正式 notes 包仍未完成，因此首个词书内容不能依赖阅读选词；本切片使用宿主持有的合成示例词书，从“选词书 → 学习/复习 → 评分 → 下次安排”验证闭环。

## 决定

### Host API 0.5（独立词表）

- 新增 `HOST_API_V0_5`、能力 `vocab.read`（Vault）、`vocab.review`（Vault）与贡献类型 `vocab_workspace`；`SUPPORTED_HOST_APIS` 增加 0.5。
- 0.5 是独立词表：不继承 0.1–0.4，旧版本语义、wire 与测试保持不变；`Manifest::supports_api` 只把 `vocab_workspace` 映射到 0.5。
- manifest 规则：声明 vocab 能力或含 `vocab_workspace` 贡献必须以 0.5 为最低版本；0.5 manifest 必须至少声明一个 vocab 能力；`vocab_workspace` 贡献必须声明 `vocab.read`。能力数量上限保持 6。
- 读取与写入分开授权：只有 `vocab.read` 的包可以列出词书和队列，但不能提交评分；复习结果会改变用户学习数据，必须持有 `vocab.review`。

### `vocab-study-v1` 有界 wire

请求：`vocab_protocol = 1`、`api_version = 0.5`、`request_id`、operation。

| operation | 语义 | 预算 |
|---|---|---|
| `Decks {}` | 当前 vault 的词书与 total/due/new 计数 | 最多 6 本；标题 ≤96 bytes |
| `Queue { wordbook_id, limit }` | 先到期复习、后新卡；limit 1..=6 | 每张卡 term/meaning/example ≤64/96/160 bytes |
| `Review { card_id, rating }` | 提交一次评分（again/hard/good/easy） | 返回下次到期、阶段与剩余计数 |

- 请求不自报 vault：Host 从会话与精确 Vault 授权绑定范围；Host 不读 Core、不执行 guest。
- 整个响应编码 ≤4 KiB；控制字符与非 UTF-8 边界拒绝或由可信适配器收缩；未知字段、错误协议/版本、越界 id 与预算全部拒绝。
- `request_id` 同时是 Core 幂等键：同一评分重复提交返回原结果；同一 operation id 换成另一卡片或评分报冲突，不重复安排。
- `due_at_ms`、`scheduled_minutes` 为非负语义值；`stage` 在响应中不得为 `new`。

### Core 数据与事务

SQLite schema 从 3 迁移到 4，新增四张 Core 表（同一 `core.sqlite`，不复制存储）：

| 表 | 内容 | 不变量 |
|---|---|---|
| `vocab_wordbook` | vault、标题、描述、创建时间 | `(vault,title)` 唯一，重复播种幂等返回原词书 |
| `vocab_card` | 词书、位置、term/meaning/example | `(wordbook_id,term)`、`(wordbook_id,position)` 唯一 |
| `vocab_schedule` | 到期、阶段、stability/difficulty、reps/lapses、上次复习 | 每卡一行；reps≥1；lapses≤reps |
| `vocab_review` | 评分、复习时间、前后阶段、记忆状态、间隔、算法版本 | 追加写；`operation_id` 唯一；引用有效卡片 |

- 评分提交在一个事务内：校验乐观 guard（reps + last_review_at）→ 追加 review → 替换 schedule；guard 不符报冲突，不回退或部分写入。
- 新端口 `learning-ports::StudyStore` 只有快照、幂等播种、事务评分和空库恢复四类操作；SQL 留在 `storage-sqlite`，Host/插件不能直接接触。
- 导出/恢复：`StudyArchive` 携带词书、卡片、当前 schedule 与全部 review；恢复要求学习数据为空，独立于捕获知识恢复；恢复前校验引用、唯一性、时间与 16 MiB 预算。
- 卸载/停用插件不影响 Core 数据；词书与复习记录仍可由宿主通用入口导出。

### FSRS-5 调度内核

`learning-domain` 从空壳变为首个真实业务实现，仍零外部依赖：

- FSRS-5 记忆数学：19 个默认参数（FSRS-5 扩展 w19=0、w20=0.5）；初始 stability/difficulty、成功/遗忘/同日稳定性更新、难度线性阻尼与均值回归、幂函数遗忘曲线与 `I(0.9,S)=S`。
- 本项目的调度策略叠加在记忆数学之上：新卡 Again/Hard 进入 10/30 分钟学习步，Good/Easy 直接以 `clamp(round(I(0.9,S)),1,36500)` 天进入复习；Learning 卡 Good/Easy 毕业；Review 卡任何评分仍为 Review，Again 使用遗忘后 stability 且至少 1 天；同日（elapsed 四舍五入为 0）使用短期公式。
- 每次 review 记录算法标识 `qusor-fsrs-5-v1`；未来更换算法或参数必须新增标识，不重解释旧历史。
- 每日新词上限 10，由应用层按宿主传入的本地日起点统计；日界与期望保持率 0.9 是当前固定策略，后续设置能力再扩展。
- 不满足输入约束（时间倒序、非有限记忆状态、越界参数）返回显式错误，不 panic、不夹取伪造结果。

### 普通签名包与可信适配器

- `plugins/vocab` 是普通包：0.5 manifest、`vocab_workspace` 贡献、`guest.wat` 依次发送 decks/queue/review 三次有界请求；`build.py` 把公开 SDK 导出到仓库外，用 SDK crate 编译签名工具并生成 `vocab.qpk`，作者 lock 不含 Host/Core/storage/NSH/runtime 私有 crate。
- `experiments/capture-core/src/vocab_bridge.rs` 是可信适配器：只把 wire 映射到 `StudyApplication`，收缩文本到 wire 预算，映射公开错误码，不授予权限、不复制调度算法。
- 端到端资格：包验证 → Host 精确授权/启用 → 0.5 session → Wasm 执行 → guest 三次请求 → Host 逐次判权 → 真实 SQLite Core 读取与评分 → guest 输出解析；只读包在 review 处被拒绝且 Core 无写入；错误密钥与篡改包被拒绝。

## 兼容与限制

- 0.1–0.4 的 manifest、请求、签名、Surface 与 notes 路径不变；`policy-v0.5.schema.json` 独立描述 0.5 词汇。
- 0.5 不继承旧能力：同时需要阅读/想法与词汇的包当前无法在同一个会话版本内声明；应拆包或等待后续合并词表。
- 原生工作台样例 UI 已按 [想法工作区](../../design/notes-workspace.md) 相同模式接入 `VocabHost`（`VocabBridge` + 真实 SQLite Core），见[实窗记录](../../../experiments/nsh-reader/evidence/vocab-workspace-live/README.md)；它仍不是正式产品工作区或安装界面，不能据此声明产品旅程完成。
- 词书内容当前由宿主/可信应用播种（合成示例，不是授权词表）；插件随包数据、用户导入与“从阅读收词”需要新的数据文件/导入能力和对应授权，本切片不假装已支持。
- 没有 AI、云同步、提醒、网络、账号或外部词库；没有移动端与渠道资格。

## 验证

本机在 macOS arm64、Rust 1.97.1 下执行（详见 [开发记录](../../development.md) 与各 crate 测试）：

- `learning-domain`：20 项（6 模块内 + 14 集成），覆盖 Again/Hard/Good/Easy 稳定性顺序、同日短期公式、夹取、确定性与非法输入。
- `core-api`：29 项（含 `vocab` 契约 3 项：seed 边界/重复、身份与归档一致性、schedule 与复习历史投影）。
- `extension-api`：66 项（含 `vocab` wire 7 项）；`extension-host`：83 项（含 `vocab` 策略 7 项）；0.1–0.4 既有回归全部保留。
- `storage-sqlite`：20 项（含真实 SQLite 学习数据 6 项：幂等播种、队列顺序、事务评分、重开恢复、导出/空库恢复、stale guard）。
- `learning-application`：43 项既有回归（学习用例经 storage-sqlite 真实存储测试覆盖）。
- `experiments/capture-core`：普通签名包端到端 3 项；`plugins/vocab/build.py` 在仓库外生成 `vocab.qpk`（sha256 `4ce99011…dd0823`，2149 bytes，固定测试发布键不是生产密钥）。
- 工作区排除 `runtime-process` 合计 282 项通过；`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、架构守卫/依赖闭包、42 项 Python 边界测试与 188 份 Markdown 本地链接通过。
- 原生工作台：`nsh check App.tsx` 通过；`vocab-workspace` headless 10 项、`notes-workspace` 回归 6 项通过；`vocab_native` 模块测试 1 项通过；`prepare.py --evidence vocab` 在真实 NSWindow 内捕获 decks/study/revealed/reviewed/done 五张，并在同一 Core 重启后补两张恢复图（见[实窗记录](../../../experiments/nsh-reader/evidence/vocab-workspace-live/record.json)）。
- 已知环境问题：本机 `runtime-process --test controller` 在干净树（stash 后）同样失败，属 macOS 进程监督用例，与本变更无关；该包未被本变更修改，因此不宣称本机全工作区整体通过。
- 机器可读记录见 [词汇包记录](../../../experiments/capture-core/evidence/vocab-package/record.json) 与 [author-proof](../../../experiments/capture-core/evidence/vocab-package/author-proof.json)。

## 未覆盖

- 正式产品工作区装配（工作台样例之外的宿主接线）、生产安装/权限界面与发布密钥管理；实窗中的评分失败重试/回退横幅未注入真实故障。
- 词书随包数据、用户导入、从阅读/笔记收词、标签与搜索。
- 持久安装后的评分、多设备/同步、提醒、移动与 Web 运行 profile。
- FSRS 参数个性化与算法升级迁移。
