//! Encoded ImageData-container lifetime only. Not decoded/native release.
//! Arc<[u8]> may remain separately owned by the existing native PNG cache.
use super::frame_scheduler::Owner;
#[cfg(test)]
use super::frame_scheduler::Snapshot;
use std::sync::{Arc, Mutex, OnceLock};
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodedStats {
    pub containers: u64,
    pub referenced_bytes: u64,
    pub created: u64,
    pub released: u64,
    pub uncertain: bool,
}
static STATS: OnceLock<Mutex<EncodedStats>> = OnceLock::new();
fn stats() -> &'static Mutex<EncodedStats> {
    STATS.get_or_init(|| Mutex::new(EncodedStats::default()))
}
pub fn snapshot() -> Result<EncodedStats> {
    stats()
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "encoded counters poisoned".into())
}
struct Guard {
    owner: Owner,
    bytes: usize,
}
impl Drop for Guard {
    fn drop(&mut self) {
        eprintln!(
            "FRAME_ENCODED_CONTAINER_DROP {:?} bytes={}",
            self.owner.binding(),
            self.bytes
        );
        self.owner.release_encoded(self.bytes);
        match stats().lock() {
            Ok(mut s) => {
                match (
                    s.containers.checked_sub(1),
                    s.referenced_bytes.checked_sub(self.bytes as u64),
                    s.released.checked_add(1),
                ) {
                    (Some(containers), Some(bytes), Some(released)) => {
                        s.containers = containers;
                        s.referenced_bytes = bytes;
                        s.released = released;
                    }
                    _ => {
                        s.uncertain = true;
                        self.owner.mark_unknown();
                    }
                }
            }
            Err(_) => self.owner.mark_unknown(),
        }
    }
}
/// ImageData::new(TrackedEncodedBytes) shares this exact container through
/// Blob::shared/clone, slices and ImageSource. This type intentionally is not Clone.
pub struct TrackedEncodedBytes {
    bytes: Arc<[u8]>,
    _guard: Guard,
}
impl TrackedEncodedBytes {
    pub fn new(bytes: Arc<[u8]>, owner: Owner) -> Result<Self> {
        // Exact existing dynamic_worker MAX_BITMAP; PNG cache remains3pages/32MiB.
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("native PNG budget exceeded".into());
        }
        owner.charge_encoded(bytes.len())?;
        let mut s = match stats().lock() {
            Ok(s) => s,
            Err(_) => {
                owner.release_encoded(bytes.len());
                return Err("encoded counters poisoned".into());
            }
        };
        let next = (
            s.containers.checked_add(1),
            s.referenced_bytes.checked_add(bytes.len() as u64),
            s.created.checked_add(1),
        );
        let (Some(containers), Some(referenced_bytes), Some(created)) = next else {
            drop(s);
            owner.release_encoded(bytes.len());
            return Err("encoded counter exhausted".into());
        };
        s.containers = containers;
        s.referenced_bytes = referenced_bytes;
        s.created = created;
        drop(s);
        Ok(Self {
            _guard: Guard {
                owner,
                bytes: bytes.len(),
            },
            bytes,
        })
    }
    #[cfg(test)]
    pub fn same_owner(&self, other: &Self) -> bool {
        self._guard.owner.same_identity(&other._guard.owner)
    }
    #[cfg(test)]
    pub fn owner_snapshot(&self) -> Result<Snapshot> {
        self._guard.owner.snapshot()
    }
    #[cfg(test)]
    pub fn owner_for_test(&self) -> Owner {
        self._guard.owner.clone()
    }
}
impl AsRef<[u8]> for TrackedEncodedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
