# ADR 0040：组合顺序驱动的 RuntimeScope 与统一 Host Runtime

- 日期：2026-11-12
- 状态：本机实现并通过真实窗口回归；随应用包启停后的整体重建已完成，单插件热卸载和 UI贡献独立挂载未完成
- 关联：C-07、C-11、C-12、F-EXT-01/03/04/06
- 承接：[ADR 0038](0038-capability-composition.md)、[ADR 0039](0039-single-core-provider.md)

## 背景

共享 Core 之后，NotesHost 和 VocabHost 仍各自持有进程级 `RUNTIME: Mutex<Option<...>>`，各自在首次调用时验包、授权、建 Session、持 Core bridge。这样虽然能工作，却重复了：

- 启动顺序判断；
- 运行时互斥和失败处理；
- Core/Host 的生命周期所有权；
- 禁用/缺失 entry 的错误路径。

如果继续为 reader/library 复制这套静态初始化，插件化会在 Host 层重新变成“每个功能一套启动代码”。

## 决策

1. **在 `capability-composition` 增加通用 `RuntimeScope<T>`**：
   - 只接受计划中启用且存在的 entry；
   - 按计划顺序挂载，拒绝重复和逆序挂载；
   - 保存挂载层来源；
   - `unmount_all` 按激活逆序释放值。
   它是纯生命周期容器，不认识 Host、Core、NSH 或具体插件。

2. **生产应用增加唯一 `runtime.rs` composition root**：
   - `AppService` 枚举保存 Core、Notes Host、Vocab Host 的运行时值；
   - `AppRuntime::boot` 按 composition plan 顺序创建并挂载 Host-side entry；
   - Notes/Vocab native 模块不再各自初始化 Host，只通过 `with_notes`/`with_vocab` 访问统一运行时；
   - 运行时锁覆盖挂载和调用，避免两个线程同时 boot 两个 Core writer。

3. **Core 与 Host 生命周期统一**：
   - `VaultStore` 由 Core entry 的运行时值持有；
   - Notes/Vocab 从该值取得 clone；
   - `AppRuntime` drop 时逆序卸载 Host，再释放 Core；
   - Host 包验签、授权、Session 和 bridge 仍由各自模块创建，不合并权限。

4. **UI contribution 暂不伪挂载**：当前 profile 的 workspace entries 仍是声明中的消费者，但 TSX fixture 尚未通过 runtime 注册表发现/卸载。`runtime.rs` 只挂载 core、notes.host、vocab.host；不把没有执行路径的 UI entry 假装成已挂载。

## 验证

- `capability-composition`：新增 RuntimeScope 顺序、重复、逆序、unknown 和逆序释放测试。
- 应用原生测试：统一 runtime 下 Notes/Vocab 的旧分库迁移、读取、应用重开仍通过。
- `nsh check` 通过；真实打包窗口在隔离 HOME 下再次打开“想法/背单词”，共享 Core 数据保持可读。

## 未验证

- 单插件热卸载而非整体重建、任意用户包安装/卸载、第三方 UI contribution 的独立发现/释放；
- 迁移冲突的 UI 提示、带图标正式包、其他平台；
- RuntimeScope 的 `T` 类型服务发现（当前由应用显式 accessor 提供，未做通用 Any 注入）。
