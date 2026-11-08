//! Native qualification composition; reader behavior is invoked from an installed ordinary Wasm package.
fn main() -> std::process::ExitCode {
    let Some(resources) = std::env::var_os("QUSOR_READER_RESOURCES") else {
        eprintln!("QUSOR_READER_RESOURCES is required");
        return std::process::ExitCode::FAILURE;
    };
    let Some(data) = std::env::var_os("QUSOR_READER_DATA") else {
        eprintln!("QUSOR_READER_DATA is required");
        return std::process::ExitCode::FAILURE;
    };
    if let Err(error) = qusor_nsh_native_host_probe::plugin_surface::initialize(
        std::path::Path::new(&resources),
        std::path::Path::new(&data),
    ) {
        eprintln!("reader initialization failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    nsh_platform_linux::set_ui_sets(vec![
        nsh_base::registry(),
        qusor_nsh_native_host_probe::__nsh_native_module(),
    ]);
    let code = nsh_linux_host::main();
    if let Err(error) = qusor_nsh_native_host_probe::plugin_surface::shutdown() {
        eprintln!("reader shutdown failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    code
}
