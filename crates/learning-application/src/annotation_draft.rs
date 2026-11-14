//! Bound source/text application drafts. Callers must independently enforce fresh Host authority.
use crate::{
    KnowledgeApplication,
    annotation_source::{FrozenAnnotationSource, FrozenPdfCompoundSource, FrozenPdfSource},
    draft_text::*,
};
use core_api::{
    knowledge::{KnowledgeError, KnowledgeErrorCode as Code, PdfReceipt, Provenance, Result},
    pdf_contract::PdfDraft,
};
use learning_ports::{KnowledgeCommand, KnowledgeQuery};
use std::io::{self, Write};
use std::sync::Arc;

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
struct Entry {
    id: DraftId,
    source: FrozenAnnotationSource,
    generation: i64,
    bytes: usize,
}
/// Only this registry can construct a save binding. It cannot be paired with another source.
#[derive(Clone, Debug)]
pub struct FrozenAnnotationSave {
    draft: DraftId,
    token: SaveToken,
}
#[derive(Default)]
pub struct AnnotationDrafts {
    provider_scope: Arc<()>,
    texts: DraftTexts,
    entries: Vec<Entry>,
}
fn fail(message: &'static str) -> KnowledgeError {
    KnowledgeError::new(Code::Conflict, message)
}
struct Counter {
    bytes: usize,
}
impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > MAX_SOURCE_BYTES - self.bytes {
            return Err(io::Error::other("source budget exceeded"));
        }
        self.bytes += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl AnnotationDrafts {
    pub(crate) fn provider_scope(&self) -> Arc<()> {
        self.provider_scope.clone()
    }
    pub(crate) fn provider_clock(&self) -> u64 {
        self.texts.provider_clock()
    }
    pub(crate) fn touch_owner_uploads(
        &mut self,
        owner: &MountOwner,
        now: u64,
    ) -> Result<(u64, bool)> {
        self.texts.touch_owner_uploads(owner, now)
    }
    pub fn new() -> Self {
        Self::default()
    }
    #[allow(clippy::too_many_arguments)] // Explicit immutable source identity, checked together.
    pub fn create_pdf<S: KnowledgeQuery>(
        &mut self,
        app: &KnowledgeApplication<S>,
        document_id: i64,
        vault: &str,
        selection: PdfDraft,
        provenance: Provenance,
        generation: i64,
    ) -> Result<DraftId> {
        if generation <= 0 {
            return Err(fail("invalid capture generation"));
        }
        if self.entries.len() >= MAX_DRAFTS {
            return Err(KnowledgeError::new(
                Code::BudgetExceeded,
                "source count exceeded",
            ));
        }
        let source = FrozenPdfSource::freeze(app, document_id, vault, selection, provenance)?;
        self.insert_source(FrozenAnnotationSource::Single(source), generation)
    }
    #[allow(clippy::too_many_arguments)] // Explicit immutable source identity, checked together.
    pub fn create_pdf_compound<S: KnowledgeQuery>(
        &mut self,
        app: &KnowledgeApplication<S>,
        document_id: i64,
        vault: &str,
        selection: core_api::pdf_compound::PdfCompoundDraft,
        provenance: Provenance,
        generation: i64,
    ) -> Result<DraftId> {
        if generation <= 0 {
            return Err(fail("invalid capture generation"));
        }
        if self.entries.len() >= MAX_DRAFTS {
            return Err(KnowledgeError::new(
                Code::BudgetExceeded,
                "source count exceeded",
            ));
        }
        let source =
            FrozenPdfCompoundSource::freeze(app, document_id, vault, selection, provenance)?;
        self.insert_source(FrozenAnnotationSource::Compound(source), generation)
    }
    fn insert_source(
        &mut self,
        source: FrozenAnnotationSource,
        generation: i64,
    ) -> Result<DraftId> {
        let mut counter = Counter { bytes: 0 };
        let counted = match &source {
            FrozenAnnotationSource::Single(s) => serde_json::to_writer(
                &mut counter,
                &(
                    "single",
                    s.document_id(),
                    s.vault(),
                    s.selection(),
                    s.provenance(),
                    generation,
                ),
            ),
            FrozenAnnotationSource::Compound(s) => serde_json::to_writer(
                &mut counter,
                &(
                    "compound",
                    s.document_id(),
                    s.vault(),
                    s.selection(),
                    s.provenance(),
                    generation,
                ),
            ),
        };
        counted.map_err(|_| {
            KnowledgeError::new(Code::BudgetExceeded, "source facts budget exceeded")
        })?;
        if self
            .source_bytes()
            .checked_add(counter.bytes)
            .is_none_or(|size| size > MAX_SOURCE_BYTES)
        {
            return Err(KnowledgeError::new(
                Code::BudgetExceeded,
                "source session budget exceeded",
            ));
        }
        let id = self.texts.create_draft("")?;
        self.entries.push(Entry {
            id: id.clone(),
            source,
            generation,
            bytes: counter.bytes,
        });
        Ok(id)
    }
    fn entry(&self, id: &DraftId) -> Result<&Entry> {
        self.entries
            .iter()
            .find(|e| &e.id == id)
            .ok_or_else(|| fail("foreign or retired annotation draft"))
    }
    /// Immutable facts for the caller's fresh capture-installation authority gate.
    pub fn source(&self, id: &DraftId) -> Result<(&FrozenAnnotationSource, i64)> {
        let e = self.entry(id)?;
        Ok((&e.source, e.generation))
    }
    pub fn source_bytes(&self) -> usize {
        self.entries.iter().map(|e| e.bytes).sum()
    }
    pub fn reserved_text_bytes(&self) -> usize {
        self.texts.reserved_bytes()
    }
    pub fn create_owner(&mut self) -> Result<MountOwner> {
        self.texts.create_owner()
    }
    pub fn revoke_owner(&mut self, owner: &MountOwner) -> Result<()> {
        self.texts.revoke_owner(owner)
    }
    pub fn state(&self, id: &DraftId) -> Result<DraftState> {
        self.entry(id)?;
        self.texts.state(id)
    }
    pub fn read_chunk(
        &self,
        id: &DraftId,
        revision: Option<u64>,
        offset: usize,
        max_bytes: usize,
    ) -> Result<DraftChunk> {
        self.entry(id)?;
        self.texts.read_chunk(id, revision, offset, max_bytes)
    }
    pub fn begin_replace(
        &mut self,
        owner: &MountOwner,
        id: &DraftId,
        expected: u64,
        total: usize,
        now: u64,
    ) -> Result<UploadId> {
        self.entry(id)?;
        self.texts.begin_replace(owner, id, expected, total, now)
    }
    pub fn append_chunk(
        &mut self,
        owner: &MountOwner,
        upload: &UploadId,
        offset: usize,
        text: &str,
        now: u64,
    ) -> Result<usize> {
        self.texts.append_chunk(owner, upload, offset, text, now)
    }
    pub fn commit_replace(
        &mut self,
        owner: &MountOwner,
        upload: &UploadId,
        now: u64,
    ) -> Result<u64> {
        self.texts.commit_replace(owner, upload, now)
    }
    pub fn cancel_replace(&mut self, owner: &MountOwner, upload: &UploadId) -> Result<()> {
        self.texts.cancel_replace(owner, upload)
    }
    pub fn expire_uploads(&mut self, now: u64) {
        self.texts.expire_uploads(now);
    }
    pub fn remove(&mut self, id: &DraftId) -> Result<()> {
        self.entry(id)?;
        self.texts.remove_draft(id)?;
        self.entries.retain(|e| &e.id != id);
        Ok(())
    }
    pub fn freeze_save(
        &mut self,
        id: &DraftId,
        expected: u64,
        operation: &str,
    ) -> Result<FrozenAnnotationSave> {
        self.entry(id)?;
        let token = self.texts.freeze_save(id, expected, operation)?;
        Ok(FrozenAnnotationSave {
            draft: id.clone(),
            token,
        })
    }
    pub fn save_source(
        &self,
        frozen: &FrozenAnnotationSave,
    ) -> Result<(&FrozenAnnotationSource, i64)> {
        self.texts.save_text(&frozen.token)?;
        self.source(&frozen.draft)
    }
    /// Caller must fresh-authorize the ORIGINAL capture identity before entering this use case.
    pub fn commit<S: KnowledgeQuery + KnowledgeCommand>(
        &mut self,
        app: &mut KnowledgeApplication<S>,
        frozen: &FrozenAnnotationSave,
    ) -> Result<PdfReceipt> {
        let entry = self.entry(&frozen.draft)?;
        let (operation, note) = self.texts.save_text(&frozen.token)?;
        let receipt = entry.source.capture(app, operation, note)?;
        self.texts
            .resolve_save(&frozen.token, SaveOutcome::Committed)?;
        Ok(receipt)
    }
    /// None or error preserves Saving; absence is not proof a prior attempt cannot commit.
    pub fn reconcile<S: KnowledgeQuery>(
        &mut self,
        app: &KnowledgeApplication<S>,
        frozen: &FrozenAnnotationSave,
    ) -> Result<Option<PdfReceipt>> {
        let entry = self.entry(&frozen.draft)?;
        let (operation, note) = self.texts.save_text(&frozen.token)?;
        let receipt = entry.source.receipt(app, operation, note)?;
        if receipt.is_some() {
            self.texts
                .resolve_save(&frozen.token, SaveOutcome::Committed)?;
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_counter_refuses_overflow_without_incrementing() {
        let mut counter = Counter {
            bytes: MAX_SOURCE_BYTES - 1,
        };
        assert!(counter.write_all(b"xx").is_err());
        assert_eq!(counter.bytes, MAX_SOURCE_BYTES - 1);
        counter.write_all(b"x").unwrap();
        assert!(counter.write_all(b"x").is_err());
        assert_eq!(counter.bytes, MAX_SOURCE_BYTES);
    }
}
