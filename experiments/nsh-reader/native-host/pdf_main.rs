//! Native PDF Surface probe, not a functional reader plugin.
#[allow(unused_imports)]
#[path = "desktop_platform.rs"]
mod platform;
fn main() -> std::process::ExitCode {
    let Some(path) = std::env::var_os("QUSOR_PDF_REPORT") else {
        eprintln!("QUSOR_PDF_REPORT is required");
        return std::process::ExitCode::FAILURE;
    };
    if let Err(error) = qusor_nsh_native_host_probe::initialize(std::path::Path::new(&path)) {
        eprintln!("cannot initialize PDF Surface: {error}");
        return std::process::ExitCode::FAILURE;
    }
    platform::set_ui_sets(vec![
        nsh_base::registry(),
        qusor_nsh_native_host_probe::__nsh_native_module(),
    ]);
    platform::main()
}
