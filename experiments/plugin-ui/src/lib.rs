//! Isolated M-1 prototype for QUSOR declarative native plugin UI protocol.
//!
//! Covers schema, strict component/token/accessibility validation, bounded
//! budgets, atomic patching, generation-fenced event dispatch, and ADR 0028
//! slot composition via `capability-composition` and `extension-api`.

pub mod host;
pub mod schema;
pub mod validator;
