# 发行、自动更新与移动端交付

2026-11-11；关联 C-17、C-15、F-PLAT-01/02、F-DATA-01；承接 [跨平台结果](../../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) 与 [ADR 0033](../architecture/adr/0033-cross-platform-runtime.md)。状态：方向已由用户确认，应用形态与流水线尚未实现；本文的 CI、密钥与发布步骤均**未在远端执行**，不构成已交付。

## 用户决定（2026-11-11）

用户询问自动更新与手机端的实际进度，并建议“通过 GitHub Release 发版本、GitHub Actions 构建”。经逐项选择确认：

1. **更新源**：主仓库 `csic21/qusor` 转为 public，`nsh.json` 用 `"updater": { "github": "csic21/qusor" }`，读取免认证的 `releases/latest/download/`。private 仓库的 Release 资源需要鉴权，NSH updater 不支持，因此仓库可见性是自动更新的前置条件，不是可选项。
2. **应用形态**：先把当前实验原生宿主改造成 `nsh package` 形态（`apps/` 生产入口），一次获得桌面打包、自动更新、OTA 包和 Android/iOS 打包；不再长期维护自建 `.app` 包装。
3. **签名条件**：当前既无 Apple Developer Program 账号，也无 Android 发布 keystore。现阶段只能产出 ad hoc / 调试签名产物：macOS 会被 Gatekeeper 拦截，iOS 只能出模拟器包，Android 只能出调试签名 APK。正式分发资格待账号就绪后另行验收。

2026-11-11 同日执行：公开前隐私检查通过后，仓库已由 private 转为 public（`https://github.com/csic21/qusor`），历史已按下列规则重写。此后 `releases/latest/download/` 可免认证读取，自动更新的更新源前提成立。

## 现状（2026-11-11 核对）

| 能力 | 当前状态 | 证据 |
|---|---|---|
| 更新接线 | **无**。所有 `nsh.json` 没有 `version`/`build`/`updater`；应用没有更新 UI；产物没有 `app.json`/`nsh-updater.json` | `experiments/nsh-reader/*/nsh.json` |
| 版本号 | **无**。`nsh.json` 无 `version`/`build`；仓库无 tag；GitHub 无 Release | `gh release list` 为空 |
| 更新签名密钥 | **无**。没有 `keys/`，未运行 `nsh keygen` | — |
| 打包 | **自建**。`dynamic-pdf/run.py` 生成按 exe 摘要命名的 `QUSOR Preview.app`，`bundle.nshc` 由 `nsh build` 生成，不是 `nsh package` 产物 | `experiments/nsh-reader/dynamic-pdf/run.py` |
| CI | 只有 `foundation.yml`（Core 测试 + 库交叉检查），**没有打包或发布任务**；修复断言后 ubuntu 已通过，windows 仍失败 | run 42965087520 |
| 移动端 | 仅 5 个 target 的**库交叉编译**通过；无 iOS/Android 宿主、无设备运行、无分发 | 同 run 的 `portable-libraries` 全部通过 |

### CI 失败根因（本次核对）

- **ubuntu**：`adapters/extension-store/src/verified_cache_tests.rs` 的断言停在“支持 3 个 Host API”，实际已是 4（未提交工作树为 5）。该文件带 `#![cfg(target_os = "linux")]`，macOS 本机不编译，所以长期没被发现。
- **windows**：`adapters/extension-store/tests/component.rs` 三例在 `Store::open_for_host` 处失败，对应 [跨平台结果](../../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) 已记录的 Windows 私有存储/文件适配缺口。
- 2026-11-11 已修 ubuntu 断言并推送（提交 `12519cd`）：改为预算断言（`<= 8`），并把“不支持版本”改用 `u16::MAX`，不再随 API 增加而失效。本机用临时放宽的 `cfg` 编译并实际跑过该测试；远端 run 42965087520 显示 ubuntu 通过、windows 仍失败。Windows 行为未改。

## 固定 NSH 基线已有能力

固定源码 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`，`nsh 0.4.0`，本机 `nsh` CLI 实测：

| 能力 | 事实 |
|---|---|
| 打包目标 | `nsh package --target`：`ios`（.ipa）、`ios-simulator`（.app）、`android`（.apk/.aab）、`macos`（ad hoc .app）、`linux`（目录 + .tar.gz）、`windows`（目录 + .zip）、`web` |
| 更新源 | `updater.github` 或 `updater.url`；`nsh package` 把配置写成 `nsh-updater.json` 资产 |
| 清单 | `nsh update-manifest` 写 `latest.json` + `latest.json.sig`（Ed25519，`nsh keygen`）；只提供更新版本；GitHub 的 draft/prerelease 不提供 |
| 桌面更新 | 下载、校验 SHA-256、替换、重启（`Application.restart()` 路径） |
| 移动更新 | iOS/Android **不能**自替换二进制，只能返回链接；代码更新走 OTA 签名包（`.nshpkg`，任何原生平台可运行，trial + 回滚） |
| 密钥层级 | OTA 需 `root`（离线）→ `release` 证书 → 包签名；manifest 单独用 `update` 密钥 |
| Rust 应用 | 用 `nsh-updater` crate 自带 HTTP；TSX + 自建 crate 的应用可直接用 `nsh/updater` 模块 |

## 目标形态

新增 `apps/qusor`（生产入口，当前 `apps/` 只有目录约定）：

- `nsh.json`：`id`、`name`、`version`、`build`、`icon`、`theme`、`root`/`rust`、`updater`。
- 入口 `App.tsx` + 现有 `experiments/nsh-reader/native-host` crate（已有 `#[native_module("qusor/pdf-surface")]`）。把 `workbench_main.rs` 里的进程内初始化（数据目录、Core 打开、插件安装/授权、PDF 库路径）迁进 native module 的初始化路径。
- 存储改用 NSH 应用存储（由 `id` 决定目录），不再依赖 `QUSOR_READER_DATA` 之类自定义环境变量。
- 先在本机跑通 `nsh package --target macos` 并实窗操作；再推 linux/windows。

## 流水线设计（GitHub Actions）

- 触发：`push` tag `v*`；`workflow_dispatch` 手动。PR 仍只跑 Core 测试，不打包。
- 矩阵：`macos-latest`（arm64）、`windows-latest`（x86_64）、`ubuntu-latest`（x86_64）。
- 步骤：checkout → 按固定 SHA clone NSH 源码 → 按固定摘要准备 PDFium → `CARGO_PROFILE_RELEASE_STRIP=none nsh package --target <os>` → 归档 → `nsh update-manifest --key` → 创建 draft Release（人工核对后发布）。
- 密钥：`NSH_UPDATE_KEY` 存仓库 secret；`root`/`release` 密钥在启用 OTA 前生成，`root.secret` 离线保存。
- 缓存：NSH 源码、Cargo target、PDFium 工件。私有仓库 Action 分钟数有限，公开后免费。
- 移动端：Android 用 ubuntu runner + NDK + 调试 keystore 出 APK；iOS 用 macOS runner + Xcode 出模拟器 `.app`；设备 `.ipa` 需证书与描述文件。

参考实现见固定 NSH 的 [App Updates](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/website/docs/app-updates.md) 与 [OTA Updates](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/website/docs/ota-updates.md)（其中含 GitHub Actions 示例）。

## 发行门槛

未满足下列条件时不得称某平台“已支持发行”：

| 平台 | 现在能产出 | 仍缺 |
|---|---|---|
| macOS | ad hoc 签名 `.app` | Developer ID 签名 + notarize；否则用户需手动放行 |
| Windows | 目录 / `.zip` | MSVC 构建与实窗；当前 store 测试仍失败 |
| Linux | 目录 / `.tar.gz` | GTK 依赖与实窗复验 |
| Android | 调试签名 APK | NDK、真机运行、渠道/商店资格 |
| iOS | 模拟器 `.app` | Apple 账号、设备 `.ipa`、真机与 TestFlight/商店 |
| Web | 静态站点 | 浏览器宿主、持久存储与真实阅读流程 |

## 本机实测（2026-11-11）

同一固定基线（`nsh 0.4.0` / `913fd76`）：

| 探针 | 结果 |
|---|---|
| 固定源码自带纯 TSX 示例 `examples/gallery` | **通过**：release runner（debug 构建，25 s）与 `Gallery.app` |
| 固定源码自带 Rust 示例 `examples/rust-exports` | **通过**：crate 1 m 13 s、release runner 4 m 42 s、`Rust Exports.app` |
| 工作台（TSX + 自建 crate，`native-workbench`） | 默认 profile 失败，`CARGO_PROFILE_RELEASE_STRIP=none` 后**通过** |

工作台的失败条件：默认 release profile（`strip=debuginfo`）下，`--extern` 指向的 `librquickjs_macro-*.dylib` 本身畸形（`dlopen`：`mis-aligned LINKEDIT string pool`），导致 `quickjs-jit` 报 `can't find crate for 'rquickjs_macro'`；删除该产物、换 `CARGO_TARGET_DIR`、`CARGO_BUILD_JOBS=1`、干净生成 workspace 均复现。设 `CARGO_PROFILE_RELEASE_STRIP=none` 后 crate 3 m 6 s、release runner 7 m 18 s，产出 `QUSOR 工作台研究.app`：`Info.plist`、arm64 可执行文件、`Resources/nsh`，ad hoc 签名（`Identifier=org.qusor.workbenchstudy`）。

同一 macro crate 在独立小工程和上述两个示例里用同样 strip 设置却能正常加载，所以尚未定位 strip 的确切成因（可能与具体依赖图/产物体积有关）。**结论：当前应用形态可以打包；本地与 CI 打包必须设 `CARGO_PROFILE_RELEASE_STRIP=none`，直到上游或工具链侧查清。**

## 发行机制本机实测（2026-11-11）

用固定源码自带的 TSX 示例（复制到 `/tmp`，未修改固定 checkout）走通完整命令链：

```sh
nsh keygen keys/update          # 生成 update.pub / update.secret（Ed25519）
# nsh.json 增加 id、version、build 与 updater.github + publicKey
nsh package --target macos      # 产出 .nsh-updater.json 与 Gallery-1.0.0-macos-aarch64.tar.gz
nsh package --target macos      # 版本升到 1.0.1、build 2，产出新归档
nsh update-manifest dist/*.tar.gz --notes NOTES.md --key keys/update.secret -o dist
```

结果：`latest.json` 含 `id`、`version`、`notes` 与 `platforms.macos-aarch64` 的 `url`/`sha256`/`size`，同时写出 89 字节的 `latest.json.sig`。混入旧版本归档会被告知期望的 `<name>-<version>-<os>-<arch>` 命名，说明清单与版本绑定。

未验证：没有独立校验器复算签名；没有真实应用调用 `checkForUpdate`/`installAndRestart`；没有上传 GitHub Release；`--package`（OTA）路径未测。

## 生产应用首次打包（2026-11-11）

`apps/qusor/` 已按 [ADR 0037](../architecture/adr/0037-production-app-assembly.md) 建立：`provider`（PDFium 渲染库）、`bridges`（三个可信适配器）、`native`（NotesHost/VocabHost 原生模块）三个独立 workspace crate，依赖 NSH 固定 Git revision，随包 `.qpk` 由 `apps/qusor/prepare.py` 生成。

本机实际命令链：

```sh
python3 apps/qusor/prepare.py
cd apps/qusor && CARGO_PROFILE_RELEASE_STRIP=none \
  nsh package --target macos --out /tmp/qusor-app-dist
```

结果：`QUSOR-0.1.0-macos-aarch64.tar.gz`（21.7 MB）、`QUSOR.app`（ad hoc 签名、`org.qusor.app`、arm64、`CFBundleShortVersionString=0.1.0`）、`.nsh-updater.json`（含 `id`/`version`/`build`/`github`/`publicKey`）。随后 `nsh update-manifest` 生成 `latest.json`（`macos-aarch64` 的 `url`/`sha256`/`size`）与 `latest.json.sig`。

启动验证：`open QUSOR.app` 后进程存在，应用数据目录自动创建；发现 notes 与 vocab 曾共用同一路径导致第二个 Host 打不开 Core，已改为 `notes/`、`vocab/` 两个存储。两者仍应合并为同一知识库，属待办。

未验证：Windows/Linux 打包、真实窗口交互与更新 UI、`checkForUpdate`/`installAndRestart` 实际调用、Developer ID 签名与公证、Actions 流水线。

## 未验证范围

- 未创建 tag 或 Release，未运行打包流水线，未生成生产密钥。仓库可见性已于 2026-11-11 转为 public，见上节。
- 上述 CI 失败修复未在远端重跑，Windows 缺口未修。

## 实施顺序

1. 修 Windows store 测试（或显式标记平台跳过并记录缺口），让 CI 恢复绿。
2. 新建 `apps/qusor` 入口，跑通 `nsh package --target macos` 实窗。
3. 写 `version`/`build`/`updater`/图标，本地用 `nsh update-manifest` 与本地 HTTP 自测一次更新流程。
4. 加发版 workflow（先 `workflow_dispatch` 出 draft Release，人工核对后再由 tag 触发）。
5. 仓库转 public 并做首次发布。**首个发布必须已内置 updater 配置**，否则后续版本收不到自动更新。
6. 再推进 Android/iOS 打包与真机资格。

## 关键约束与风险

- **首个包决定后续**：`updater` 与 trusted roots 在打包时写入，之后修改需要新的应用包。
- **转 public 会公开全部历史**：当前仓库 20 个提交、2202 个文件、约 90 MB 包（含证据图片/压缩件）。已扫描跟踪文件未见私钥或令牌，但公开前仍需一次人工确认。
- **macOS 分发**：没有 Developer ID 与 notarize，其他 Mac 默认打不开。
- **移动端更新语义**：iOS/Android 只能提示商店链接或走 OTA 包，不能像桌面那样自替换。
