# 普通 A 组件的原生全文编辑资格

日期：2026-11-08。关联 C-09/C-11/C-12、F-NOTE-01、F-READ-03、F-EXT-03/04/05/06、F-UX-01/02。沿用现有分区与设计 token，不新增 WebView。当前是独立原生消费者实验，尚非正式阅读／笔记功能包。

## 实际链路

真实 GTK 文件选择器打开构建后生成的合成 PDF；原生拖选创建来源。点击“写想法”后，可信适配器冻结完整 Single 或 Compound 来源，创建应用草稿并通过显式挂载意图批准当前 Draft。普通签名 A 包仍由仓库外 SDK 构建，包 SHA 为 `7769c10ae964187765e8e7de9c84ff6422d00da7426b7581ed961f18c6a030b8`。

全文输入经共享 EditServices、同一 Store/DurableHost、真实 Controller/ManagedRun、fresh worker 执行。保存方法只接受 binding/revision，从应用草稿冻结正文，重新核对原捕获身份与许可后调用同一 Core writer；不接收前端传来的替代保存正文。未知 apply 保留原 payload，未知保存保留原 operation；已提交回执可在捕获撤权后只读核对，未提交时撤权仍拒绝新写入。

输入状态留在 App，面板隐藏不删除交付状态。每个 Input 实例有独立的不透明 UI token，旧实例、错误草稿／来源／binding 的回调均拒绝。组合输入期间暂不交付新快照或保存；这条状态逻辑通过测试；真实 IBus/libpinyin 的组合、提交、取消与保存恢复见下文。

## 实窗与恢复证据

| 阶段 | 真实结果 | 限制 |
|---|---|---|
| [首次接线](../evidence/native-editor-ui-attempt-1/record.json) | 显式点击输入后，15 VM/50 calls，全回收；保存、关闭后 Core 重开、空库恢复及原件字节一致 | 自动聚焦失败，保留原截图与247份源码快照 |
| [多段来源](../evidence/native-editor-ui-attempt-2/record.json) | 实际跨两行拖选，完整2段保存与恢复；15 VM/50 calls，全回收 | 只增加延迟仍未修复聚焦；不冒充通过 |
| [就绪状态修复](../evidence/native-editor-ui-attempt-3/record.json) | 点击写想法后不点击 Input，直接键入 `ready component thought`；15 VM/50 calls，全回收；保存／正常退出／重开与空库恢复一致 | 仅该 Linux/Xvfb 小文本旅程；非物理键盘或 IME 资格 |

聚焦根因是固定 NSH 的 Each 在 tracked 区读取 list，却在 untracked 区计算 key。把 readiness 只写进 key 不会触发新 ref。最终将 draft、native binding、editable 放入显式响应式 row；准备完成真正创建可编辑控件，正常打字保持同一实例。延迟请求仍核对实例、来源和当前状态，实际是否聚焦由新窗口输入结果证明。

最终247份测量输入构建前后及旅程结束时哈希一致，源码 ZIP、bundle、真实 ABI、原生 binary 哈希、截图、worker 日志与 Core 恢复结果均按阶段保留。第三阶段的[自动输入截图](../evidence/native-editor-ui-attempt-3/autofocus-input.png)与[恢复记录](../evidence/native-editor-ui-attempt-3/core-recovery.json)属于新工件，不复用失败阶段截图。

根226项测试、15项 native library 测试及 Clippy 已通过。后者包含真实 Core/Store 的 begin 幂等、BUSY 未知结果、撤权后的既有回执恢复及撤权后拒绝未提交写入；fixture 直接设置正文，不计作编辑 guest／GUI 证据。

## 真实中文组合输入增量

[隔离 IME 实验](../evidence/native-editor-ime-attempt-1/record.json)复用第三阶段的原生 binary 与 bundle，新建私有 HOME/XDG、DBus、Xvfb111 和官方校验的 IBus/libpinyin 环境，不读个人词库。真实拉丁拼音按键 `nihao` 经输入法产生中文预编辑：保存禁用、编辑 VM 为0；Space确认后普通A执行3个真实worker/10calls，全回收，最终正文为“你好”。随后 `ceshi` 加 Esc 实际取消组合，原正文和面板保留，没有新增编辑VM。

正常关闭后重开SQLite、导出并恢复空库，正文UTF-8 `e4bda0e5a5bd`、来源与原PDF一致。主负责人独立核对65份归档工件哈希及Core结果。trace只记录CompositionChange事件种类；组合开始／结束依据固定源码、可见预编辑／提交次序、保存门和实际worker行为联合证明，没有直接抓取composing/text payload。候选面板被禁用，不能据此签署候选窗UI或位置、物理桌面／其他平台，也不代表全局Esc已经实现。

最初single模式缺少config组件导致准备失败，在同一私有Bus/display补上内存配置服务后继续；失败记录保留。所有自有应用、输入法、引擎与虚拟显示进程均按原启动身份确认退出，没有重启来掩盖观察超时。

## 回收故障修复增量

后续 NativeEditor 保留真实 PendingRun owner、未消费事件和持久 lease；启动后的 invocation／gate 失败也继续退役，实际 Reaped 与 Store finish 分别确认。finish失败或等待未知不遗失 owner，继续操作可以排空；丢弃只核对当前组件安装的 lease，不受其他安装活动误阻断。

[六项真实OS回归](../evidence/native-editor-retirement/record.json)与[主负责人同二进制独立复跑](../evidence/native-editor-retirement/root-independent.json)均通过；包含真实守卫进程、同／不同安装隔离、实际回收后清理失败和未知等待。预算仍为250ms／100万fuel／8calls。测试故障入口只在非默认 `native-qualification` feature，不进入JS ABI或公开SDK。该切片不证明原生释放ACK、terminate背压或完整后台回收服务。

回收修复后的[默认 feature GUI2](../evidence/native-editor-retirement-gui-default-2/record.json)已通过：真实选文→写想法自动聚焦→原生键入→正式保存→继续阅读；第二份草稿先取消放弃确认，正文保留，再确认放弃并返回阅读。33个实际worker／110 calls全部真实回收，正常WM_DELETE关闭exit0，关闭后Core重开、完整导出／空库恢复与原PDF字节一致，持久worker lease为0，Core仅保留一次正式保存。[主负责人独立核对](../evidence/native-editor-retirement-gui-default-2/root-independent.json)确认49份工件hash、33次VM与REAP对应、110 calls、唯一保存正文及重开／恢复整行相等。源码输入、bundle及完整原生binary均已冻结；binary SHA为 `7e52ffd15c223237f94985bc11c74017425a76a87c4682f6f7ffdf70cbd31883`。

此前[portal环境失败attempt](../evidence/native-editor-retirement-gui-default/record.json)保留：私有DBus启动时继承错误display，chooser阻塞且正常关窗未响应；核对身份后显式终止，实际终态确认，未创建Core／包数据库。独立GUI2在创建DBus前设置完整隔离环境，实际使用私有GTK portal chooser；`GTK_USE_PORTAL=0`不等于绕过portal。

GUI2仅覆盖上述 `7e52…` 默认feature版本，**不覆盖随后已应用的[shutdown／准备失败恢复候选](../evidence/native-editor-shutdown-integration/)**。GUI2没有重新运行IME，也未自然进入暂停／重试状态；旧IME证据仍只属于原冻结工件。native mount release ack实际仍为false，不能把正常进程退出写成释放确认。

shutdown／准备失败恢复增量已通过[八项真实OS回归](../evidence/native-editor-shutdown/README.md)、主负责人同冻结exe独立复跑、完整16项native lib测试及Clippy。随后重新构建的[默认GUI](../evidence/native-editor-shutdown-gui-default/record.json)通过自动聚焦、真实输入／保存、第二草稿取消放弃后保留正文、明确放弃与正常关闭；30个实际worker／100 calls全部回收，Core重开／空库恢复与原PDF字节一致，lease为0，150份精选输入指纹前后相同。binary为 `59d2febbe03057061b6e4b7a2d996451fdcc40f4f224907a156b80fecd4da549`，bundle为 `93a8d4079ac3df9949c4ebf2a6e9f197687c6c1f674b49a40508a6d8559f3a83`；[独立核对](../evidence/native-editor-shutdown-gui-default/root-independent.json)覆盖42份工件、完整冻结binary／bundle及SQL整行和真实VM／reap。正常关窗仅记录一次 `pending_owner=false retirement=not_needed`，未冒充触发异常排空。

该GUI未自然触发分块准备失败、未知apply／save、暂停／重试或IME，不替代相应状态门的局部／OS证据；native release ack仍为false。后续图像任务和observer源不包含在此冻结版本，需另构建验证。

## 尚未通过

- 输入法候选窗UI／位置、物理桌面与其他平台IME、全局Esc、跨面板焦点及完整宽窄窗口回归。
- B 的原生段落视图、A→B→A原生替换；已有无界面共存不替代它们。
- 原生释放确认；目前 Closing 配额保留，进程退出不冒充存活宿主中的 release ACK，见[调查](../../../docs/architecture/native-release-ack.md)。
- 1MiB 的原生交互延迟、长书／EPUB、正式功能包与其他平台。

并行链接的资源抖动已留证，后续准备默认串行，动态实验只构建所需 binary。SDK作者包重命名遗留 default-run 与临时目录空间不足也保留失败记录，修正后才生成真实 ABI。源码检查、构建、方法级测试、实窗和恢复各自描述，不互相替代。
