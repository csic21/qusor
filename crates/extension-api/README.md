# extension-api：版本化策略、Surface与组件wire

关联 F-EXT-01、F-EXT-03、F-EXT-04、F-EXT-06。本 crate 提供可执行校验的 JSON DTO：权限声明、贡献注册元数据、授权查询和错误。它供宿主策略及后续 SDK 绑定使用；当前没有执行插件、读取选文、提交捕获、安装包或渲染 UI。

[旧0.1 Schema](schema/policy-v0.1.schema.json) 保留不变；[0.2策略](schema/policy-v0.2.schema.json) 与 [Surface v1](schema/surface-v1.schema.json) 独立描述新增词汇。这些不是完整插件包或授权凭据；签名格式由extension-package提供，运行、业务与平台仍分别验证。设计边界见 [插件架构](../../docs/architecture/extensions.md)。

## 输入与输出

- `Manifest::from_slice` / `from_json`：先限制整个输入的字节数，再解析和校验。`to_json` 同样执行校验与输出限制。
- `CapabilityRequest` 和 `ApiError` 提供同名入口。
- 直接使用 Serde 的调用者必须在接受值前调用 `validate`，并自行限制输入大小。字段公开允许宿主构造值，不代表这些值已经有效。
- 所有对象拒绝未知字段、重复字段、缺少必需字段及错误数值类型。未知能力、贡献种类、资源种类和错误码均拒绝。整数按 Rust 的 `u16` / `u32` 范围校验，不接受小数或字符串编码。
- 解析失败仅返回稳定类别和通用安全消息，不把未知字段名、载荷值或原始解析诊断回显到公开错误。

请求中的 `request_id` 是关联标识，不是授权凭证或幂等写入保证。请求没有插件自报 actor、installation、session 或 grant epoch 字段；这些身份与生命周期信息由宿主可信上下文提供。

## 版本与权限

`HOST_API` 保持旧默认0.1；`HOST_API_V0_2` 与 `SUPPORTED_HOST_APIS` 显式声明新增协议。`Manifest::supports_api` / `negotiate_api` 同时核对数值区间与该版本可表达的功能词汇；`validate_supported_apis` 可在打开存储前检查宿主配置。任何reader.surface（含optional）或ReaderWorkspace声明必须以0.2为最低版本，不能声称支持旧0.1。manifest仍为1，API连接实际版本由Host冻结，授权查询必须匹配其会话；wire接受0.2不等于旧会话自动升级。协议版本、manifest 版本和数字三元组 package 版本独立；没有将 Rust ABI 作为 wire 契约。

manifest 的 required/optional 能力列表不能重复或相交。贡献需要的能力必须在其中声明，可以引用 optional 能力；该贡献能否启用由宿主结合实际可用能力及授权决定。贡献 ID 必须位于 `package_id + "."` 下，且包内唯一。`publisher_id` 只是声明，不证明签名或发布者身份，也不自动授予权限。

宿主检查以下能力和资源组合，以及实际 grants、资源范围和会话状态：

| 能力 | 合法资源 | 当前意义 |
|---|---|---|
| `selection.read` | 0.1/0.2的Selection；0.2的精确Document | Document语义仅为该Surface中由真实用户产生的当前selection lease，broker还须核对handle/来源/世代，不允许任意范围读取 |
| `document.metadata.read` | Document、Vault | 评估资料元数据读取是否被策略允许 |
| `capture.propose` | Vault | 评估提出捕获候选是否被策略允许 |
| `reader.surface` | 仅0.2精确Document | 受控原生呈现；不自动授予选区、原始文件或数据出站权限 |

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
| 每份能力列表 | 最多4项且不重复；旧0.1仍只可表达原三种能力 |

opaque ID 不解释为文件路径或 URL；真实资源存在性与归属由后续受信任能力适配器核验，当前宿主只检查库绑定与授权资源标识，不读取真实资源。字符串不执行隐式大小写转换或 Unicode 归一化。

JSON Schema 用于结构校验与编辑器提示；它不能完整表达 UTF-8 字节预算、字段间比较、动态 namespace 前缀、跨列表集合关系及按 ID 唯一性。JSON Schema 的数字语义也可能接受 `1.0` 这类数值等价表示，而 Rust wire 整数解码更严格。因此 wire 入口是最终判定，不能用仅通过 Schema 来绕过运行时校验。

## 可复现示例与检查

[manifest](fixtures/manifest.json)、[授权请求](fixtures/request.json)、[错误](fixtures/error.json) 均由 [协议测试](tests/wire.rs) 实际解析和往返编码。这些 fixture 是合成策略输入，不是已运行的插件包。

```sh
cargo test -p extension-api --locked --offline
cargo clippy -p extension-api --all-targets --locked --offline -- -D warnings
```

测试覆盖版本协商边界、命名空间越界、未知字段和能力、重复声明、资源形状、整数越界、UTF-8 字节预算及错误回显。通过这些测试只证明当前 DTO 与校验代码，不能证明运行隔离、第三方包分发、业务持久化或 NSH 平台验收。


## 受控Surface与调用参数

`surface::SurfaceRequest` 只有Open、Configure、ReadSelection、Close，Surface协议1明确要求Host0.2。Open绑定精确Document与期望SourceVersion；配置仅传有界页码/倍率意图。请求没有raw指针、frame-ready、任意路径、actor或用户批准字段。Host原生ticket与这些可反序列化的wire元数据是不同类型。

`SurfaceDescriptor` 包含来源、页码/总页数、世代、加载状态与有界frame元数据。Ready必须有frame，其他状态不能暴露frame或selection lease。选区响应保留完整版本1/2草案JSON字符串；API不解释Core草案为事实，原生服务必须在发布lease前用对应Core契约校验。完整编码响应含转义仍受4096-byte预算，超限拒绝而非截断首段。

`invocation::InvocationRequest` 只读取当前trusted调用绑定的命令参数，不能选择其他invocation、actor或resource。broker在返回参数前重新检查当前session及该操作所需能力/资源；参数不是授权、原生输入或批准。响应按命令的typed wire直接解析（阅读命令为SurfaceRequest），不通过Value重序列化丢失重复字段。固定签名Wasm可读取这些动态参数，不修改包内module字节。

[6项新增wire测试](tests/surface.rs) 覆盖显式版本、旧范围拒新词汇、Document选区语义版本门、封闭操作/伪原生声明、digest/世代/帧状态、完整响应预算与调用参数身份隔离。实际Host授权、原生输入和普通插件通路由相应消费者另行验证；本crate的DTO测试不代表正式SDK发布或产品旅程通过。

## 显式0.3组件契约

新增 [0.3策略schema](schema/policy-v0.3.schema.json)、[Draft](schema/draft-v1.schema.json)、[组件descriptor](schema/composition-v1.schema.json) 与 [own-invocation Context](schema/component-context-v1.schema.json)。仅NoteDraftEdit可用于精确Draft，旧能力未自动扩展到0.3；0.1/0.2请求携带新Draft词汇直接拒绝。普通Session授权不代替临时mount。

Draft请求至多64KiB、每文本块512bytes、完整响应4KiB、单想法文本1MiB；composition资产16KiB，Context请求/回复4KiB。直接Serde仍须先限帧并调用validate，JSON Schema不代替运行时UTF-8字节预算和实际授权。两种有限native编辑root不携带URL或任意控件属性。Context身份由可信run/mount解析，不从guest自报。

这是尚未发布SDK的接线切片，Context事件缓冲与多VM原子续传、普通Wasm提供者和native替换仍待实现。当前验证见 [本轮结果](../../experiments/capture-core/COMPONENT_CONTRACT_RESULTS.md)。

## 只读段落投影（实施切片）

`projection`与`paragraph-projection-v1.schema.json`提供独立API0.3/protocol1的own context、query、CAS/checkpoint及最多8行的span窗口。没有文本effect；`window`必须显式给出对象或null。窗口ready由context推导，UTF-8边界/前态推进/fresh权限由应用与协调器继续校验；结构不证明段落语义或EOF诚实。Host/应用局部检查不等于普通guest或原生界面已接通，详见[ADR0030](../../docs/architecture/adr/0030-readonly-paragraph-projection.md)。
