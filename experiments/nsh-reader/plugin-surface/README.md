# 普通 Wasm 包消费原生 PDF Surface

关联 F-EXT-01、F-READ-01、C-09 和 ADR 0023；这是独立 Workspace 资格探针。旧 `pdf-surface` 的 0.1 C3 证据保持独立。

```sh
python3 experiments/nsh-reader/native-host/prepare_plugin_surface.py
python3 experiments/nsh-reader/native-host/run_plugin_surface.py
```

prepare 从固定许可 fixture 生成本地 native bitmap/中性快照，在仓库外从公开 SDK 构建并签名普通 0.2 包。实验 Store 持久禁用安装、重验 frozen 工件；可信 native 测试装配显式授予权限，再通过 Host session 执行已签名 Wasm。它不代表生产安装器完成。

Wasm 只调用本次 invocation 参数、Open/Configure/ReadSelection/Close 和 CapturePropose。原生票据、图像 ready、pointer、坐标和手势序列不进入 guest 协议。Host registry 校验 owner/session/epoch/source/handle/generation/lease；NSH Image/Canvas 的节点和迟到回调同时绑定 handle/generation/frame_id。输入使用 native raw responder；无 DOM、WebView 或 Web 服务。

真实拖选后，guest 解码完整草案并提出候选；此时没有知识写入。点击原生“保存摘录”才在新鲜权限/选区/source 检查后调用 Core application，SQLite 保存完整来源和不可变快照并返回持久回执。一次保存意图使用独立 operation ID，同意图重试复用；不同候选不得复用固定 guest RPC ID。原始 PDF 不上传。

Close 进入 Closing；本探针缺少 NSH mount 释放回执，已挂载 Surface 的 cleanup 保留到实际进程退出，不把删除 Rust map 当作释放证明。仅失败 VM 的新 Open 在从未返回 UI handle、未创建 mount/job 时可以确认清理。

测试读取 prepare 生成的实际签名包；覆盖完整 JSON 转义、非法转义拒绝，以及已执行 Open 后 guest trap/超时的清理。实屏通过与未验证范围以本机证据为准，源码和构建通过不等于移动平台、完整阅读 UX 或生产 SDK 通过。

本轮实际结果、复现层级、失败与限制见 [RESULTS](RESULTS.md)。
