# Android APK 构建、安装与实机界面资格

日期：2026-11-12。关联 F-PLAT-01、F-EXT-04/06、F-LEARN-01；承接 [Android 打包预检](../android-packaging-preflight/README.md)。本文记录的是调试签名 arm64 APK 在 Android 模拟器上的实际构建、安装与交互，不是商店发行或真机资格。

## 环境与工件

- macOS arm64；固定 NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。
- Android Studio JBR 25.0.2、Android SDK 35/36、NDK `27.2.12479018`。
- Rust stable `1.99.1`（`aarch64-linux-android` target）。
- 模拟器：Android Emulator `35.4.9.0`，AVD `Medium_Phone_API_36`，Android 16 / API 36，arm64-v8a。
- APK：`/private/tmp/qusor-android-import-final/QUSOR.apk`；48,894,720 bytes；SHA-256 `1711353a3a80c4607621277ae13d5360114211325289855f0de9139a28238e66`。
- 包：`org.qusor.app` 0.1.0（versionCode 1），minSdk 28，targetSdk 35，启动 Activity `com.longbridge.nsh.AppActivity`，debug key 签名。

构建命令：

```sh
python3 apps/qusor/prepare.py
cd apps/qusor
env RUSTUP_TOOLCHAIN=stable \
  JAVA_HOME='/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
  PATH='/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin:/Users/karl/.cargo/bin:/Users/karl/Library/Android/sdk/platform-tools:/usr/bin:/bin:/usr/sbin:/sbin' \
  ANDROID_HOME='/Users/karl/Library/Android/sdk' \
  ANDROID_NDK_HOME='/Users/karl/Library/Android/sdk/ndk/27.2.12479018' \
  ../../experiments/nsh-reader/target/nsh/target/debug/nsh package \
    --target android --abi arm64-v8a --out /private/tmp/qusor-android-import-final
```

安装与启动：

```sh
adb install -r /private/tmp/qusor-android-import-final/QUSOR.apk
adb shell monkey -p org.qusor.app -c android.intent.category.LAUNCHER 1
```

## 阻断根因更新

前次预检把 `can't find crate for rquickjs_macro` 归为固定 NSH/Android 组合问题。进一步复现后，最小 Rust 工程同样在 macOS release 下生成无法 `dlopen` 的 proc-macro dylib：

```text
mis-aligned LINKEDIT string pool
```

同一最小工程用 Rust `1.99.1` 构建和加载正常。本次没有修改 NSH 源码或 QUSOR 架构；只在 Android 打包入口显式选择 stable。该结论修正了“Android 阻断来自 NSH 源码”的旧判断。

## 实际结果

| 范围 | 结果 |
|---|---|
| APK 构建 | Rust app、TSX 97.8 KB / bytecode 118.2 KB、`libnsh.so` arm64-v8a 和 APK 全部实际生成 |
| 安装与启动 | `adb install` 成功；进程存在；`dumpsys activity` 显示 `AppActivity` 为 topResumedActivity |
| 手机首屏 | 修复首轮“侧栏与正文同时横排、正文被挤到右侧”的缺陷后，默认显示单栏产品设计概览 |
| 响应式导航 | 窄屏打开导航时占满可用宽度；返回键先关闭导航，进程不退出 |
| 插件返回 | 从导航打开“想法”，系统返回键回到宿主工作区，未杀死应用 |
| 真实 Core 读取 | 想法工作区显示“本机 Core · 由 notes 插件读取”；空 Core 显示诚实的 0 条空态 |
| 背单词真实 Core | 首次启动在空学习库中幂等安装自编“QUSOR 入门 12 词”；词书标注“本机 Core · 由 vocab 插件读取”，学习卡、揭晓和四档评分可触控 |
| Core 评分持久化 | 对 `abandon` 选择“认识”，回执显示“下次 3.0 天后”，词书新词从 12 降为 11；强制停止并重开应用后仍为 11，没有重复词书或重复评分 |
| 用户词书导入 | 通过 Android 系统文件选择器选择 2 词 JSON，成功新增“我的英语笔记”，来源仍为本机 Core，并自动进入 `portable` 学习卡 |
| 导入持久化 | 强制停止并重开应用后，词书列表仍为“QUSOR 入门 12 词”与“我的英语笔记”两本，来源为本机 Core |
| 横屏 | 学习卡内容在独立滚动区中可达；横屏时滚动后能查看释义、例句和评分区 |
| 崩溃检查 | 本轮操作后进程仍存在；筛过的 logcat 未见 `FATAL` / `AndroidRuntime` / Rust panic |

首轮布局缺陷与修复后截图：

| 文件 | 内容 |
|---|---|
| [before-responsive.png](before-responsive.png) | 修复前：侧栏占宽、正文横向裁切 |
| [overview-portrait.png](overview-portrait.png) | 修复后：手机竖屏单栏概览 |
| [navigation-portrait.png](navigation-portrait.png) | 全宽导航；返回键可关闭 |
| [notes-mobile.png](notes-mobile.png) | 想法插件读取 Android 本机 Core 的空态 |
| [vocab-mobile.png](vocab-mobile.png) | 背单词词书页；无词书时显示样例来源 |
| [vocab-study.png](vocab-study.png) | 学习卡先回想状态、音标与触控按钮 |
| [vocab-revealed.png](vocab-revealed.png) | 揭晓后的释义、例句和四档评分 |
| [vocab-core.png](vocab-core.png) | 首次启动后由 Core 读出“QUSOR 入门 12 词” |
| [vocab-core-reviewed.png](vocab-core-reviewed.png) | “认识”评分回执、下一张卡与新词 11 |
| [vocab-core-restart.png](vocab-core-restart.png) | 强制停止并重开后的同一 Core 词书与新词 11 |
| [vocab-import-picker.png](vocab-import-picker.png) | Android 系统文件选择器中的词书 JSON |
| [vocab-imported.png](vocab-imported.png) | 导入“我的英语笔记”后的词书列表与学习卡 |
| [vocab-import-restart.png](vocab-import-restart.png) | 重开应用后两本词书仍由本机 Core 读取 |
| [vocab-landscape-scrolled.png](vocab-landscape-scrolled.png) | 横屏滚动后的学习卡内容 |

## 代码修复

- `apps/qusor/App.tsx` 使用反应式 `windowSize()` 判定窄屏，不再依赖根节点 `onLayout` 的启动时序。
- 窄屏默认收起侧栏；导航打开时正文隐藏且导航占满宽度，避免两栏互相挤压。
- `BackHandler` 按“搜索/插件/功能/导航/标签/退出”顺序处理 Android 返回键。
- 想法详情和资料库阅读页各自注册更内层的返回监听，先返回列表，再退出插件。
- 主体内容显式声明 `flexDirection: "column"`，避免 Android 上标签栏与正文被错误横排。
- 应用首次挂载 Core 时，只有在学习库完全为空的情况下才幂等安装入门词书；已有用户词书不会额外追加。
- 背单词工作区提供“导入词书”，读取不超过 2 MiB 的用户 JSON；原生导入器拒绝未知字段、无效结构、同名词书和超预算内容，再由 Core 一次提交整本词书。当前是应用侧可信导入动作，普通插件公开导入 wire 尚未定义。

## 未验证

- 没有 Android 真机、折叠屏、平板、软键盘/中文 IME、触摸选文或性能/电量数据。
- APK 是 debug key 签名，不是商店分发工件；没有 bundle、签名升级键或 Play 渠道。
- 没有向 Android Core 注入资料并验证 PDF 阅读、选文保存、重开恢复；本轮想法 Core 为空。
- 入门词书是应用自编内容，不是考试词表；用户词书导入已可用，但还没有覆盖、删除、导入预览、来源许可元数据或公开插件导入能力。
- release APK 不可 `run-as`，本轮没有检查应用私有目录内容；文件布局由成功 Host/空态与包运行间接证明。

机器可读记录见 [record.json](record.json)。
