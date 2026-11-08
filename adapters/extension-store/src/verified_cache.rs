//! Bounded memoization of successful pure authentication, never installation authority.
use extension_api::ApiVersion;
use extension_package::package::{MAX_TOTAL_FILE_BYTES, VERIFIER_POLICY, VerifiedPackage};
use std::{collections::VecDeque, sync::Arc};

pub(super) const MAX_ENTRIES: usize = 2;
pub(super) const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
// Conservative allowance for all bounded manifest strings/vectors, at most64
// file paths/MIME values, fixed hashes/keys/profile, Entry and Arc metadata.
// Raw canonical bytes and decoded files are charged separately. Reaudit if the
// package verifier's field/count limits or VerifiedPackage representation change.
const METADATA_ALLOWANCE: usize = 1024 * 1024;

struct Entry {
    package: Arc<VerifiedPackage>,
    supported_apis: Vec<ApiVersion>,
    verifier_policy: u16,
    charge: usize,
}
#[derive(Default)]
pub(super) struct VerifiedCache {
    entries: VecDeque<Entry>,
    retained_bytes: usize,
}
impl VerifiedCache {
    pub(super) fn get(
        &mut self,
        bytes: &[u8],
        trusted_key: &str,
        supported_apis: &[ApiVersion],
    ) -> Option<Arc<VerifiedPackage>> {
        let index = self.entries.iter().position(|entry| {
            entry.verifier_policy == VERIFIER_POLICY
                && entry.supported_apis == supported_apis
                && entry.package.publisher_key() == trusted_key
                && entry.package.raw_bytes() == bytes
        })?;
        let entry = self.entries.remove(index)?;
        let package = Arc::clone(&entry.package);
        self.entries.push_back(entry);
        Some(package)
    }
    pub(super) fn insert(&mut self, package: Arc<VerifiedPackage>, supported_apis: &[ApiVersion]) {
        let Some(charge) = package
            .raw_bytes()
            .len()
            .checked_add(MAX_TOTAL_FILE_BYTES)
            .and_then(|size| size.checked_add(METADATA_ALLOWANCE))
        else {
            return;
        };
        if charge > MAX_RETAINED_BYTES {
            return;
        }
        // An exact key already present needs no second retained Arc/key copy.
        if self
            .get(package.raw_bytes(), package.publisher_key(), supported_apis)
            .is_some()
        {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.retained_bytes > MAX_RETAINED_BYTES - charge
        {
            let Some(old) = self.entries.pop_front() else {
                return;
            };
            self.retained_bytes -= old.charge;
        }
        self.retained_bytes += charge;
        self.entries.push_back(Entry {
            package,
            supported_apis: supported_apis.to_vec(),
            verifier_policy: VERIFIER_POLICY,
            charge,
        });
    }
}

#[cfg(test)]
#[path = "verified_cache_tests.rs"]
mod tests;
