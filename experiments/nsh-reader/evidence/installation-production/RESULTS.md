# C7：安装持久层迁移后的原生消费者复验

日期：2026-11-08。关联 C-08/C-09、F-EXT-01/03/05/06、F-READ-01、F-NOTE-01、F-SRC-01 与 F-DATA-01。沿用 [C6 普通包/原生 Surface](../../plugin-surface/RESULTS.md) 的同一公开协议与用户动作，在安装 SQL 唯一权威源迁入 `extension-store` 后重新构建、实际交互和恢复；不以旧二进制或截图代替验证。

## 最终通过的实际链路

- `prepare_plugin_surface.py` 全流程 exit0：锁定 NSH/native profile、实际 native host 链接、仓库外 SDK build/clippy、普通签名包与 bundle 重建。当前源记录100份；四份受控 registry 基线（固定 NSH、PDF provider、根、package实验）无未知 name/version/source/checksum 漂移，未删除守卫。最终 binary 中可见生产 `extension_store::Store` 安装/读取路径，实验 Store 仅消费该实现。
- 新 native 定向测试3/3通过，包含四类真实签名坏 guest（trap/timeout/错误final/重复Open）、完整单段/复合草案转义、审批重试/修改note拒绝/独立操作/撤权后拒绝。native all-target Clippy `-D warnings` 和 Python 编译通过。
- 使用与C6摘要相同的隔离 Xvfb 工具，真实 GTK/XTest pointer 与键盘输入：`QUSOR` + `native note one`，`native` + `native note two`，跨行 `QUSOR native PDF qualification\nRepeat` + `native compound note`。第三条为 protocol2 两段，一次 Core compound 提交；三条 capture/operation各自独立。正常关闭 exit0 后，通过重建的 Core CLI 独立查询并断言引文、note和段数。
- 对本轮全新数据完整 export-all → 空库 restore-all，恢复1原件/3摘录；三条完整 record与恢复前逐值相同。原件 SHA与固定许可 PDF fixture相同，三个精确解析的 anchor/draft逐项相等。
- 最终源/工件/日志/截图指纹见 [机器记录](after-compile-fix/qualification-fingerprints.json)，[交互与查询](after-compile-fix/plugin-surface-interaction.json)、[完整恢复](after-compile-fix/plugin-surface-recovery.json)、[跨行保存截图](after-compile-fix/plugin-surface-saved-3.png)、[检查命令](after-compile-fix/checks.json) 分别留存。最终取证前后100份源指纹未变，NSH上游 tracked source保持 clean。

最终 native binary SHA256 `1fdf5ded1b069b5a558e16ad823be6afeb0e70e378f5152a1f1bf6aae0b82130`。普通包依旧为 `895b6b116c114dba2f12a67ae6e07b38f37fc99f152e070d86964ed65fcf0dff`：公开协议/guest没有被替换成私有调用，变更发生在实际宿主持久实现。最终 ELF [ldd](after-compile-fix/native-ldd.log) 全部库可解析，未链接 WebKit/JavaScriptCore/libsoup。

## 历史保留与重跑

C6的原有证据与RESULTS未覆盖；其5个实际工件在重建前逐一核对原SHA并复制到 ignored `target/c6-frozen-artifacts`，位置见 [保存映射](c6-artifact-preservation.json)。旧native/helper锁也留存。C7第一轮已真实通过；随后仅为非Unix unused-variable编译修正增加cfg声明，Linux行为不变。第一轮交互/恢复/截图仍在本目录，工件保存在 ignored `target/c7-initial-artifacts`；[首轮指纹](initial-success-fingerprints.json) 将它与补丁后的最终重跑分开。

最终重跑采用全新 `target/installation-production-native-final-data` 和 `target/installation-production-restored-final-core`，对应 `after-compile-fix/` 证据，没有复用首轮Core库。启动Xvfb的精确命令、原/临时工具摘要沿用 [启动记录](xvfb-start.json)；本轮所有UI正常退出，Xvfb已停止。最终prepare日志与5条后续命令均exit0。

## 范围

这是同一个原生资格消费者对生产持久适配器的实际复验。它仍由可信实验装配显式授权，不交付完整信任/授权UI、正式library/reader/notes插件或生产长驻supervisor。进程身份adapter的独立真实进程测试与本轮UI是不同证据层级；本轮Wasm仍不能据此宣称完整进程监督装配。

键盘层级是虚拟X11显示中的真实GTK/XTest路径，不证明当前桌面物理键盘、中文IME、触控、无障碍、复杂PDF/EPUB或其他平台通过。已挂载NSH view缺少release acknowledgement，cleanup继续保留至实际进程退出，不以删除映射冒充释放。原始文件未上传。
