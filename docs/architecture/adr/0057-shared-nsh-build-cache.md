# ADR 0057: 共享 NSH 构建缓存

- 日期：2026-11-17
- 状态：Accepted
- 关联：[ADR 0037](0037-production-app-assembly.md)

## 背景

同一份 NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f` 曾放在 `experiments/nsh-reader/target/nsh`。那个目录既是检出，又是 Cargo 产物目录。删掉 `target` 会连源码和 `nsh` CLI 一起删掉。生产 `apps/qusor/native` 用 git rev 引用上游，实验宿主用 path 指向另一份检出。Cargo 把它们当成两个来源，同一批 crate 会再编译一遍。各 workspace 还留着交叉三元组产物。

[ADR 0037](0037-production-app-assembly.md) 仍然有效：生产应用保持独立 workspace。NSH 与实验宿主不进入根 workspace。Core CI 不编译 NSH。生产依赖继续写 `git = "https://github.com/longbridge/nsh"` 与该 rev。改成 crates.io 版本会因为找不到未发布的 `nsh-platform-tui` 而失败。

## 选项

- **选项 A：把实验宿主并入根 workspace。**
  缺点：Core CI 会开始编译 NSH，违反 ADR 0037。
- **选项 B：升级 NSH，或改 pin。**
  缺点：这次只处理磁盘上的重复产物，不改变已固定的上游行为。
- **选项 C：用 `git clean -fdx` 回收磁盘。**
  缺点：会删掉源码、`.local/`、证据和作者包，范围远大于构建缓存。
- **选项 D：用 `[build] target-dir` 把仓内 Cargo 都指到一个目录。**
  缺点：会覆盖调用方已经设置的 `CARGO_TARGET_DIR`，仓库外作者构建和显式隔离的脚本不再安全。
- **选项 E：检出只留一份，放在 `third_party/nsh`；需要共享产物的命令自己把 `CARGO_TARGET_DIR` 设为仓库根 `.build/cargo-target` 的绝对路径（采用）。**
  `[.cargo/config.toml](../../../.cargo/config.toml)` 只保留 `[env] CARGO_TARGET_DIR = { value = ".build/cargo-target", relative = true }`。Cargo 1.97.1 决定自己的产物目录时不读 `[env]`，只读进程环境里的同名变量。因此已设置的变量仍优先，仓库外临时目录里的作者构建看不到这份配置。

## 决定

1. 唯一检出是 `third_party/nsh`，HEAD 为 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。原检出整份搬来，包含已经在里面的 `target`。根 `.gitignore` 忽略 `/third_party/` 与 `/.build/`，不提交上游源码。不保留第二份检出，也不用符号链接冒充另一份检出。
2. 活入口的 NSH 路径只从 `scripts/build_layout.py` 读取：仓库根、检出、固定 SHA、`.build/cargo-target`、宿主 `nsh` CLI。发行流水线构建 `nsh-cli` 时设置 `CARGO_TARGET_DIR` 为该目录。不设置时，`cargo build -p nsh-cli` 仍写入 `third_party/nsh/target`。
3. 上游 `runtime::checkout` 只承认路径恰好是 `<检出>/target/<profile>/nsh` 的 CLI。`.build/cargo-target/debug/nsh` 不满足。生产应用是 Rust app，打包和测试走应用 workspace，不靠这条发现规则。`cargo xtask android` 把运行时写死在 `third_party/nsh/target/android`，也不看 `CARGO_TARGET_DIR`。Android 打包设置 `NSH_RUNTIME` 为 `third_party/nsh/target`。
4. 实验宿主 `native-host` 的 path 依赖改为 `../../../third_party/nsh/...`。25 条指向该检出的 path 在磁盘上都能落到带 `Cargo.toml` 的目录。锁文件没有 path 项，因此没改锁。完整 `cargo metadata --offline --locked` 在下载 `atomic_refcell` 时因离线缓存缺失而停止，这与路径改写无关。
5. 生产 `[patch.'https://github.com/longbridge/nsh']` 已撤回。命令与要点：

```text
cargo metadata --manifest-path apps/qusor/native/Cargo.toml --offline --locked --format-version 1
error: failed to load source for dependency `nsh-core`
unable to update third_party/nsh/crates/nsh-core
error inheriting `homepage` from workspace root manifest's `workspace.package.homepage`
`workspace.package.homepage` was not defined
```

   `nsh-core` 使用 `homepage.workspace = true` 以及其它 workspace 继承。`third_party/nsh` 的 `[workspace.package]` 定义了 homepage，但该 patch 在 `apps/qusor/native` 这个没有 `[workspace.package]` 的 workspace 里解析。未改上游清单，也未把 NSH 的 workspace 键抄进应用。生产依赖保持原来的 git rev。没有接着做 macOS `nsh package`。不能为了共享缓存留下一个解析失败的应用。
6. `scripts/clean_build_cache.py` 默认只删 `.build/cargo-target` 里的 `incremental`，以及非宿主三元组目录（ios、ios-sim、android 各 ABI、windows-gnu、windows-msvc、linux-gnu 及同类）。保留宿主 `debug/` 与 `release/`，包括没有 triple 后缀的那份。`--deps` 再删这些目录里的 `deps/` 与 `build/`，但留下 profile 根上的最终二进制。`--all-targets` 连宿主产物也删。不删除 `.local/`、`evidence/`、源码、`.git`、`plugins/*/target` 里的 `*.qpk` 与 `author-proof.json`，也不删除 `third_party/nsh` 里的源码和历史 `target`。脚本可以识别搬走后遗留的 `experiments/nsh-reader/target/nsh/target`。不使用 `git clean`，也不做定时清理。

## 后果

- 根 workspace 仍不包含 NSH。交互式 `cargo test --workspace` 在未设置 `CARGO_TARGET_DIR` 时继续使用仓库根 `target/`。
- 第一次按上面的变量构建宿主 `nsh-cli` 会在 `.build/cargo-target` 里重新生成产物。这次没有执行该构建，也没有对现有缓存执行清理脚本。
- 非 NSH 的包内脚本若仍假设自己的 `target/debug`，下一次在设置了共享变量之后会和旧路径错开。这次没有把整个仓库的 Cargo 调用都改掉。
- 清理脚本只在临时夹具上跑过。本机 `third_party/nsh/target` 与其它已有 target 都还在。
- `scripts/check_docs.py` 跳过 `third_party` 与 `.build`。上游 Markdown 不是本仓库的文档。检出原来在 `target/` 下，那次遍历本来就不会进入它。

## 修正

本节是 2026-11-17 的修正。同日用锁定的 Cargo 1.97.1 复核了产物目录。选项 D 现在是仓内 Cargo 的产物目录机制。原来写在选项 D 上的缺点在这条工具链上不成立：进程环境变量 `CARGO_TARGET_DIR` 仍优先于 `[build] target-dir`，不会被配置盖掉。仓库外作者构建和显式设置了该变量的脚本继续隔离。

`[env] CARGO_TARGET_DIR` 只传给 Cargo 拉起的子进程。Cargo 决定自己的产物目录时不读它，所以它不能让 Cargo 写入 `.build/cargo-target`。配置里不再保留这一项。[`.cargo/config.toml`](../../../.cargo/config.toml) 只保留：

```toml
[build]
target-dir = ".build/cargo-target"
```

相对路径相对于放置 `.cargo` 的仓库根。未设置环境变量时，根 workspace 与 `apps/qusor` 的 `target_directory` 都是仓库根 `.build/cargo-target`。上面「不设置 `CARGO_TARGET_DIR` 时仍写入各自 `target/`」的后果，由这一节代替。

上游 `third_party/nsh/crates/gpui/.cargo/config.toml` 自己写了 `target-dir = "../../target"`，会盖过根配置，所以编 GPUI 仍可能写回 `third_party/nsh/target`。`cargo xtask android` 仍把运行时写死在 `third_party/nsh/target/android`，打包时 `NSH_RUNTIME` 仍是 `third_party/nsh/target`。这两处都不改上游。清掉 `third_party/nsh/target` 之后，下次打 Android 包或编 GPUI 会再生成这里。

`scripts/clean_build_cache.py` 的默认行为不变：只清理 `.build/cargo-target` 的 `incremental` 和非宿主三元组，不在每次默认清理里删除 `third_party/nsh/target`。同日按允许清单一次性删除了带 Cargo 标记的旧 `target/`。下一次本机构建是冷构建，产物进入 `.build/cargo-target`。唯一检出、固定 git rev、生产依赖和不加 `[patch]` 的决定不变。
