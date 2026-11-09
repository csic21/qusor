# macOS 原生阅读本机资格

2026-11-09，macOS 27.0 / Apple Silicon，Xcode 27.2（27B52），Rust 1.97.1。NSH 固定 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`；主分支与发布核对见 [六端结果](../../CROSS_PLATFORM_RESULTS.md)。

## 范围

本机真实 AppKit 窗口，使用既有 TSX 阅读界面、真实 Core、仓库外公开 SDK 构建的普通签名阅读包与已有普通签名编辑 A 包。没有把第三方身份换成内置特权实现。

选择的 `runtime-three-pages.pdf` 为构建 bundle 后生成的三页合成文件，原件 SHA-256 为 `44e845ecc1c21414526b08a97f83ae33dfb02a358c7af6d6b821bfedbd905a1b`。原生 chooser、翻至第三页、拖选 `A runtime selected local document`、进入原生想法输入框并保存都由本次会话的 CUA 实际操作。中文与 emoji 通过原生输入框粘贴验证；不据此宣称中文输入法组合、取消或提交通过。

Core 退出重开查询、完整导出、恢复到新空库、逐条精确定位与原件字节比较见 [恢复记录](core-recovery.json)。文档版本、选区与非空中文想法均由真实 Core 返回，不以 UI 的“已保存”文案单独证明落库。应用新启动仍显示初始页，未实现自动阅读位置恢复或想法列表。

## 工件与检查

最终工件、锁摘要和每项实际状态保存在 [资格记录](qualification.json)。共享库的五个交叉目标结果见 [编译记录](portable-core.json)，不覆盖目标应用、UI 或持久化适配器。iOS arm64 Simulator 的 NSH 原生桥接 library 也经过 `cargo check`，没有启动 UIKit 应用。

根工作区 218 项 Rust 测试与 Clippy 通过；边界守卫 42 项通过。macOS 原生桥接 library 的 30 项测试在默认并行测试模式下通过。额外覆盖冷启动重新选文档：原先固定授权操作 ID 会与不同导入输入冲突，现改为每次可信打开独有的有界操作 ID；回归同时确认被动恢复不能改权限范围、已有授权期限和能力不增加、禁用状态不会自动恢复。

四个桌面实验入口均在 Mac 编译链接，动态阅读的已验收二进制摘要保持不变。真实进程控制另通过 5 类场景，新增父进程跳过析构退出，子进程已消费 stdin 后进入 5 秒测试睡眠；观察继承管道在约 30 ms 结束，验证实际父死终止，而非下次 stdin 读取触发退出。没有依据此测量宣称普遍的最大延迟或 OS 沙箱资格。

同步修复了实验测试中的 Linux 路径假设（provider binary/动态库、`true` 位置、临时目录规范化）；共享编码容器的生命周期断言改为具体 frame owner，避免并行测试互相污染全局计数。运行时计数与原生 ACK 语义没有因此放宽。

## 关闭与限制

正常关闭核对拥有的子进程和 Rust 退休服务，不使用恢复后的裸 PID 发信号。NSH 原生图像缓存/挂载尚无释放 ACK；日志明确保留 `native_release_ack:false`、`allocation_may_remain_in_native_png_cache:true`。Rust 编码容器归零和进程退出不等于原生缓存单独释放验证。

当前预算仍为 16 MiB / 16 页，未验收长书、EPUB、原生 B 替换、正式安装包或其他 CPU 的 Mac。历史失败尝试也记录在资格记录中；未把一次成功覆盖成所有尝试成功。

GUI 图像保留在本次会话的 CUA 输出；本目录保存动作/结果、工件和 Core 数据证据，未声称存在本目录 PNG 截图。复现构建与启动见 [跨平台入口](../../CROSS_PLATFORM_RESULTS.md)。
