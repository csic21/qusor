//! Durable knowledge use cases independent of SQL, filesystems, Host, UI and runtime.
pub mod annotation_draft;
pub mod annotation_source;
pub mod draft_text;
pub mod pdf_notes;
pub mod provider_events;
pub mod provider_projections;
pub mod study;
use core_api::{
    contract::{CaptureDraft, ResolvedRange, sha256},
    knowledge::*,
    pdf_compound::{PdfCompoundDraft, PdfCompoundSelection},
    pdf_contract::{PdfDraft, PdfSelection, PdfSnapshot},
};
use learning_ports::{CommitPlan, KnowledgeStore};
pub struct KnowledgeApplication<S> {
    store: S,
}
enum CaptureSelection<'a> {
    Single(&'a PdfDraft),
    Compound(&'a PdfCompoundDraft),
}
fn fail<T>(message: &str) -> Result<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Conflict, message))
}
impl<S: KnowledgeStore> KnowledgeApplication<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
    pub fn export_all(&self) -> Result<ArchiveV2> {
        let a = self.store.snapshot()?;
        validate_archive(&a)?;
        Ok(a)
    }
    pub fn receipt(&self, op: &str) -> Result<Option<Receipt>> {
        Ok(self
            .export_all()?
            .text_entries
            .into_iter()
            .find(|e| e.record.receipt.operation_id == op)
            .map(|e| e.record.receipt))
    }
    pub fn query(&self, id: i64) -> Result<CaptureRecord> {
        self.export_all()?
            .text_entries
            .into_iter()
            .map(|e| e.record)
            .find(|r| r.receipt.capture_id == id)
            .ok_or_else(|| KnowledgeError::new(KnowledgeErrorCode::NotFound, "capture not found"))
    }
    pub fn resolve(
        &self,
        id: i64,
        hash: &str,
        normalization: &str,
        unit: &str,
    ) -> Result<ResolvedRange> {
        self.resolve_version(id, &id.to_string(), hash, normalization, unit)
    }
    pub fn resolve_version(
        &self,
        id: i64,
        expected_version: &str,
        hash: &str,
        normalization: &str,
        unit: &str,
    ) -> Result<ResolvedRange> {
        let r = self.query(id)?;
        if expected_version != r.receipt.document_version_id.to_string()
            || hash != r.receipt.snapshot_hash
            || normalization != r.draft.selector.normalization
            || unit != r.draft.selector.unit
        {
            return fail("locator version/hash/normalization/unit mismatch");
        }
        let anchor = r.draft.anchor(&r.receipt.document_version_id.to_string())?;
        Ok(core_api::contract::resolve_anchor(
            &r.draft.snapshot_text,
            &anchor,
            expected_version,
        )?)
    }
    pub fn commit(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<Receipt> {
        draft.validate()?;
        validate_native(op, vault, origin, user_note)?;
        let initial = self.export_all()?;
        let id = initial.text_entries.len() as i64 + 1;
        let receipt = Receipt {
            operation_id: op.into(),
            capture_id: id,
            source_id: id,
            document_version_id: id,
            commit_rev: initial.commits.len() as i64 + 1,
            snapshot_hash: draft.snapshot_hash(),
            draft_hash: draft.draft_hash()?,
        };
        let record = CaptureRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            draft: draft.clone(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let hash = input_hash(&record)?;
        if let Some(old) = initial
            .text_entries
            .iter()
            .find(|e| e.record.receipt.operation_id == op)
            .map(|e| e.record.receipt.clone())
        {
            let prior = initial
                .text_entries
                .iter()
                .find(|e| e.record.receipt.operation_id == op)
                .ok_or("receipt operation missing")?
                .input_hash
                .clone();
            if prior != hash {
                return fail("operation id payload conflict");
            }
            return Ok(old);
        }
        let ev = event(&record)?;
        let entry = ExportEntry {
            record,
            input_hash: hash.clone(),
            operation: ev.clone(),
            outbox: ev.clone(),
            index_job: ev,
        };
        let mut projected = initial.clone();
        ensure_new_operation(&initial, op)?;
        projected.text_entries.push(entry.clone());
        if !projected
            .blobs
            .iter()
            .any(|b| b.hash == receipt.snapshot_hash)
        {
            projected
                .blobs
                .push(BinaryBlob::new(draft.snapshot_text.as_bytes()));
        }
        projected.commits.push(text_commit(&entry)?);
        validate_archive(&projected)?;
        self.store.commit(CommitPlan {
            expected_revision: receipt.commit_rev - 1,
            next: projected,
        })?;
        Ok(receipt)
    }
    pub fn export(&self) -> Result<ExportBundle> {
        let archive = self.export_all()?;
        if !archive.pdf_documents.is_empty() || !archive.pdf_captures.is_empty() {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::UnsupportedVersion,
                "PDF knowledge requires complete schema 2 export_all",
            ));
        }
        let bundle = ExportBundle {
            schema_version: 1,
            entries: archive.text_entries,
            blobs: archive
                .blobs
                .into_iter()
                .map(|b| {
                    Ok(Blob {
                        hash: b.hash.clone(),
                        text: String::from_utf8(b.decode()?)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        validate_bundle(&bundle)?;
        Ok(bundle)
    }
    pub fn restore_json(&mut self, bytes: &[u8]) -> Result<RestoreReceipt> {
        if bytes.len() > LIMIT {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "restore byte budget exceeded",
            ));
        }
        let bundle: ExportBundle = serde_json::from_slice(bytes)?;
        self.restore(&bundle)
    }
    pub fn restore(&mut self, bundle: &ExportBundle) -> Result<RestoreReceipt> {
        validate_bundle(bundle)?;
        let archive = ArchiveV2::from_legacy(bundle)?;
        self.restore_all(&archive)?;
        Ok(RestoreReceipt {
            restored: bundle.entries.len(),
        })
    }
    pub fn read_original(&self, id: i64) -> Result<Vec<u8>> {
        self.original_pdf(id)
    }
    pub fn pdf_receipt(&self, op: &str) -> Result<Option<PdfReceipt>> {
        let archive = self.export_all()?;
        Ok(archive
            .pdf_captures
            .into_iter()
            .find(|record| record.receipt.operation_id == op)
            .map(|record| record.receipt))
    }
    pub fn document_receipt(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        let archive = self.export_all()?;
        Ok(archive
            .pdf_documents
            .into_iter()
            .find(|record| record.receipt.operation_id == op)
            .map(|record| record.receipt))
    }
    pub fn query_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        Ok(self
            .export_all()?
            .commits
            .into_iter()
            .find(|commit| commit.operation_id == op))
    }
    pub fn import_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
    ) -> Result<PdfDocumentReceipt> {
        native_import(op, vault, title)?;
        if original.len() > MAX_ORIGINAL {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "PDF original budget exceeded",
            ));
        }
        snapshot.validate_original(original)?;
        let mut archive = self.export_all()?;
        let id = archive.pdf_documents.len() as i64 + 1;
        let receipt = PdfDocumentReceipt {
            operation_id: op.into(),
            document_id: id,
            commit_rev: archive.commits.len() as i64 + 1,
            original_sha256: sha256(original),
            extraction_revision: snapshot.extraction_revision()?,
        };
        let record = PdfDocumentRecord {
            receipt: receipt.clone(),
            vault: vault.into(),
            title: title.into(),
            snapshot: snapshot.clone(),
        };
        let operation = operation_for_document(&record)?;
        if let Some(prior) = archive
            .pdf_operations
            .iter()
            .find(|old| old.operation_id == op)
        {
            if prior.kind != operation.kind || prior.input_hash != operation.input_hash {
                return fail("operation id payload conflict");
            }
            return Ok(serde_json::from_str(&prior.receipt)?);
        }
        ensure_new_operation(&archive, op)?;
        archive.pdf_documents.push(record.clone());
        archive.pdf_operations.push(operation.clone());
        archive
            .commits
            .push(commit_for_pdf(&operation, receipt.commit_rev));
        if !archive
            .blobs
            .iter()
            .any(|blob| blob.hash == receipt.original_sha256)
        {
            archive.blobs.push(BinaryBlob::new(original));
        }
        validate_archive(&archive)?;
        self.store.commit(CommitPlan {
            expected_revision: receipt.commit_rev - 1,
            next: archive,
        })?;
        Ok(receipt)
    }
    pub fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.capture_pdf_inner(
            op,
            document_id,
            CaptureSelection::Single(draft),
            origin,
            user_note,
        )
    }
    pub fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.capture_pdf_inner(
            op,
            document_id,
            CaptureSelection::Compound(draft),
            origin,
            user_note,
        )
    }
    fn capture_pdf_inner(
        &mut self,
        op: &str,
        document_id: i64,
        selection: CaptureSelection<'_>,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        let mut archive = self.export_all()?;
        let document = archive
            .pdf_documents
            .iter()
            .find(|d| d.receipt.document_id == document_id)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "PDF document not found")
            })?
            .clone();
        validate_native(op, &document.vault, origin, user_note)?;
        let (draft, compound, draft_hash) = match selection {
            CaptureSelection::Single(draft) => {
                core_api::pdf_contract::resolve_pdf(&document.snapshot, draft)?;
                (draft, None, draft.draft_hash()?)
            }
            CaptureSelection::Compound(compound) => {
                compound.resolve(&document.snapshot)?;
                let draft = compound
                    .segments
                    .first()
                    .ok_or("compound PDF has no segments")?;
                (draft, Some(compound), compound.draft_hash()?)
            }
        };
        let receipt = PdfReceipt {
            operation_id: op.into(),
            capture_id: archive.pdf_captures.len() as i64 + 1,
            document_id,
            commit_rev: archive.commits.len() as i64 + 1,
            original_sha256: document.receipt.original_sha256.clone(),
            extraction_revision: document.receipt.extraction_revision.clone(),
            draft_hash,
        };
        let record = PdfCaptureRecord {
            receipt: receipt.clone(),
            draft: draft.clone(),
            compound: compound.cloned(),
            provenance: origin.clone(),
            user_note: user_note.into(),
        };
        let operation = operation_for_capture(&record, &document)?;
        if let Some(prior) = archive
            .pdf_operations
            .iter()
            .find(|old| old.operation_id == op)
        {
            if prior.kind != operation.kind || prior.input_hash != operation.input_hash {
                return fail("operation id payload conflict");
            }
            return Ok(serde_json::from_str(&prior.receipt)?);
        }
        ensure_new_operation(&archive, op)?;
        archive.pdf_captures.push(record.clone());
        if compound.is_some() {
            archive.schema_version = 3;
        }
        archive.pdf_operations.push(operation.clone());
        archive
            .commits
            .push(commit_for_pdf(&operation, receipt.commit_rev));
        validate_archive(&archive)?;
        self.store.commit(CommitPlan {
            expected_revision: receipt.commit_rev - 1,
            next: archive,
        })?;
        Ok(receipt)
    }
    pub fn query_document(&self, id: i64) -> Result<PdfDocumentRecord> {
        self.export_all()?
            .pdf_documents
            .into_iter()
            .find(|d| d.receipt.document_id == id)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "PDF document not found")
            })
    }
    pub fn query_pdf(&self, id: i64) -> Result<PdfCaptureRecord> {
        let record = self.query_pdf_complete(id)?;
        if record.compound.is_some() {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::UnsupportedVersion,
                "compound PDF record requires complete compound query",
            ));
        }
        Ok(record)
    }
    pub fn query_pdf_complete(&self, id: i64) -> Result<PdfCaptureRecord> {
        self.export_all()?
            .pdf_captures
            .into_iter()
            .find(|d| d.receipt.capture_id == id)
            .ok_or_else(|| {
                KnowledgeError::new(KnowledgeErrorCode::NotFound, "PDF capture not found")
            })
    }
    pub fn original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        let doc = self.query_document(id)?;
        self.export_all()?
            .blobs
            .into_iter()
            .find(|b| b.hash == doc.receipt.original_sha256)
            .ok_or("PDF original missing")?
            .decode()
    }
    pub fn resolve_pdf(&self, id: i64, expected: &PdfDraft) -> Result<PdfSelection> {
        let capture = self.query_pdf(id)?;
        if capture.compound.is_some() {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::UnsupportedVersion,
                "compound PDF anchor requires complete compound resolution",
            ));
        }
        if &capture.draft != expected {
            return fail("saved PDF anchor mismatch");
        };
        let document = self.query_document(capture.receipt.document_id)?;
        Ok(core_api::pdf_contract::resolve_pdf(
            &document.snapshot,
            expected,
        )?)
    }
    pub fn resolve_pdf_compound(
        &self,
        id: i64,
        expected: &PdfCompoundDraft,
    ) -> Result<PdfCompoundSelection> {
        let capture = self.query_pdf_complete(id)?;
        if capture.compound.as_ref() != Some(expected) {
            return fail("saved compound PDF anchor mismatch");
        }
        let document = self.query_document(capture.receipt.document_id)?;
        Ok(expected.resolve(&document.snapshot)?)
    }
    pub fn restore_all_json(&mut self, bytes: &[u8]) -> Result<RestoreAllReceipt> {
        if bytes.len() > ARCHIVE_LIMIT {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::BudgetExceeded,
                "archive byte budget exceeded",
            ));
        };
        let archive: ArchiveV2 = serde_json::from_slice(bytes)?;
        self.restore_all(&archive)
    }
    pub fn restore_all(&mut self, archive: &ArchiveV2) -> Result<RestoreAllReceipt> {
        if !self.export_all()?.commits.is_empty() {
            return fail("restore requires empty Core");
        };
        validate_archive(archive)?;
        self.store.restore(archive)?;
        Ok(RestoreAllReceipt {
            text_captures: archive.text_entries.len(),
            pdf_documents: archive.pdf_documents.len(),
            pdf_captures: archive.pdf_captures.len(),
        })
    }
}

fn ensure_new_operation(archive: &ArchiveV2, op: &str) -> Result<()> {
    if archive.commits.iter().any(|c| c.operation_id == op) {
        return fail("operation id already belongs to a different payload or kind");
    }
    Ok(())
}
