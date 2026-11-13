//! Isolated Android JNI bridge calling `org.qusor.app.QusorPlatform`.
//!
//! Keeps `unsafe` JNI/dlsym lookups out of `qusor-app-native` (which enforces
//! `unsafe_code = "forbid"`). On non-Android targets, all entry points return a
//! clean unsupported-OS error.

/// Speaks `text` via Android `TextToSpeech` with `"uk"` or `"us"` accent and `rate` speed.
pub fn speak(text: &str, accent: &str, rate: f32) -> Result<(), String> {
    platform::speak(text, accent, rate)
}

/// Stops active Android `TextToSpeech` playback if initialized.
pub fn stop_speak() {
    platform::stop_speak();
}

/// Shares a private app file through Android's system share sheet (`FilesProvider`).
pub fn share_file(path: &str, mime: &str, title: &str) -> Result<(), String> {
    platform::share_file(path, mime, title)
}

#[cfg(target_os = "android")]
mod platform {
    use jni::{
        JNIEnv, JavaVM,
        objects::{GlobalRef, JClass, JString, JValue},
    };
    use std::sync::{Mutex, OnceLock};

    const PLATFORM_CLASS: &str = "org.qusor.app.QusorPlatform";

    struct PlatformBridge {
        vm: JavaVM,
        platform_class: GlobalRef,
    }

    static BRIDGE: OnceLock<PlatformBridge> = OnceLock::new();
    static INIT_LOCK: Mutex<()> = Mutex::new(());

    fn take_jni_error(env: &mut JNIEnv<'_>, err: jni::errors::Error) -> String {
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        format!("Android JNI call failed: {err}")
    }

    fn bridge() -> Result<&'static PlatformBridge, String> {
        if let Some(cached) = BRIDGE.get() {
            return Ok(cached);
        }
        let _guard = INIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = BRIDGE.get() {
            return Ok(cached);
        }

        type GetCreatedJavaVmsFn = unsafe extern "system" fn(
            vm_buf: *mut *mut jni::sys::JavaVM,
            buf_len: jni::sys::jsize,
            n_vms: *mut jni::sys::jsize,
        ) -> jni::sys::jint;

        // SAFETY: `JNI_GetCreatedJavaVMs` is exported by Android's ART runtime in-process.
        // We verify the resolved symbol pointer is non-null before transmuting and calling it.
        let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"JNI_GetCreatedJavaVMs".as_ptr()) };
        if sym.is_null() {
            return Err("JNI_GetCreatedJavaVMs symbol not found".into());
        }
        // SAFETY: `sym` is the standard JNI `JNI_GetCreatedJavaVMs` entry point matching `GetCreatedJavaVmsFn`.
        let get_created_java_vms: GetCreatedJavaVmsFn = unsafe { std::mem::transmute(sym) };
        let mut vm_ptr: *mut jni::sys::JavaVM = std::ptr::null_mut();
        let mut vm_count: jni::sys::jsize = 0;
        // SAFETY: `vm_ptr` and `vm_count` point to valid writable stack slots for 1 VM pointer.
        let status = unsafe { get_created_java_vms(&mut vm_ptr, 1, &mut vm_count) };
        if status != jni::sys::JNI_OK || vm_count <= 0 || vm_ptr.is_null() {
            return Err(format!(
                "no active Android JavaVM (status={status}, count={vm_count})"
            ));
        }

        // SAFETY: `vm_ptr` was returned by `JNI_GetCreatedJavaVMs` and remains valid for the process lifetime.
        let vm = unsafe { JavaVM::from_raw(vm_ptr) }
            .map_err(|err| format!("failed to wrap Android JavaVM: {err}"))?;

        let platform_class = {
            let mut env = vm
                .attach_current_thread()
                .map_err(|err| format!("failed to attach thread to JavaVM: {err}"))?;

            let at_class = env
                .find_class("android/app/ActivityThread")
                .map_err(|err| take_jni_error(&mut env, err))?;
            let app_obj = env
                .call_static_method(
                    &at_class,
                    "currentApplication",
                    "()Landroid/app/Application;",
                    &[],
                )
                .and_then(|val| val.l())
                .map_err(|err| take_jni_error(&mut env, err))?;
            if app_obj.is_null() {
                return Err("ActivityThread.currentApplication() returned null".into());
            }

            let loader_obj = env
                .call_method(&app_obj, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
                .and_then(|val| val.l())
                .map_err(|err| take_jni_error(&mut env, err))?;
            if loader_obj.is_null() {
                return Err("Application.getClassLoader() returned null".into());
            }

            let class_name = env
                .new_string(PLATFORM_CLASS)
                .map_err(|err| take_jni_error(&mut env, err))?;
            let platform_obj = env
                .call_method(
                    &loader_obj,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&class_name)],
                )
                .and_then(|val| val.l())
                .map_err(|err| take_jni_error(&mut env, err))?;
            if platform_obj.is_null() {
                return Err(format!("failed to load {PLATFORM_CLASS}"));
            }

            env.new_global_ref(platform_obj)
                .map_err(|err| take_jni_error(&mut env, err))?
        };

        let _ = BRIDGE.set(PlatformBridge { vm, platform_class });
        Ok(BRIDGE.get().expect("OnceLock just initialized"))
    }

    fn interpret_status_string(
        env: &mut JNIEnv<'_>,
        obj: jni::objects::JObject<'_>,
    ) -> Result<(), String> {
        if obj.is_null() {
            return Err("QusorPlatform returned null status".into());
        }
        let jstr = JString::from(obj);
        let text: String = env
            .get_string(&jstr)
            .map_err(|err| take_jni_error(env, err))?
            .into();
        if text == "ok" {
            Ok(())
        } else {
            Err(text)
        }
    }

    pub(super) fn speak(text: &str, accent: &str, rate: f32) -> Result<(), String> {
        let b = bridge()?;
        let mut env = b
            .vm
            .attach_current_thread()
            .map_err(|err| format!("failed to attach thread to JavaVM: {err}"))?;
        let class: &JClass<'_> = b.platform_class.as_obj().into();
        let j_text = env
            .new_string(text)
            .map_err(|err| take_jni_error(&mut env, err))?;
        let j_accent = env
            .new_string(accent)
            .map_err(|err| take_jni_error(&mut env, err))?;
        let ret = env
            .call_static_method(
                class,
                "speak",
                "(Ljava/lang/String;Ljava/lang/String;F)Ljava/lang/String;",
                &[
                    JValue::Object(&j_text),
                    JValue::Object(&j_accent),
                    JValue::Float(rate),
                ],
            )
            .and_then(|val| val.l())
            .map_err(|err| take_jni_error(&mut env, err))?;
        interpret_status_string(&mut env, ret)
    }

    pub(super) fn stop_speak() {
        let Ok(b) = bridge() else {
            return;
        };
        let Ok(mut env) = b.vm.attach_current_thread() else {
            return;
        };
        let class: &JClass<'_> = b.platform_class.as_obj().into();
        if env.call_static_method(class, "stopSpeak", "()V", &[]).is_err()
            && env.exception_check().unwrap_or(false)
        {
            let _ = env.exception_clear();
        }
    }

    pub(super) fn share_file(path: &str, mime: &str, title: &str) -> Result<(), String> {
        let b = bridge()?;
        let mut env = b
            .vm
            .attach_current_thread()
            .map_err(|err| format!("failed to attach thread to JavaVM: {err}"))?;
        let class: &JClass<'_> = b.platform_class.as_obj().into();
        let j_path = env
            .new_string(path)
            .map_err(|err| take_jni_error(&mut env, err))?;
        let j_mime = env
            .new_string(mime)
            .map_err(|err| take_jni_error(&mut env, err))?;
        let j_title = env
            .new_string(title)
            .map_err(|err| take_jni_error(&mut env, err))?;
        let ret = env
            .call_static_method(
                class,
                "shareFile",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&j_path),
                    JValue::Object(&j_mime),
                    JValue::Object(&j_title),
                ],
            )
            .and_then(|val| val.l())
            .map_err(|err| take_jni_error(&mut env, err))?;
        interpret_status_string(&mut env, ret)
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    pub(super) fn speak(_text: &str, _accent: &str, _rate: f32) -> Result<(), String> {
        Err("Android platform bridge is unavailable on this OS".into())
    }

    pub(super) fn stop_speak() {}

    pub(super) fn share_file(_path: &str, _mime: &str, _title: &str) -> Result<(), String> {
        Err("Android platform bridge is unavailable on this OS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "android"))]
    #[test]
    fn non_android_host_returns_clean_unavailable_errors() {
        assert_eq!(
            speak("hello", "us", 1.0).unwrap_err(),
            "Android platform bridge is unavailable on this OS"
        );
        stop_speak();
        assert_eq!(
            share_file("/tmp/backup.json", "application/json", "Backup").unwrap_err(),
            "Android platform bridge is unavailable on this OS"
        );
    }
}
