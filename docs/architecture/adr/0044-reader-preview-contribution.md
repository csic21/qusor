# ADR 0044：阅读预览包的 Manifest 工作区贡献

日期：2026-11-12

状态：本机资格切片；不是生产 Reader、PDF/EPUB 解析、Core 阅读数据或用户包安装。

## 背景

现有阅读界面只是 shell 内的 `fixture.reading`，不经过普通包目录、Manifest 贡献或 profile 依赖。这样无法验证 `reader_workspace` 的真实发现链路，也会让阅读继续停留在旁路样例。

## 决定

1. 增加普通签名包 `plugins/reader`，package ID 为 `org.qusor.reader.preview`，Manifest 贡献 `org.qusor.reader.preview.workspace`，kind 为 `reader_workspace`。
2. 应用 profile 登记原生 preview provider `app.qusor.reader.preview`，提供 `reader.surface` 0.2；workspace consumer 显式依赖该服务。没有该 provider 时目录不会显示阅读入口。
3. 包入口当前是空 Wasm 模块，应用不执行它，也不授予 Core 读取/写入；原生 renderer 仍是窗口内阅读 fixture，界面明确标注“窗口内样例”。
4. 阅读入口与 notes/vocab 共用 Manifest 目录、启用意图和工作区移除逻辑。它不替代正式 reader/library 纵切，也不证明插件自带 TSX UI 已运行。

## 后果

- 目录现在有三个随应用包贡献：想法、背单词、阅读预览。
- 阅读预览可被停用/启用；停用不涉及 Core 数据，因为当前没有真实阅读写入。
- 后续正式 reader 包必须替换 preview 包，接入真实解析、Surface、Core 位置/摘录和存储；不能把本 ADR 的 fixture 当作生产阅读交付。

## 验证

- `prepare.py` 通过导出 SDK 在仓库外构建并验签 reader 包；package catalog、profile 依赖和工作区目录测试通过。
- 应用 12 项测试、`nsh check App.tsx`、架构守卫与原生 UI 边界通过。
