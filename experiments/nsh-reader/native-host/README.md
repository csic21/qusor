# 原生 Rust+TSX 桥接探针

关联 P02/P03、F-PLAT-01 和 C-09。此独立 Workspace 适配固定 NSH 的 `rust-exports` 示例，注册真实 Rust module/class/component 与 GTK Meter；显式关闭 `nsh-linux-host` 的可选 WebView 默认 feature。示例没有 WebView 功能，未用占位框代替阅读。它不是正式 QUSOR UI 或插件 SDK。

前置：按 [上级说明](../README.md) 准备固定 `target/nsh`、锁定 Cargo 缓存和已校验的 `target/native-prefix`。源码固定 SHA 不得改变，全部原生链接库必须可解析。根生产 Workspace 不引入 NSH。

从仓库根目录运行：

```sh
python3 experiments/nsh-reader/native-host/prepare.py --output experiments/nsh-reader/target/native-probe-preparation.json
python3 experiments/nsh-reader/native-host/run.py
```

prepare 检查源码与原始 lock、独立依赖规则和 Linux active feature graph，拒绝 WebKit/JavaScriptCore/soup 或 webview feature，再离线构建真实 CLI、原生 host 与 bundle。原生 host 使用已提交 lock；上游 CLI 生成 helper 时若没有 lock，预先复制固定上游 lock，保留被 yanked 的已锁版本。后续PDF Surface共用此Workspace时，衍生lock的允许基线为固定NSH原锁、已资格PDF provider锁、根Core锁与实验package锁的明确受控并集，记录基线摘要；禁止并集以外的registry版本/来源/checksum漂移；并未修改上游源文件。

run 使用独立实验 data-dir 和本地 prefix，加载本地 bytecode，无 dev server。窗口尺寸为请求值，窗口管理器可能调整。示例没有持久保存账户余额，重开回到初始数据是示例行为，不是业务恢复通过。

需要采集证据时，先用 `xwininfo`/`xprop` 查明该进程的可见 client window 和 PID，再运行 `capture_window.py --window ID --pid PID --output PATH`；只捕获身份匹配的窗口，不抓桌面。`close_window.py --window ID --pid PID` 发送正常 WM_DELETE_WINDOW；仍须等待子进程退出才能计作正常关闭。

[本机证据](../evidence/linux-native-window.json) 及三张截图记录构建/链接、可见原生窗口、实际 Deposit 按钮经 Rust method/listener 更新画面、正常关闭与重开。GTK/Mesa 警告保留；没有打包安装、中文输入、完整异步 bridge、PDF/EPUB 或阅读 UX 的通过结论。


PDF Surface 使用同一隔离Workspace的另一个bin和纯数据原生模块，复现与当前证据见 [PDF Surface](../pdf-surface/README.md)。原先 ledger 的 JSON/截图保留为当时证据；新增PDF代码和依赖后，旧artifact hash不表示当前源码仍相同。原有ledger入口仍可构建，但它不验证PDF阅读或知识持久化。

普通签名 Wasm 的公开 Surface 消费与原生批准保存探针见 [plugin-surface](../plugin-surface/README.md)。
