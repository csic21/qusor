//! Experiment-only compatibility composition of production application and adapters.
//! Crash qualification terminates only here, outside production APIs.
use crate::{
    contract::{CaptureDraft, ResolvedRange},
    pdf_compound::{PdfCompoundDraft, PdfCompoundSelection},
    pdf_contract::{PdfDraft, PdfSelection, PdfSnapshot},
};
pub use core_api::knowledge::*;
use learning_application::KnowledgeApplication;
use std::{path::Path, sync::Arc};
use storage_sqlite::{SqliteStore, StorageStage};
pub struct Core {
    application: KnowledgeApplication<SqliteStore>,
}
fn fault(point: Option<&str>, migration: bool) -> Result<Arc<dyn Fn(StorageStage) + Send + Sync>> {
    let stage = match point {
        None => None,
        Some("before_migration_commit") if migration => Some(StorageStage::BeforeMigrationCommit),
        Some("after_migration_commit") if migration => Some(StorageStage::AfterMigrationCommit),
        Some("after_blob_write") if !migration => Some(StorageStage::AfterBlobWrite),
        Some("before_blob_publish") if !migration => Some(StorageStage::BeforeBlobPublish),
        Some("after_blob_publish") if !migration => Some(StorageStage::AfterBlobPublish),
        Some("before_transaction") if !migration => Some(StorageStage::BeforeTransaction),
        Some("after_entities") if !migration => Some(StorageStage::AfterEntities),
        Some("before_commit") if !migration => Some(StorageStage::BeforeCommit),
        Some("after_commit") if !migration => Some(StorageStage::AfterCommit),
        _ => {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::InvalidInput,
                "unknown crash point",
            ));
        }
    };
    Ok(Arc::new(move |at| {
        if stage == Some(at) {
            std::process::exit(86);
        }
    }))
}
impl Core {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_migration_probe(path, None)
    }
    pub fn open_migration_probe(path: impl AsRef<Path>, point: Option<&str>) -> Result<Self> {
        let store = SqliteStore::with_observer(path, fault(point, true)?)?;
        Ok(Self {
            application: KnowledgeApplication::new(store),
        })
    }
    pub fn receipt(&self, op: &str) -> Result<Option<Receipt>> {
        self.application.receipt(op)
    }
    pub fn query(&self, id: i64) -> Result<CaptureRecord> {
        self.application.query(id)
    }
    pub fn resolve(
        &self,
        id: i64,
        hash: &str,
        normalization: &str,
        unit: &str,
    ) -> Result<ResolvedRange> {
        self.application.resolve(id, hash, normalization, unit)
    }
    pub fn resolve_version(
        &self,
        id: i64,
        expected_version: &str,
        hash: &str,
        normalization: &str,
        unit: &str,
    ) -> Result<ResolvedRange> {
        self.application
            .resolve_version(id, expected_version, hash, normalization, unit)
    }
    pub fn commit(
        &mut self,
        op: &str,
        vault: &str,
        draft: &CaptureDraft,
        origin: &Provenance,
        user_note: &str,
        point: Option<&str>,
    ) -> Result<Receipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application.commit(op, vault, draft, origin, user_note)
    }
    pub fn export(&self) -> Result<ExportBundle> {
        self.application.export()
    }
    pub fn restore_json(&mut self, bytes: &[u8], point: Option<&str>) -> Result<RestoreReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application.restore_json(bytes)
    }
    pub fn restore(
        &mut self,
        bundle: &ExportBundle,
        point: Option<&str>,
    ) -> Result<RestoreReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application.restore(bundle)
    }
    pub fn read_original(&self, id: i64) -> Result<Vec<u8>> {
        self.application.read_original(id)
    }
    pub fn pdf_receipt(&self, op: &str) -> Result<Option<PdfReceipt>> {
        self.application.pdf_receipt(op)
    }
    pub fn document_receipt(&self, op: &str) -> Result<Option<PdfDocumentReceipt>> {
        self.application.document_receipt(op)
    }
    pub fn query_operation(&self, op: &str) -> Result<Option<CommitEntry>> {
        self.application.query_operation(op)
    }
    pub fn import_document(
        &mut self,
        op: &str,
        vault: &str,
        title: &str,
        original: &[u8],
        snapshot: &PdfSnapshot,
        point: Option<&str>,
    ) -> Result<PdfDocumentReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application
            .import_document(op, vault, title, original, snapshot)
    }
    pub fn capture_pdf(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfDraft,
        origin: &Provenance,
        user_note: &str,
        point: Option<&str>,
    ) -> Result<PdfReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application
            .capture_pdf(op, document_id, draft, origin, user_note)
    }
    pub fn capture_pdf_compound(
        &mut self,
        op: &str,
        document_id: i64,
        draft: &PdfCompoundDraft,
        origin: &Provenance,
        user_note: &str,
        point: Option<&str>,
    ) -> Result<PdfReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application
            .capture_pdf_compound(op, document_id, draft, origin, user_note)
    }
    pub fn query_document(&self, id: i64) -> Result<PdfDocumentRecord> {
        self.application.query_document(id)
    }
    pub fn query_pdf(&self, id: i64) -> Result<PdfCaptureRecord> {
        self.application.query_pdf(id)
    }
    pub fn query_pdf_complete(&self, id: i64) -> Result<PdfCaptureRecord> {
        self.application.query_pdf_complete(id)
    }
    pub fn original_pdf(&self, id: i64) -> Result<Vec<u8>> {
        self.application.original_pdf(id)
    }
    pub fn resolve_pdf(&self, id: i64, expected: &PdfDraft) -> Result<PdfSelection> {
        self.application.resolve_pdf(id, expected)
    }
    pub fn resolve_pdf_compound(
        &self,
        id: i64,
        expected: &PdfCompoundDraft,
    ) -> Result<PdfCompoundSelection> {
        self.application.resolve_pdf_compound(id, expected)
    }
    pub fn export_all(&self) -> Result<ArchiveV2> {
        self.application.export_all()
    }
    pub fn restore_all_json(
        &mut self,
        bytes: &[u8],
        point: Option<&str>,
    ) -> Result<RestoreAllReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application.restore_all_json(bytes)
    }
    pub fn restore_all(
        &mut self,
        archive: &ArchiveV2,
        point: Option<&str>,
    ) -> Result<RestoreAllReceipt> {
        self.application
            .store_mut()
            .set_observer(fault(point, false)?);
        self.application.restore_all(archive)
    }
}
