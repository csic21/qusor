//! Versioned capture, source and anchor contracts; independent of Host and infrastructure.
//! Each protocol retains its own version and bounded untrusted-data entrypoints.
pub mod contract;
pub mod error;
pub mod knowledge;
pub mod pdf_compound;
pub mod pdf_contract;
pub mod vocab;
