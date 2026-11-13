//! QUSOR app native surface: a validated desktop composition plus the notes and
//! vocabulary hosts backed by ordinary signed packages shipped with the app.
//!
//! TSX imports this set as `qusor/pdf-surface`; it never sees a database path,
//! a package key or Host authority.
pub mod app_info;
pub mod composition_native;
pub mod epub_reader;
pub mod extension_preferences;
pub mod library_native;
pub mod notes_native;
pub mod package_catalog;
pub mod pdf_page;
#[cfg(unix)]
pub mod pdf_reader;
#[cfg(unix)]
mod pdf_service;
pub mod profile;
mod runtime;
pub mod vault;
pub mod vocab_native;

use nsh_core::prelude::*;

#[native_module("qusor/pdf-surface")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
    registry.module::<app_info::AppInfo>();
    registry.module::<composition_native::CompositionHost>();
    registry.module::<notes_native::NotesHost>();
    registry.module::<vocab_native::VocabHost>();
    registry.module::<library_native::LibraryHost>();
}
