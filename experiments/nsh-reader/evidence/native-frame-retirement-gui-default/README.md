# Frame/operation整合后的默认原生GUI

2026-11-08，当前frame生命周期与operation等待版本的默认feature Linux Xvfb GUI通过：[记录](record.json)、[原始窗口日志](window.log)、[Core恢复](core-recovery.json)。155份项目源码在构建前冻结、真实终态复核一致，默认binary与bundle路径/哈希见记录；没有使用native-qualification工件或用户文档。

真实chooser打开构建后新生成的合成PDF；换到第2页并缩放65%。随后10ms事件间隔发出6次实际换页/缩放配置，generation5–10均被宿主接收，只有最后generation10产生完成的DYNAMIC_FRAME；稳定截图显示最后请求的第1页/75%，没有观察到旧frame覆盖。该结果只证明本次实际burst；恢复整页55%后进行编辑。

选文→Write自动聚焦，无需点Input即可输入并保存`frame retirement thought`；继续阅读，第二草稿取消放弃确认保留`discard frame thought`，再明确放弃返回正文。WM_DELETE_WINDOW正常关闭，App exit0；30个实际编辑VM、100calls、30个owned Reaped，worker lease0、正式capture仍为1。Core重开及空库恢复保留摘录/正文、原PDF字节精确。

FRAME_RUST_RETIREMENT原始行的phase为Active，发生在ticket.wait_until之前，不把该行改写成Joined。实际App成功退出路径必须经过frame ticket等待成功、operation等待成功和retirement service关闭成功；对应函数只有实际JoinHandle.join成功才返回成功。FRAME_ENCODED_SCOPE本身位于service join成功之后，是实际post-service-join控制流见证。记录保留相关源码哈希与原始日志行；未伪造ObservedJoined事件。

FRAME_ENCODED_SCOPE实际created5/released5、containers0/referenced_bytes0、uncertain=false；仍明确`allocation_may_remain_in_native_png_cache=true`、`native_release_ack=false`。这是encoded共享容器的可观察范围，不证明decoded图片、GTK/GPU/native mounter资源释放，也不把0计数或进程退出当ACK。App/Xvfb/private DBus及观察到的portal PID均已退出。

本轮未覆盖IME/Esc、unknown结果、partial-read准备失败、实机或其他平台。先前8项OS故障与GTK探针是独立证据，不能替代本GUI；后续改动也须新版本留证。
