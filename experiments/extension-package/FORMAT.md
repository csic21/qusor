# B2 独立签名包实验格式 v1

本格式的纯验证实现已迁入生产契约 crate，仍用于本机包完整性、发布者键绑定及冻结内容资格验证，不是生产安装器或稳定发行 SDK。关联 F-EXT-01/03/05/06、EXT-B02/B04 与 EXT-P04/P08/P11；阶段与未覆盖项沿用[生命周期规格](../../docs/product/plugin-lifecycle.md)。实现入口是 [package.rs](../../crates/extension-package/src/package.rs)，反例与固定向量在[验证测试](../../crates/extension-package/tests/package.rs)。

## 容器和规范化

单个 UTF-8 JSON 文件，无压缩、无目录项、无符号链接或硬链接，也不执行解包。所有文件内容用小写十六进制字符串表示。容器只能有以下字段，字段顺序同时是 canonical JSON 顺序：

```text
{
  payload: {
    package_format,
    manifest,
    profile,
    entrypoint,
    files: [{ path, size, mime, sha256, data }, ...]
  },
  signature: {
    algorithm,
    publisher_key,
    publisher_fingerprint,
    signature
  }
}
```

上面展示字段结构，不是可直接使用的 JSON 工件。真实编码使用固定版本 serde_json 1.0.149 的紧凑结构体序列化：无缩进、无额外空白、无末尾换行；字符串按该序列化器的转义规则编码，非 ASCII 字符保持 UTF-8。数字是无符号十进制整数。解析后重新编码必须逐字节等于输入，等价的字段重排、Unicode 转义、空白变体也拒绝。格式变更须新增版本，不能自行替换为另一种 JSON canonicalization 算法。

`manifest` 直接使用权威 [extension-api::Manifest](../../crates/extension-api/src/lib.rs)，字段顺序为 manifest_version、package_id、publisher_id、package_version、host_api、required_capabilities、optional_capabilities、contributions；嵌套字段顺序也由同一权威结构体确定。它的命名空间、贡献点、能力声明和 64 KiB 大小约束全部执行。未知能力（包括未知 optional 能力）、未知贡献类型、不兼容 Host API 都拒绝；当前只兼容 API 0.1。能力与贡献数组的顺序原样保留，不重新排序。

`package_format` 固定为 1；`profile` 固定为 `wasm32-unknown-unknown-v1`。文件按 path 的 ASCII 字节升序排列，路径大小写保留。每个 file record 的 size 必须等于解码后字节数，sha256 必须等于这些字节的 SHA-256 小写 hex。MIME 仅允许小写 type/subtype token（ASCII 字母、数字、点、加号、短横线），没有参数，最多 128 字节。

entrypoint 必须精确匹配一个已列出的文件，其 MIME 必须为 application/wasm，内容以 Wasm v1 的 8 字节头 `00 61 73 6d 01 00 00 00` 开始。这里只验证容器和头部；完整 Wasm 结构、imports/exports、执行预算和宿主 ABI 由运行器继续验证。签名成功不代表模块可以启动，也不代表运行隔离成立。

## 签名、键和包标识

- 算法固定为 Ed25519，使用 ed25519-dalek 2.2.0 的 `verify_strict`；没有自造密码算法或 OpenSSL 子进程。
- 签名输入是 ASCII 字节 `QUSOR-PACKAGE-PROBE-V1`、一个零字节、再连接规范化 payload JSON 字节。签名信封自身不参与签名载荷。
- payload 包含完整 manifest、格式/profile/入口及每个文件的 path/size/MIME/SHA-256/data，修改任何一项都会使签名失效；单纯重算文件摘要不能绕过验证。
- publisher_key 是 32 字节 Ed25519 公钥的 64 位小写 hex；publisher_fingerprint 是该原始公钥字节的 SHA-256 小写 hex；signature 是 64 字节签名的 128 位小写 hex。
- 完整包 digest 是整个规范化容器（含签名证据）的 SHA-256 小写 hex。输入必须完全 canonical，因此同一 digest 对应同一冻结字节表示。
- Host 必须显式提供 trusted_key。验证器比较完整公钥、核对指纹，再验证签名；不会将包自带公钥加入信任，也不会由 publisher_id 显示名称决定可信身份。
- 调用方应依据已知 package ID 选择其绑定键。此函数不维护信任数据库、首次信任 UI、键轮换、远程撤销或真实世界身份认证；这些不由自签名替代。

## 预算和路径规则

| 边界 | 上限 |
|---|---:|
| 输入 JSON（解析前） | 9 MiB |
| 普通文件数 | 64 |
| 单文件解码后字节数 | 1 MiB |
| 全部文件解码后字节总和 | 4 MiB |
| 路径 ASCII 字节数 | 240 |
| 路径段数 | 8 |
| manifest 紧凑 JSON | 64 KiB |
| MIME 字节数 | 128 |

总输入有界，然后检查 typed JSON、canonical 编码、格式和 manifest、文件数量/声明总量，再按已核对的长度解码 hex。serde 的递归限制保持开启。未知字段和重复字段在所有 typed object（含 manifest 嵌套对象）都拒绝；不先经过会丢弃重复键的通用 JSON map。

路径只能包含 ASCII 字母、数字、`.`、`_`、`-` 和分隔符 `/`。拒绝绝对路径、反斜线、盘符/冒号、空段、`.`/`..`、末尾点、空格、非 ASCII、Windows 保留设备名（CON/PRN/AUX/NUL/COM1–9/LPT1–9，包括带后缀形式），以及超深/超长路径。拒绝重复路径、大小写折叠冲突和“一个文件同时是另一个文件的父路径”。链接没有可表示字段，添加 link/type 等字段会被拒绝。本实验读取容器内的冻结字节，不把这些路径写入主机目录树。

## 作者与 Host 的共同 API

```rust
use extension_api::Manifest;
use qusor_package_probe::package::{PackageFile, build_signed, publisher_key, verify};

let seed: [u8; 32] = /* 明确提供的作者测试 seed */;
let files: Vec<PackageFile> = /* path + mime + bytes */;
let manifest: Manifest = /* 来自权威 API 并通过校验 */;
let package = build_signed(manifest, "module.wasm", files, &seed)?;
let trusted_key = publisher_key(&seed); // 真实 Host 应从独立信任记录取得。
let verified = verify(&package, &trusted_key)?;
```

`build_signed` 自动排序文件、计算大小/摘要并确定性签名，随后使用相同 verify 入口做自检；它不写磁盘、不自动注册信任。`PackageFile` 的三个字段为公开的 String path、String mime、Vec<u8> bytes。种子生成、安全保存和生产密钥管理尚未实现；测试常量不得作为真实分发签名密钥。

`VerifiedPackage` 的字段私有，没有公开构造器或 Deserialize 实现；只有成功验证能取得。它保存独立复制的原始包字节与已解码文件。只读方法为 manifest、digest、publisher_key、publisher_fingerprint、entrypoint、profile、verifier_policy、raw_bytes、module_bytes，以及按精确路径读取 MIME/字节的 file。持久化必须写入 raw_bytes；运行必须消费 module_bytes 或重新验证已存储同一容器，不重新打开安装时的下载路径。

错误有稳定实验枚举 code：Malformed、BudgetExceeded、InvalidManifest、Incompatible、UnsafePath、FileMismatch、UntrustedKey、InvalidSignature、NonCanonical。诊断字符串固定，不回显攻击者输入或密钥材料。结构/预算失败可能先于签名校验返回，这不意味着已经接受发布者身份。

## 固定向量和实际覆盖

[deterministic_package_binds_frozen_bytes_and_all_files](../../crates/extension-package/tests/package.rs) 固定以下输入，并对完整包摘要做常量断言：

- seed 为 32 个 `07` 字节，仅供测试。
- 公钥：`ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c`。
- manifest 使用该测试中的原始 JSON：org.example.reader、org.example、版本 1.0.0、Host API 0.1、selection.read 必需能力和 inspect 选区动作。
- module.wasm 内容为 `0061736d01000000`，MIME application/wasm。
- assets/notice.txt 内容为 ASCII `immutable asset`，MIME text/plain。
- 完整包 SHA-256：`e148f545dcfdfb238981ed863daea3df518cbd9b481c8a967b982ae308050165`。

测试验证文件输入顺序不影响结果，冻结后修改原输入缓冲区不改变已验证内容；覆盖 manifest/身份/权限/文件元数据/入口篡改、模块与资产篡改及重算摘要、陌生签名键、坏签名/指纹、未知/重复字段、未知能力/贡献、格式/API/profile 不兼容、路径攻击、大小/总量/数量预算和 canonical 编码歧义。实际命令和整体验收结果由本实验入口/结果记录维护。

这不证明生产密码集成审计、生产 SDK 发行、移动渠道允许动态代码、完整多平台安装恢复或正式知识保存/卸载保留。空 Wasm 头部测试也不证明业务功能或阅读 Surface 已实现。