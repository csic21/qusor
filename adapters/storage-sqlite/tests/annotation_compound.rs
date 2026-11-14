#![cfg(unix)]
//! Real SQLite source/receipt qualification. Maps are synthetic, not parser evidence.
#[allow(dead_code)]
#[path = "../../../crates/learning-application/tests/common/mod.rs"]
mod common;
use core_api::{
    knowledge::*,
    pdf_compound::{PDF_COMPOUND_PROTOCOL_VERSION, PDF_SELECTION_PROFILE, PdfCompoundDraft},
};
use learning_application::{
    KnowledgeApplication,
    annotation_draft::{AnnotationDrafts, FrozenAnnotationSave},
    annotation_source::{FrozenAnnotationSource, FrozenPdfCompoundSource, FrozenPdfSource},
    draft_text::{DraftId, DraftState},
};
#[allow(deprecated)]
use learning_ports::{CommitPlan, KnowledgeStore};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-compound-annotation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn compound() -> PdfCompoundDraft {
    let (_, _, base) = common::pdf_fixture();
    let mut first = base.clone();
    first.start = 0;
    first.end = 2;
    first.exact = "he".into();
    let mut second = base.clone();
    second.start = 3;
    second.end = 5;
    second.exact = "lo".into();
    PdfCompoundDraft {
        protocol_version: PDF_COMPOUND_PROTOCOL_VERSION,
        original_sha256: base.original_sha256,
        extraction_revision: base.extraction_revision,
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments: vec![first, second],
        exact: "he\nlo".into(),
    }
}
fn imported(path: &std::path::Path) -> KnowledgeApplication<SqliteStore> {
    let mut app = KnowledgeApplication::new(SqliteStore::open(path).unwrap());
    let (original, snapshot, _) = common::pdf_fixture();
    app.import_document("import", "vault", "Synthetic source", &original, &snapshot)
        .unwrap();
    app
}
fn freeze(
    drafts: &mut AnnotationDrafts,
    id: &DraftId,
    note: &str,
    operation: &str,
) -> FrozenAnnotationSave {
    let owner = drafts.create_owner().unwrap();
    let upload = drafts.begin_replace(&owner, id, 1, note.len(), 1).unwrap();
    drafts.append_chunk(&owner, &upload, 0, note, 1).unwrap();
    let revision = drafts.commit_replace(&owner, &upload, 1).unwrap();
    drafts.revoke_owner(&owner).unwrap();
    drafts.freeze_save(id, revision, operation).unwrap()
}
#[test]
fn all_segments_commit_once_survive_reopen_restore_and_reject_same_first_different_payload() {
    let scratch = Scratch::new();
    let path = scratch.0.join("live");
    let mut app = imported(&path);
    let selection = compound();
    let mut drafts = AnnotationDrafts::new();
    let id = drafts
        .create_pdf_compound(&app, 1, "vault", selection.clone(), common::origin(), 9)
        .unwrap();
    let (source, generation) = drafts.source(&id).unwrap();
    assert_eq!(generation, 9);
    assert!(source.single().is_none());
    assert!(matches!(source, FrozenAnnotationSource::Compound(_)));
    assert_eq!(source.compound().unwrap().selection(), &selection);
    assert_eq!(source.exact(), "he\nlo");
    assert_eq!(source.original_sha256(), selection.original_sha256);
    assert_eq!(source.extraction_revision(), selection.extraction_revision);
    let note = "自己的想法😀\r\n\0";
    let save = freeze(&mut drafts, &id, note, "save-compound");
    let receipt = drafts.commit(&mut app, &save).unwrap();
    assert_eq!(drafts.state(&id).unwrap(), DraftState::Saved);
    assert_eq!(receipt.draft_hash, selection.draft_hash().unwrap());
    assert_eq!(drafts.commit(&mut app, &save).unwrap(), receipt);
    assert_eq!(
        drafts.reconcile(&app, &save).unwrap(),
        Some(receipt.clone())
    );
    assert_eq!(app.export_all().unwrap().commits.len(), 2);
    let record = app.query_pdf_complete(receipt.capture_id).unwrap();
    assert_eq!(record.compound.as_ref(), Some(&selection));
    assert_eq!(record.draft, selection.segments[0]);
    assert_eq!(record.user_note.as_bytes(), note.as_bytes());
    assert_eq!(record.provenance, common::origin());
    assert!(
        app.capture_pdf_compound(
            "save-compound",
            1,
            &selection,
            &common::origin(),
            "changed note"
        )
        .is_err()
    );
    let mut different = selection.clone();
    different.segments[1].end = 4;
    different.segments[1].exact = "l".into();
    different.exact = "he\nl".into();
    assert_eq!(different.segments[0], selection.segments[0]);
    let other =
        FrozenPdfCompoundSource::freeze(&app, 1, "vault", different, common::origin()).unwrap();
    assert!(other.receipt(&app, "save-compound", note).is_err());
    assert!(other.capture(&mut app, "save-compound", note).is_err());
    let single = FrozenPdfSource::freeze(
        &app,
        1,
        "vault",
        selection.segments[0].clone(),
        common::origin(),
    )
    .unwrap();
    assert!(single.receipt(&app, "save-compound", note).is_err());
    assert_eq!(app.export_all().unwrap().pdf_captures.len(), 1);
    drop(app);
    let app = KnowledgeApplication::new(SqliteStore::open(&path).unwrap());
    assert_eq!(app.query_pdf_complete(receipt.capture_id).unwrap(), record);
    let archive = app.export_all().unwrap();
    assert_eq!(archive.schema_version, 3);
    let mut restored =
        KnowledgeApplication::new(SqliteStore::open(scratch.0.join("restored")).unwrap());
    restored.restore_all(&archive).unwrap();
    assert_eq!(
        restored.query_pdf_complete(receipt.capture_id).unwrap(),
        record
    );
    assert_eq!(restored.original_pdf(1).unwrap(), common::pdf_fixture().0);
    assert_eq!(drafts.reconcile(&restored, &save).unwrap(), Some(receipt));
}
struct LostReply {
    inner: SqliteStore,
    lose: bool,
}
#[allow(deprecated)]
impl KnowledgeStore for LostReply {
    fn snapshot(&self) -> Result<ArchiveV2> {
        self.inner.snapshot()
    }
    fn commit(&mut self, plan: CommitPlan) -> Result<()> {
        self.inner.commit(plan)?;
        if self.lose {
            self.lose = false;
            return Err(KnowledgeError::new(
                KnowledgeErrorCode::Storage,
                "injected reply loss after real SQLite commit",
            ));
        }
        Ok(())
    }
    fn restore(&mut self, archive: &ArchiveV2) -> Result<()> {
        self.inner.restore(archive)
    }
}
#[test]
fn compound_unknown_reply_reconciles_exact_sqlite_payload_without_recommit() {
    let scratch = Scratch::new();
    let path = scratch.0.join("live");
    drop(imported(&path));
    let mut app = KnowledgeApplication::new(LostReply {
        inner: SqliteStore::open(&path).unwrap(),
        lose: true,
    });
    let mut drafts = AnnotationDrafts::new();
    let id = drafts
        .create_pdf_compound(&app, 1, "vault", compound(), common::origin(), 3)
        .unwrap();
    let save = freeze(&mut drafts, &id, "pending想法", "lost-compound");
    assert!(drafts.reconcile(&app, &save).unwrap().is_none());
    assert!(drafts.commit(&mut app, &save).is_err());
    assert_eq!(drafts.state(&id).unwrap(), DraftState::Saving);
    drop(app);
    // Registry remains alive; reopening SQLite does not resurrect a draft session.
    let reopened = KnowledgeApplication::new(SqliteStore::open(&path).unwrap());
    let receipt = drafts.reconcile(&reopened, &save).unwrap().unwrap();
    assert_eq!(drafts.state(&id).unwrap(), DraftState::Saved);
    assert_eq!(
        reopened
            .query_pdf_complete(receipt.capture_id)
            .unwrap()
            .compound,
        Some(compound())
    );
    assert_eq!(reopened.export_all().unwrap().commits.len(), 2);
}
#[test]
fn invalid_later_segment_vault_generation_and_receipt_kind_leave_no_partial_source() {
    let scratch = Scratch::new();
    let app = imported(&scratch.0.join("live"));
    let mut drafts = AnnotationDrafts::new();
    let selection = compound();
    assert!(
        drafts
            .create_pdf_compound(&app, 1, "wrong", selection.clone(), common::origin(), 1)
            .is_err()
    );
    assert!(
        drafts
            .create_pdf_compound(&app, 1, "vault", selection.clone(), common::origin(), 0)
            .is_err()
    );
    let mut bad = selection.clone();
    bad.segments[1].exact = "xx".into();
    bad.exact = "he\nxx".into();
    assert!(
        drafts
            .create_pdf_compound(&app, 1, "vault", bad, common::origin(), 1)
            .is_err()
    );
    assert_eq!(drafts.source_bytes(), 0);
    assert_eq!(drafts.reserved_text_bytes(), 0);
    let source =
        FrozenPdfCompoundSource::freeze(&app, 1, "vault", selection.clone(), common::origin())
            .unwrap();
    assert!(source.receipt(&app, "import", "").is_err());
    for i in 0..8 {
        if i % 2 == 0 {
            drafts
                .create_pdf_compound(&app, 1, "vault", selection.clone(), common::origin(), 1)
                .unwrap();
        } else {
            drafts
                .create_pdf(
                    &app,
                    1,
                    "vault",
                    selection.segments[0].clone(),
                    common::origin(),
                    1,
                )
                .unwrap();
        }
    }
    assert!(
        drafts
            .create_pdf_compound(&app, 1, "vault", selection, common::origin(), 1)
            .is_err()
    );
    assert!(app.export_all().unwrap().pdf_captures.is_empty());
}
