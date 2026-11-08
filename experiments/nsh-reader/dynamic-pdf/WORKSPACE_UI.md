# 原生阅读工作区 UI 验证

本轮在固定NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`、Linux GTK完成真实窗口资格。采用[工作区设计](../../../docs/design/reading-workspace.md)与[组合边界](../../../docs/design/composition.md)，官方Design Guides核对见[记录](../../../docs/design/nsh-check.md)。没有升级NSH，也没有新增WebView/DOM路径。

## 实现

- `theme.json`把设计基线浅/深语义角色映射至NSH主题；`ui.tsx`在`nsh/base`的Button/Input之上提供应用样式与hover、pressed、focus、disabled状态。
- `WorkspaceShell`只装配toolbar/content/auxiliary/status props，`ReaderViewport`持有原生Image、Canvas高亮和原生滚动，`CapturePanel`按需出现。默认没有常驻表单、资料库假数据、进度、目录、笔记列表或假提供者菜单。
- 选区稳定才出现保存/写想法动作。宽窗为辅助区预留336逻辑点并保持原frame/zoom，纸张在阅读区域居中；窄窗以单任务面板覆盖，Image/frame不卸载，底层pointer与正文Tab停靠关闭。
- 草稿绑定真实文档、报告、handle/generation/frame与candidate。打开面板或resize不configure；有未保存想法时限制重选、翻页与缩放，仍可滚动/返回正文。输入实例按来源与候选key绑定，迟到输入不写新草稿；导入时同时锁本草稿输入与事件路由。
- 保存冻结candidate/note快照；未知结果保持原请求重试，或复制后明确离开，说明离开不撤销已有保存。迟到页面失败不抹掉正在保存的身份，真实回执仍可确认。已保存想法只读，不复用已提交candidate改写旧笔记。
- 已导入但阅读失败再重开时，原来源quote/note保留为detached，不接到新资料。原生Clipboard显式复制可用于保留；没有把临时lease当持久草稿。

这是受信宿主区域/样式承载实现，普通签名Wasm与Core保存链保持原样。没有宣称公开provider替换协议、独立notes包或正式0.3草稿能力已经完成。

## 实测与证据

最终证据在[final-focus](../evidence/reading-workspace/final-focus/qualification-fingerprints.json)。native binary `9ac240f1…17cfe`、最终bundle `b196b67f…bd391`、普通签名包 `895b6b11…0dff` 在本轮运行中hash不变；119项构建输入前后相同且归档至ignored `target/reading-workspace-focus-final/sources.zip`，完整hash见机器记录。

实际完成：新bundle构建后生成许可三页PDF；原生picker→第三页真实选文→附近动作→点写想法后不点击字段直接键入；1280/860宽窄resize、关闭/重新打开面板保持同一frame与草稿；取消picker及坏PDF保留原内容；保存以Core receipt确认；保存后键入被只读字段拒绝；显式原生复制后通过专用X11 display UTF8剪贴板逐字核对；继续阅读一次关闭辅助区并保持原页。

[窄窗Tab循环](../evidence/reading-workspace/final-focus/narrow-tab-5.png)由六个可见停靠点组成，覆盖正文不再占一跳；[保存截图](../evidence/reading-workspace/final-focus/saved-core-receipt.png)分开显示原文与想法。覆盖区点击没有新增guest/selection调用。正常WM关闭后进程消失，无pending worker；独立Core重开、完整archive导出/全新空库恢复、精确第三页引用`Focused reading page 3`、非空想法`focused native thought retained`及original逐字节恢复通过，见[恢复记录](../evidence/reading-workspace/final-focus/core-recovery.json)。

Node执行的状态测试覆盖旧输入/旧来源拒绝、不可变原样重试、已提交candidate不能再保存、detached保留原来源，以及save过程中source失败仍接纳同请求真实回执。`check_ui.py`用固定NSH类型及真实缓存native ABI独立类型检查，不冒充Rust编译或实窗验证。

首轮119源与工件另封存，真实发现纸张左靠、旧导入提示残留；居中阶段再实际发现一个被覆盖正文Tab停靠点。两个阶段的截图、恢复和精确源码zip均保留，没有覆盖失败。首轮`third-selected-actions.png`是FrameReady前的失败选文尝试，不计通过；实际成功图为`third-selected-actions-ready.png`。

## 尚未验证或未实现

原生Xvfb/GTK/XTest不是物理键盘、IME、辅助技术或其他平台资格。浅/深主题角色已定义，本轮只实测当前浅色外观；未宣称对比度/系统字号放大已验。安全处理Input组合态的Escape关闭尚未实现：固定keymap默认把Escape留给输入，强制always会抢输入，因此不假标快捷键。

草稿只保留当前会话，没有跨退出恢复或完整窗口关闭保护承诺。编辑时限制换页/重选是现0.2选区lease边界，正式边编辑边任意翻页需新公开草稿契约。结果未知、迟到source失败及detached竞态有模型测试，未以伪造UI状态冒充实窗故障注入。当前不支持修改已存笔记、查询未知保存回执、资料列表/持久阅读进度、提供者替换、长书或真实native释放ack。16MiB/16页预算与Closing资源限制继续如实保留。
