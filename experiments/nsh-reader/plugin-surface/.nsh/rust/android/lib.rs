//! The app's `libnsh.so`: the Android runtime with `nsh/base` and the
//! app's Rust components.

#[cfg(target_os = "android")]
nsh::ui_sets!(vec![nsh_base::registry(), set_app_qusor_nsh_native_host_probe()]);

/// The UI set of the crate qusor-nsh-native-host-probe: the entry `#[native_module("…")]` makes in it.
#[cfg(target_os = "android")]
fn set_app_qusor_nsh_native_host_probe() -> nsh_core::element::Registry {
    #[allow(unused_imports)]
    use app_qusor_nsh_native_host_probe::*;
    __nsh_native_module() // the crate qusor-nsh-native-host-probe (nsh.json "rust")
}

// Without the entry: nsh's stand-in, which fails the build saying what to add.
#[cfg(target_os = "android")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_module as __nsh_native_module;
