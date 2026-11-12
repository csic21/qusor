# GitHub draft Release 首次验收

日期：2026-11-12。关联 C-17、F-PLAT-02；方案见[发行与更新](../../../../docs/product/release-and-update.md)，装配见 [ADR 0037](../../../../docs/architecture/adr/0037-production-app-assembly.md)。

## 结果

GitHub Actions 在 `main@62a82e6` 手动运行 [Release workflow](https://github.com/csic21/qusor/actions/runs/43142491761)，`Package (macos)` 与 `Publish the release` 均成功，创建 draft `v0.1.0`，`targetCommitish` 精确指向本次 SHA。此前同流程在 [run 42995272099](https://github.com/csic21/qusor/actions/runs/42995272099) 已成功打包，却因未匹配的 `*.zip` glob 被当成字面路径传给 `nsh update-manifest` 而失败；后续 `nullglob` 修复与本次验证均消除了该失败。

Release 包含三项资产：

| 资产 | 大小 | SHA-256 |
|---|---:|---|
| `QUSOR-0.1.0-macos-aarch64.tar.gz` | 21,786,549 | `bbac070c482e7464ba963499db2b63b06c1db746294ea7b9452c771da82155e1` |
| `latest.json` | 312 | `a46cdbe005ad9df5a389fb5319f12dc2cb71f73db10218ee2be3aea9eab07e43` |
| `latest.json.sig` | 89 | `d25be11166c4bee384813ce88cac82ae21d5ff2e8724db54ccbb95aa90d193aa` |

## 实际检查

- 下载 draft 的三项资产；清单中的 macos-aarch64 字节数与 SHA-256 和归档一致。
- 从应用包内 `nsh.json` 的 `updater.publicKey` 构造 Ed25519 公钥，用 OpenSSL 独立验证签名。固定 NSH 对 `latest.json` 的签名载荷带域隔离前缀 `nsh-updater-manifest\0`；按该 framing 验证成功。
- 归档可正常解出 `QUSOR.app`；`Info.plist` 为 `org.qusor.app`、`0.1.0`、build `1`，可执行文件为 arm64 Mach-O。
- `codesign --verify --deep --strict` 通过；签名类型为 ad hoc，没有 Team ID。
- 把下载产物解出到临时目录后实际启动，进程创建了原生窗口（窗口 id 4837），侧栏显示 `0.1.0`，随后正常退出。窗口截图见 [release-app.png](release-app.png)。
- 同一工作树最终源码的完整更新自测与根 Workspace 回归另行通过；机器数据见 [record.json](record.json)。

## 未覆盖

- Release 仍是 draft，没有发布为 latest；因此 `releases/latest/download/latest.json` 的真实免认证拉取与已发布应用的在线更新尚未验证。
- 只有 ad hoc 签名，没有 Developer ID、notarization 或 Gatekeeper 放行；没有 Windows/Linux/移动端发行。
- 为满足提交时间约束，`main` 历史已归一化；draft 由归一化后的 `62a82e6` 重新构建，源码内容与时间整理前冻结 tree 一致。
- 证据是本机一次验收，不是独立维护者对同一冻结产物的复跑。
