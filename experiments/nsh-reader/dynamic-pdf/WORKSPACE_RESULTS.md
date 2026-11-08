# 原生阅读分区验证

日期：2026-11-08。关联C-09/C-11/C-12及F-READ-01/03、F-NOTE-01、F-UX-01/02。当前为独立原生消费者资格，不是正式library/reader/notes插件或公开组件替换交付。设计依据见 [组合规格](../../../docs/design/composition.md)。

## 已验证的界面与真实结果

外壳、阅读视口、想法面板和语义样式分别实现；默认阅读不常驻想法表单。宽窗辅助区独立336逻辑点，窄窗切到单任务想法视图；页图保持挂载，面板/窗口变化不调用configure来重置倍率或选区。用户引用与个人输入分开，保存成功必须来自Core回执。

首轮真实窗口覆盖默认空态、文件选择取消、第三页选文及上下文动作、想法自动焦点/非空输入、宽窄切换保留原frame和草稿、坏PDF保留、真实保存/只读及返回阅读。正常退出后独立重开Core，完整导出到空库恢复，第三页引用精确解析，原件字节一致；想法严格为`quiet native thought retained`。源码119份及对应工件已封存，主agent逐项核对zip内hash。见 [首轮指纹](../evidence/reading-workspace/qualification-fingerprints.json) 与 [Core恢复](../evidence/reading-workspace/core-recovery.json)。

第二阶段修复阅读页居中、导入期间冻结草稿输入与操作、保存清除过期提示，并增加实际复制按钮。独立数据的第三页`Quiet reading page 3`真实保存，想法`centered native thought`空库恢复一致；原生复制按钮到隔离X11剪贴板UTF-8读取逐字一致。另119份源码zip已独立核对，见 [居中阶段指纹](../evidence/reading-workspace/final-centered/qualification-fingerprints.json)、[恢复](../evidence/reading-workspace/final-centered/core-recovery.json) 和 [实际剪贴板](../evidence/reading-workspace/final-centered/native-clipboard.json)。

## 保留的失败与最终复验

首轮`third-selected-actions.png`在FrameReady前尝试选文，无真实选区，未计通过。居中阶段的窄窗Tab循环发现被遮挡正文仍可获得焦点；失败截图和该bundle均封存，不用后续截图冒充原阶段通过。

已修复窄窗覆盖时正文focusable与指针接入，同时保留Image/frame；修正主按钮焦点对比。最终bundle通过真实NSH声明类型检查、草稿状态测试及独立原生窗口完整链。窄窗Tab不再停到遮挡正文，坏PDF/取消保留原想法，实际保存`focused native thought retained`，正常退出后空库恢复精确解析`Focused reading page 3`，原件字节一致。主agent已独立复核119份封存源码hash及Core恢复记录；见 [最终指纹](../evidence/reading-workspace/final-focus/qualification-fingerprints.json)、[恢复](../evidence/reading-workspace/final-focus/core-recovery.json) 与 [逐项实窗说明](WORKSPACE_UI.md)。

## 尚未完成

- Esc安全返回和中文输入法组合态；XTest输入不等于物理输入或IME通过。
- 公开0.3组件提供者、两普通插件实际替换、正式资料库/阅读/笔记包。
- 长书/EPUB、native真实释放ack、其他平台及正式打包。
- 跨退出草稿恢复、既有笔记编辑、保存未知的公开回执查询。当前未知状态保留原submission并原样重试，不谎称已完成查询。

Native binary与普通签名包在各阶段中保持不变，bundle按阶段分别封存。原生Image/Canvas/输入/文件对话框/Clipboard工作，没有新增WebView/DOM产品实现；真实资源释放仍须另验，Blob URL撤销和组件cleanup不构成释放确认。
