# 原生宿主与插件工作区组合 · 本机记录

2026-11-09；关联 F-UX-01/02、F-EXT-01/03/04/06、M-1。源码与复现见 [native-workbench](../../native-workbench/README.md)，设计约束见[外壳布局](../../../../docs/design/shell-layout.md)。

状态：macOS 独立原生样例通过宽窗与紧凑窗组合检查；仍是窗口内 TSX fixture，不是已安装普通插件、正式阅读器或 Core 闭环。

## 上游与环境

- 2026-11-09 核对[官方 releases](https://github.com/longbridge/nsh/releases)：最新 release 为 `v0.4.0`，tag 对象 `06771fff7150e9ae115a0edde2ab764ec4e8454d`，解引用提交 `e626a8dd8f8d8b277b9507cfd5ac74d0e67020f3`。
- 同日核对 [`main`](https://github.com/longbridge/nsh/commit/64ab8a1dad8c419f6c34768d938049619b080f5b) 为 `64ab8a1dad8c419f6c34768d938049619b080f5b`；最新提交为 List 尾部追加偏移优化，没有修改本轮使用的 TSX/desktop 组合接口。
- 已核对固定提交中的 [README](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/README.md) 与当前 [Design Guides](https://github.com/longbridge/nsh/blob/64ab8a1dad8c419f6c34768d938049619b080f5b/skills/nsh-design-guides/references/design-guides.md)。本轮继续固定 checkout `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`，未升级运行时。
- macOS 27.0 / 26A428，arm64，AppKit 原生宿主。实际 native-host SHA-256 为 `42b60abbdf6a332757af0ce94dec578ce11b1388978ab0d19cbd068f40d3edc9`。
- 固定 checkout 的 `Cargo.lock` SHA-256 为 `3495ffc4a1fd150bb583e069afcb5b4f2d389dbfb5f8b1c942279eaf9e80414b`。

## 实际结果

| 场景 | 观察 | 结果 |
|---|---|---|
| 宽窗进入 | 1280×820 从“工作区样例”打开阅读后，宿主侧栏、内容列表、对象标签退出；宿主返回、当前身份和更多菜单保留 | 通过 |
| 插件自有书库 | 阅读贡献显示本地导航、格式计数、搜索、网格/列表、书库卡片和禁用导入样例 | 通过 |
| 搜索 | 输入“插件”后从 4 份资料过滤为 1 份，清空后恢复 4 份 | 通过 |
| 格式筛选 | 点击 PDF 后显示 2 份 PDF；再次点击可清除筛选 | 通过 |
| 插件内阅读 | 打开 PDF 样例进入正文；插件内返回书架、窗口内书签状态和禁用的更多工具符合各自状态 | 通过 |
| 返回来源 | 先从画布进入阅读贡献，再点击宿主返回；窗口回到此前画布标签及内容 | 通过 |
| 紧凑书架 | 900×680 下宿主通用侧栏不出现；阅读贡献显示书库导航入口、搜索和四份资料 | 通过 |
| 紧凑导航 | 点击书库导航入口后，贡献导航替代书架内容；贡献内返回恢复搜索与书架 | 通过 |
| 紧凑正文 | 打开资料后只保留阅读 chrome 与正文；宿主返回仍可离开贡献 | 通过 |

可访问状态观察到宿主返回标签为“返回之前的 QUSOR 工作区”，阅读容器为“阅读插件工作区”，紧凑窗口包含“显示书库导航”，禁用项包含“导入资料（样例未接入）”。截图通过本机 CUA 现场读取；本记录不以截图替代业务数据、权限或恢复证据。

## 未覆盖

- `PluginWorkspace` 只是实验 TSX 类型；没有签名包安装、SDK 发行、跨进程隔离、授权或组件替换资格。
- 书库数据、搜索、筛选、书签和阅读位置全部是窗口内 fixture；没有文件解析、Core 保存、重开恢复或导出。
- 未验证键盘完整路径、读屏、输入法、动态字号、深色主题的新阅读控件、连续拖动缩放及资源释放 ACK。
- 未运行 Windows、Linux、iOS、Android 或 Web；macOS 实窗不能替代六端资格。

## 复现

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820
python3 experiments/nsh-reader/native-workbench/prepare.py --size 900x680
```

两个命令分别构建并启动对应窗口配置。工件收据写入 `experiments/nsh-reader/target/native-workbench/artifact.json`。
