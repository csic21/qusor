//! QUSOR app native surface: the notes and vocabulary hosts, each backed by a
//! real Core database in the app's own data directory and by an ordinary signed
//! package shipped with the app.
//!
//! TSX imports this set as `qusor/pdf-surface`; it never sees a database path,
//! a package key or Host authority.
pub mod notes_native;
pub mod vocab_native;

use nsh_core::prelude::*;

#[native_module("qusor/pdf-surface")]
fn native(registry: &mut Registry) {
    registry.module::<notes_native::NotesHost>();
    registry.module::<vocab_native::VocabHost>();
}
