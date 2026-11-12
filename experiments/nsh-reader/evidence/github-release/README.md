# GitHub draft Release 首次验收

日期：2026-11-12。关联 C-17、F-PLAT-02；方案见[发行与更新](../../../../docs/product/release-and-update.md)，装配见 [ADR 0037](../../../../docs/architecture/adr/0037-production-app-assembly.md)。

## 结果

GitHub Actions 在 `main@e1c025e` 手动运行 [Release workflow](https://github.com/csic21/qusor/actions/runs/43136573631)，`Package (macos)` 与 `Publish the release` 均成功，创建 draft `v0.1.0`。此前同流程在 [run 42995272099](https://github.com/csic21/qusor/actions/runs/42995272099) 已成功打包，却因未匹配的 `*.zip` glob 被当成字面路径传给 `nsh update-manifest` 而失败；`e1c025e` 的 `nullglob` 修复消除了该失败。

Release 包含三项资产：

| 资产 | 大小 | SHA-256 |
|---|---:|---|
| `QUSOR-0.1.0-macos-aarch64.tar.gz` | 21,781,955 | `0f483e2c75eac23416dce5438eee181f083b6b96db23306e9d023cad4e52b118` |
| `latest.json` | 312 | `f622e2ba9468c1d0accdbea5882468975d81f2a17b640ca33bd88e5974a67c4c` |
| `latest.json.sig` | 89 | `dd7a546417d39438a441e64f5e08c71f73560935ad0e59e972516643f8faa256` |

## 实际检查

- 下载 draft 的三项资产；清单中的 macos-aarch64 字节数与 SHA-256 和归档一致。
- 从应用包内 `nsh.json` 的 `updater.publicKey` 构造 Ed25519 公钥，用 OpenSSL 独立验证签名。固定 NSH 对 `latest.json` 的签名载荷带域隔离前缀 `nsh-updater-manifest\0`；按该 framing 验证成功。
- 归档可正常解出 `QUSOR.app`；`Info.plist` 为 `org.qusor.app`、`0.1.0`、build `1`，可执行文件为 arm64 Mach-O。
- `codesign --verify --deep --strict` 通过；签名类型为 ad hoc，没有 Team ID。
- 把下载产物解出到临时目录后实际启动，进程创建了原生窗口（窗口 id 4715），侧栏显示 `0.1.0`，随后正常退出。窗口截图见 [release-app.png](release-app.png)。
- 同一工作树最终源码的完整更新自测与根 Workspace 回归另行通过；机器数据见 [record.json](record.json)。

## 未覆盖

- Release 仍是 draft，没有发布为 latest；因此 `releases/latest/download/latest.json` 的真实免认证拉取与已发布应用的在线更新尚未验证。
- 只有 ad hoc 签名，没有 Developer ID、notarization 或 Gatekeeper 放行；没有 Windows/Linux/移动端发行。
- 首次云端构建来自 `e1c025e`。工作树后续对更新源显示、证据标记和 workflow 的收紧尚未进入该 draft。
- 证据是本机一次验收，不是独立维护者对同一冻结产物的复跑。
