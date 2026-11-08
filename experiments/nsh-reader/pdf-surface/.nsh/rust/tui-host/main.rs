//! The terminal development host with the app's UI sets.
fn main() -> std::io::Result<()> {
    let mut options = nsh_platform_tui::AppOptions::default();
    options.ui_sets = vec![nsh_base::registry(), set_app_qusor_nsh_native_host_probe()];
    nsh_tui_host::main_with_options(options)
}

/// The UI set of the crate qusor-nsh-native-host-probe: the entry `#[native_module("…")]` makes in it.
fn set_app_qusor_nsh_native_host_probe() -> nsh_core::element::Registry {
    #[allow(unused_imports)]
    use app_qusor_nsh_native_host_probe::*;
    __nsh_native_module() // the crate qusor-nsh-native-host-probe (nsh.json "rust")
}

// Without the entry: nsh's stand-in, which fails the build saying what to add.
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_module as __nsh_native_module;
