> 这是 2026-11-07 的技术研究基线。当前工程状态见 [项目入口](../../README.md)，产品确认状态见 [功能范围](../product/scope.md)。研究中的建议不等于已经实现或经过用户确认。

同日新增 [UI / UX 设计参考研究](07-ui-ux-design-study.md)：阅读、知识组织、插件交互和原生设计规范的第一轮公开资料研究；设计假设与技术验收分开记录。

# QUSOR 技术研究与实施规划

研究日期：2026-11-07。范围：NSH最新源码、五平台支持、GPUI差异、阅读与画布技术路线、Rust Core、Schema与定位协议、Agent与检索、Supabase同步、M-1/M0/M1任务和风险验收。本轮只编写研究文档，没有创建业务工程、数据库迁移或云资源。

**结论：继续采用NSH + 独立Rust Core，先执行M-1，再决定M0开工。** 当前NSH已有真实iOS/Android及三桌面后端、原生WebView、RichEditor和Canvas；其native TSX仍不是完整浏览器DOM。移动端实现存在，但QUSOR场景的五平台运行成熟度尚未验收。

## 1 文档入口

| 文档 | 覆盖任务 |
|---|---|
| [01 NSH 与 GPUI 源码审查](./01-nsh-gpui-audit.md) | NSH架构、平台/移动实现、CI证据、GPUI差异、版本兼容与许可 |
| [02 阅读 编辑 画布技术路线](./02-reading-canvas-routes.md) | PDF、EPUB、Markdown、富文本、无限画布、知识图谱、备选与探针 |
| [03 Rust Core 数据模型与定位协议](./03-core-data-architecture.md) | crate职责/依赖、线程/API、完整实体数据字典、索引/FK、QLA v1、历史/导出 |
| [04 Agent Runtime 与知识检索](./04-agent-retrieval.md) | Provider、工具调用、权限/预算、候选审核/撤销、中文FTS/向量、引用评测 |
| [05 本地优先 Supabase 同步](./05-local-first-sync.md) | 原子outbox、每库CAS/head、幂等、pull/snapshot、冲突、RLS/认证、隐私 |
| [06 阶段任务 风险与验收](./06-milestones-risks.md) | 14项M-1、14项M0、13项M1任务，依赖/责任/投入，16项风险和Go/No-Go |
| [07 UI / UX 设计参考研究](./07-ui-ux-design-study.md) | 官方案例、视觉观察、交互机制、QUSOR设计假设与待验证任务 |

协议冲突时：来源定位与实体定义以03为准；云同步wire/服务端表以05为准；运行支持承诺以06的设备证据为准。实施前按M-1结果统一修改并冻结v1，不能各模块独立解释同一字段。

## 2 源码事实与待验证项

NSH固定到[913fd76e1f8434ec6cdf248f4476c1ebe08af19f](https://github.com/longbridge/nsh/commit/913fd76e1f8434ec6cdf248f4476c1ebe08af19f)，Workspace版本0.4.0；upstream GPUI固定到Zed[b5cd310b3b54dfc659fcc06453e6358843dfe216](https://github.com/zed-industries/zed/commit/b5cd310b3b54dfc659fcc06453e6358843dfe216)。NSH可选GPUI backend当前依赖GPUI Fast，与upstream不是同一版本对象。

关键发现：

- NSH采用QuickJS/Rust响应式图、shadow tree、Taffy与平台MountBatch；平台渲染实现有差异，不能概括成每节点都是系统标准控件。
- Android/iOS均有实际输入法组合态、选择、生命周期和无障碍实现；构建配置不等于实机质量证明。
- NSH内嵌WebView可以作为浏览器阅读库的承载候选；资源加载、Worker、frame权限、双向通信与选择仍须逐端测。
- RichEditor是Rust移植内核与native view，不是npm Tiptap插件直接兼容。
- 最新GPUI已有iOS实现，不能沿用“完全不支持移动端”的旧判断；固定版本的iOS文件选择/保存仍有未实现处，默认平台分派未见Android。
- NSH兼容性文档尚未把0.4.0纳入已开始的冻结契约，是维护风险之一。

本轮未执行五平台构建/安装/实机测试，也没有取得固定SHA工作流全部通过的证据。性能数字均为拟定门槛，不能解释为已测成绩。

## 3 当前架构决定

| 范围 | 决定 | 验证或降级 |
|---|---|---|
| UI | NSH各平台默认native后端，Rust+TSX | 固定SHA；nsh-adapter唯一依赖入口 |
| PDF | 优先验证PDFium Rust adapter | WebView+PDF.js作备选；iOS链接、位图成本、原生选择先测 |
| EPUB | 优先验证受控WebView+EPUB.js | Readium TS及移动toolkit对照；不在M0重写完整CSS排版 |
| 编辑 | M0 Markdown可靠基线，原生RichEditor优先spike | 受限QUSOR schema；失败时延后高级富文本 |
| 画布 | Rust几何/场景 + NSH Canvas | 裁剪/LOD/后台布局；实测有必要再做专用Surface |
| 图谱 | SQLite实体/关系/证据，独立于画布布局 | 可编辑列表/检查器，来源可追踪 |
| 存储 | 本地SQLite、原始文件、可重建索引 | 事务包含操作审计/outbox；恢复与导出为发布门 |
| 来源 | 不可变版本 + 格式selector + quote/context + source_map | Unicode单位显式，歧义不误配，文件缺失可见 |
| Agent | 一个Runtime、多角色配置、有限工具 | 候选→人审→CAS原子应用→补偿撤销；原文/推断/用户观点分开 |
| 检索 | M0中文FTS基线，M1向量试点 | 索引/embedding可重建，不预先绑定大型向量服务 |
| 同步 | Supabase Auth + 自建RPC/日志/快照 | 每(owner,vault)流；Realtime只提示，默认不传原件/摘录 |

Core保持独立Cargo Workspace。除了原设想模块，增加learning-application、learning-ports、core-api、retrieval-engine与platform-services，明确事务编排、接口边界及平台能力责任；不是要求M0立即铺设所有空crate。

## 4 长期数据不变量

1. 原始文档与来源版本不可被AI改写；检索与模型结果不是原文。
2. 每条事实性知识结论和关系可绑定证据；无证据假设显式标记，不能伪装已证实。
3. 所有正式写入经过统一Core命令，AI批准绑定payload hash及前置版本。
4. 本地业务、审计、outbox同事务；撤销/redo是新补偿操作，保护后续编辑。
5. 本地local_version、云entity_version、流commit_rev分开；设备时间不裁决冲突。
6. push ACK不得推进pull cursor；快照不得覆盖未同步内容；日志/墓碑/receipt共同防重放。
7. 所有云引用同时限定owner与vault；账户切换隔离在途回调与存储。
8. “原书不上传”不等于quote、笔记、embedding或AI请求没有出站，必须分项控制。
9. 数据可完整导出与迁移，凭据除外；索引是派生物，丢索引不丢知识。

## 5 下一阶段

从[任务P01–P03](./06-milestones-risks.md#3-m-1-可执行清单)开始，优先暴露移动输入、WebView离线资源、PDF构建/选择和定位风险。M-1的1–2周是设备就绪、两名工程师并行时的首轮筛查目标；失败项需要额外修复或重新估算。

M0验收是PDF/EPUB导入→阅读→高亮→真实AI工具与引用→审核保存Note→重开精确回源→导出恢复。M1再扩展到多书研究Canvas、知识关系、候选结构、人审撤销和双设备离线冲突同步。

品牌仍使用内部QUSOR代号；商标、域名和GitHub Organization尚未核查，不作已可用声明。
