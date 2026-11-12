# QUSOR 基座开发与验证

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
```

这不是商店签名。APK 在 API 36 模拟器的安装、单栏导航、返回键、想法 Core 空态和背单词触控结果见 [Android 资格记录](../experiments/nsh-reader/evidence/android-package-launch/README.md)。

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

## 基座验证记录（2026-11-07）

已在本地 WSL Ubuntu、Rust 1.97.1 下通过格式检查、Clippy、离线编译/测试和库文档生成。依赖边界检查通过，6 个 Python 测试方法通过；三个项目技能通过系统 skill-creator 的格式校验，仓库 Markdown 本地链接检查通过。

上述初始骨架记录中 Rust 业务测试为 0，当时仅有空库边界。未运行远端 GitHub Actions、原生 Windows/macOS、NSH 或移动端验收；上述检查不代表 M-1 通过。

同日插件化设计增量：已复核依赖允许清单、22份Markdown文件目标和两个修改后的项目技能格式。新增插件架构/ADR、范围与验证计划；没有新增Rust依赖、插件运行时或业务代码。EXT-P01–P10均未执行。

同日C-07确认后：第一阶段第三方开发/安装/分发和第一方功能插件化已写入ADR 0003，新增EXT-B01–B08与EXT-P11–P13。23份Markdown的本地文件目标及两个修改后的技能格式校验通过，旧的待确认分期已清理。此轮只修改文档/技能，未重跑无变更的Rust构建；SDK、独立包、隔离和平台验收均未执行。

## 无界面插件策略实现验证（2026-11-07）

用户已授权从工程骨架推进基座功能，实现范围见 [功能规格](product/foundation.md) 与 [ADR 0004](architecture/adr/0004-headless-extension-policy.md)。本次在 WSL Ubuntu / Rust 1.97.1 实测：

- 35 个 Rust 测试通过：21 个 JSON 协议测试，14 个宿主授权与生命周期测试。
- policy_walkthrough 示例执行通过，展示注册、授权、跨库拒绝、撤权拒绝旧回调、升级清空授权及重装后旧会话失效。
- fmt、Clippy 全目标检查和库文档构建通过；Clippy / rustdoc 均将警告视为错误。
- 直接依赖声明与配置模块的已解析传递闭包检查通过；21 个 Python 反例测试通过。
- 本地 Markdown 文件目标检查通过；不涵盖远端 URL 与页内锚点。

协议 Schema 已进行 Draft 2020-12 结构检查及合法/非法输入校验；该补充检查使用本机已有 jsonschema，不增加项目 Python 运行依赖。Rust 校验仍是 wire 入口的运行时强制约束。

检查使用本机依赖缓存离线完成，没有在此会话提交或推送。保留并行会话新增的 MIT / GitHub 元数据。CI 已加入锁定依赖准备与无界面示例，但本轮未执行远端流水线；未验证原生 Windows/macOS、NSH、移动、发布渠道、独立插件包、沙箱、持久化或真实知识读写。

## 插件目录与 NSH 前置验证（2026-11-07）

落实 [ADR 0006](architecture/adr/0006-plugin-workspace-layout.md) 的目录/功能归属，新增 Cargo 目录登记和生产本地依赖边界检查。WSL 实测 42 项 Python 回归通过（新增 21 项），生产架构守卫通过；现有 Wasm 执行器/作者两个 Workspace 的真实离线 Cargo metadata 与原 validate_graph 兼容检查通过。本地文档链接、git diff --check 和三个项目技能的 quick_validate 通过。

NSH 前置脚本实际检查固定源码、工具链、可加载 libclang、原生库与显示线索，并尝试离线 Cargo check；另有 3 项 libclang 缺失/无效及就绪状态回归通过。本机依赖准备阻断，未进入应用编译/窗口或阅读交互，详见 [实验结果](../experiments/nsh-reader/RESULTS.md)。持久包管理仍为架构/验收设计，无生产安装器、SDK 发行或真实知识写入成果。

本轮未改变 Rust 代码或依赖，不重跑无关 Rust/Wasm 执行测试；没有远端 CI、其他原生平台或移动验收。目录检查不代替独立插件内部依赖审计、仓库外构建或执行隔离。

## 签名包与持久安装事务增量（2026-11-07）

运行 [包实验](../experiments/extension-package/README.md)：

```sh
python3 experiments/extension-package/validate.py --output /tmp/qusor-package-validation.json
```

本机 20 项包/存储测试、84 个 CLI 场景通过，其中安装/更新/卸载共 12 次真实进程终止注入；仓库外仅用导出 SDK/API 编译两版本并保持宿主 hash 不变。两个 Workspace 的 fmt/test/clippy、独立依赖图和生产依赖图通过，源/工件/hash 见 [证据](../experiments/extension-package/RESULTS.md)。

原 Wasm 实验仅增加冻结字节入口，16 项运行回归及 fmt/clippy 通过；生产架构守卫与 42 项 Python 回归通过。NSH 固定 Linux 缓存 online/offline fetch 均通过，编译在 gstreamer-sys 缺开发库处失败，未运行窗口。

此次没有生产 Rust 行为/依赖改动，不重复无关 Core 业务检查；没有提交/推送、远端 CI、其他原生平台、断电或移动验收。完整授权/执行恢复、正式安装器与阅读仍待各自门槛。


## 持续授权与策略会话恢复增量（2026-11-07）

同一包实验验证入口现覆盖 [ADR 0008](architecture/adr/0008-durable-grant-recovery-probe.md)。最终 [机器证据](../experiments/extension-package/evidence/linux-grant-recovery.json) 为 29 项 Rust 测试、156 次 CLI 检查、20 次进程中止；两个实验 Workspace 的 fmt/test/clippy、仓库外作者构建和依赖图均通过，宿主构建前后 hash 相同。12 份源/测试/锁文件摘要与当前文件已核对。生产架构守卫、42 项 Python 回归、文档链接和 diff 空白检查通过。

仅新增实验对权威 extension-host 的直接依赖，生产 Rust 行为不变；恢复验证不含 Wasm 调度或真实 Core 写入。NSH 前置条件再次只读检查，仍缺 GTK4/Graphene/GStreamer/WebKitGTK，未重跑已知阻断的编译或启动窗口。实际执行恢复、私有状态迁移/回滚、长驻监督及业务阅读仍待各自验证；没有远端 CI、提交或推送。


## 授权恢复后受限执行增量（2026-11-07）

按 [ADR 0009](architecture/adr/0009-restored-wasm-execution-probe.md) 执行同一包验证入口：[执行证据](../experiments/extension-package/evidence/linux-execution-recovery.json) 为 30 项 Rust 测试、194 次 CLI 检查（含既有 20 次进程终止）；仓库外作者、fmt/clippy 和依赖图通过。已核对20份源/测试/fixture/锁文件摘要与当前文件。

受影响 Wasm 运行器另完整运行原 validate.py，[回归证据](../experiments/extension-package/evidence/linux-runtime-authority-regression.json) 为20项Rust运行回归及20个受监护场景，84份摘要核对通过。架构守卫、42项Python架构回归、文档链接和diff空白检查通过。生产行为/依赖未改；此证据不证明长驻双插件、并行持久撤权、真实Core/阅读或NSH平台通过。下一项迁移/回滚见[实验设计](../experiments/extension-package/MIGRATION.md)，仍待实现。


## 私有状态成对迁移/回滚增量（2026-11-07）

按 [ADR 0010](architecture/adr/0010-paired-private-state-probe.md) 使用同一包验证入口。最终 [包证据](../experiments/extension-package/evidence/linux-private-lifecycle.json) 通过37项Rust、432次CLI检查（含40次真实进程中止），仓库外签名作者、fmt/clippy/依赖图均通过；38份源/测试/fixture/锁文件摘要核对当前文件。

[运行器回归证据](../experiments/extension-package/evidence/linux-private-runtime-regression.json) 通过25项运行器与4项作者Rust测试、20个受监护场景，99份摘要核对。仅两个实验增加dev-only wat，生产依赖不变。根架构守卫、42项Python架构回归、Markdown本地目标和diff空白检查通过。未验长驻双插件、真实Core、断电、多平台/渠道或NSH阅读；下一项为长驻监督，不能以独立进程看门狗替代。


## 长驻双插件监督增量（2026-11-08）

按 [ADR 0011](architecture/adr/0011-persistent-supervisor-probe.md) 使用原包验证入口：[最新证据](../experiments/extension-package/evidence/linux-persistent-supervisor.json) 通过43项Rust、454次CLI（含此前40次管理进程中止），另同PID62条管理命令、14项监督场景。46份源/测试/fixture/锁文件摘要与当前文件核对，宿主独立作者构建前后hash相同。新增Rust测试全部使用真实父子进程。

[运行器证据](../experiments/extension-package/evidence/linux-managed-runtime-regression.json) 通过31项运行器、4项作者测试和20个旧受监护场景，101份摘要核对。无新依赖或生产行为变动；根架构守卫、42项Python回归、文档本地目标、diff空白检查通过。kill/wait失败为明确注入，异步broker pending不代表可抢占任意同步父I/O。正常关闭已验证，异常崩溃/残留协调仍待下一项，Core/NSH/移动/远端CI未验。


## 宿主异常退出协调增量（2026-11-08）

[ADR 0012](architecture/adr/0012-supervisor-crash-reconciliation.md) 使用原包验证入口追加真实身份/启动/恢复；[正式证据](../experiments/extension-package/evidence/linux-supervisor-crash-recovery.json) 通过59项Rust和543次CLI（含既有40次事务进程中止），另正常长驻14项/本次72条管理命令、异常恢复14项/本次97条管理命令与10次父进程异常退出。50份源码/测试/fixture/锁文件摘要与当前文件一致，独立作者构建期间宿主hash不变。查询次数受调度影响，不是性能指标。

新增仅实验rustix=1.1.4，SDK导出仍剔除进程/SQLite/Host；normal/dev/build/目标声明及解析闭包检查通过，生产依赖不变。未变更Wasm运行器，不重复无关完整回归；本轮真实包/能力/监督路径已执行。59项包含共享纯授权门、GuardReady、Boot前过期拒绝、显式锁释放与live pidfd清理，未以sleep重试放宽安全门。

根架构守卫、42项Python架构回归、本地文档目标、diff空白检查通过。已尝试spawn但无身份的同内核窗口仍隔离；Terminated不等于非子进程已reaped；没有真实内核重启/断电或OS沙箱成绩。[NSH只读前置记录](../experiments/nsh-reader/evidence/linux-20261108-preflight.json) 仍缺6个库组，未重跑已知阻断的编译。下一项为捕获存储/定位资格切片，真实Core和正式阅读尚未交付。


## Core 公开捕获契约

[core-api](../crates/core-api/README.md) 已承接文本/PDF/复合摘录契约与纯测试，包含在根 fmt/clippy/test 和依赖闭包守卫中。实验兼容模块转发同一实现，SDK 直接导出该权威源；因此更改契约需要同时运行实验真实消费者与仓库外作者验证：

```sh
cargo test -p core-api --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --locked --offline
python3 experiments/capture-core/validate_compound.py
```

完整 C3 脚本包含 SDK 仓库外构建/依赖路径核验、普通签名插件、真实 PDF 页图选段、单事务保存、生命周期与恢复；不是 NSH 窗口或完整阅读旅程验收。架构决定见 [ADR 0018](architecture/adr/0018-core-api-contract-source.md)。


## 知识应用与持久适配器

[ADR 0019](architecture/adr/0019-atomic-knowledge-persistence.md) 的 production application/ports 与 SQLite/CAS 现由实验 Host 直接消费，实验不复制存储实现。根工作区检查包含这些真实成员，补充边界测试与完整插件/原生回归：

```sh
cargo test -p storage-sqlite --locked --offline
cargo test --manifest-path experiments/capture-core/Cargo.toml --locked --offline
python3 experiments/capture-core/validate_compound.py
```

[实际结果](../experiments/capture-core/PERSISTENCE_RESULTS.md) 区分根测试、新整体计划/事务边界和完整 C3 通路。修改源码时指纹覆盖根 manifest/lock、core-api、application/ports、SQL/CAS及实验Host，不以只有协议测试代替持久路径。其他平台adapter与正式阅读工作区仍待独立验证。


## 包与执行适配器生产集成

[ADR 0021](architecture/adr/0021-signed-package-authority.md) 的纯签名包、[ADR 0022](architecture/adr/0022-authority-free-wasm-runtime.md) 的执行端口/Wasmi适配器已由旧实验实际消费。根工作区现88项测试，包含14项包格式及3项直接运行端口测试；包543场景、Wasm20受监护场景和最后一次完整C3均已通过，详见 [C5统一记录](../experiments/capture-core/PLUGIN_PRODUCTION_RESULTS.md)。精确VM燃料断言使用记录过的串行测试参数，未放宽250ms生产期限；测试内部的撤权/监督并发仍照常执行。

公开Surface的 [UX/权限规格](product/reader-surface-capability.md) 和 [0.2兼容设计](architecture/adr/0023-public-reader-surface.md) 已实现公开wire与Host registry，独立原生消费者已有集成资格，不代表已经发布新Host API。生产持久安装/监督、正式功能插件与完整阅读旅程继续按门槛推进。


## 公开 Surface 兼容回归

0.2契约/Host registry改动后运行根检查、包 `validate.py`、Wasm `validate.py` 和 `validate_compound.py`；最近结果为 [108项根测试与完整兼容回归](../experiments/capture-core/SURFACE_COMPATIBILITY_RESULTS.md)。版本、权限和SDK改动须继续运行这些真实消费者，不能只跑Surface DTO测试。原生输入与产品旅程由NSH独立实验分别留证。


普通签名包的原生选文/非空笔记/持久回执和完整恢复，复现步骤见 [NSH集成资格](../experiments/nsh-reader/plugin-surface/RESULTS.md)：`prepare_plugin_surface.py`、`qualify_plugin_surface.py`、`recover_plugin_surface.py`。键盘需支持焦点的X11显示；本机记录为隔离Xvfb中的真实GTK/XTest，不代表当前桌面焦点或中文IME通过。


## 持久安装与进程身份生产内核

[extension-store](../adapters/extension-store/README.md) 与 [runtime-process](../adapters/runtime-process/README.md) 已注册根工作区；根检查覆盖121项Rust。旧包实验真实消费同一SQL/进程身份实现；完整回归入口仍为包validate、Wasm validate及C3 validate_compound，精确范围与源码快照见 [C7结果](../experiments/extension-package/INSTALLATION_PRODUCTION_RESULTS.md)。

统一Host+真实Core metadata消费者为 `cargo test --manifest-path experiments/capture-core/Cargo.toml --test durable_installation --locked --offline`。该测试有真实签名/Wasmi/持久Core，但属于受控文本fixture的接口/事务集成，不代替进程监督或UI验收。原生消费者迁移后另以新工件/新数据实际重跑，见 [原生C7](../experiments/nsh-reader/evidence/installation-production/RESULTS.md)。

## C8 阅读权限与动态PDF

根当前134项Rust及42项边界守卫通过，完整包/Wasm/C3入口保持；`durable_installation`另含真实双Store跨库卸载证明拒绝后继续读取Core。Surface/持续权限或生命周期改动必须复验这些消费者，保留旧profile1语义。汇总、日志与185份精确源码归档见 [C8结果](../experiments/capture-core/READER_AUTHORITY_RESULTS.md)。

动态原生入口见 [dynamic-pdf](../experiments/nsh-reader/dynamic-pdf/README.md)：先prepare通用binary/bundle，再生成新fixture、启动真实native chooser；用当前构建的第三/四页交互与保存恢复取证，不能换report或重编PNG冒充动态。最终native11项测试、Clippy及主链恢复通过；首次异常状态与最终工件明确分开，尚无长书/IME/真实mount释放ack成绩。
