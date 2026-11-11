# ADR 0037：生产应用装配与自动更新发布

- 日期：2026-11-11
- 状态：应用中；桌面 macOS 打包已实测，Windows/Linux 与移动端未验证
- 关联：C-17、C-15、F-PLAT-01/02、F-EXT-01/03、F-DATA-01
- 承接：[发行与更新方案](../../product/release-and-update.md)、[ADR 0003](0003-plugin-first-delivery.md)、[ADR 0006](0006-plugin-workspace-layout.md)、[ADR 0033](0033-cross-platform-runtime.md)

## 背景

到 2026-11-11，QUSOR 的全部可运行 UI 仍在 `experiments/nsh-reader`：`native-host` 是自建原生宿主，`dynamic-pdf`/`native-workbench` 通过 `nsh build` 生成的 bundle 与自定义 CLI 参数启动。`apps/` 只有目录约定。用户要求通过 GitHub Release 发版、GitHub Actions 构建，并保留现有 PDF 阅读/编辑能力。

发布流水线要求一个真正的应用入口，且 `apps/` 与 `crates/` 同属生产根，生产代码不能依赖 `experiments/`。现有实验宿主同时承担了“定格的资格证据”和“可运行的应用”，两者需要区分。

## 决策

1. **生产应用放在 `apps/qusor/`，是独立的 Cargo workspace**，包含三个 crate：
   - `apps/qusor/provider`（`qusor-pdf-provider`）：从 `experiments/pdf-provider` 复制的 PDFium 渲染与字符映射库，裁剪掉 CLI 探针与外部 fixtures；
   - `apps/qusor/bridges`（`qusor-app-bridges`）：从 `experiments/capture-core` 复制的 `notes_bridge`、`vocab_bridge`、`component_edit` 三个可信适配器，逐字节保留；
   - `apps/qusor/native`（`qusor-app-native`）：NSH 原生模块，提供 `NotesHost`/`VocabHost`，从 `System::data_dir` 取得应用数据目录、用 `include_bytes!` 嵌入随包 `.qpk`，不再读取 `QUSOR_*` 环境变量。
2. **独立 workspace 而不是根 workspace 成员**：根 workspace 的 CI 覆盖 Core 与五个交叉编译目标；NSH/QuickJS/原生 UI 不应进入 Core CI。`config/architecture.toml` 新增 `[apps.qusor.packages.*]` 分组与 workspace 清单，`scripts/check_architecture.py` 接受 `apps` 组并按生产根校验。这些 crate 的依赖图需要各自的 `validate_graph` runner，**尚未实现**。
3. **NSH 依赖用 Git revision 固定**：`nsh-core` 以 `git = "https://github.com/longbridge/nsh", rev = "913fd76…"` 声明。`nsh package` 生成 runner workspace 时会把 app crate 的 NSH 依赖重定向到该 checkout；而 crates.io 未发布 `nsh-platform-tui`/`nsh-tui-host`，用版本依赖会让 runner 解析失败（实测错误：`no matching package named nsh-platform-tui`）。
4. **随包插件资产由 `apps/qusor/prepare.py` 生成**：运行 `plugins/notes/build.py` 与 `plugins/vocab/build.py`，把 `.qpk` 与发布者公钥复制到 `apps/qusor/native/assets/`。应用只嵌入字节与公钥，安装/授权仍走 Host 的公开链路。
5. **打包必须关闭 release strip**：固定基线 `913fd76` + 本机 rustc 1.97.1 组合下，`strip=debuginfo` 会产出畸形的 proc-macro dylib，导致 `rquickjs_macro` 无法加载；`CARGO_PROFILE_RELEASE_STRIP=none` 可通过（见发行方案的本机实测）。
6. **更新源与发布**：`nsh.json` 配 `"updater": { "github": "csic21/qusor", "publicKey": … }`；发布流程为 tag → 三平台 `nsh package` → `nsh update-manifest` → GitHub Release。更新私钥只存本地/CI secret，公钥随应用内置。首个发布必须已含该配置，否则已装版本收不到更新。

## 后果

- 根 workspace 与 Core CI 不受影响；应用代码可独立编译、打包。
- 生产目录出现与实验目录的**临时重复**（provider、三个桥接模块、两个 native 模块）。后续应把实验侧改为依赖生产 crate，或把实验冻结为归档；此前的实验证据路径保持不变，但重跑脚本仍指向实验副本。
- 生产应用尚未接入 PDF 阅读/编辑 UI：`App.tsx` 当前是工作台外壳（想法/背单词真实 Core，其余为窗口内 fixture）。阅读器 UI 依赖的 `ReaderSurface`/`DynamicReader`/`NativeEditor` 原生模块仍在实验宿主，接入需要一个单独的界面积分切片。
- 缺少应用 workspace 的依赖图校验、Windows/Linux 交叉打包实测、真机与商店签名资格。

## 未验证

- macOS 打包命令链之外的任何平台；Windows 的 `pdfium-render` 与 Unix-only 文件操作路径未验证。
- 发布 workflow 从未在 GitHub Actions 运行；更新清单签名没有独立校验器复算；`checkForUpdate`/`installAndRestart` 未由真实应用调用。
- 无 Apple Developer ID / Android keystore，产物是 ad hoc 签名。
