//! Describes the app's UI sets for `nsh`: one JSON document on standard
//! output (nsh-core `ui::description`).

fn main() {
    let sets = vec![set_app_qusor_nsh_native_host_probe()];
    println!("{}", nsh_core::element::description::Description::of(&sets).to_json());
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
