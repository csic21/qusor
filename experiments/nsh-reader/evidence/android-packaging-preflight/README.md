# Android packaging preflight

日期：2026-11-12

目标：确认 QUSOR `apps/qusor` 的 Rust/TSX 应用本体能否进入 Android APK 打包，并把阻断定位到应用代码或固定 NSH 基线。

## 环境

- macOS arm64，固定 NSH `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。
- Android SDK：`/Users/karl/Library/Android/sdk`。
- Android Studio JBR：`/Applications/Android Studio.app/Contents/jbr/Contents/Home`。
- 临时安装的 Android NDK：`/Users/karl/Library/Android/sdk/ndk/27.2.12479018`。
- Rust target：`aarch64-linux-android` 已安装。

## 命令

```sh
env JAVA_HOME='/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
  PATH='/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin:/Users/karl/.cargo/bin:/Users/karl/Library/Android/sdk/platform-tools:/usr/bin:/bin:/usr/sbin:/sbin' \
  ANDROID_HOME='/Users/karl/Library/Android/sdk' \
  ANDROID_NDK_HOME='/Users/karl/Library/Android/sdk/ndk/27.2.12479018' \
  ../../experiments/nsh-reader/target/nsh/target/debug/nsh package \
    --target android --abi arm64-v8a --out /tmp/qusor-android-preflight
```

## 结果

- `qusor-app-native` 的 Rust 检查通过。
- `App.tsx` 原生 bundle 构建通过（94.2 KB JS / 113.6 KB bytecode）。
- Android NDK 已被识别，NSH Android runtime 开始实际编译；runtime 构建阶段耗时约 4 分钟。
- 随后应用 crate 的 `libnsh.so` 构建失败：固定 NSH 依赖的 `quickjs-jit 0.14.2` 在 Android target 下找不到宿主宏 crate `rquickjs_macro`，退出于 `quickjs-jit/src/lib.rs`。没有生成 APK。

这证明 QUSOR 应用本体已进入 Android 工具链；阻断是固定 NSH Android runtime 的上游依赖/target 组合，而不是当前 Library/Reader 业务代码。

## 下一步门槛

- 先修复或固定一个能在 `aarch64-linux-android` 编译的 NSH/QuickJS 组合，再重跑同一命令。
- 成功产出 APK 后仍需 Android 私有存储、包/解释器 profile、设备安装、启动、触控/IME、阅读与词汇旅程及分发资格；本预检不等同于 Android 支持完成。
