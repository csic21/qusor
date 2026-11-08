fn main() {
    match runtime_process::worker_main(&runtime_wasm::WasmiRuntime) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("worker failed: {error}");
            std::process::exit(1);
        }
    }
    if let Err(error) =
        qusor_nsh_native_host_probe::plugin_surface::native_editor::qualification::run_cases()
    {
        eprintln!("NATIVE_RETIREMENT_FAILED {error}");
        std::process::exit(1);
    }
}
