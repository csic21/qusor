//! Shared native desktop wiring for the retained qualification entry points.
#[cfg(target_os = "linux")]
pub use nsh_linux_host::main;
#[cfg(target_os = "macos")]
pub use nsh_macos_host::main;
#[cfg(target_os = "linux")]
pub use nsh_platform_linux::{Components, set_components, set_ui_sets};
#[cfg(target_os = "macos")]
pub use nsh_platform_macos::{Components, set_components, set_ui_sets};
#[cfg(target_os = "windows")]
pub use nsh_platform_windows::{Components, set_components, set_ui_sets};
#[cfg(target_os = "windows")]
pub use nsh_windows_host::main;
