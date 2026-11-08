# 图片生命周期纵切片：冻结工件

固定NSH913，独立homecache worktree；主pin、QUSOR Core和插件SDK未改。本阶段feature-off/on检查通过、9个observer场景分别独立进程通过、40项原Core图像相关回归（含一个新有限cache扣账测试）通过。全部owner仍Unprovable，没有Host释放ACK。

冻结observer测试exe：

`/home/user/.cache/qusor-native-release-observer/image-tests-attempt-1/observer-image-tests`

SHA256 `1a85437a4488920062fce98bc7f8f1fd4f09e26549da7f14d6af9847b3f771e0`。

冻结Core图像测试exe：

`/home/user/.cache/qusor-native-release-observer/image-tests-attempt-1/core-image-tests`

SHA256 `2e310f3d5f316780b93e1485458939dd136a7f19784720670b10894b421648a1`。

大exe留homecache，不纳入仓库。仓库保存baseSHA、锁hash、完整13源delta ZIP/patch、前后hash、失败编译attempt和所有测试日志。

使用私有free Xvfb（先查lock和abstract socket，显式编号，不改全局目录）。测试进程LD_LIBRARY_PATH为`experiments/nsh-reader/target/native-prefix/usr/lib/x86_64-linux-gnu`的绝对路径，GSK_RENDERER=cairo、GSETTINGS_BACKEND=memory、GIO_USE_VFS=local、GTK_IM_MODULE=simple，HOME/XDG使用新私有目录。Xvfb自身使用`/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local`及其私有LD路径。原实际Xvfb身份和wait0见record。

先用冻结observer exe `--list`，逐条选择含`retirement_`的9个test名称，每条分别执行：

```
<observer-image-tests> --exact <full-name> --nocapture
```

不能把危险pool场景合并进同一测试进程：每进程真实entered握手，RAII/finally释放屏障；原driver每场90秒执行deadline，超时记录失败并终止，不因观察超时重启。所有9条精确命令/PID/start/exit在record.json，逐场日志test-1.log至test-9.log。

Core冻结exe直接运行`image:: --test-threads=1 --nocapture`，应出现40 passed。编译命令及实际过滤数量见core-image.log；这不是NSH全部测试或其它平台资格。

证明范围：真实queued/running decode物理任务完成、取消订阅与queued UI payload分离、A/B私有cache共用进程Budget、真实Texture clone持有pixels、owner frame身份、锁外payload析构、4353次实际GTK节点复用及在途超限。ImageData容器身份参与tracked cache key，实际Picture保留该容器，snapshot从真正append的Picture核stamp。

qdata只证明最终化阶段通知，不是整个finalizer返回或GPU fence。native children、完整outro与deferred/progress/playback计数尚未闭合，严禁写成完整release。已知后续问题：本工件还没有Budget弱cache registry注册时prune修正；后续源码单独冻结在homecache/image-weak-registry-source，不能把本次40项通过覆盖它。
