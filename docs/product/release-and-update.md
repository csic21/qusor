# 发行、自动更新与移动端交付

2026-11-12；关联 C-17、C-15、F-PLAT-01/02、F-DATA-01；承接 [跨平台结果](../../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) 与 [ADR 0033](../architecture/adr/0033-cross-platform-runtime.md)。状态：生产应用、本机自动更新、GitHub Actions 的 macOS draft 构建以及 Android debug APK/模拟器启动已验证；draft 尚未公开为 latest，Developer ID/公证、Android 商店签名与其它平台发行仍未完成。

## 用户决定（2026-11-11）

用户询问自动更新与手机端的实际进度，并建议“通过 GitHub Release 发版本、GitHub Actions 构建”。经逐项选择确认：

1. **更新源**：主仓库 `csic21/qusor` 转为 public，`nsh.json` 用 `"updater": { "github": "csic21/qusor" }`，读取免认证的 `releases/latest/download/`。private 仓库的 Release 资源需要鉴权，NSH updater 不支持，因此仓库可见性是自动更新的前置条件，不是可选项。
2. **应用形态**：先把当前实验原生宿主改造成 `nsh package` 形态（`apps/` 生产入口），一次获得桌面打包、自动更新、OTA 包和 Android/iOS 打包；不再长期维护自建 `.app` 包装。
3. **签名条件**：当前既无 Apple Developer Program 账号，也无 Android 发布 keystore。现阶段只能产出 ad hoc / 调试签名产物：macOS 会被 Gatekeeper 拦截，iOS 只能出模拟器包，Android 只能出调试签名 APK。正式分发资格待账号就绪后另行验收。

2026-11-11 同日执行：公开前隐私检查通过后，仓库已由 private 转为 public（`https://github.com/csic21/qusor`），历史已按下列规则重写。此后 `releases/latest/download/` 可免认证读取，自动更新的更新源前提成立。

2026-11-12 首次云端发行验收：Release workflow [run 43142491761](https://github.com/csic21/qusor/actions/runs/43142491761) 在 `main@62a82e6` 成功完成 macOS 打包、签名清单与 draft 创建，draft 的 `targetCommitish` 固定为该 SHA。draft `v0.1.0` 含 21,786,549 字节的 arm64 归档、`latest.json` 与 `latest.json.sig`；清单大小/SHA-256、Ed25519 域隔离签名、ad hoc code signature 和实际窗口启动均已复核。此前 run 42995272099 的空 `*.zip` glob 失败已由后续提交修复。详见 [GitHub draft Release 验收](../../experiments/nsh-reader/evidence/github-release/README.md)。

## 现状（2026-11-12 核对）

| 能力 | 当前状态 | 证据 |
|---|---|---|
| 更新接线 | **已接线**：`apps/qusor/nsh.json` 有 `version`/`build`/`updater.github`；应用有更新面板（侧栏入口、检查/下载/安装、不能自替换时打开下载页）；本机回环自测走通检查→下载→替换→重启，见[更新记录](../../experiments/nsh-reader/evidence/app-update/README.md)。实验目录的 `nsh.json` 仍无这些字段 | `apps/qusor/nsh.json`、`apps/qusor/update.tsx`、`apps/qusor/native/src/app_info.rs` |
| 版本号 | 应用为 `0.1.0`/`build 1`；已创建 draft `v0.1.0`，尚未发布 | [Release 验收](../../experiments/nsh-reader/evidence/github-release/record.json) |
| 更新签名密钥 | 更新公钥已内置；私钥保存在本机 `.local/keys` 与 GitHub secret `NSH_UPDATE_KEY`，未提交。draft 清单已用该密钥签名并由独立 OpenSSL 复算通过 | 同左 |
| 打包 | 生产入口使用 `nsh package --target macos`，产出 ad hoc `QUSOR.app` 和归档；实验目录仍有早期自建 Preview 包装 | `apps/qusor`、[Release 验收](../../experiments/nsh-reader/evidence/github-release/README.md) |
| CI | `foundation.yml` 三 OS Core/库任务通过；`release.yml` 的 macOS package 与 publish 在 run 43142491761 首次完整通过并创建固定 SHA 的 draft | run 43142491761；[平台缺口](../../adapters/extension-store/tests/platform_profile.rs) |
| 移动端 | 5 个 target 的库交叉编译通过；另有 Android arm64 debug APK 在 API 36 模拟器实际安装、启动并完成手机布局/返回键/想法与背单词界面验证 | [Android APK/界面记录](../../experiments/nsh-reader/evidence/android-package-launch/README.md) |

### CI 失败根因与处理（2026-11-11）

- **ubuntu**：`adapters/extension-store/src/verified_cache_tests.rs` 的断言停在“支持 3 个 Host API”，实际已是 4（未提交工作树为 5）。该文件带 `#![cfg(target_os = "linux")]`，macOS 本机不编译，所以长期没被发现。
- **windows**：`adapters/extension-store/tests/component.rs` 三例在 `Store::open_for_host` 处失败，对应 [跨平台结果](../../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md) 已记录的 Windows 私有存储/文件适配缺口。
- **ubuntu**：`adapters/extension-store/src/verified_cache_tests.rs` 的 Host API 数量断言已改为预算断言（`<= 8`），“不支持版本”改用 `u16::MAX`（提交 `113950b`），不再随 API 增加失效。
- **windows**：`adapters/extension-store/tests/component.rs` 的三例真实 store 场景只在 Unix 有合格文件系统 profile（0700 目录、no-follow 包读取、单一 writer lease）。Windows 的私有存储 ACL 与 SQLite/blob 适配器仍未实现，因此测试按平台跳过（`#![cfg(unix)]`），另加 `tests/platform_profile.rs`：Unix 断言能打开并持有 writer lease，Windows 断言 `StoreErrorCode::Unsupported` 且诊断含 `qualified native filesystem profile`。这样 Windows 任务仍然运行并固定“故意拒绝”，而不是把跳过当成能力覆盖（提交 `8391d9c`、`ff5045b`）。
- 结果：2026-11-11 run 42987643346 三个 OS 的 `Core foundation` 与五个 `portable-libraries` 全部通过。Windows 仍不能运行 store/知识库；CI 绿不代表 Windows 产品资格。

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
| Android | 调试签名 arm64 APK；API 36 模拟器已安装、启动并操作手机布局、返回键、想法 Core 空态与背单词学习卡 | 真机/平板、商店签名与渠道、持久数据播种、PDF/IME/选文、性能与耗电 |
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

启动验证：`open QUSOR.app` 后进程存在，应用数据目录自动创建。早期为绕开双 Host writer 曾临时使用 `notes/`、`vocab/` 两个存储；2026-11-12 已改为单一 `core/` SQLite/CAS，Notes/Vocab 共享同一 `VaultStore`，旧目录通过 staging 迁移后保留为回退副本，见 [ADR 0039](../architecture/adr/0039-single-core-provider.md)。隔离 HOME 下的临时包已通过真实窗口与重开迁移验证，但带图标正式包在本机 `iconutil` 处阻断，正式安装升级仍未复验。

未验证：Windows/Linux 打包、真实窗口交互与更新 UI、`checkForUpdate`/`installAndRestart` 实际调用、Developer ID 签名与公证、Actions 流水线。

## 应用内自动更新本机实测（2026-11-11）

`apps/qusor` 增加软件更新面板（设计见[软件更新面板](../design/update-dialog.md)）与原生 `AppInfo` 模块：

- 侧栏底部“软件更新”行尾显示应用自己的版本，点开 470pt 浮层：当前版本、状态文案、新版本说明、下载进度、检查/下载安装/打开下载页按钮；发布/安装失败保留当前应用并显示原因。
- 版本来自应用包内 `nsh-updater.json` 资产（`nsh package` 写出），不读环境变量、不读仓库文件。
- 证据模式（`QUSOR_UPDATE_EVIDENCE` 或它留下的一次性标记）只用于本机自测：自动走同一套检查/安装路径、每个状态写 JSON 收据、重启后的构建报告版本与 Core 可读性后退出；普通启动没有该开关。

本机自测 `python3 apps/qusor/update-selftest.py --install-evidence`：从同一工作树打包 0.1.0 与 0.1.1（临时把更新源指向 `127.0.0.1`，`nsh.json` 结束后恢复），用 `.local/keys/update.secret` 生成签名清单，启动 0.1.0 走完整更新链。2026-11-12 最终复跑 17 项断言全部通过、整轮 98.6 秒：签名清单被接受；下载的归档 SHA-256/大小与清单一致；0.1.0 退出后平台启动 0.1.1；安装后的应用资产为 `version=0.1.1`、`build=2`；重启构建报告同一数据目录、`VocabHost.decks` 与 `NotesHost.page` 读取同一 Core 成功；带版本与 build 的一次性标记被消费。三张窗口截图（发现更新/安装中/重启后 0.1.1）与机器记录见[更新记录](../../experiments/nsh-reader/evidence/app-update/README.md)。

同时修正一处 TSX 反应式写法：NSH 组件只运行一次，信号必须在 JSX 绑定里读取（`when={updateState() === …}`），先放进局部变量会冻结；首轮截图暴露了“文案已更新、按钮/进度条没更新”的混合状态，修正后三个状态各自稳定。

未验证：已发布 GitHub Releases 的 `releases/latest/download` 实际拉取（当前仅为 draft）、拒绝路径（签名错误、应用 id 不符、旧清单、篡改归档）、Developer ID/公证、Windows/Linux 替换、移动端商店/OTA 语义、下载中断恢复、只读安装位置的下载页跳转。

## 未验证范围

- draft `v0.1.0` 已创建但未发布，GitHub 的 `releases/latest` 不提供 draft；更新私钥已在 `.local/keys`（未提交），仓库 secret `NSH_UPDATE_KEY` 已配置并完成云端签名。仓库可见性已于 2026-11-11 转为 public。
- 应用内更新在本机回环源上通过；已发布 GitHub Releases 源的在线拉取、拒绝路径与其它平台未验证。
- 上述 CI 失败修复未在远端重跑，Windows 缺口未修。

## 实施顺序

1. ~~修 Windows store 测试（或显式标记平台跳过并记录缺口），让 CI 恢复绿。~~ 已完成：Windows 任务显式固定拒绝路径，run 42987643346 三 OS 通过；Windows 能力缺口保留。
2. ~~新建 `apps/qusor` 入口，跑通 `nsh package --target macos` 实窗。~~ 已完成（ad hoc `.app`，见下方“生产应用首次打包”）。
3. ~~写 `version`/`build`/`updater`/图标，本地用 `nsh update-manifest` 与本地 HTTP 自测一次更新流程。~~ 更新 UI 与本地回环自测已实现，实测结果见下方“应用内自动更新本机实测”。
4. ~~加发版 workflow，先 `workflow_dispatch` 出 draft Release。~~ 已完成：run 43142491761 的 macOS draft 与产物验收通过，且 draft 固定到源码 SHA。
5. 仓库已转 public；人工复核 draft 后发布，并用公开 `releases/latest/download` 验证真实在线更新。**首个发布必须已内置 updater 配置**，否则已装版本收不到自动更新。
6. 再推进 Android/iOS 打包与真机资格。

## 关键约束与风险

- **首个包决定后续**：`updater` 与 trusted roots 在打包时写入，之后修改需要新的应用包。
- **转 public 会公开全部历史**：当前仓库 20 个提交、2202 个文件、约 90 MB 包（含证据图片/压缩件）。已扫描跟踪文件未见私钥或令牌，但公开前仍需一次人工确认。
- **macOS 分发**：没有 Developer ID 与 notarize，其他 Mac 默认打不开。
- **移动端更新语义**：iOS/Android 只能提示商店链接或走 OTA 包，不能像桌面那样自替换。
