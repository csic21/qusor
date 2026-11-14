//! Shared synthetic Core fixture and explicitly injected lost-reply store.
use core_api::knowledge::*;
use learning_application::KnowledgeApplication;
#[allow(deprecated)]
use learning_ports::{CommitPlan, KnowledgeStore};
pub struct Memory {
    archive: ArchiveV2,
    pub lose_reply: bool,
}
impl Memory {
    pub fn empty() -> Self {
        Self {
            archive: ArchiveV2 {
                schema_version: 2,
                text_entries: vec![],
                pdf_documents: vec![],
                pdf_captures: vec![],
                pdf_operations: vec![],
                epub_documents: vec![],
                epub_captures: vec![],
                commits: vec![],
                blobs: vec![],
            },
            lose_reply: false,
        }
    }
}
#[allow(deprecated)]
impl KnowledgeStore for Memory {
    fn snapshot(&self) -> Result<ArchiveV2> {
        Ok(self.archive.clone())
    }
    fn commit(&mut self, plan: CommitPlan) -> Result<()> {
        let max_rev = self
            .archive
            .commits
            .iter()
            .map(|c| c.commit_rev)
            .max()
            .unwrap_or(0);
        assert_eq!(plan.expected_revision, max_rev);
        validate_archive(&plan.next)?;
        self.archive = plan.next;
        if self.lose_reply {
            self.lose_reply = false;
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::Storage,
                "injected lost reply after commit",
            ));
        }
        Ok(())
    }
    fn restore(&mut self, a: &ArchiveV2) -> Result<()> {
        validate_archive(a)?;
        self.archive = a.clone();
        Ok(())
    }
    fn update_pdf_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        validate_note_input(note)?;
        let Some(pos) = self
            .archive
            .pdf_captures
            .iter()
            .position(|c| c.receipt.capture_id == capture_id)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "PDF capture not found",
            ));
        };
        let doc_id = self.archive.pdf_captures[pos].receipt.document_id;
        let Some(doc) = self
            .archive
            .pdf_documents
            .iter()
            .find(|d| d.receipt.document_id == doc_id && d.vault == vault)
            .cloned()
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "PDF capture not found",
            ));
        };
        self.archive.pdf_captures[pos].user_note = note.to_owned();
        let updated_op = operation_for_capture(&self.archive.pdf_captures[pos], &doc)?;
        if let Some(op) = self
            .archive
            .pdf_operations
            .iter_mut()
            .find(|o| o.operation_id == updated_op.operation_id)
        {
            *op = updated_op.clone();
        }
        if let Some(commit) = self
            .archive
            .commits
            .iter_mut()
            .find(|c| c.operation_id == updated_op.operation_id)
        {
            commit.input_hash = updated_op.input_hash;
        }
        validate_archive(&self.archive)?;
        Ok(())
    }
    fn delete_pdf_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        let Some(pos) = self
            .archive
            .pdf_captures
            .iter()
            .position(|c| c.receipt.capture_id == capture_id)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "PDF capture not found",
            ));
        };
        let doc_id = self.archive.pdf_captures[pos].receipt.document_id;
        if !self
            .archive
            .pdf_documents
            .iter()
            .any(|d| d.receipt.document_id == doc_id && d.vault == vault)
        {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "PDF capture not found",
            ));
        }
        let op_id = self.archive.pdf_captures.remove(pos).receipt.operation_id;
        self.archive
            .pdf_operations
            .retain(|o| o.operation_id != op_id);
        self.archive.commits.retain(|c| c.operation_id != op_id);
        self.archive.schema_version = archive_schema_version(&self.archive);
        validate_archive(&self.archive)?;
        Ok(())
    }
    fn delete_pdf_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        let Some(pos) = self
            .archive
            .pdf_documents
            .iter()
            .position(|d| d.receipt.document_id == document_id && d.vault == vault)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "PDF document not found",
            ));
        };
        let doc = self.archive.pdf_documents.remove(pos);
        let mut ops = vec![doc.receipt.operation_id.clone()];
        self.archive.pdf_captures.retain(|c| {
            if c.receipt.document_id == document_id {
                ops.push(c.receipt.operation_id.clone());
                false
            } else {
                true
            }
        });
        self.archive
            .pdf_operations
            .retain(|o| !ops.contains(&o.operation_id));
        self.archive
            .commits
            .retain(|c| !ops.contains(&c.operation_id));
        if !self
            .archive
            .pdf_documents
            .iter()
            .any(|d| d.receipt.original_sha256 == doc.receipt.original_sha256)
            && !self
                .archive
                .text_entries
                .iter()
                .any(|t| t.record.receipt.snapshot_hash == doc.receipt.original_sha256)
        {
            self.archive
                .blobs
                .retain(|b| b.hash != doc.receipt.original_sha256);
        }
        self.archive.schema_version = archive_schema_version(&self.archive);
        validate_archive(&self.archive)?;
        Ok(())
    }
    fn update_epub_capture_note(&mut self, vault: &str, capture_id: i64, note: &str) -> Result<()> {
        validate_note_input(note)?;
        let Some(pos) = self
            .archive
            .epub_captures
            .iter()
            .position(|c| c.receipt.capture_id == capture_id)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB capture not found",
            ));
        };
        let doc_id = self.archive.epub_captures[pos].receipt.document_id;
        let Some(doc) = self
            .archive
            .epub_documents
            .iter()
            .find(|d| d.receipt.document_id == doc_id && d.vault == vault)
            .cloned()
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB capture not found",
            ));
        };
        self.archive.epub_captures[pos].user_note = note.to_owned();
        let new_hash = epub_capture_input(&self.archive.epub_captures[pos], &doc)?;
        let op_id = self.archive.epub_captures[pos].receipt.operation_id.clone();
        if let Some(commit) = self
            .archive
            .commits
            .iter_mut()
            .find(|c| c.operation_id == op_id)
        {
            commit.input_hash = new_hash;
        }
        validate_archive(&self.archive)?;
        Ok(())
    }
    fn delete_epub_capture(&mut self, vault: &str, capture_id: i64) -> Result<()> {
        let Some(pos) = self
            .archive
            .epub_captures
            .iter()
            .position(|c| c.receipt.capture_id == capture_id)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB capture not found",
            ));
        };
        let doc_id = self.archive.epub_captures[pos].receipt.document_id;
        if !self
            .archive
            .epub_documents
            .iter()
            .any(|d| d.receipt.document_id == doc_id && d.vault == vault)
        {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB capture not found",
            ));
        }
        let op_id = self.archive.epub_captures.remove(pos).receipt.operation_id;
        self.archive.commits.retain(|c| c.operation_id != op_id);
        self.archive.schema_version = archive_schema_version(&self.archive);
        validate_archive(&self.archive)?;
        Ok(())
    }
    fn delete_epub_document(&mut self, vault: &str, document_id: i64) -> Result<()> {
        let Some(pos) = self
            .archive
            .epub_documents
            .iter()
            .position(|d| d.receipt.document_id == document_id && d.vault == vault)
        else {
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::NotFound,
                "EPUB document not found",
            ));
        };
        let doc = self.archive.epub_documents.remove(pos);
        let mut ops = vec![doc.receipt.operation_id];
        self.archive.epub_captures.retain(|c| {
            if c.receipt.document_id == document_id {
                ops.push(c.receipt.operation_id.clone());
                false
            } else {
                true
            }
        });
        self.archive
            .commits
            .retain(|c| !ops.contains(&c.operation_id));
        self.archive.schema_version = archive_schema_version(&self.archive);
        validate_archive(&self.archive)?;
        Ok(())
    }
}
pub fn origin() -> Provenance {
    Provenance {
        package_id: "ordinary.reader".into(),
        publisher_id: "ordinary.author".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    }
}
pub fn pdf_fixture() -> (
    Vec<u8>,
    core_api::pdf_contract::PdfSnapshot,
    core_api::pdf_contract::PdfDraft,
) {
    use core_api::{contract::sha256, pdf_contract::*};
    let original =
        b"%PDF-1.7\nSynthetic immutable Core fixture, parser validated independently.\n".to_vec();
    let snapshot:PdfSnapshot=serde_json::from_value(serde_json::json!({"protocol_version":1,"original_sha256":sha256(&original),"original_bytes":original.len(),"extraction":{"provider":"pdfium","algorithm":"qusor-pdfium-char-map-v1","wrapper":"pdfium-render-0.9.5","abi":"pdfium_8125","library_sha256":"a".repeat(64),"mapping_check":"unicode_scalar_validity_only","normalization":PDF_NORMALIZATION},"engine_report_sha256":"b".repeat(64),"pages":[{"page_index":0,"engine_text":"hello","text_sha256":sha256(b"hello"),"rotation_degrees":0,"media_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"crop_box":{"left":0.0,"bottom":0.0,"right":100.0,"top":100.0},"characters":"hello".chars().enumerate().map(|(i,c)|serde_json::json!({"pdf_char_index":i,"unicode_value":c as u32,"mapping":"scalar","generated":false,"scalar_start":i,"scalar_end":i+1,"utf8_start":i,"utf8_end":i+1,"utf16_start":i,"utf16_end":i+1,"pdf_bounds":{"left":i as f32,"bottom":1.0,"right":i as f32+1.0,"top":2.0}})).collect::<Vec<_>>()}]})).unwrap();
    let draft = PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(b"hello"),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 5,
        exact: "hello".into(),
    };
    (original, snapshot, draft)
}
pub fn imported() -> (
    KnowledgeApplication<Memory>,
    core_api::pdf_contract::PdfDraft,
) {
    let (original, snapshot, draft) = pdf_fixture();
    let mut app = KnowledgeApplication::new(Memory::empty());
    app.import_document("import", "vault", "Source", &original, &snapshot)
        .unwrap();
    (app, draft)
}
