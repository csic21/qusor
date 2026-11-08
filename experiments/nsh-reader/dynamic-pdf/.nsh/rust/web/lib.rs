//! The app's web runtime: `nsh.wasm` with `nsh/base` and the app's
//! Rust components.

#[cfg(target_arch = "wasm32")]
nsh_platform_web::ui_set!(nsh_ui_set_qusor_pdf_surface, set_app_qusor_nsh_native_host_probe());

/// The UI set of the crate qusor-nsh-native-host-probe: the entry `#[native_module("…")]` makes in it.
#[cfg(target_arch = "wasm32")]
fn set_app_qusor_nsh_native_host_probe() -> nsh_core::element::Registry {
    #[allow(unused_imports)]
    use app_qusor_nsh_native_host_probe::*;
    __nsh_native_module() // the crate qusor-nsh-native-host-probe (nsh.json "rust")
}

// Without the entry: nsh's stand-in, which fails the build saying what to add.
#[cfg(target_arch = "wasm32")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_module as __nsh_native_module;

// Off WebAssembly it is empty, as nsh-platform-web is.
#[cfg(not(target_arch = "wasm32"))]
use nsh_core as _;
#[cfg(not(target_arch = "wasm32"))]
use nsh_base as _;
