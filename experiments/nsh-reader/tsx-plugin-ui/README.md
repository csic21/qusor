# TSX 插件原生 UI：通路资格研究

日期：2026-11-08。关联 NSH P03/P07/P08、EXT-B01/B03/B05、EXT-P05/P06/P08 与 C-09/C-12，Feature F-EXT-01/03/04/05/06、F-UX-01/02。状态：源码调研及仓库外作者构建完成；原生挂载/安全隔离未执行。报告见 [研究09](../../../docs/research/09-tsx-plugin-native-ui.md)。本目录不是生产插件或新增 Cargo Workspace。

## 假设与通过门槛

假设：普通第三方作者能使用公开 SDK 构建自己的 TSX UI，宿主在精确授权区域内以独立运行边界加载源码，保持原生渲染、权限不扩大和可靠替换；无需网页 runtime 或直接依赖内部源码。

已有 [0.3想法输入契约](../../../docs/product/component-provider-contract.md) 是业务能力来源，本实验不重定义草稿/保存权限，不以“代码中出现0.3”代替普通消费者验收。完整TSX profile 尚未冻结，不能直接把JS作为现有Wasm entrypoint。

## 固定对象与当前实测

- 最新源码审查固定 `893fb84db625045f36363ae9de1f999f588d25fd`；发布/近期提交及文件URL见 [上游记录](evidence/upstream-review.json)。没有升级已有checkout。
- 本地现有 CLI `nsh 0.4.0 (913fd76)`；二进制与 fixture 的 SHA-256、输出与退出状态见 [构建证据](evidence/linux-build.json)。版本字符串不是最新提交运行证据。
- Linux / WSL2，本轮使用 CLI 构建，不依赖 display、原生控件或输入法；Rust 工具链沿仓库1.97.1，但本轮没有重新编译 Rust。后续原生实验必须另记具体 lockfile/设备/DPI/字体/输入法/构建配置。
- fixture [App.tsx](fixtures/App.tsx)、[nsh.json](fixtures/nsh.json) 为本项目自拟，沿仓库 MIT；无真实账号、用户内容或网络调用。

```sh
python3 experiments/nsh-reader/tsx-plugin-ui/probe_build.py --output experiments/nsh-reader/tsx-plugin-ui/evidence/linux-build.json
```

脚本只复制文件到临时仓库外目录、构建并记录hash，结束清理临时目录；不安装、不启动、不加载字节码、不调用宿主桥。不需要新Python/Node/Cargo依赖。生成 `.nshc` 是 CLI 的默认输出，不表示同意加载发布者提供的字节码。

## 步骤、结果和剩余验证

| ID | 步骤/通过条件 | 当前实际与证据 |
|---|---|---|
| UI-T01 | 仓库外作者构建TSX，无内部源码依赖 | 已通过构建；JS、bytecode、map、app及能力文件齐备；不计完整类型检查 |
| UI-T02 | 当前QUSOR包绑定UI源码/资源、版本/贡献角色、普通安装身份 | 未执行；现有包profile限定Wasm入口，需要版本化UI运行契约 |
| UI-T03 | 外壳槽位同时挂载A/B两个独立VM，分别原生显示和收发事件 | 未执行；先用公开自拟样例，不能把宿主自带TSX视为插件自带UI |
| UI-T04 | 同名app id、不同安装/库/草稿的通道与存储隔离 | 未执行；宿主身份注入、新通道及独立数据根，旧事件不流入后继 |
| UI-T05 | 默认拒绝网络/文件/进程/剪贴板；伪造host方法/资源/模块/组件请求失败 | 未执行；NSH能力与Host授权双重核对，反例验证 |
| UI-T06 | 无限循环、caught OOM、节点/原生资源增长、输出洪泛有界且不拖垮另一实例 | 未执行；单次/持续预算和全局资源限额分别验证，记录内存样本与运行总量 |
| UI-T07 | 关闭/替换/撤权时旧epoch调用和迟到结果拒绝，原生实际释放后才退quota | 未执行；不能用unmount/cleanup代替释放ack |
| UI-T08 | 插件不能关闭/改写宿主窗口、弹窗覆盖权限层或偷取其他slot焦点 | 未执行；重点复核Linux窗口命令及各端不同实现；必要时补宿主拦截或上游接口 |
| UI-T09 | 中文IME/emoji/换行/键盘/触控/读屏和窄窗，不丢草稿、不误发送 | 未执行；原生截图和事件证据分别保存，不能靠bundle成功判定 |
| UI-T10 | 两个普通提供者编辑同一草稿，保存与Core重开/空库恢复正确 | 未执行；复用公开草稿版本与冻结保存契约 |
| UI-T11 | macOS/Windows/iOS/Android构建、挂载与相同反例 | 未执行；移动分发另验渠道，不把Linux成绩外推 |

## 装配约束与失败备选

1. 首个挂载实验采用受信Slot原生包装 + 独立 AppView/VM + 专用 HostChannel，显式最小能力和限额；不直接import第三方UI进宿主VM。宿主外壳、可信权限和停止界面不由插件绘制。
2. 尝试两个独立提供者时复用一个真实Host和草稿应用服务；不各建私有Host、复制Core存储或塞入第一方包特判。实例ID和安装digest/generation绑定，停止与任务寿命分别处理。
3. 先用源码。原生组件必须受控、逐次判权，不能把注册的终端或PDF组件当作自动受到NSH权限限制。
4. 若同进程VM无法满足资源或安全隔离门槛，停止其生产集成，研究受监督JS执行与有界UI操作桥，宿主继续原生渲染；不能把native指针跨进程或换成隐藏WebView掩盖失败。
5. 暂时不可安全运行任意TSX时，继续验证现有有界组合资产提供者，不将其宣称为完整TSX插件生态。新依赖/成员按架构策略登记，普通SDK/包/平台成绩分别留证。

本轮实际检查另包括Python AST解析、fixture JSON解析、本地文档链接和差异空白检查；没有真实UI、安全测试或SDK发布成绩。
