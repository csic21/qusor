# ADR 0023：公开阅读 Surface 的版本与授权边界

日期：2026-11-08。状态：公开契约与 Host registry 已实现，普通签名包的原生集成资格通过；本记录不代表 0.2 已发布。关联 C-08/C-09、F-READ-01/03、F-SRC-01、F-EXT-01/03/04/05/06。用户流程和退出条件见 [能力规格](../../product/reader-surface-capability.md)。

## 问题与当前事实

原生探针已证明页图与窗口选择，但通过 trusted native module 访问单实例状态，不是普通插件的公开能力。实施前 Host 只有0.1，request严格匹配HOST_API、包兼容只检查这一个版本，Session没有独立协商版本。直接把常量替换成0.2会破坏已签名的旧包及请求。

此外，现有 replace_grants 会推进授权 epoch 并关闭旧会话。每次选文后增加一个SelectionId授权，会让Surface自己失效；不能用另一套不经过manifest的授权表掩盖这个问题。

## 决定

保留 HOST_API=0.1 的兼容入口，新增显式支持集合和0.2常量。旧 open_session 明确建立0.1会话；新入口选择支持集合与manifest范围的合法交集，并把实际版本冻结进连接。每次请求必须与该连接版本匹配，能力本身也有最低版本规则。包验证新增显式宿主版本集合入口，不能改变原签名domain或把不兼容包当成兼容。

0.2增加 reader.surface、阅读工作区贡献及精确Document范围。选区读取仍是独立 selection.read：0.2的Document范围仅能读取该文档、该Surface中真实用户选择的当前lease；broker必须额外核对handle、来源、generation与lease。0.1继续只接受原有Selection范围，不获得新语义。临时lease更新不调用replace_grants，不改变持久授权本身。

Host统一持有纯数据Surface registry，绑定安装实例、package digest、session/授权epoch、vault/document及来源版本，记录frame/job/mount身份、视口世代和当前selection lease。不存NSH/PDFium对象或数据库连接。opaque handle还需独立宿主启动身份，不能跨重启复用；启动身份来自可信装配层，不能由guest声明。

## 公开操作与可信通道

| 边界 | 能力 |
|---|---|
| guest SDK | 有界Open、页码/倍率意图、读取当前selection lease、关闭自己的Surface |
| Host原生后端 | 真实文档绑定、实际viewport、frame加载完成、raw responder输入、资源释放确认 |
| Core应用 | 版本/原件/锚点校验、一次提交、回执、查询、导出恢复 |

输入和FrameReady只接收Host签发、不可从guest JSON反序列化的内部ticket；普通插件不能发送坐标或自报Ready成为来源事实。位图/字符报告在受控服务与原生后端之间传递，Wasm只得到有界元数据和可挂载资源描述。正式reader与第三方按同一身份、能力和broker路径运行。

Open快速返回Pending，原生解析/渲染异步执行；不得在Wasmi broker回调里做长时间PDF工作。燃料和250ms协作deadline不等于native回调抢占，实际监督与资源回收仍需外层机制。

所有操作、原生事件与异步结果发布都重新核对当前会话/授权/来源/世代。disable/revoke/upgrade/uninstall/close_session统一失效关联Surface，不能靠调用方记住清理。旧frame不放行新输入、旧gesture不取消后继gesture；关闭后的结果只做清理。不能确认底层释放时保留清理中/失败状态，不虚报已释放。

## 实施与验证

extension-api承接独立surface wire及版本规则，extension-host承接协商与registry；原生实验后端用真实lease替换全局单实例调用。尚未通过完整平台输入门槛前，NSH实现继续在独立实验中，不自动成为生产adapter。

最小实际切片必须由仓库外SDK构建的普通签名Wasm包发起Open/Configure，挂载真实NSH Surface，经真窗口选择，再由同一个handle读取完整草案并沿既有Core提案/回执通路保存。禁止用空DTO、固定broker响应或静态链接官方插件代替。

回归同时证明旧0.1包/请求/签名和捕获仍运行，新能力在旧session或未授权时拒绝；跨安装、vault、会话、版本的handle拒绝，撤权/关闭/升级后的在途结果不再发布。原生探针、单独插件测试和Core测试不能直接相加为完整旅程通过。

当前已实现0.2 wire、会话版本冻结、Host registry与显式宿主版本集合的签名验证；默认0.1入口保留。原生消费者正在独立实验中接入仓库外SDK构建的普通签名Wasm，尚不作为正式reader功能包交付。

失败收尾遵循实际资源状态：同次调用最多一个Surface操作；执行失败或最终响应不符，已触及的Surface进入关闭流程。未曾暴露给UI、没有挂载/渲染任务的新Open可以由可信后端确认释放；已有挂载继续保留Closing，不能以删除映射冒充原生资源释放。

[本机集成资格](../../../experiments/nsh-reader/plugin-surface/RESULTS.md) 已由普通仓库外签名Wasm、Host registry和真实NSH窗口贯通：三次选文与非空个人笔记经原生明确保存，包含protocol2跨行整组；进程正常退出后独立查询、完整导出/空库恢复及精确解析均通过。键盘实测为Xvfb中的GTK/XTest，当前桌面XWayland焦点失败另存；不等于中文输入或正式工作区通过。输入法、键盘/触控/无障碍、发布包、EPUB和其他平台继续独立验收，不缩减长期要求。
