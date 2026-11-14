//! Durable knowledge use cases: text/PDF/EPUB capture, library/notes queries,
//! immutable annotation source bindings, and full-store archive export/restore.
pub mod annotation_source;
pub mod epub_library;
pub mod pdf_library;
pub mod pdf_notes;

pub use learning_ports::archive;

use crate::fail;
use core_api::{
    contract::{CaptureDraft, ResolvedRange},
    knowledge::*,
    pdf_compound::{PdfCompoundDraft, PdfCompoundSelection},
    pdf_contract::{PdfDraft, PdfSelection, PdfSnapshot},
};
use learning_ports::{
    KnowledgeArchive, KnowledgeCommand, KnowledgeQuery, OperationLookup, PdfCaptureWithVault,
    archive::{archive_from_legacy, validate_archive, validate_bundle},
};

pub struct KnowledgeApplication<S> {
    store: S,
}

impl<S> KnowledgeApplication<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    pub fn into_store(self) -> S {
        self.store
    }
}

impl<S: KnowledgeQuery> KnowledgeApplication<S> {
    pub fn receipt(&self, op: &str) -> Result<Option<Receipt>> {
        self.store.text_receipt_by_operation(op)
    }

    pub fn query(&self, id: i64) -> Result<CaptureRecord> {
        self.store
            .text_capture(id)?
            .ok_or_else(|| KnowledgeError::not_found("capture not found"))
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

    pub fn read_original(&self, id: i64) -> Result<Vec<u8>> {
        self.original_pdf(id)
    }

    pub fn pdf_receipt(&self, op: &str) -> Result<Option<PdfReceipt>> {
        match self.store.pdf_capture_by_operation(op)? {
            OperationLookup::Match(found) => Ok(Some(found.record.receipt)),
            OperationLookup::OtherOperationKind | OperationLookup::Absent => Ok(None),
        }
    }

    pub(crate) fn pdf_capture_operation(
        &self,
        op: &str,
    ) -> Result<OperationLookup<PdfCaptureWithVault>> {
        self.store.pdf_capture_by_operation(op)
    }

    pub fn document_receipt(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        self.store.pdf_document_receipt_by_operation(op)
    }

    pub fn query_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        self.store.commit_by_operation(op)
    }

    pub fn query_document(&self, id: i64) -> Result<PdfDocumentRecord> {
        self.store
            .pdf_document(id)?
            .ok_or_else(|| KnowledgeError::not_found("PDF document not found"))
    }

    pub fn query_pdf(&self, id: i64) -> Result<PdfCaptureRecord> {
        let record = self.query_pdf_complete(id)?;
        if record.compound.is_some() {
            return Err(KnowledgeError::unsupported_version(
                "compound PDF record requires complete compound query",
            ));
        }
        Ok(record)
    }

    pub fn query_pdf_complete(&self, id: i64) -> Result<PdfCaptureRecord> {
        self.store
            .pdf_capture(id)?
            .ok_or_else(|| KnowledgeError::not_found("PDF capture not found"))
    }

    pub fn original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        self.store.read_original_pdf(id)
    }

    pub fn resolve_pdf(&self, id: i64, expected: &PdfDraft) -> Result<PdfSelection> {
        let capture = self.query_pdf(id)?;
        if capture.compound.is_some() {
            return Err(KnowledgeError::unsupported_version(
                "compound PDF anchor requires complete compound resolution",
            ));
        }
        if &capture.draft != expected {
            return fail("saved PDF anchor mismatch");
        }
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
}

impl<S: KnowledgeCommand> KnowledgeApplication<S> {
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
        self.store
            .commit_text_capture(op, vault, draft, origin, user_note)
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
            return Err(KnowledgeError::budget_exceeded(
                "PDF original budget exceeded",
            ));
        }
        snapshot.validate_original(original)?;
        self.store
            .import_pdf_document(op, vault, title, original, snapshot)
    }

    pub fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.store
            .capture_pdf(op, document_id, draft, origin, user_note)
    }

    pub fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
    ) -> Result<PdfReceipt> {
        self.store
            .capture_pdf_compound(op, document_id, draft, origin, user_note)
    }

    /// Update the user note of a PDF capture scoped by `vault`.
    pub fn update_pdf_capture_note(
        &mut self,
        vault: &str,
        capture_id: i64,
        note: &str,
    ) -> Result<()> {
        if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
            return Err(KnowledgeError::invalid_input("invalid vault"));
        }
        validate_note_input(note)?;
        self.store.update_pdf_capture_note(vault, capture_id, note)
    }

    /// Delete a PDF capture scoped by `vault`.
    pub fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
            return Err(KnowledgeError::invalid_input("invalid vault"));
        }
        self.store.delete_pdf_capture(vault, capture_id)
    }

    /// Delete a PDF document and its captures scoped by `vault`.
    pub fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
            return Err(KnowledgeError::invalid_input("invalid vault"));
        }
        self.store.delete_pdf_document(vault, document_id)
    }
}

impl<S: KnowledgeArchive> KnowledgeApplication<S> {
    pub fn export_all(&self) -> Result<ArchiveV2> {
        let a = self.store.snapshot()?;
        validate_archive(&a)?;
        Ok(a)
    }

    pub fn export(&self) -> Result<ExportBundle> {
        let archive = self.export_all()?;
        if !archive.pdf_documents.is_empty()
            || !archive.pdf_captures.is_empty()
            || !archive.epub_documents.is_empty()
            || !archive.epub_captures.is_empty()
        {
            return Err(KnowledgeError::unsupported_version(
                "PDF/EPUB knowledge requires complete schema 2 export_all",
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
            return Err(KnowledgeError::budget_exceeded(
                "restore byte budget exceeded",
            ));
        }
        let bundle: ExportBundle = serde_json::from_slice(bytes)?;
        self.restore(&bundle)
    }

    pub fn restore(&mut self, bundle: &ExportBundle) -> Result<RestoreReceipt> {
        validate_bundle(bundle)?;
        let archive = archive_from_legacy(bundle)?;
        self.restore_all(&archive)?;
        Ok(RestoreReceipt {
            restored: bundle.entries.len(),
        })
    }

    pub fn restore_all_json(&mut self, bytes: &[u8]) -> Result<RestoreAllReceipt> {
        if bytes.len() > ARCHIVE_LIMIT {
            return Err(KnowledgeError::budget_exceeded(
                "archive byte budget exceeded",
            ));
        }
        let archive: ArchiveV2 = serde_json::from_slice(bytes)?;
        self.restore_all(&archive)
    }

    pub fn restore_all(&mut self, archive: &ArchiveV2) -> Result<RestoreAllReceipt> {
        if !self.store.is_empty()? {
            return fail("restore requires empty Core");
        }
        validate_archive(archive)?;
        self.store.restore(archive)?;
        Ok(RestoreAllReceipt {
            text_captures: archive.text_entries.len(),
            pdf_documents: archive.pdf_documents.len(),
            pdf_captures: archive.pdf_captures.len(),
            epub_documents: archive.epub_documents.len(),
            epub_captures: archive.epub_captures.len(),
        })
    }
}
