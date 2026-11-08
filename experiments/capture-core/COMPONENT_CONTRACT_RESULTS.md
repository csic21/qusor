# 0.3 组件契约与同Host临时挂载增量

日期：2026-11-08。关联C-09/C-12、F-EXT-01/03/04/05/06与F-NOTE-01。本轮完成wire、Host/Store接线和兼容验证；不代表普通提供者已经执行、native组件替换或正式功能包交付。

## 实现

- extension-api新增显式0.3、精确Draft与NoteDraftEdit、分块编辑、有限原生descriptor和own-invocation输入事件。旧0.1默认和0.2阅读能力不扩大，旧请求直接拒绝新Draft词汇；旧schema与fixtures保持原样。
- 同一Host安装目录拥有临时mount，普通grants/Session不能获得新能力。统一persistent安装映射保留epoch及Closing配额，未完成旧ID卸载证明和真实资源释放时拒绝换ID注册。
- extension-store验证完整签名composition资产，bind_component_vault只写库亲和绑定，不写Draft授权或伪造enable状态。用户明确仅本会话启用并挂载，每次回调重核当前安装、签名、库、private/pending与worker事实。停止/更新使旧mount无效。
- 应用草稿继续持有文本/来源/原capture generation，保存时仍需原capture权限重新判定。当前新mount路径尚未装配到应用草稿或原生工作区。

## 实际验证

| 范围 | 结果与证据 |
|---|---|
| 根Workspace | 177项Rust测试、四组真实process controller场景通过；Clippy、格式和架构守卫通过；42项架构反例测试通过 |
| Wire | 33项API测试；最坏控制字符及最大标识的序列化回复仍≤4096；四新schema合法样例/未知字段检查通过 |
| Host | 8项组件策略、5项持久组件facade，加既有reading回归通过；资源ack在此为可信策略输入，不是native实测 |
| Store | 3项签名包/真实数据库测试，包含无grant重开、库隔离、缺asset拒绝及停止后旧mount拒绝 |
| 旧包 | [543场景与仓库外作者](../extension-package/evidence/linux-component-wire-package.json)通过 |
| 旧Wasm | [20场景](../extension-package/evidence/linux-component-wire-wasm.json)通过 |
| 完整捕获 | [314命令、286 CLI、8 native案例](evidence/linux-component-wire-compound.json)通过 |
| 生产Controller | [新二进制九场景](evidence/linux-component-wire-supervised.json)通过，124份输入前后hash一致 |
| 公开SDK | [仓库外作者](evidence/linux-component-sdk-author.json)只用导出契约编译、签名与校验0.3包，旧Host拒新包；没有执行该包中的Wasm |

兼容日志位于本机`/tmp/qusor-component-wire-*.log`；C9旧资格及原log已恢复保存，新资格使用独立文件名。新源码与schemas/SDK工具共133份已逐项hash封存，见 [归档核对](evidence/component-contract-source-archive.json)。本机target归档不是已提交或发布的发行工件。

## 后续必须完成

当前Context只是wire，尚无实际事件缓冲、跨VM状态续传或提供者执行装配。fresh VM、每次8calls与1MiB文本之间必须形成可运行闭环：草稿效果和事件checkpoint需要原子推进，Begin/Append/Commit响应丢失后可读回同一结果，不能靠另一次非原子checkpoint猜进度。

下一片复用同一ManagedRun生命周期，增加组件mount authority类型；补Host own-context gate和应用有界事件/原子续传。随后使用两个仓库外普通签名Wasm提供者完成实际文本输入/段落换位、A→B→A、1MiB多VM、中断续传、明确保存与Core恢复。段落业务保留guest，不能用Host硬编码或降低容量代替。完整实施契约见 [规格](../../docs/product/component-provider-contract.md)。

既有 [原生UI资格](../nsh-reader/dynamic-pdf/WORKSPACE_RESULTS.md)属于已封存的119源/旧二进制，不包含本轮0.3接线。中文IME、Esc、真实native释放ack、长书/EPUB及其他平台仍未完成。
