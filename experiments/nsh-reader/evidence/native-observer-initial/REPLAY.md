# 第一纵切片：GTK4后端独立复跑

此记录不是GTK3 portal chooser，也不是阅读App/guestVM资格。固定NSH913 + 本目录patch；feature-off/on检查及3项GTK4测试实际通过。所有owner仍Unprovable，不存在Host release ACK。

冻结测试exe（455,278,768 bytes）保留homecache，避免向仓库提交大debug工件：

```
/home/user/.cache/qusor-native-release-observer/gtk-attempt-2/observer-gtk-tests
SHA256 7270d6b725f4d30072aff01fea738942f1e43f0c160b8e79b28a7bb9e7b07f8d
```

在独立free Xvfb显示上执行，不复用当前桌面/他人display。Xvfb需本机已校验prefix：`/tmp/qusor-xvfb-prefix/usr/bin/Xvfb-local`，其server自身LD_LIBRARY_PATH为`/tmp/qusor-xvfb-prefix/usr/lib/x86_64-linux-gnu`，PATH加该prefix/usr/bin。先扫描lock文件和abstract UNIX socket确认free编号，再显式启动；`-displayfd`在此WSLg环境因pathname socket目录不能写而失败，原完整日志已gzip保留，未改全局目录。

测试进程的必要环境与精确参数：

```
DISPLAY=:<owned-free-display>
LD_LIBRARY_PATH=/workspace/qusor/experiments/nsh-reader/target/native-prefix/usr/lib/x86_64-linux-gnu
GSK_RENDERER=cairo
GSETTINGS_BACKEND=memory
GIO_USE_VFS=local
GTK_USE_PORTAL=0
HOME/XDG_CONFIG_HOME/XDG_DATA_HOME/XDG_CACHE_HOME/XDG_RUNTIME_DIR=<new-private-dirs>
/home/user/.cache/qusor-native-release-observer/gtk-attempt-2/observer-gtk-tests retirement_ --test-threads=1 --nocapture
```

GTK test helper使用固定源码已有OnceLock+mpsc专用GTK线程。默认cargo测试线程不直接初始化GTK。实际窗口为2x2合成PNG的GTK测试窗口，未运行阅读业务。末尾必须实际出现3 passed，并核对Xvfb同PID/start后正常终止、wait终态。

原构建/测试driver在`gtk-check-2.py`，会重新编译；独立复跑冻结exe不需调用它。原`gtk4-tests/record.json`记录Cargo命令、Xvfb PID/start、真实exit0和运行时间。`record.json`记录pin/lock/toolchain/prefix/前后patch源hash。源码ZIP仅保存变化文件，以固定baseSHA重建；不声称整个依赖源码均打包。

qdata通知只说明finalization阶段已达到，不证明整个finalizer返回；nativechildren/jobs/decode/cache/texture尚未闭合。snapshot只说明真实append参与，不说明物理屏幕present。不得把这3项通过写成完整释放完成。
