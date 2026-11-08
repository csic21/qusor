# 原生动态 PDF 资格

最终vault-bound复验已通过，固定NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`、Linux原生host。根Host跨vault卸载tombstone修复后重新构建，native all-lib 11项、Clippy拒绝警告通过；新binary `c7156292…e67c`、同一通用bundle `1994de4e…1214` 和普通签名包 `895b6b11…0dff` 在新一轮真实选择、第三页/第四页选文保存及空库恢复期间hash不变，102项构建源前后相同。完整[最终指纹](../evidence/dynamic-pdf/final-vault-bound/qualification-fingerprints.json)与102源归档标识已留存，首轮旧工件与97源另外封存，没有覆盖。

最终build结束后才生成内容不同的“Final novel page 3”和“Final alternate page 4”两份三/四页许可PDF；真实GTK picker→guarded native parser→同一Core→production profile2/DurableHost→普通签名Wasm→native File/Image，在同一进程先后实际拖选与键盘输入“final native note”“final second note”，均以真实Core receipt确认保存。见[最终第三页](../evidence/dynamic-pdf/final-vault-bound/third-saved.png)、[第四页](../evidence/dynamic-pdf/final-vault-bound/fourth-saved.png)、[真实输入事件](../evidence/dynamic-pdf/final-vault-bound/interaction.json)。正常WM关闭后进程消失、无pending worker marker；独立Core重开、完整导出→空库恢复、两条exact resolve及两份original逐字节比对全部通过，见[最终恢复](../evidence/dynamic-pdf/final-vault-bound/core-recovery.json)。独立ldd核对全部resolved且无WebKit/JSC/libsoup，见[native链接](../evidence/dynamic-pdf/final-vault-bound/native-ldd.txt)。

以下保留首轮更完整的坏文件、预算、草稿与取消测试事实；这些案例在跨vault修复前的冻结工件执行，最终复验重跑动态主链与恢复，没有将未重跑的失败注入记作最终工件实测。

通用bundle无PDF/PNG/frame imports，构建完成后才生成MIT合成三页、四页、十七页与取消用十六页文件。相同binary `336edf06…36873`、bundle `1994de4e…1214`、普通独立签名包 `895b6b11…0dff` 完成下列实际操作，全程hash不变，97项构建源前后hash相同。完整值见[机器指纹](../evidence/dynamic-pdf/qualification-fingerprints.json)。native all-lib 11项、all-target Clippy拒绝警告通过，其中动态4项覆盖FIFO/symlink快速拒绝、owned Child确认reap、未知身份持久隔离，以及只执行最新pending frame工作。

真实GTK chooser先Escape取消，确认尚无Core知识库；随后实际点选新三页PDF并Open，经同exe guard/GuardReady→父端OS身份核对→有界Boot、PDFium串行worker、Core导入与production profile2/DurableHost、普通签名Wasm Surface Open/Configure，显示第三页。页图为native PNG File→可信TSX Blob URL→原生Image，Canvas只高亮。实际鼠标拖选“Runtime novel page 3”，键盘输入“runtime native note”，按保存后收到Core capture1/document1/commit_rev2，见[保存截图](../evidence/dynamic-pdf/third-saved.png)。pixel/path/File未进入guest。

同一进程新建未保存想法“keep this draft”，切资料前显示原生确认。真实选择坏PDF、十七页PDF、16MiB+1字节文件分别得到解析错误、明确页数预算拒绝、明确字节预算拒绝；原第三页、选段与草稿均保留。之后成功选择四页不同来源PDF，旧草稿按确认清除，第四页显示“Alternate source page 4”，真实选段及“second source note”保存到capture2/document2/commit_rev4，见[第四页保存](../evidence/dynamic-pdf/fourth-saved.png)。再选十六页PDF，在GuardReady后点击取消，未导入第三份文档，旧第四页保留，worker已确认回收、无pending/quarantine遗留。

窗口正常WM关闭后进程消失。另一个独立Core CLI在进程退出后重开库读取两份摘录，将完整archive导入全新空目录；两份记录完全相同，精确resolve分别返回原第3/4页锚点，导出的两个original与构建后fixture逐字节相同。见[恢复机器证据](../evidence/dynamic-pdf/core-recovery.json)。没有声称App资料库历史列表或重启自动阅读位置已经实现。

实窗运行在专用Xvfb :98，是真实GTK控件与XTest输入路径，不代表物理键盘、IME、触屏、辅助功能、Wayland或其他平台通过。首轮session portal调用未出现可用chooser，随后local GTK明确暴露缺少编译schema；在ignored提取GTK4 prefix运行系统glib-compile-schemas并隔离session bus后，第三次启动真实chooser通过。失败日志保留。首轮阻塞picker进程发送WM close后仍存活，核对其无Core FD/子进程后SIGTERM，确认/proc消失，见[旧进程清理](../evidence/dynamic-pdf/old-picker-process-cleanup.json)；不能将已发关闭请求当退出确认。

当前预算只支持16MiB/16页以内，普通长书仍未完成；不静默截断。页图缓存最多3页/32MiB，frame队列最多1执行+1最新pending。没有真实native unmount ack的资源仍Closing，撤销Blob URL不代表释放。managed-worker提供Linux父进程死亡guard与身份核对清理，不是PDF安全沙箱。崩溃遗留pending/quarantine会拒绝替代worker，尚不提供自动恢复。当前错误UI统一提示检查文件/验证预算；正式流程需按损坏、页数、大小给具体可行动提示。保存失败重试、撤权迟到与旧frame/来源拒绝有共享服务测试，未把未执行的失败注入或实窗竞态冒充已观察结果。
