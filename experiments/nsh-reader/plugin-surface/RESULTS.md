# 普通包消费 NSH Native PDF Surface：Linux 本机资格结果

2026-11-08；关联 F-EXT-01、F-READ-01、C-09、ADR 0023。本次验证的是公开接口、普通独立签名包与可信原生消费者的接线资格，不是正式阅读器、生产安装器或生产 NSH adapter。

## 实际通过

- 仓库外导出 SDK、离线独立 build/clippy、普通 0.2 签名包：没有 Host、NSH、SQLite、runtime 私有依赖或仓库源码路径。manifest required 仅 `reader.surface`；`selection.read`/`capture.propose` 为 optional，由测试装配显式授予。没有 packageId 特权分支。SDK 源码与三份 schema 指纹见 [author-proof](../evidence/plugin-surface/author-proof.json)。
- 实验 Store 持久禁用安装、取得重验 frozen 工件；Host 0.2 session 执行实际签名 Wasm。origin installation_id 来自持久安装回执。该 Store 仅作资格安装器；不代表生产安装/持续授权完成。
- 同一不透明 handle：guest Open/Configure → Host registry → NSH Image/Canvas → 原生私有 frame/input/gesture ticket → native selection → guest ReadSelection →完整草案 CapturePropose →候选。guest 无 Ready、输入或坐标权限。generation/frame_id 与 handle 一起绑定 node key 和迟到回调。
- 最终工件在独立 Xvfb X11 的真实 GTK 窗口，用 XTest pointer 拖选、XTest 键盘经 GDK/GTK Input onChange 输入个人想法，再点击原生 Save。第一条 `QUSOR`/`native note one`，第二条 `native`/`native note two`，第三条跨行 `QUSOR native PDF qualification\nRepeat`/`native compound note`。第三条经同一普通 guest 完整 protocol2、两段来源、一次 Core compound 提交。候选阶段不写入，saved 仅来自 Core durable receipt。
- 三条 capture_id、Core operation_id 各自独立；Core CLI 在 UI 进程正常 exit 0 后逐条重新打开数据库查询，断言引文、笔记和 compound 两段完整。见 [交互与查询](../evidence/plugin-surface/plugin-surface-interaction.json)、[跨行已保存截图](../evidence/plugin-surface/plugin-surface-saved-3.png)。
- 完整 export-all →空目标 restore-all：恢复 1 原件/3 摘录，三条完整 record（含 provenance、note、receipt）与恢复前完全相同；原件 SHA 与许可 fixture 相同，三条精确 resolve 的 anchor/draft 均与保存值相同。见 [恢复证据](../evidence/plugin-surface/plugin-surface-recovery.json)。不依赖插件运行或 NSH 对象读取已收录知识。
- 新增 3 个 Rust 定向测试：真实签名 guest 的完整 quote/backslash/newline/control/emoji 转义和非法 surrogate/escape 拒绝；实际 Open 后 trap/timeout/错误 final/重复 Open 均拒绝且清理从未挂载的 Open；原生审批覆盖第二 pointer 不抹旧手势、同 note 重试同 receipt、改 note 拒绝、两选区两 Core op、撤权后拒绝保存。3/3 通过；native all-target Clippy `-D warnings`、Rust 特定文件格式检查及 Python 编译通过。见 [测试](../evidence/plugin-surface/native-tests.log)、[Clippy](../evidence/plugin-surface/native-clippy.log)。这些定向测试与实窗交互是不同证据层级。

## 构建与复现

```sh
python3 experiments/nsh-reader/native-host/prepare_plugin_surface.py
# 在支持键盘焦点的 X11 display；data 必须不存在：
DISPLAY=:98 python3 experiments/nsh-reader/native-host/qualify_plugin_surface.py \
  --data experiments/nsh-reader/target/plugin-surface-asserted-data \
  --evidence experiments/nsh-reader/evidence/plugin-surface
python3 experiments/nsh-reader/native-host/recover_plugin_surface.py \
  --interaction experiments/nsh-reader/evidence/plugin-surface/plugin-surface-interaction.json \
  --restored experiments/nsh-reader/target/plugin-surface-recovered-final-core \
  --evidence experiments/nsh-reader/evidence/plugin-surface/plugin-surface-recovery.json
```

prepare 使用固定 NSH 源码与原锁，关闭 WebView feature，检查 active graph；没有 WebKit/DOM/Web 服务。受控 registry 基线明确为固定 NSH、PDF provider、根 Core、实验 package 四份锁；按 name/version/source/checksum 拒绝未知漂移。首次确实拦截了实验 Store 引入的 rustix1.1.4；保留 [拒绝日志](../evidence/linux-plugin-surface-lock-rejection.log) 与 [明确加入记录](../evidence/linux-plugin-surface-baseline-addition.json)，没有删守卫。新 helper 从已检查的旧 PDF helper lock 播种，避免 CLI 无锁选取新版本。最终 NSH tracked source clean、host/helper drift 均为空。最终 ELF 的真实 [ldd](../evidence/plugin-surface/native-ldd.log) exit0，全部库可解析，未链接 WebKit/JavaScriptCore/libsoup。

最终 native bin SHA256 `117d3deba54a3c4a9cbaefa6137e3b0f7e5827548eff6923c6defb2a174c506f`；bundle `fb964eb9918c66ca48c5b208089a328f4f203c8196dcfadaca15433b059eb532`；普通签名包 `895b6b116c114dba2f12a67ae6e07b38f37fc99f152e070d86964ed65fcf0dff`。最终截图、交互与恢复对应这些工件；完整源码/SDK/四份基线/工件摘要见 [指纹](../evidence/plugin-surface/qualification-fingerprints.json)。原0.1 C3保留，兼容旧链回归由独立结果记录，不能用旧截图证明此工件。

## 限制与失败证据

当前桌面 XWayland 的 pointer 有效，但键盘 focus 被拒绝（XGetInputFocus=0），未计键盘通过。随后使用临时目录解包的官方 Xvfb deb，在独立虚拟 X11 display 跑真实 GTK/XTest 键盘路径；未修改 NSH 上游或安装系统包。该临时 Xvfb 可执行文件只把一处固定 `/usr/bin` 改为等长 `/tmp/bin`，用于找到同样解包的官方 xkbcomp；deb SHA、原/临时可执行 SHA 与完整命令均在指纹中。它不证明物理键盘、当前桌面焦点、中文 IME、辅助功能或移动平台通过。初期 Each/Input 导入位置与 nested module 描述错误、锁漂移和恢复 helper 错把 document_id 当 capture_id 的失败均保留，修正后重新验证。

Close 只进入 Closing、取消 native 输入和候选。已挂载 NSH view 没有 release acknowledgement，因此 pending cleanup 保留至实际进程退出；没有将删除 map 宣称为原生资源已释放。仅 VM 失败的新 Open 从未返回 UI handle、没有创建 mount/job，可以当场确认清理。跨线程 nativeMutex 和 raw responder 约束保留。缺少正式库/文件选择/历史摘录列表、崩溃后的长驻恢复、生产安装器、全平台及复杂 PDF 布局资格。所有输入固定为真实许可本地 fixture，原始文件未上传。
