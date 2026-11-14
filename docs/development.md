# QUSOR 基座开发与验证

> 权威范围：本文件是**本地检查命令、实验/打包复现入口与检查覆盖边界**的权威说明（[ADR 0048](architecture/adr/0048-doc-governance.md)）。各 Feature 与模块的实时验证状态见 [项目状态表](status.md)。

## 环境

Rust 版本以 [rust-toolchain.toml](../rust-toolchain.toml) 为准；Python 3.11+ 用于标准库检查脚本。没有 Node/npm、数据库、Supabase CLI 或模型服务的启动要求。

在本机原生文件系统中的仓库根目录执行以下检查；不绑定历史 WSL 工作区路径。运行数据使用明确的应用本地目录，不能把源码工作区的位置当作数据库部署位置。macOS 原生阅读预览及六端实际资格见 [跨平台结果](../experiments/nsh-reader/CROSS_PLATFORM_RESULTS.md)。

```sh
cargo fmt --all -- --check
python3 scripts/check_architecture.py
python3 scripts/check_native_ui.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo run -p extension-host --example policy_walkthrough --locked --offline
cargo doc --workspace --no-deps --locked --offline
```

能力组合内核与桌面 profile 的专项检查：

```sh
cargo test -p capability-composition --offline
cargo test --manifest-path apps/qusor/native/Cargo.toml --lib --offline
```

原生 Windows 环境使用 python 替代 python3。首次安装固定 Rust 工具链/组件及下载 serde/serde_json 的锁定依赖需要网络；先运行 cargo fetch --locked，缓存就绪后执行上述离线检查。CI 在准备阶段下载锁定依赖，验证阶段仍使用 --locked --offline。

Cargo.lock 已纳入工程。当前配置变更需要重新生成锁文件时使用 cargo generate-lockfile --offline；不要为了绕过 --locked 检查忽略锁文件。

## 独立插件执行实验

用户已授权的 [Wasm 实验](../experiments/extension-wasm/README.md) 使用独立执行器和作者 Workspace。先分别以 cargo fetch --manifest-path <对应 Cargo.toml> --locked 准备缓存，再在 WSL 从根目录运行：

```sh
python3 experiments/extension-wasm/validate.py --output /tmp/qusor-wasm-results.json
```

此命令在仓库外构建作者及全部样例，检查依赖、格式、测试和 Clippy，运行监督进程场景并保存 hash/结果。实际通过范围及限制见 [本机记录](../experiments/extension-wasm/RESULTS.md)。根 Core CI 不执行该实验，也不证明其原生/移动平台可用。

## 目录登记与独立包边界

schema2 PDF/混合资料回归使用 `python3 experiments/capture-core/validate_pdf.py`，先准备固定 PDFium 运行库。它包含旧 C0 文本 CLI 回归、真实原件/映射、仓库外普通插件、混合恢复再渲染以及实际升级/回退/卸载；见 [PDF 结果](../experiments/capture-core/PDF_RESULTS.md)。历史 C0 JSON 不作为新代码当前指纹，新证据另存 `linux-pdf-core.json`。

多行整组保存回归为 `python3 experiments/capture-core/validate_compound.py`，包括原 C0/C1 场景、真实原生多段选区、普通插件、schema3及V3整组恢复、旧入口拒绝截断。完整预算/迁移/崩溃范围见 [复合记录](../experiments/capture-core/COMPOUND_RESULTS.md)，不代替 NSH 原生 UI 验收。

[C0 捕获资格实验](../experiments/capture-core/README.md) 的完整复现入口为 `python3 experiments/capture-core/validate.py`。它验证仓库外公开 SDK 作者、真实 Wasm 提案与批准、Core 事务、故障恢复、卸载保留及空库恢复，并重跑受影响的包管理和运行器测试；实际范围见 [结果](../experiments/capture-core/RESULTS.md)。新实验不进入生产根 Workspace，不证明 NSH 或正式阅读通过。

[背单词插件首个切片](product/vocab-plugin.md) 的公开能力、Core 学习数据与普通包回归：

```sh
cargo test -p learning-domain -p extension-api -p extension-host -p storage-sqlite --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --test vocab_package --locked --offline
python3 plugins/vocab/build.py
```

`build.py` 在仓库外导出 SDK 并生成 `plugins/vocab/target/vocab.qpk` 与作者证明；固定测试签名种子不是生产发布密钥。注释：本机 macOS 上 `runtime-process --test controller` 在干净树同样失败（进程监督用例），与本切片无关，因此不要把本机全工作区测试结果表述为整体通过。

原生工作台背单词工作区（fixture + 真实 Core 双模式）的回归：

```sh
python3 experiments/nsh-reader/native-workbench/prepare.py --size 1280x820 --evidence vocab
cd experiments/nsh-reader/native-workbench
../target/nsh/target/debug/nsh check App.tsx
../target/nsh/target/debug/nsh test tests/vocab-workspace.test.tsx
../target/nsh/target/debug/nsh test tests/notes-workspace.test.tsx
cargo test --manifest-path experiments/nsh-reader/native-host/Cargo.toml --offline --test vocab_native
cargo test --manifest-path experiments/nsh-reader/native-host/Cargo.toml --offline --lib vocab_native
```

`--evidence vocab` 会在应用进程内把真实 `NSWindow` 渲染成 decks/settings/study/accent/revealed/reviewed/done 七张 PNG（含美/英与自动朗读状态、读音设置），不依赖屏幕录制授权；第一次运行先恢复默认设置再打开，同一 Core 重启后补两张恢复图并保留偏好，见[实窗记录](../experiments/nsh-reader/evidence/vocab-workspace-live/README.md)。音标来自样例词书资源，读音是实验宿主调用 macOS 系统语音的样例能力，读音偏好存应用自身存储，都不是公开 wire 或 Core 数据。

[目录架构](architecture/workspace-layout.md) 规定 apps/crates/adapters 为生产源码范围，plugins/sdk/experiments 独立。新增 Cargo package 和独立 Workspace 都必须在 [architecture.toml](../config/architecture.toml) 登记。根检查先扫描管理目录的 manifest，再核对生产 Cargo 图；隐藏目录与构建缓存不作为项目模块，源码符号链接被拒绝。

生产成员及 Cargo metadata 可见包的 path 声明不能指向插件、SDK、实验或未登记的本地实现，normal/dev/build/optional/target 声明均检查，已解析间接 helper 的本地路径也在范围内。独立 Workspace 的登记不代表其依赖图已通过，仍需自身验证入口调用 validate_graph。Rust include!/构建脚本任意读文件、未解析第三方可选分支、插件运行权限和 SDK 仓库外可用性不由该检查证明。

## 生产应用（apps/qusor）

应用入口按 [ADR 0037](architecture/adr/0037-production-app-assembly.md) 装配：`provider`、`bridges`、`native` 三个独立 workspace crate，NSH 依赖固定为 Git revision，随包 `.qpk` 由准备脚本生成。

```sh
python3 apps/qusor/prepare.py                     # 生成 notes/vocab 包与公钥到 native/assets
cd apps/qusor && CARGO_PROFILE_RELEASE_STRIP=none \
  ../experiments/nsh-reader/target/nsh/target/debug/nsh package --target macos --out /tmp/qusor-app-dist
```

`CARGO_PROFILE_RELEASE_STRIP=none` 是必需项：默认 `strip=debuginfo` 在固定基线上会产出畸形的 proc-macro dylib。发行清单与 GitHub Actions 流程见 [发行与更新](product/release-and-update.md)；`apps/qusor` 是独立 workspace，不在根 `cargo test --workspace` 范围内，需单独跑 `cargo check --manifest-path apps/qusor/native/Cargo.toml`。

Android debug APK 的本机打包入口（需 Android Studio JBR、SDK 与 NDK 27.2；Rust 1.97.1 在 release 下有已知 proc-macro dylib 缺陷，因此显式用 stable）：

```sh
cd apps/qusor
export JAVA_HOME='/Applications/Android Studio.app/Contents/jbr/Contents/Home'
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$ANDROID_HOME/platform-tools:/usr/bin:/bin:/usr/sbin:/sbin"
export RUSTUP_TOOLCHAIN=stable
../../experiments/nsh-reader/target/nsh/target/debug/nsh package \
  --target android --abi arm64-v8a --out /tmp/qusor-android
python3 package_android_pdfium.py /tmp/qusor-android/QUSOR.apk --abi arm64-v8a
```

`nsh package` 只写 `libnsh.so`；最后一步把固定 PDFium 注入 `lib/arm64-v8a/`，按 16 KB 页面要求对齐并重新签名。这不是商店签名。APK 在 API 36 模拟器的安装、单栏导航、返回键、想法 Core 空态和背单词触控结果见 [Android 资格记录](../experiments/nsh-reader/evidence/android-package-launch/README.md)；真实 PDF 导入、重开恢复与旋转页图见 [PDF 页图证据](../experiments/nsh-reader/evidence/pdf-page-import/README.md)。

PDF 导入与页图的 mac 真窗口回归：

```sh
cd apps/qusor
../../experiments/nsh-reader/target/nsh/target/debug/nsh test --target macos tests/pdf-import.test.tsx
cargo test --manifest-path native/Cargo.toml --locked --offline
```

资料库页级继续阅读偏好的无界面回归：

```sh
cd apps/qusor
../../experiments/nsh-reader/target/nsh/target/debug/nsh test tests/reading-position.test.tsx
```

该测试验证存储格式、损坏/越界值拒绝、页数变化失效、应用重开恢复和翻页写入；它不是 Core 阅读锚点、跨设备同步或导出资格。

应用内自动更新（检查→下载→替换→重启）的本机自测：

```sh
python3 apps/qusor/update-selftest.py               # 写入 /tmp，不覆盖仓库证据
python3 apps/qusor/update-selftest.py --install-evidence   # 同时写入 evidence/app-update
```

该脚本在同一工作树打包 0.1.0/0.1.1、用 `.local/keys/update.secret` 签名清单、在回环 HTTP 上完整走一次更新，窗口截图只截应用窗口。它要写应用数据目录并替换应用自身，因此不是沙箱内任务；密钥内容不打印、不复制。范围与限制见[更新记录](../experiments/nsh-reader/evidence/app-update/README.md)。

## NSH 前置实验

原生 NSH Cargo 构建与测试一律显式设置 `CARGO_BUILD_JOBS=1`，并传 `-j1`。即使指定单个 integration test，Cargo 也可能隐式构建已有多个 bin；2026-11-08 本机观察到四个并行链接任务争用内存并拖慢系统。需要中断时保留日志，确认自有 cargo/rustc/linker 全部退出后再串行重试；编译中断不等于已运行的 OS worker 被回收，不能混用清理证据。

NativeEditor shutdown/lease收尾的非默认 `native-qualification` 回归已有[真实OS证据](../experiments/nsh-reader/evidence/native-editor-shutdown/README.md)：8项真实进程案例、完整native lib 16项与target Clippy通过，Root同冻结exe独立8例通过。该资格不包含新默认GUI/IME或原生资源释放ACK；带此feature构建过的既有bin须重新默认features构建后再做GUI验收。

原生 PDF Surface 复现入口见 [探针说明](../experiments/nsh-reader/pdf-surface/README.md)：`prepare_pdf.py` 实际调用原生 provider 冻结页图/报告并构建绑定，`run_pdf.py` 启动本地 bundle。该路径验证真实窗口触点与候选高亮，尚未接普通 reader 插件或持久保存；不能以它替代 C3/C4 事务回归。局部3项 adapter 边界测试与 Clippy 命令也在探针说明中。

按 C-09，探针现默认 `--profile native`，只启用原生 GTK/JS 路径；可选 WebView 必须显式 `--profile webview`。原生 PDF 服务独立复现入口为 `python3 experiments/pdf-provider/validate.py`，先按 [准备说明](../experiments/pdf-provider/README.md) 验证固定运行库；[结果](../experiments/pdf-provider/RESULTS.md) 仅证明 provider 子项，不证明 NSH 或正式 PDF 保存。

原生选区增量运行 `python3 experiments/pdf-provider/validate_selection.py`，覆盖实际字形像素输入、指针/视口失效、分段顺序和单段经普通插件入库恢复；[记录](../experiments/pdf-provider/SELECTION_RESULTS.md) 与早期 provider/C1 基线分开。此早期单段脚本保留多段拒绝路径；多段原子保存由上面的 C3 validate_compound 单独验证，不计完整阅读通过。

[NSH 实验入口](../experiments/nsh-reader/README.md) 提供固定源码取得与复现步骤；本机 [结果](../experiments/nsh-reader/RESULTS.md) 记录各阶段证据；隔离本地依赖前缀已解除 native 编译阻断，独立宿主实际显示离线 Rust+TSX、按钮回调并正常关闭重开。完整 P02 打包、中文输入和阅读仍待验。已有上游 checkout 时，从 Linux/WSL 仓库根目录运行：

```sh
python3 experiments/nsh-reader/preflight.py
python3 experiments/nsh-reader/preflight.py --cargo-check --output experiments/nsh-reader/target/preflight.json
```

退出 1 表示缺依赖或检查失败；退出 0 只表示脚本所检查的前置项满足，不是应用启动或阅读通过。脚本不安装系统软件；--cargo-check 会在上游缓存 Workspace 写构建缓存，生产依赖不变。

## 检查分别证明什么

| 检查 | 证明 | 不证明 |
|---|---|---|
| fmt/clippy | 当前 Rust 代码可格式化、通过静态检查 | 产品功能存在 |
| cargo test | JSON 协议、宿主授权与生命周期的真实回归通过 | 阅读器/知识入库/Agent/同步已测试 |
| policy_walkthrough 示例 | 注册、显式授权、跨库拒绝、撤权、升级与重装失效 | 插件包已安全安装执行或状态已持久保存 |
| cargo doc | 当前库文档可生成 | 设计已实机验证 |
| 架构检查及 Python 测试 | 管理目录/Workspace 登记、生产源码边界、当前声明与配置闭包的反例有效 | 独立插件依赖图、未解析第三方可选分支、FFI、运行权限均已审查 |
| 文档链接检查 | 仓库内 Markdown 文件目标存在 | 所有远端URL或页内锚点有效 |
| GitHub Actions 配置 | 计划在三桌面runner执行相同检查 | 流水线已运行、NSH三桌面或移动通过 |

GitHub Actions 采用官方 [checkout](https://github.com/actions/checkout) 与 [setup-python](https://github.com/actions/setup-python) 当前文档的 v7 主版本，只读 contents、关闭凭据持久化；没有部署、发布或远端数据库步骤。

## 新增工作

正式UI先读取 [设计工作基线](design/README.md)，使用 [分区/角色/提供者](design/composition.md) 与 [UI验收](design/review.md)；功能与界面同步验证。设计token目前是文档数据，尚未接通运行主题。离线可视稿可用 `python3 docs/design/assets/generate_boards.py` 重建SVG；可选 `--png` 通过本机librsvg/Cairo渲染预览，不下载依赖。静态图稿不证明NSH交互或真实保存通过。

先关联 [Feature ID](product/scope.md)。涉及架构决策时使用 [架构技能](../.agents/skills/qusor-architecture/SKILL.md)；技术验证使用 [实验约定](../experiments/README.md)。

新增 Workspace member 或依赖需修改 [允许清单](../config/architecture.toml)，给出使用理由和相应验证，不能只是删掉检查。当前不需要为纯文案改动编写业务测试。

## 技能校验

三个项目技能都是仓库文件；格式为 YAML frontmatter 加 Markdown。创建/修改时运行环境中 skill-creator 的 quick_validate.py，并检查引用路径。本仓库不复制该系统技能的校验器，也不依赖用户机器上的绝对技能路径来运行 CI。

## 专项持久与捕获回归入口

修改 `core-api`、`learning-ports`、`learning-application`、`storage-sqlite`、`extension-package`、`extension-store` 或 Host 公开契约后，除根 Workspace 检查外，须同时运行对应真实消费者与仓库外 SDK 实验：

```sh
python3 experiments/extension-package/validate.py --output /tmp/qusor-package-validation.json
cargo test -p core-api -p storage-sqlite --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --test durable_installation --locked --offline
```

- 公开 Surface 兼容改动（`extension-api` / `extension-host`）：同步重跑包 `validate.py`、Wasm `validate.py` 与 `validate_compound.py`，并按 [NSH 集成资格](../experiments/nsh-reader/plugin-surface/RESULTS.md) 与 [dynamic-pdf](../experiments/nsh-reader/dynamic-pdf/README.md) 核验原生消费端。
- 历史各轮机器快照与测试计数保留在对应 `experiments/*/RESULTS.md` 及 `experiments/*/evidence/`；日常状态汇总统一更新到 [项目状态表](status.md)，不再在本文件追加日期流水。
