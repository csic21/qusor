# 微信读书插件接入研究

日期：2026-11-08。用户提供 dsh-weread，明确希望 QUSOR 也能使用微信读书集成。本轮完成源码研究与接入验收设计，未连接账号、运行上游测试或实现正式插件；具体首版深度和交付时间未确定。

## 固定来源与发现

审查版本：[80fa890d43bb4810b61d1e1d55c3bc5279986a54](https://github.com/zhaohongxuan/dsh-weread/tree/80fa890d43bb4810b61d1e1d55c3bc5279986a54)。

| 来源 | 源码事实与借鉴 |
|---|---|
| [gateway.ts](https://github.com/zhaohongxuan/dsh-weread/blob/80fa890d43bb4810b61d1e1d55c3bc5279986a54/src/host/weread/gateway.ts) | POST 到 `https://i.weread.qq.com/api/agent/gateway`，Bearer Key，平铺 `api_name`、`skill_version=1.0.3` 与参数；没有 Cookie 回退。可用作候选连接协议，不等于已验证官方稳定契约 |
| [paginate.ts](https://github.com/zhaohongxuan/dsh-weread/blob/80fa890d43bb4810b61d1e1d55c3bc5279986a54/src/host/weread/paginate.ts) | 笔记本使用 lastSort 翻页；重复/缺失游标或超安全页数明确失败，不把截断列表标成完整 |
| [journalCache.ts](https://github.com/zhaohongxuan/dsh-weread/blob/80fa890d43bb4810b61d1e1d55c3bc5279986a54/src/host/store/journalCache.ts) | Key 哈希隔离缓存，按笔记数/想法数/更新时间复用；单飞同步、首次扫描检查点、失败保留旧快照。QUSOR 应借鉴恢复机制，但账户身份不能直接等同 Key 哈希，轮换凭据不应生成第二套知识 |
| [readingLinks.ts](https://github.com/zhaohongxuan/dsh-weread/blob/80fa890d43bb4810b61d1e1d55c3bc5279986a54/src/readingLinks.ts) | 有有效 chapterUid 和 range 才生成 bestbookmark 链接，否则退回 reading 书籍链接。源码的 precise 标记只证明参数构造，不能证明各端实际精确回跳 |
| [LICENSE](https://github.com/zhaohongxuan/dsh-weread/blob/80fa890d43bb4810b61d1e1d55c3bc5279986a54/LICENSE) | MIT，原版权行为 `Copyright (c) 2026 Aurora contributors`。后续若复制代码须保留版权和许可；本轮没有引入上游代码 |

网关把 403 解释为出口风控，但仅凭 HTTP 状态不能证明 Key 有效或唯一失败原因；QUSOR 应显示拒绝访问/待诊断，不能直接判定授权有效。笔记数及更新时间也只是增量提示，不证明远端内容完全未变化，需保留手动重新获取和完整性状态。

## QUSOR 接入方向

关联 F-EXT-01/02/03/05/06、F-SRC-01、F-DATA-01。沿用 [插件架构](../architecture/extensions.md) 和 [目录边界](../architecture/workspace-layout.md)，无需新增服务专属 Core 分支或改变既有 ADR。

建议插件源码位置 `plugins/sources/weread/`，实际实施时建立独立包、锁文件，依赖发行的公开 SDK 工件，不能加入生产根 Workspace 或依赖内部源码。贡献入口为来源连接设置、手动获取、书籍/笔记列表和收录动作；贡献形态及 SDK 版本待公开契约验证后冻结，不冒充现有可运行接口。

数据链路：用户主动连接 → 宿主受限网络服务 → 插件解析上游响应 → 预览划线与个人想法 → 统一 CaptureDraft → Core 校验、持久提交、审计及逐项回执。来源插件负责微信读书字段，Core 负责通用知识与不可变快照。dsh 的 Cordis 路由、React DOM/CSS、Node 文件系统和 ctx.llm 不能直接放入当前 QUSOR Wasm/NSH 环境。

建议先验证书籍元数据、个人划线/想法、手动增量获取、本地收录和来源打开；今日回顾、统计和书墙可后续扩展。AI 总结仍受现有 AI 范围、出站及审核要求约束，不因参考插件包含 AI 就自动启动。

## 用户流程与数据

用户在插件设置连接账户，由宿主管理凭据；预览待收录笔记，主动选择后保存。取消预览不写正式知识；分批收录显示逐项成功/失败，重试复用冻结输入与回执。断网显示已有本地内容及最后获取时间；认证失败允许重新连接，拒绝访问、超时、限流和响应格式变化分别报告，不清空已有内容。

书籍元数据不表示已经获取整本书；划线保存为 excerpt 文本快照。书名、作者、章节、上游项 ID、采集时间、解析版本、定位字段及定位单位/未确认状态随来源保存；原文和用户想法分开。远端编辑生成新版本，远端删除不静默删除本地引用。跨账户及 Key 轮换通过宿主连接身份隔离与关联，不能按书名合并。

插件缓存仅用于加速，已收录知识归 Core；停用后停止获取，撤权后拒绝后续调用和旧任务结果，卸载后本地快照仍可查看、引用、导出。凭据存宿主安全存储，不进入知识、普通日志、同步或导出。请求仅发往批准的服务端点，重定向不得携带凭据到未授权来源；封面请求另按实际域名授权。网络和凭据 broker 当前尚未生产实现。

## 验收与实施门槛

| 场景 | 可观察结果 |
|---|---|
| 首次获取及分页异常 | 显示实际已获取范围；重复游标/部分失败不提示全量成功 |
| 同项重试、上游编辑 | 重试不重复入库；修改形成版本，旧引用仍打开原快照 |
| 断网、进程重启 | 已提交内容可用；失败项可恢复，游标不跨过尚未可靠接管的项 |
| 切换账户、轮换 Key | 不串数据；经验证的同账户重新连接不重复生成知识 |
| 来源打开 | 本地摘录可靠可读；有参数的外部链接仍须真机验证，失败或缺参数明确降级到作品/不可打开 |
| 撤权、卸载及导出 | 停止获取、拒绝过期结果；已收录内容及来源完整保留，导出不含凭据 |
| 普通插件身份 | 仓库外仅用公开 SDK 构建、安装与运行；没有私有写库或 NSH 对象依赖 |

下一步先核对微信读书官方接口说明、Key 授权范围、返回结构与错误语义；使用合成响应验证解析/分页/幂等，再在用户连接账户后做最小只读实测。当前观察到官方域名入口只修正“尚无可调查入口”的信息状态，不能据此宣布服务规则、长期稳定性或五端支持已验证。生产交付仍依赖宿主网络/凭据能力、真实 CaptureDraft 提交与存储、公开 SDK、安装隔离和 NSH 贡献点验收。
