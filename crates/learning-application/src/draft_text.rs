//! Session-only, renderer-independent bounded text drafts. Handles are trusted local values.
use core_api::knowledge::{KnowledgeError, KnowledgeErrorCode as Code, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_CHUNK_BYTES: usize = 512;
pub const MAX_DRAFTS: usize = 8;
pub const MAX_SESSION_BYTES: usize = 16 * 1024 * 1024;
/// Upload inactivity timeout in seconds.
pub const UPLOAD_TIMEOUT: u64 = 30;

macro_rules! handle {
    ($name:ident) => {
        #[derive(Clone, Debug)]
        pub struct $name {
            scope: Arc<()>,
            id: u64,
        }
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                Arc::ptr_eq(&self.scope, &other.scope) && self.id == other.id
            }
        }
        impl Eq for $name {}
    };
}
handle!(DraftId);
handle!(MountOwner);
handle!(UploadId);
handle!(SaveToken);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftState {
    Editing,
    Saving,
    Saved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Unknown,
    DefinitelyNotCommitted,
    Committed,
}
#[derive(Debug, PartialEq, Eq)]
pub struct DraftChunk {
    pub revision: u64,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
}
struct Draft {
    text: String,
    revision: u64,
    state: DraftState,
    save: Option<(u64, String)>,
}
struct Upload {
    draft: u64,
    owner: u64,
    revision: u64,
    total: usize,
    text: String,
    touched: u64,
}

/// All `now` values use seconds from one caller-owned monotonic clock (or one
/// consistently normalized time source). The logical clock clamps regressions;
/// process-controller millisecond timestamps must be converted before use.
pub struct DraftTexts {
    scope: Arc<()>,
    next: u64,
    clock: u64,
    drafts: BTreeMap<u64, Draft>,
    owners: BTreeSet<u64>,
    uploads: BTreeMap<u64, Upload>,
}
fn err(code: Code, message: &'static str) -> KnowledgeError {
    KnowledgeError::new(code, message)
}
impl Default for DraftTexts {
    fn default() -> Self {
        Self::new()
    }
}
impl DraftTexts {
    pub fn new() -> Self {
        Self {
            scope: Arc::new(()),
            next: 0,
            clock: 0,
            drafts: BTreeMap::new(),
            owners: BTreeSet::new(),
            uploads: BTreeMap::new(),
        }
    }
    fn allocate(&mut self) -> Result<u64> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| err(Code::BudgetExceeded, "draft identity exhausted"))?;
        Ok(self.next)
    }
    fn scope(&self, scope: &Arc<()>) -> Result<()> {
        if Arc::ptr_eq(&self.scope, scope) {
            Ok(())
        } else {
            Err(err(Code::Conflict, "foreign draft session"))
        }
    }
    fn draft(&self, id: &DraftId) -> Result<&Draft> {
        self.scope(&id.scope)?;
        self.drafts
            .get(&id.id)
            .ok_or_else(|| err(Code::NotFound, "draft not found"))
    }
    fn owner(&self, owner: &MountOwner) -> Result<()> {
        self.scope(&owner.scope)?;
        if self.owners.contains(&owner.id) {
            Ok(())
        } else {
            Err(err(Code::Conflict, "mount owner retired"))
        }
    }
    pub fn create_owner(&mut self) -> Result<MountOwner> {
        if self.owners.len() >= MAX_DRAFTS {
            return Err(err(Code::BudgetExceeded, "mount owner budget exceeded"));
        }
        let id = self.allocate()?;
        self.owners.insert(id);
        Ok(MountOwner {
            scope: self.scope.clone(),
            id,
        })
    }
    pub fn revoke_owner(&mut self, owner: &MountOwner) -> Result<()> {
        self.owner(owner)?;
        self.owners.remove(&owner.id);
        self.uploads.retain(|_, u| u.owner != owner.id);
        Ok(())
    }
    pub fn create_draft(&mut self, text: &str) -> Result<DraftId> {
        if text.len() > MAX_TEXT_BYTES
            || self.drafts.len() >= MAX_DRAFTS
            || self.reserved_bytes() + text.len() > MAX_SESSION_BYTES
        {
            return Err(err(Code::BudgetExceeded, "draft text budget exceeded"));
        }
        let id = self.allocate()?;
        self.drafts.insert(
            id,
            Draft {
                text: text.into(),
                revision: 1,
                state: DraftState::Editing,
                save: None,
            },
        );
        Ok(DraftId {
            scope: self.scope.clone(),
            id,
        })
    }
    pub fn remove_draft(&mut self, id: &DraftId) -> Result<()> {
        if self.draft(id)?.state == DraftState::Saving {
            return Err(err(Code::Conflict, "save outcome unresolved"));
        }
        self.drafts.remove(&id.id);
        self.uploads.retain(|_, u| u.draft != id.id);
        Ok(())
    }
    /// Includes the full declared upload reservation, not merely bytes received.
    pub fn reserved_bytes(&self) -> usize {
        self.drafts.values().map(|d| d.text.len()).sum::<usize>()
            + self.uploads.values().map(|u| u.total).sum::<usize>()
    }
    pub fn state(&self, id: &DraftId) -> Result<DraftState> {
        Ok(self.draft(id)?.state)
    }
    pub fn read_chunk(
        &self,
        id: &DraftId,
        revision: Option<u64>,
        offset: usize,
        max_bytes: usize,
    ) -> Result<DraftChunk> {
        let d = self.draft(id)?;
        if revision.is_some_and(|r| r != d.revision) {
            return Err(err(Code::Conflict, "draft revision changed"));
        }
        if max_bytes == 0
            || max_bytes > MAX_CHUNK_BYTES
            || offset > d.text.len()
            || !d.text.is_char_boundary(offset)
        {
            return Err(err(Code::InvalidInput, "invalid text chunk boundary"));
        }
        let mut end = offset.saturating_add(max_bytes).min(d.text.len());
        while !d.text.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset && offset != d.text.len() {
            return Err(err(
                Code::InvalidInput,
                "chunk cannot fit next UTF-8 scalar",
            ));
        }
        Ok(DraftChunk {
            revision: d.revision,
            offset,
            total_bytes: d.text.len(),
            text: d.text[offset..end].into(),
        })
    }
    /// Advance the logical clock in seconds and discard inactive uploads.
    pub fn expire_uploads(&mut self, now: u64) {
        self.clock = self.clock.max(now);
        let clock = self.clock;
        self.uploads
            .retain(|_, u| clock.saturating_sub(u.touched) < UPLOAD_TIMEOUT);
    }
    pub(crate) fn provider_clock(&self) -> u64 {
        self.clock
    }
    pub(crate) fn touch_owner_uploads(
        &mut self,
        owner: &MountOwner,
        now: u64,
    ) -> Result<(u64, bool)> {
        self.owner(owner)?;
        self.expire_uploads(now);
        let mut present = false;
        for upload in self.uploads.values_mut().filter(|u| u.owner == owner.id) {
            upload.touched = self.clock;
            present = true;
        }
        Ok((self.clock, present))
    }
    pub fn begin_replace(
        &mut self,
        owner: &MountOwner,
        id: &DraftId,
        expected_revision: u64,
        total_bytes: usize,
        now: u64,
    ) -> Result<UploadId> {
        self.expire_uploads(now);
        self.owner(owner)?;
        let d = self.draft(id)?;
        if d.state != DraftState::Editing
            || d.revision != expected_revision
            || self
                .uploads
                .values()
                .any(|u| u.draft == id.id || u.owner == owner.id)
        {
            return Err(err(Code::Conflict, "draft not available for replacement"));
        }
        if total_bytes > MAX_TEXT_BYTES || self.reserved_bytes() + total_bytes > MAX_SESSION_BYTES {
            return Err(err(Code::BudgetExceeded, "upload text budget exceeded"));
        }
        let upload = self.allocate()?;
        self.uploads.insert(
            upload,
            Upload {
                draft: id.id,
                owner: owner.id,
                revision: expected_revision,
                total: total_bytes,
                text: String::with_capacity(total_bytes),
                touched: self.clock,
            },
        );
        Ok(UploadId {
            scope: self.scope.clone(),
            id: upload,
        })
    }
    fn upload(&self, owner: &MountOwner, id: &UploadId) -> Result<&Upload> {
        self.owner(owner)?;
        self.scope(&id.scope)?;
        self.uploads
            .get(&id.id)
            .filter(|u| u.owner == owner.id)
            .ok_or_else(|| err(Code::Conflict, "upload expired or foreign owner"))
    }
    pub fn append_chunk(
        &mut self,
        owner: &MountOwner,
        id: &UploadId,
        offset: usize,
        text: &str,
        now: u64,
    ) -> Result<usize> {
        self.expire_uploads(now);
        let u = self.upload(owner, id)?;
        if text.is_empty()
            || text.len() > MAX_CHUNK_BYTES
            || offset != u.text.len()
            || text.len() > u.total - u.text.len()
        {
            return Err(err(Code::InvalidInput, "invalid upload chunk"));
        }
        let clock = self.clock;
        let u = self.uploads.get_mut(&id.id).expect("validated upload");
        u.text.push_str(text);
        u.touched = clock;
        Ok(u.text.len())
    }
    pub fn commit_replace(&mut self, owner: &MountOwner, id: &UploadId, now: u64) -> Result<u64> {
        self.expire_uploads(now);
        let u = self.upload(owner, id)?;
        if u.text.len() != u.total {
            return Err(err(Code::Conflict, "upload incomplete"));
        }
        let d = self
            .drafts
            .get(&u.draft)
            .ok_or_else(|| err(Code::NotFound, "draft removed"))?;
        if d.state != DraftState::Editing || d.revision != u.revision {
            self.uploads.remove(&id.id);
            return Err(err(Code::Conflict, "draft revision changed"));
        }
        let next = d
            .revision
            .checked_add(1)
            .ok_or_else(|| err(Code::BudgetExceeded, "draft revision exhausted"))?;
        let u = self.uploads.remove(&id.id).expect("validated upload");
        let d = self.drafts.get_mut(&u.draft).expect("validated draft");
        d.text = u.text;
        d.revision = next;
        Ok(next)
    }
    pub fn cancel_replace(&mut self, owner: &MountOwner, id: &UploadId) -> Result<()> {
        self.upload(owner, id)?;
        self.uploads.remove(&id.id);
        Ok(())
    }
    pub fn freeze_save(
        &mut self,
        id: &DraftId,
        expected_revision: u64,
        operation_id: &str,
    ) -> Result<SaveToken> {
        let d = self.draft(id)?;
        if operation_id.is_empty()
            || operation_id.len() > 128
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return Err(err(Code::InvalidInput, "invalid save operation"));
        }
        if d.revision != expected_revision
            || d.state == DraftState::Saved
            || self.uploads.values().any(|u| u.draft == id.id)
        {
            return Err(err(Code::Conflict, "draft cannot be frozen"));
        }
        if let Some((token, op)) = &d.save {
            if op == operation_id {
                return Ok(SaveToken {
                    scope: self.scope.clone(),
                    id: *token,
                });
            }
            return Err(err(Code::Conflict, "another save is unresolved"));
        }
        let token = self.allocate()?;
        let d = self.drafts.get_mut(&id.id).expect("validated draft");
        d.state = DraftState::Saving;
        d.save = Some((token, operation_id.into()));
        Ok(SaveToken {
            scope: self.scope.clone(),
            id: token,
        })
    }
    pub fn save_text(&self, token: &SaveToken) -> Result<(&str, &str)> {
        self.scope(&token.scope)?;
        self.drafts
            .values()
            .find_map(|d| {
                d.save
                    .as_ref()
                    .filter(|(id, _)| *id == token.id)
                    .map(|(_, op)| (op.as_str(), d.text.as_str()))
            })
            .ok_or_else(|| err(Code::Conflict, "save token retired"))
    }
    pub fn resolve_save(&mut self, token: &SaveToken, outcome: SaveOutcome) -> Result<()> {
        self.scope(&token.scope)?;
        let d = self
            .drafts
            .values_mut()
            .find(|d| d.save.as_ref().is_some_and(|(id, _)| *id == token.id))
            .ok_or_else(|| err(Code::Conflict, "save token retired"))?;
        if d.state == DraftState::Saved {
            return if outcome == SaveOutcome::Committed {
                Ok(())
            } else {
                Err(err(Code::Conflict, "save already committed"))
            };
        }
        match outcome {
            SaveOutcome::Unknown => {}
            SaveOutcome::DefinitelyNotCommitted => {
                d.state = DraftState::Editing;
                d.save = None;
            }
            SaveOutcome::Committed => d.state = DraftState::Saved,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_ids_and_revisions_fail_closed() {
        let mut e = DraftTexts::new();
        let o = e.create_owner().unwrap();
        let d = e.create_draft("unchanged").unwrap();
        e.drafts.get_mut(&d.id).unwrap().revision = u64::MAX;
        let u = e.begin_replace(&o, &d, u64::MAX, 0, 0).unwrap();
        assert_eq!(
            e.commit_replace(&o, &u, 0).unwrap_err().code,
            Code::BudgetExceeded
        );
        assert_eq!(e.read_chunk(&d, None, 0, 512).unwrap().text, "unchanged");
        e.next = u64::MAX;
        assert!(e.create_draft("").is_err());
        assert!(e.create_owner().is_err());
    }
    #[test]
    fn full_reservations_are_accounted_before_appending() {
        let mut e = DraftTexts::new();
        for _ in 0..MAX_DRAFTS {
            let o = e.create_owner().unwrap();
            let d = e.create_draft(&"a".repeat(MAX_TEXT_BYTES)).unwrap();
            e.begin_replace(&o, &d, 1, MAX_TEXT_BYTES, 0).unwrap();
        }
        assert_eq!(e.reserved_bytes(), MAX_SESSION_BYTES);
        e.expire_uploads(30);
        assert_eq!(e.reserved_bytes(), MAX_SESSION_BYTES / 2);
    }
}
