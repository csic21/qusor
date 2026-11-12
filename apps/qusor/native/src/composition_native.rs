//! Native diagnostic entry for the resolved application composition.
use crate::profile;
use nsh_core::prelude::*;

#[module]
pub struct CompositionHost;

#[module]
impl CompositionHost {
    /// Return the validated profile, ordered entries and provider/consumer edges.
    pub fn dump() -> String {
        profile::report_json()
    }
}
