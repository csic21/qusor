//! Native dynamic-document qualification composition. No fixture import at startup.
fn main() -> std::process::ExitCode {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--managed-worker") {
        let Some(run) = args.get(4) else {
            return std::process::ExitCode::FAILURE;
        };
        if args.len() != 5 {
            return std::process::ExitCode::FAILURE;
        }
        let canonical_id = |id: &str| {
            id.split_once('-').is_some_and(|(boot, counter)| {
                boot.len() == 32
                    && boot
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && !counter.starts_with('0')
                    && counter.parse::<u64>().is_ok_and(|n| n > 0)
            })
        };
        if let Some(id) = run.strip_prefix("pdf-") {
            if !canonical_id(id) {
                return std::process::ExitCode::FAILURE;
            }
            return qusor_nsh_native_host_probe::dynamic_worker::managed_main(&args[2..]);
        }
        let canonical = canonical_id(run);
        if !canonical {
            return std::process::ExitCode::FAILURE;
        }
        return match runtime_process::worker_main(&runtime_wasm::WasmiRuntime) {
            Ok(true) => std::process::ExitCode::SUCCESS,
            _ => std::process::ExitCode::FAILURE,
        };
    }
    let variable = |name| {
        std::env::var_os(name)
            .map(std::path::PathBuf::from)
            .ok_or_else(|| format!("{name} is required"))
    };
    let configured = (|| {
        let resources = variable("QUSOR_READER_RESOURCES")?;
        let data = variable("QUSOR_READER_DATA")?;
        let worker = std::env::current_exe().map_err(|e| e.to_string())?;
        let library = variable("QUSOR_PDF_LIBRARY")?;
        qusor_nsh_native_host_probe::dynamic_module::initialize(
            &resources, &data, &worker, &library,
        )
    })();
    if let Err(error) = configured {
        eprintln!("dynamic initialization failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    platform::set_ui_sets(vec![
        nsh_base::registry(),
        qusor_nsh_native_host_probe::__nsh_native_module(),
    ]);
    let code = platform::main();
    // First latch editor admission without waiting, invalidate dynamic State,
    // stop frames and wait for the entire parser/COMMITTING Job to finish.
    let frame_shutdown = qusor_nsh_native_host_probe::dynamic_module::shutdown();
    if let Err(error) = frame_shutdown {
        // Do not take SERVICE while any parser can still install/replace it.
        // One complete editor retirement attempt, retaining Service/leases.
        let editor_retired = qusor_nsh_native_host_probe::plugin_surface::native_editor::shutdown();
        eprintln!(
            "dynamic shutdown incomplete: {error}; editor_retired={editor_retired}; service_retained=true"
        );
        return std::process::ExitCode::FAILURE;
    }
    if let Err(error) = qusor_nsh_native_host_probe::plugin_surface::shutdown() {
        eprintln!("dynamic shutdown failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    code
}

#[cfg(target_os = "linux")]
mod platform {
    pub use nsh_linux_host::main;
    pub use nsh_platform_linux::set_ui_sets;
}
#[cfg(target_os = "macos")]
mod platform {
    pub use nsh_macos_host::main;
    pub use nsh_platform_macos::set_ui_sets;
}
#[cfg(target_os = "windows")]
mod platform {
    pub use nsh_windows_host::main;
    pub use nsh_platform_windows::set_ui_sets;
}
