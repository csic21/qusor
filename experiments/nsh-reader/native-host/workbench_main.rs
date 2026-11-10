//! Workbench host: the designed shell plus the notes native bridge over public
//! notes.read. Startup input comes from QUSOR_NOTES_* environment variables.
#[allow(unused_imports)]
#[path = "desktop_platform.rs"]
mod platform;

fn main() -> std::process::ExitCode {
    if let Err(error) = qusor_nsh_native_host_probe::notes_native::initialize_from_env() {
        eprintln!("notes initialization failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    platform::set_ui_sets(vec![
        nsh_base::registry(),
        qusor_nsh_native_host_probe::__nsh_native_module(),
    ]);
    platform::main()
}
