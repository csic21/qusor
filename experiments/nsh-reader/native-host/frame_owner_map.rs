//! Trusted caller ownership map. No Host confirmation or semantic parsing.
use super::frame_scheduler::{
    Binding, Cancellation, FrameJob, FrameScheduler, Owner, RetirementTicket, Snapshot,
};
use std::{collections::BTreeMap, sync::Mutex};
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameKey {
    pub mount_id: String,
    pub generation: u64,
    pub frame_id: String,
}
fn reclaimable(owner: &Owner) -> bool {
    owner.snapshot().is_ok_and(|s| s.local_drained())
}
pub struct FrameCaller<T: Send + 'static> {
    scheduler: FrameScheduler<T>,
    owners: Mutex<BTreeMap<FrameKey, Owner>>,
}
impl<T: Send + 'static> FrameCaller<T> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            scheduler: FrameScheduler::new()?,
            owners: Mutex::new(BTreeMap::new()),
        })
    }
    pub fn submit(
        &self,
        key: FrameKey,
        work: impl FnOnce(Cancellation) -> Result<T> + Send + 'static,
    ) -> Result<FrameJob<T>> {
        let (owner, previous) = {
            let mut owners = self.owners.lock().map_err(|_| "frame owner map poisoned")?;
            owners.retain(|_, o| !reclaimable(o));
            if let Some(owner) = owners.get(&key) {
                (owner.clone(), Vec::new())
            } else {
                let owner = self.scheduler.new_owner(Binding {
                    mount: key.mount_id.clone(),
                    generation: key.generation,
                    frame: key.frame_id.clone(),
                })?;
                let previous = owners
                    .values()
                    .filter(|o| o.snapshot().is_ok_and(|s| !s.closed))
                    .cloned()
                    .collect::<Vec<_>>();
                owners.insert(key, owner.clone());
                (owner, previous)
            }
        };
        // User work/result destructors never run under caller map or reader Gate locks.
        for previous in previous {
            self.scheduler.close_owner(&previous)?;
        }
        self.scheduler.submit(&owner, work)
    }
    pub fn snapshot_owners(&self) -> Result<Vec<(FrameKey, Snapshot)>> {
        self.owners
            .lock()
            .map_err(|_| "frame owner map poisoned")?
            .iter()
            .map(|(k, o)| Ok((k.clone(), o.snapshot()?)))
            .collect()
    }
    pub fn close(&self) -> Result<()> {
        self.scheduler.close()
    }
    pub fn retirement_ticket(&self) -> RetirementTicket {
        self.scheduler.retirement_ticket()
    }
}
