# 独立实验作者工具

关联 F-EXT-01、F-EXT-05、F-EXT-06 与 EXT-P11 的局部验证。此目录拥有独立 Cargo Workspace 和锁文件，只依赖公开的 wat、sha2、serde_json，不依赖 QUSOR crate、主 Workspace 或源码相对路径。复制本目录到其他位置仍可构建工具；调用方另行提供 WAT 与公开 manifest。

在本目录准备依赖并编译：

```sh
cargo fetch --locked
cargo build --locked --offline
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
```

工具命令为 `build-plugin <source.wat> <manifest.json> <output-dir>`。在本目录使用实验提供的样例：

```sh
cargo run --locked --offline -- ../fixtures/valid.wat ../fixtures/manifest.json ../packages/valid
cargo run --locked --offline -- ../fixtures/valid-v2.wat ../fixtures/manifest-v2.json ../packages/valid-v2
```

也可以从仓库根目录调用：

```sh
cargo run --manifest-path experiments/extension-wasm/author/Cargo.toml --locked --offline -- experiments/extension-wasm/fixtures/valid.wat experiments/extension-wasm/fixtures/manifest.json experiments/extension-wasm/packages/valid
```

工具将 WAT 编译为 Wasm，保留 manifest 原始 JSON 字节，并生成三个固定文件：

- `module.wasm`：编译后的模块。
- `manifest.json`：作者提供的策略 manifest。
- `package.json`：`format_version=1`、`entry="module.wasm"`、模块 SHA-256 小写十六进制摘要。

输入解析成功后才创建输出文件。同一路径重复构建会覆盖这三个文件；I/O 失败仍可能留下不完整目录，后续必须由 runner 全量检查，不能把此工具当作事务化安装器。模块摘要只检测模块字节变化，没有认证发布者，也没有绑定 manifest；没有签名、信任存储、归档提取或生产升级恢复。

WAT 编译成功不证明模块符合宿主 ABI 或权限。manifest 只在打包时检查 JSON 语法，未知字段、重复字段和权限语义由 runner 使用公开协议进行严格校验。工具保留原始 JSON，避免将重复字段静默归一化后绕过宿主拒绝规则。

WAT fixture 使用实验 ABI：导出 `memory`，导出 `run() -> (i32 pointer, i32 length)`；宿主唯一入口为 `qusor.call(request_pointer, request_length, output_pointer, output_capacity) -> i32`，全部参数与结果均为 i32。正常样例将宿主实际返回的 UTF-8 字节添加前缀，不包含合成选文正文。ABI 与预算是实验契约，尚未冻结生产 SDK。