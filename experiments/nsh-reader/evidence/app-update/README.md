# 应用内自动更新本机记录

日期：2026-11-12。关联 C-17、F-PLAT-01；方案见[发行与更新](../../../../docs/product/release-and-update.md)，设计见[软件更新面板](../../../../docs/design/update-dialog.md)，装配见 [ADR 0037](../../../../docs/architecture/adr/0037-production-app-assembly.md)。

## 这次验证的是什么

生产应用 `apps/qusor` 的自动更新链：检查更新 → 校验并下载 → 替换自身 → 重启进入新版本 → 新版本打开同一应用数据。初始为 0.1.0（build 1），更新源是本机回环 HTTP 服务，清单由本仓库更新私钥签名、公钥随包内置；替换产物为 0.1.1（build 2）。

应用内新增了用户可见入口（侧栏底部“软件更新”与浮层面板），面板行为见设计文档；自测走的是面板按钮背后的同一条 `checkForUpdates`/`installUpdate` 路径，没有另写一套。

## 怎么跑

```sh
python3 apps/qusor/update-selftest.py --install-evidence
```

脚本：从本仓库打包两个版本（`nsh package --target macos`，构建时临时把 `nsh.json` 的更新源指向回环，结束后恢复）、用 `nsh update-manifest` 生成签名清单、把 0.1.0 安装到 `/tmp/qusor-update-selftest/install` 并启动（环境变量 `QUSOR_UPDATE_EVIDENCE` 指向收据目录）、按收据在三个状态截取窗口、最后核对安装后的版本与数据目录。窗口截图用 `screencapture -l <窗口 id>` 只截应用窗口（窗口 id 由 `apps/qusor/window_ids.swift` 列出），不含桌面。

脚本需要写应用数据目录并替换应用自身，因此要在允许本机文件写入的环境运行；它不是 CI 任务。应用数据目录在 macOS 上是 `~/Library/Application Support/org.qusor.app`，脚本开跑前把它移到 `/tmp/qusor-update-selftest/data-before`，测试后保留测试产生的新目录。

## 结果

机器记录见 [record.json](record.json)，17 项断言全部通过，整轮 98.6 秒：

| 范围 | 实际结果 |
|---|---|
| 签名清单 | `latest.json` + `latest.json.sig` 由 `.local/keys/update.secret` 签名；`nsh update-manifest` 签名后按 `nsh.json` 的 `publicKey` 复验通过 |
| 检查更新 | 0.1.0 的应用接受清单，报告 0.1.1、`canInstall = true`、说明文本与归档 URL |
| 下载 | 下载完成，进度 1.0；归档 `QUSOR-0.1.1-macos-aarch64.tar.gz` 的 SHA-256 与大小都与清单一致（NSH 在下载过程中核对） |
| 替换与重启 | 0.1.0 进程正常退出；平台启动 0.1.1；安装后的应用资产为 `version=0.1.1`、`build=2`，id 仍为 `org.qusor.app` |
| 数据保留 | 重启后的构建报告同一 `data_dir`，`VocabHost.decks` 与 `NotesHost.page` 都从同一 Core 读取成功；一次性重启标记被消费；数据目录仍在 |
| 界面 | 三个状态各一张窗口图：发现有更新、下载完成正在安装、重启后的 0.1.1 窗口（侧栏版本号已变） |

组件版本与工件：

| 工件 | 值 |
|---|---|
| 0.1.0 归档 SHA-256 | `d3594ea6d78ed8a4bc8a203b08ba4eed1f5cddf31ed08afaaf362df5875b96a7` |
| 0.1.1 归档 SHA-256 | `07742ec32560d0241c68ff4d522ad98678a77a3f5f83aab7f913f43ffc53eaf7` |
| 清单 SHA-256 | `63c556e41ea526e6d4dc5552c944500c8addd17021a46968e244d04c556987d2` |
| 清单签名 | `915aWClHX72Pctx5HyIBDzQ3/WYXJKqUK9hfMS6q5SfQ4YaWIUEWeXxfEWPVBGUnwBhXLP5vktc0Cd7x7P/wAw==` |
| 源码约束 | 两个归档都由同一工作树打包，只有临时 `nsh.json` 的 `version`/`build`/`updater` 不同；`nsh.json` 的 SHA-256 见 record |

截图：

| 文件 | 内容 |
|---|---|
| [available.png](available.png) | 面板显示“发现新版本 0.1.1”、版本说明、实际回环更新源与“下载并安装”；侧栏行尾版本为 0.1.0 |
| [installing.png](installing.png) | “下载完成，正在安装并重启…”，进度条满，“正在安装…”禁用状态 |
| [restarted.png](restarted.png) | 重启后的 0.1.1 窗口：侧栏“软件更新”行尾已是 0.1.1，面板不在前台 |

## 未覆盖

- 没有从 GitHub Releases 实际拉取清单（真机 GitHub 源）；回环服务与 Releases 的差别只在 URL，但这一条仍是未验证范围。
- 没有独立签名校验器复算清单签名，也没有验证签名错误、应用 id 不符、旧版本清单、篡改归档这些拒绝路径；本机只走了接受路径，拒绝路径由 NSH updater 自身的错误分支承担，未在本轮注入。
- 没有 Developer ID 签名与公证；产物是 ad hoc 签名，其他 Mac 默认打不开。
- 没有 Windows/Linux 的替换路径、移动端“只能跳转商店/OTA”的语义、下载中断与磁盘不足的恢复。
- 没有验证面板关闭后的键盘路径、辅助技术朗读；没有验证在只读安装位置（`canInstall = false`）时的下载页跳转。
- 没有验证非空知识库在更新后逐条可见；本轮 Core 由应用首次启动新建，重启后的构建只证明同一 Core 可打开、两个宿主读取成功。应用数据目录此前的内容已移走保留在 `/tmp/qusor-update-selftest/data-before`。
- 证据目录没有独立复跑记录；[record.json](record.json) 是本机同一次运行的机器记录。
