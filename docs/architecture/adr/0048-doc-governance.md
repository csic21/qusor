# ADR 0048：文档治理与单一状态源

- 日期：2026-11-14
- 状态：Accepted
- 关联：P2-8、C-01、C-08、C-15、F-PLAT-01

## 背景

在 [2026-11-14 架构评审](../reviews/2026-11-14-architecture-review.md) 中指出，当前文档维护成本过高且已出现多处漂移：

1. **进度流水多处重复**：`AGENTS.md`、`docs/architecture/README.md`、`docs/product/scope.md`、`docs/product/next-phase.md` 与 `docs/development.md` 均夹杂大量按日期追加的进度流水，导致 `AGENTS.md` 膨胀至约 10.9 KB，每次协作消耗过多上下文且各处描述不同步。
2. **ADR 职责混杂**：7 天内产生 47 篇 ADR，其中混入实现进度、测试计数与多次追加日志，削弱了 ADR 作为“决策快照”的可读性。
3. **事实漂移**：`docs/architecture/README.md` 与 `docs/architecture/workspace-layout.md` 中对 `extension-api` 版本存在 `0.1` 与 `0.7` 的自相矛盾，且架构图中的 crate 依赖方向与 `cargo metadata` 实际依赖图不符。

## 选项

1. **方案 A：人工约定分拆文档，不加自动化校验**
   - 优点：初期脚本改造成本低。
   - 代价：随后续增量推进，日期流水和版本数字仍会重新回流到 `AGENTS.md` 与 ADR 中，依赖图与索引漂移无法在本地检查阶段拦截。
2. **方案 B：单一权威状态表 + 精简规则入口 + 脚本校验（采用）**
   - 优点：明确每类文档的权威范围，将进度与多层级验证状态集中在单一 `docs/status.md`；ADR 严格收敛为决策记录；`scripts/check_docs.py` 自动校验 ADR 字段/状态、索引一致性、`docs/status.md` 证据链接及 `AGENTS.md` 体积上限，阻断后续漂移。
   - 代价：需要一次性迁移既有文档中的进度段落，并约定新增 ADR（自 `0048` 起）遵循统一模板与状态枚举。

## 决定

确立以下文档治理规则，并由 [文档检查脚本](../../../scripts/check_docs.py) 在本地与 CI 检查中强制执行：

1. **单一权威范围（Single Source of Truth）**：
   - [AGENTS.md](../../../AGENTS.md)：只保留项目不变量、阅读入口、工程与协作约定及验证层级原则，体积上限为 `4350` bytes（原体积的 40% 以内）；不写入进度流水。
   - [docs/status.md](../../status.md)：全仓库按 Feature 与模块维度的唯一状态表，分列 `源码`、`构建`、`本机运行`、`模拟器`、`真机` 与 `证据与关联 ADR`，保留“源码存在 ≠ 构建通过 ≠ 本机运行 ≠ 模拟器 ≠ 真机”的分层事实。
   - [docs/architecture/README.md](../README.md)：维护实际工作区 crate 依赖图（与 `cargo metadata` 对齐）、层间边界及由脚本校验的 ADR 索引表。
   - [docs/architecture/workspace-layout.md](../workspace-layout.md)：维护目录归属、独立 Workspace 边界与功能插件划分。
   - [docs/product/scope.md](../../product/scope.md)：维护已明确产品要求（Feature ID）、用户确认决策（C 编号）、待用户拍板事项与非目标。
   - [docs/product/next-phase.md](../../product/next-phase.md)：维护当前实施路线、阶段准入与退出门槛，具体状态指向 `docs/status.md`。
   - [docs/development.md](../../development.md)：维护检查命令、实验与打包复现入口，以及各检查“证明什么/不证明什么”。
2. **ADR 规范（自 ADR 0048 起强制执行）**：
   - 使用 [ADR 模板](../../templates/adr.md)，仅记录单次架构决策，必备头部字段 `日期`、`状态`（仅限 `Proposed`、`Accepted`、`Superseded`）与 `关联`，必备章节 `## 背景`、`## 选项`、`## 决定`（或 `## 决策`）、`## 后果`。
   - 待用户拍板的架构选择（如第一方交付形态、iOS/Web 动态第三方插件、插件 UI 技术路线）在拍板前必须标记为 `Proposed`。
   - **禁止进度日期流水**：ADR 正文中不得追加按日期排列的实现进展或测试数量流水；历史 ADR（`0001`–`0047`）保留历史快照不强行重写，仅要求其标题/链接纳入索引表校验。
3. **脚本校验**：
   - 扩展 `scripts/check_docs.py` 校验本地 Markdown 链接、ADR 索引表与 `docs/architecture/adr/` 文件双向一致、ADR 0048+ 必备字段/章节与状态枚举、ADR 正文禁止进度流水、`docs/status.md` 状态值合法性与证据链接存在性，以及 `AGENTS.md` 字节上限。

## 后果

- **正面收益**：Agent 每次加载 `AGENTS.md` 的固定上下文成本下降 60% 以上；实现与平台验证状态集中可查，消除跨文档数字冲突与未实证的“已完成”表述。
- **代价与约束**：后续作者新增 ADR 或完成切片时，必须同步更新 `docs/architecture/README.md` 索引表与 `docs/status.md`，否则 `python3 scripts/check_docs.py` 失败。
- **兼容性**：`0001`–`0047` 历史 ADR 保留原貌并在索引表中映射为对应规范状态（`Accepted` / `Superseded` / `Proposed`），避免大规模修改历史文本引入噪声。
