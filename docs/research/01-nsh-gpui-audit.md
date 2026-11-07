# NSH 平台成熟度与 GPUI 源码审查

审查日期：2026-11-07。结论：继续采用 NSH，进入有退出条件的 M-1；当前证据不足以宣布 QUSOR 五平台生产可用。本轮进行了源码与官方文档审查，没有执行五平台构建、安装或实机测试。

## 1 固定研究基线

| 对象 | 固定版本 | 含义 |
|---|---|---|
| NSH | [913fd76e1f8434ec6cdf248f4476c1ebe08af19f](https://github.com/longbridge/nsh/commit/913fd76e1f8434ec6cdf248f4476c1ebe08af19f) | 取样时main；提交2026-11-07 11:34:54 UTC；Workspace 0.4.0 |
| upstream GPUI | Zed [b5cd310b3b54dfc659fcc06453e6358843dfe216](https://github.com/zed-industries/zed/commit/b5cd310b3b54dfc659fcc06453e6358843dfe216) | 取样时main；提交2026-11-07 09:46:58 UTC；GPUI crate 0.2.2 |
| NSH GPUI backend依赖 | gpui-fast/gpui-fast-platform 0.1.13 | NSH独立GPUI Workspace依赖，不是上述upstream HEAD |

版本依据：[NSH manifest](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/Cargo.toml)、[GPUI manifest](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/Cargo.toml)、[NSH GPUI manifest](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/gpui/Cargo.toml#L51)。后续更新不得无条件沿用本报告。

证据分四级：官方声明、实际源码实现、CI配置、运行验证。只有最后一级能表示某个设备场景通过。本轮GitHub connector查询NSH固定SHA的workflow runs返回空列表，因此不宣称该SHA的CI全绿；空列表也不等于CI失败。

## 2 实际架构

```text
TSX → 编译/打包JS ES module → QuickJS engine thread
        → Rust响应式图和原生renderer → Op
        → nsh-core shadow tree / text preparation / Taffy
        → MountBatch → 平台主线程原生view或retained tree绘制
平台输入 → EngineControl事件 → engine thread → Rust/JS处理
```

[Host](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-js/src/host.rs#L110)实际持有QuickJS context/runtime、调度与资源预算；[nsh:rt](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-js/src/dom.rs#L3538)导出element/text/append/insert/prop等原生renderer操作；[Core::apply](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-core/src/core/mod.rs#L2073)应用操作，[Core::commit](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-core/src/core/mod.rs#L2556)处理文字、布局和列表并产生MountBatch。[nsh-engine](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-engine/src/lib.rs)把一个Surface的Core、Rust根或JS bundle放在专用线程。

[官方架构](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/architecture.md)描述通常主线程不等待JS，macOS窗口resize有最多16ms有界例外。长时间解析、SQLite查询和模型请求应在独立Core任务中运行，不占用NSH engine thread。

### 2.1 TSX 不等于浏览器

源码名dom.rs表达NSH节点操作，并不代表浏览器Document、Range、Selection、CSSOM、contenteditable已经实现。原生JS运行时确有Fetch、Streams、URL、Blob、AbortController等常见API，但这不是完整Web兼容承诺。[原生模块加载器](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-js/src/modules.rs)。

| 执行环境 | 可以据源码依赖 | 仍不能推导 |
|---|---|---|
| 原生TSX/QuickJS | NSH模块、renderer和已提供Web API | DOM、任意React组件或npm浏览器插件兼容 |
| Web target | 浏览器HTML/CSS及Web runtime | 五端native行为与性能相同 |
| 内嵌WebView | 系统浏览器引擎支持的DOM | 资源scheme、Worker、桥接安全和触控行为五端一致 |

PDF.js/EPUB.js只能在真实WebView阅读子系统中评估；不能直接挂到原生TSX树。

### 2.2 Rust 与 TSX 桥接

[多Rust crate设计](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/rust-modules.md)和[rust-exports示例](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/examples/rust-exports/native/src/lib.rs)展示native_module注册组件、模块、函数与类，生成TS声明，CLI链接应用Rust registry。nsh.json支持Rust crate或crate列表。

QUSOR通过单个nsh-adapter注册应用API，不把各业务crate全部暴露给UI。桥接存在仍不能证明SQLite链接、移动沙箱路径和后台任务已通过；这些必须做混合应用探针。

## 3 五平台真实支持

官方下限来自[平台表](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/README.md)，最终QUSOR下限还受阅读器与系统组件约束。

| 平台 | NSH官方下限 | 已审实现 | 当前判断 |
|---|---|---|---|
| iOS | iOS15+ | UIKit/CoreText、UITextInput/UITextInteraction、Swift/C ABI嵌入 | 有真实移动后端；真机与分发待验 |
| Android | API28 / Android9+ | JNI、Java View、StaticLayout、EditText、Choreographer/Mounter | 有真实移动后端；设备差异和生命周期待验 |
| macOS | macOS14+ | AppKit、Core Animation、CoreText | 有原生后端；本轮未运行 |
| Windows | Windows10 1809+ | 单窗口retained tree、Direct2D/DirectWrite、Win32/UI Automation | 有原生后端与native-window测试配置 |
| Linux | GTK4.14+ | GTK Widget、GSK、Pango | 有原生后端与Xvfb测试配置；发行版依赖待验 |
| 可选GPUI | 三桌面，Linux需Vulkan | GPUI Fast SurfaceElement | 独立后端，不是五端统一默认renderer |
| Web | 现代浏览器 | HTML/CSS runtime | 独立目标，不能代替移动native验收 |

“原生”不等于每节点一个标准系统控件：

- [macOS view](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-macos/src/view.rs)：layer-backed NSView，文字与Canvas进入内容layer。
- [Linux view](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/view.rs)：GTK subclass按NSH布局帧snapshot生成GSK内容。
- [Windows render](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-windows/src/render.rs)：一个Surface绘整棵retained tree，非每节点一个HWND；WebView另处理。
- [Android backend](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/src/lib.rs)：engine thread经JNI测量，批次编码后由UI thread Mounter应用。
- [iOS backend](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/lib.rs)：UIKit Surface、CoreText排版、主线程mount。

这些差异意味着某平台的大段文字/图形结果不能推广到其他平台。

## 4 移动成熟度证据

### Android

| 能力 | 源码证据 | 未解决的验收问题 |
|---|---|---|
| IME组合态 | [InputBox.java](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/InputBox.java#L26)：EditText/InputConnection处理setComposingText/Region、finishComposingText，避免JS更新破坏组合态 | 厂商中文键盘、候选选择、撤销、emoji |
| 文本选择 | [SelectableView.java](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/SelectableView.java)：Rust选区模型、JNI命中、长按/手柄/浮动菜单 | 跨屏拖动、阅读器选区映射、滚动竞争 |
| 无障碍 | [A11y.java](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/A11y.java#L14)：AccessibilityDelegate、角色/状态/动作 | TalkBack真实顺序与可编辑图谱 |
| 生命周期 | [NshView.java](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/NshView.java#L682)：fontScale/键盘/safe area/Activity callbacks | 进程回收、大文件内存、SAF持久授权 |
| 文字和ABI | [text.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/src/text.rs)、[构建脚本](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/xtask/src/android.rs#L14)：API28、arm64-v8a/armeabi-v7a/x86_64 | 具体QUSOR依赖的ABI、16KB页和包体 |

### iOS

| 能力 | 源码证据 | 未解决的验收问题 |
|---|---|---|
| 输入 | [input.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/input.rs)：UITextField/UITextView和marked text | 中文输入、动态字体、键盘遮挡 |
| 选择 | [text_selection.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/text_selection.rs)：UITextInput/UITextInteraction、放大镜/手柄，UTF-16位置 | CoreText选择到QUSOR Anchor的正确映射 |
| 字体回退 | [text.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/text.rs#L119)：CoreText cascade与CJK italic缺失处理 | 实际中文字体组合、罕见字、布局 |
| 无障碍 | [native_view.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/native_view.rs#L75)：UIAccessibility traits/actions | VoiceOver阅读/编辑闭环 |
| 构建 | [dist.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/xtask/src/dist.rs#L43)：device、Apple Silicon simulator、Catalyst、XCFramework | 真机签名安装、恢复、WebView进程回收 |

结论是移动端具有真实实现基础；本轮没有形成设备成熟度证明。必须保留每台设备的OS、SDK、SHA、安装记录与测试结果。

## 5 CI 覆盖及证据缺口

审查了固定版本14个工作流文件。

| 工作流 | 配置中安排的检查 | 不能替代 |
|---|---|---|
| [core.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/core.yml) | 共享Rust lint/test | 原生移动输入/渲染 |
| [rust-app.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/rust-app.yml) | 三桌面Rust app、headless与部分Chrome测试 | iOS/Android UI |
| [windows.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/windows.yml#L121) | crate tests、Win32窗口示例测试 | 所有IME/GPU/DPI |
| [linux.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/linux.yml#L208) | GTK/GPUI Xvfb示例测试 | Wayland和真实发行版 |
| [gpui.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/gpui.yml) | 独立GPUI Workspace三桌面检查 | 默认native backend |
| [release.yml](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/.github/workflows/release.yml#L188) | distribution、iOS/Android embedding SDK、init/build smoke | 移动包安装和交互验收 |

本轮没有在已读工作流找到完整iOS simulator/Android emulator UI job；源码却有[test_ios.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-cli/src/test_ios.rs)和[test_android.rs](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-cli/src/test_android.rs)启动器。准确表述是“具备工具与构建路径，未取得持续移动运行证据”，而非“不支持移动”。

## 6 NSH 与 GPUI 的实现差异

比较对象分为NSH默认后端、upstream GPUI、NSH可选GPUI backend。三者不能混用结论。

| 维度 | NSH默认native | upstream GPUI固定版本 |
|---|---|---|
| 编写方式 | Rust/TSX/混合；QuickJS可选 | Rust Entity、Render/RenderOnce、Element |
| 状态 | signals/memos/effects、owner/context | Entity/App/Context，notify驱动更新 |
| 布局 | shadow tree + Taffy + MountBatch | request_layout/prepaint/paint + Taffy |
| 绘制 | UIKit/AppKit/GTK及Win32 retained tree等 | Scene primitives交平台GPU renderer |
| 文字 | CoreText/StaticLayout/Pango/DirectWrite | PlatformTextSystem、WindowTextSystem和glyph sprites |
| 移动 | iOS与Android均有实现 | iOS已有实现；默认分派未见Android |
| 无障碍 | 各平台映射 | AccessKit语义树及Element a11y |
| 扩展 | 原生组件、Canvas、受限Shader、WebView | 自定义Element和底层绘制 |
| API稳定 | 0.4.0尚无已开始的兼容承诺 | pre-1.0，可能有breaking changes |

依据：[GPUI Element](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/src/element.rs#L57)、[View/Entity](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/src/view.rs)、[Scene](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/src/scene.rs#L47)、[AccessKit](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/src/_accessibility.rs)。

GPUI是hybrid immediate/retained，不应写成“每帧无条件重建所有内容”再推导性能胜负。两者均需要目标工作负载基准。

### 6.1 最新 GPUI 已有 iOS

[平台分派](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui_platform/src/gpui_platform.rs#L68)包含iOS、macOS、Windows、Linux/FreeBSD和Wasm；[gpui_ios](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui_ios/src/gpui_ios.rs)接入UIKit/CoreText/Metal，[window](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui_ios/src/ios/window.rs)有CAMetalLayer、触摸和隐藏文本输入视图，并有[text_input smoke test](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui_ios/tests/text_input.rs)。

但[iOS Platform](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui_ios/src/ios/platform.rs#L309)的文件选择和保存对话框仍明确未实现。本轮固定版本默认分派未见Android。因此不能说“GPUI完全不支持移动端”，也不能认为已有iOS crate就满足完整阅读工作流。用户已确定NSH，此比较不改变UI选择。

### 6.2 NSH 可选 GPUI backend

[后端说明](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/gpui.md)与[SurfaceElement](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/gpui/nsh-platform-gpui/src/render.rs)表明：

- 整个NSH Surface由一个GPUI Element绘retained tree。
- Canvas用tiny-skia栅格化再上传图像，不代表所有path直接GPU矢量绘制。
- Linux WebView采用wry+GTK3/WebKitGTK4.1，需要X11/Xwayland；纯Wayland有限制。
- 默认Linux native则使用GTK4/WebKitGTK6.0，二者依赖不同。

QUSOR首先验证各平台默认native backend。只有性能分解证明需要时才增加专门Surface；Core不引入GPUI依赖。

## 7 阅读和编辑能力的含义

原生WebView确实存在：[iOS](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-ios/src/web.rs)、[Windows](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-windows/src/native/web.rs)、[Linux](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-linux/src/native/web.rs)、[Android](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/crates/nsh-platform-android/java/com/longbridge/nsh/WebBox.java)。这支持专业Web阅读子系统的可行性研究；本地资源、Worker、注入时序、frame权限和选区仍须逐个平台验收。

[RichEditor](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/rich-editor.md)是Rust ProseMirror架构移植与Tiptap风格extension层，包含schema/step/transaction/history与native view；不等于npm Tiptap插件可直接运行。其UTF-16临时位置不能代替稳定block ID。即便编辑内核不依赖view，也不得让业务Core直接依赖nsh-base。

[Canvas](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/canvas.md)提供2D子集和命中辅助，但无限画布还需要相机、空间索引、裁剪/LOD、后台布局、撤销、持久化及可访问语义。CanvasNode的无障碍暴露在部分backend受限，必须有原生列表/检查器。具体路线见[阅读与画布设计](./02-reading-canvas-routes.md)。

## 8 兼容性与许可

[compatibility.md](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/docs/dev/compatibility.md)明确0.3.0和0.4.0尚未进入首次兼容性记录，因此未来兼容目标不能视为当前保证。M-1固定完整SHA、lockfile、Rust/平台SDK；升级只通过nsh-adapter，先跑桥接/IME/定位/Surface契约测试。热更新用于开发，不作为发布包运行依赖。

NSH根[Apache许可](https://github.com/longbridge/nsh/blob/913fd76e1f8434ec6cdf248f4476c1ebe08af19f/LICENSE-APACHE)与[GPUI manifest](https://github.com/zed-industries/zed/blob/b5cd310b3b54dfc659fcc06453e6358843dfe216/crates/gpui/Cargo.toml)均为Apache-2.0；RichEditor移植保留上游ProseMirror/Tiptap MIT来源说明。最终依赖和二进制需要SBOM/notices，不能以仓库根许可替代依赖审计。

## 9 必须进入 M-1 的验证

| 项目 | 必须保存的证据 | 通过条件 |
|---|---|---|
| 五端混合启动/分发 | 固定版本、设备/OS/SDK、日志、安装记录 | 冷启动/关闭/重开；移动至少各一台真机 |
| Rust↔TSX | 请求/事件/错误/取消测试 | 无长任务阻塞，无关闭后悬空回调 |
| SQLite | 写入/重启/异常退出/导出 | 原子、持久、符合平台目录与生命周期 |
| 中文/字体 | 五端IME及CJK/emoji/DPI样本 | 不丢字、不重复、选择位置一致 |
| 阅读Surface | 断网PDF/EPUB与选区回跳 | 安装包离线可读、引用精确、恢复正确 |
| 富文本/Markdown | round-trip/undo/IME fixtures | 无静默内容丢失，可迁移 |
| 画布 | 帧时间/可见规模/内存 | 达到任务清单预算且输入正确 |
| 无障碍 | NVDA/VoiceOver/TalkBack/Orca记录 | 核心导航阅读可达；图谱有语义替代 |
| 生命周期 | 后台/回收/恢复 | 已确认保存不丢失、不重复 |

五平台启动、核心输入或精确引用出现阻断时，不启动完整M0。只有build证据的格子保持“待运行”。高级RichEditor可回退已验证Markdown，画布规模可以明确限制；PDF/EPUB基础能力不能悄悄改成纯文本后算通过。详见[阶段清单和风险](./06-milestones-risks.md)。
