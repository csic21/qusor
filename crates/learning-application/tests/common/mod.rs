//! Shared synthetic Core fixture and explicitly injected lost-reply store.
use core_api::knowledge::*;
use learning_application::KnowledgeApplication;
use learning_ports::{CommitPlan, KnowledgeStore};
pub struct Memory {
    archive: ArchiveV2,
    pub lose_reply: bool,
}
impl Memory {
    fn new() -> Self {
        Self {
            archive: ArchiveV2 {
                schema_version: 2,
                text_entries: vec![],
                pdf_documents: vec![],
                pdf_captures: vec![],
                pdf_operations: vec![],
                commits: vec![],
                blobs: vec![],
            },
            lose_reply: false,
        }
    }
}
impl KnowledgeStore for Memory {
    fn snapshot(&self) -> Result<ArchiveV2> {
        Ok(self.archive.clone())
    }
    fn commit(&mut self, plan: CommitPlan) -> Result<()> {
        assert_eq!(plan.expected_revision, self.archive.commits.len() as i64);
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
    let mut app = KnowledgeApplication::new(Memory::new());
    app.import_document("import", "vault", "Source", &original, &snapshot)
        .unwrap();
    (app, draft)
}
