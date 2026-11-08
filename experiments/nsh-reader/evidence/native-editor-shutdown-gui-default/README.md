# Shutdown修复后的默认feature原生GUI

2026-11-08，当前shutdown/UI恢复版本在Linux Xvfb默认feature原生窗口通过正常闭环：[record](record.json)、[窗口日志](window.log)、[Core恢复](core-recovery.json)。真实chooser打开构建后新合成PDF；拖选、Write自动聚焦、无需点Input即输入并保存`shutdown native thought`，继续阅读；第二草稿取消放弃确认仍保留文字，再明确放弃返回正文；WM_DELETE_WINDOW正常关闭，App exit0。

30个实际编辑VM、100calls、30个owned Reaped，worker lease0。150份源码在构建前与真实终态指纹一致；默认binary/bundle冻结路径及哈希见record，未启用native-qualification。本次正常关窗只记录一次`pending_owner=false retirement=not_needed`；异常pending排空由独立OS故障证据覆盖，本GUI未自然触发。原生资源ACK仍为false，不能把Rust worker reap当native view/renderer释放。

第一次拖选NoHit及第二次长拖选末点NoHit保留在日志/截图；有效字形重试成功后才进行输入资格。一次无选段时的write/type没有进入Input，不计入自动聚焦成绩。真实成功输入截图、取消确认后的文字截图以及discard后的正文截图分别保留。

Core重开、空库恢复、来源摘录与正文精确、原PDF字节一致；放弃的第二草稿未增加正式capture。App/Xvfb/private DBus与观察到的portal进程已终态，不保留运行中GUI。partial read、unknown结果、IME/Esc与后续动态FrameScheduler候选不在本GUI范围，旧成功证据也不外推到下一版本。
