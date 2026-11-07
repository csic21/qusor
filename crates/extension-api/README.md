# extension-api：0.1 策略契约

关联 F-EXT-01、F-EXT-03、F-EXT-04、F-EXT-06。本 crate 提供可执行校验的 JSON DTO：权限声明、贡献注册元数据、授权查询和错误。它供宿主策略及后续 SDK 绑定使用；当前没有执行插件、读取选文、提交捕获、安装包或渲染 UI。

[JSON Schema](schema/policy-v0.1.schema.json) 是当前策略契约，尚不是完整插件包 manifest。运行 profile、包摘要、签名、发布者信任、迁移和独立包分发仍须后续验证。设计边界见 [插件架构](../../docs/architecture/extensions.md)。

## 输入与输出

- `Manifest::from_slice` / `from_json`：先限制整个输入的字节数，再解析和校验。`to_json` 同样执行校验与输出限制。
- `CapabilityRequest` 和 `ApiError` 提供同名入口。
- 直接使用 Serde 的调用者必须在接受值前调用 `validate`，并自行限制输入大小。字段公开允许宿主构造值，不代表这些值已经有效。
- 所有对象拒绝未知字段、重复字段、缺失字段及错误数值类型。未知能力、贡献种类、资源种类和错误码均拒绝。整数按 Rust 的 `u16` / `u32` 范围校验，不接受小数或字符串编码。
- 解析失败仅返回稳定类别和通用安全消息，不把未知字段名、载荷值或原始解析诊断回显到公开错误。

请求中的 `request_id` 是关联标识，不是授权凭证或幂等写入保证。请求没有插件自报 actor、installation、session 或 grant epoch 字段；这些身份与生命周期信息由宿主可信上下文提供。

## 版本与权限

`HOST_API` 为 0.1；授权查询只接受该精确版本。manifest 当前版本为 1；`host_api` 表示同一 major 内的闭区间。manifest 校验只检查区间有序，宿主用 `supports(HOST_API)` 决定实际兼容性。协议版本、manifest 版本和数字三元组 package 版本独立；没有将 Rust ABI 作为 wire 契约。

manifest 的 required/optional 能力列表不能重复或相交。贡献需要的能力必须在其中声明，可以引用 optional 能力；该贡献能否启用由宿主结合实际可用能力及授权决定。贡献 ID 必须位于 `package_id + "."` 下，且包内唯一。`publisher_id` 只是声明，不证明签名或发布者身份，也不自动授予权限。

宿主检查以下能力和资源组合，以及实际 grants、资源范围和会话状态：

| 能力 | 合法资源 | 当前意义 |
|---|---|---|
| `selection.read` | Selection | 评估选区读取是否被策略允许 |
| `document.metadata.read` | Document、Vault | 评估资料元数据读取是否被策略允许 |
| `capture.propose` | Vault | 评估提出捕获候选是否被策略允许 |

这些授权查询不返回真实知识内容，也不提交业务副作用。`capture.propose` 不能替代正式知识写入所需的审核、事务和审计。

## 限制

| 项目 | 限制 |
|---|---|
| manifest JSON | 65,536 bytes，包括空白与编码转义 |
| request / error JSON | 4,096 bytes，包括空白与编码转义 |
| package / publisher namespace | ASCII、最多 128 bytes；至少两段，段格式 `[a-z][a-z0-9-]*`，以 `.` 分隔 |
| contribution ID | 相同 namespace 语法，最多 192 bytes，并以包 namespace 加 `.` 开头 |
| request / resource ID | 最多 128 ASCII bytes；字母或数字起始，后续可含字母、数字、`.`、`_`、`:`、`-` |
| title | 非空白、无 Unicode 控制字符，最多 256 UTF-8 bytes |
| error message | 非空白、无 Unicode 控制字符，最多 512 UTF-8 bytes |
| contribution 数量 | 每 manifest 最多 64 项 |
| 每份能力列表 | 最多 3 项，不重复；当前只有三种能力 |

opaque ID 不解释为文件路径或 URL；真实资源存在性与归属由后续受信任能力适配器核验，当前宿主只检查库绑定与授权资源标识，不读取真实资源。字符串不执行隐式大小写转换或 Unicode 归一化。

JSON Schema 用于结构校验与编辑器提示；它不能完整表达 UTF-8 字节预算、字段间比较、动态 namespace 前缀、跨列表集合关系及按 ID 唯一性。JSON Schema 的数字语义也可能接受 `1.0` 这类数值等价表示，而 Rust wire 整数解码更严格。因此 wire 入口是最终判定，不能用仅通过 Schema 来绕过运行时校验。

## 可复现示例与检查

[manifest](fixtures/manifest.json)、[授权请求](fixtures/request.json)、[错误](fixtures/error.json) 均由 [协议测试](tests/wire.rs) 实际解析和往返编码。这些 fixture 是合成策略输入，不是已运行的插件包。

```sh
cargo test -p extension-api --locked --offline
cargo clippy -p extension-api --all-targets --locked --offline -- -D warnings
```

测试覆盖版本协商边界、命名空间越界、未知字段和能力、重复声明、资源形状、整数越界、UTF-8 字节预算及错误回显。通过这些测试只证明当前 DTO 与校验代码，不能证明运行隔离、第三方包分发、业务持久化或 NSH 平台验收。
