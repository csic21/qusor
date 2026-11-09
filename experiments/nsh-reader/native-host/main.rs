// Native-only adaptation of the fixed upstream rust-exports generated host (MIT).
// It registers real Rust modules/components; no WebView is used by this example.
//! Native desktop development host with `nsh/base` and the app's Rust components.
#[path = "desktop_platform.rs"]
mod platform;

fn main() -> std::process::ExitCode {
    platform::set_ui_sets(vec![nsh_base::registry(), set_app_rust_exports_native()]);
    platform::set_components(app_components);
    platform::main()
}

/// The UI set of the crate rust-exports-native: the entry `#[native_module("…")]` makes in it.
fn set_app_rust_exports_native() -> nsh_core::element::Registry {
    #[allow(unused_imports)]
    use app_rust_exports_native::*;
    __nsh_native_module() // the crate rust-exports-native (nsh.json "rust")
}

// Without the entry: nsh's stand-in, which fails the build saying what to add.
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_module as __nsh_native_module;

/// Native components declared by the upstream example for this desktop.
fn app_components(__nsh_components: &platform::Components) {
    {
        #[allow(unused_imports)]
        use app_rust_exports_native::*;
        #[cfg(target_os = "linux")]
        __nsh_native_components_linux(__nsh_components);
        #[cfg(target_os = "macos")]
        __nsh_native_components_macos(__nsh_components);
        #[cfg(target_os = "windows")]
        __nsh_native_components_windows(__nsh_components);
    }
}

// Without the entry: nsh's stand-in, which registers nothing.
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_components as __nsh_native_components_linux;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_components as __nsh_native_components_macos;
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_components as __nsh_native_components_windows;
