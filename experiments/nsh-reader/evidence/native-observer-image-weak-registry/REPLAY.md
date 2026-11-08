# 弱缓存登记修正版：独立复跑

本版固定NSH913+13源delta，新增的注册时prune只删除strong_count为0的历史Weak，保留真实cache/lease。Core图像41项、feature-on/off检查、重建后9个observer独立进程均通过。旧版9+40证据在兄弟目录保留，不覆盖。

冻结observer exe（大debug工件留homecache）：

`/home/user/.cache/qusor-native-release-observer/image-weak-observer-tests/observer-image-tests`

SHA256 `6e10ebc7f9aca2ec442bd144c3522065b52a0aabfa7dd7e257e9c84c3551aafc`。

冻结Core exe：

`/home/user/.cache/qusor-native-release-observer/image-weak-final-validation/core-image-tests`

SHA256 `b0f24193773ec5b06b93edec55c24e9940a9635c6ad7a6871113f3943420a9c2`。

复跑observer需独立free Xvfb（检查lock与abstract socket后显式编号，不用-displayfd，不动全局socket目录）。Xvfb使用`/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local`与其LD_LIBRARY_PATH；test exe使用native-prefix/usr/lib/x86_64-linux-gnu的绝对LD路径、GSK_RENDERER=cairo、GTK_IM_MODULE=simple、私有HOME/XDG目录。详细启动方式沿用前版[说明](../native-observer-image-attempt-1/REPLAY.md)，本版原脚本weak-observer-tests.py完整留存。

`<observer-exe> --list`找到9个含retirement_的场景，逐条独立进程执行`--exact <full-name> --nocapture`；不能把危险pool屏障共享成一个进程。实际9条完整命令、PID/start和exit0在record.json；每场原driver为90秒执行deadline，所有场景均自然完成，没有触发。测试池拥有真实entered握手与RAII/finally放屏障，不因观察超时重启。

Core exe运行`image:: --test-threads=1 --nocapture`应41 passed。新增单项精确名称：

`image::memory::retirement_cache_drop_tests::retirement_finite_cache_registry_stays_bounded_without_pressure`

该测试实际4353轮finite cache创建/丢弃，全程无pressure/clear；每轮断言registry长度<=2、旧真实lease持续占16bytes；释放旧lease后新注册只余1项。没有从旧40项成绩推导新版通过。

所有步骤源码前后hash相同；11-09官方上游核对在validation/upstream-check.json，仍以固定913+delta资格，不升级到main。依然partial/Unprovable：native children、完整outro及其它未跟踪回调未闭合，qdata只是最终化阶段通知，snapshot不是物理present，**不产生Host ACK**。
