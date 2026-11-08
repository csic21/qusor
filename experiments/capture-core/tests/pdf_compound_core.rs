use qusor_capture_probe::{
    contract::sha256,
    core::{Core, Provenance},
    pdf_compound::*,
    pdf_contract::*,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-compound-core-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn original() -> Vec<u8> {
    b"%PDF-1.7\nSynthetic persistence unit fixture; native PDF is separately parsed.\n".to_vec()
}
fn snapshot() -> PdfSnapshot {
    let bytes = original();
    let bounds = PdfRect {
        left: 0.,
        bottom: 0.,
        right: 100.,
        top: 100.,
    };
    PdfSnapshot {
        protocol_version: 1,
        original_sha256: sha256(&bytes),
        original_bytes: bytes.len() as u64,
        extraction: ExtractionIdentity {
            provider: "pdfium".into(),
            algorithm: "qusor-pdfium-char-map-v1".into(),
            wrapper: "pdfium-render-0.9.5".into(),
            abi: "pdfium_8125".into(),
            library_sha256: "a".repeat(64),
            mapping_check: "unicode_scalar_validity_only".into(),
            normalization: PDF_NORMALIZATION.into(),
        },
        engine_report_sha256: "b".repeat(64),
        pages: ["lower upper", "third"]
            .iter()
            .enumerate()
            .map(|(index, text)| PdfPage {
                page_index: index as u32,
                engine_text: (*text).into(),
                text_sha256: sha256(text.as_bytes()),
                rotation_degrees: if index == 0 { 90 } else { 0 },
                media_box: bounds.clone(),
                crop_box: bounds.clone(),
                characters: text
                    .chars()
                    .enumerate()
                    .map(|(i, c)| PdfCharacter {
                        pdf_char_index: i as u64,
                        unicode_value: c as u32,
                        mapping: "scalar".into(),
                        generated: false,
                        scalar_start: i as u64,
                        scalar_end: i as u64 + 1,
                        utf8_start: i as u64,
                        utf8_end: i as u64 + 1,
                        utf16_start: i as u64,
                        utf16_end: i as u64 + 1,
                        pdf_bounds: if c.is_whitespace() {
                            None
                        } else {
                            Some(PdfRect {
                                left: i as f32 * 2.,
                                bottom: 1.,
                                right: i as f32 * 2. + 1.,
                                top: 2.,
                            })
                        },
                    })
                    .collect(),
            })
            .collect(),
    }
}
fn segment(page: u32, start: u64, end: u64, exact: &str) -> PdfDraft {
    let s = snapshot();
    PdfDraft {
        protocol_version: 1,
        original_sha256: s.original_sha256.clone(),
        extraction_revision: s.extraction_revision().unwrap(),
        page_index: page,
        page_text_sha256: s.pages[page as usize].text_sha256.clone(),
        unit: PDF_TEXT_UNIT.into(),
        start,
        end,
        exact: exact.into(),
    }
}
fn compound() -> PdfCompoundDraft {
    let s = snapshot();
    PdfCompoundDraft {
        protocol_version: 2,
        original_sha256: s.original_sha256.clone(),
        extraction_revision: s.extraction_revision().unwrap(),
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments: vec![
            segment(0, 6, 11, "upper"),
            segment(0, 0, 5, "lower"),
            segment(1, 0, 5, "third"),
        ],
        exact: "upper\nlower\nthird".into(),
    }
}
fn origin() -> Provenance {
    Provenance {
        package_id: "org.test.compound-reader".into(),
        publisher_id: "org.test".into(),
        package_digest: "c".repeat(64),
        installation_id: 1,
    }
}
fn imported(path: &Path) -> Core {
    let mut core = Core::open(path).unwrap();
    core.import_document("import", "vault", "PDF", &original(), &snapshot(), None)
        .unwrap();
    core
}

#[test]
fn compound_is_one_intent_receipt_revision_and_index_job_with_legacy_resolution_guard() {
    let tmp = Scratch::new();
    let path = tmp.join("core");
    let mut core = imported(&path);
    let draft = compound();
    let receipt = core
        .capture_pdf_compound("collect", 1, &draft, &origin(), "one user note", None)
        .unwrap();
    assert_eq!(receipt.capture_id, 1);
    assert_eq!(receipt.commit_rev, 2);
    assert_eq!(receipt.draft_hash, draft.draft_hash().unwrap());
    assert_eq!(core.pdf_receipt("collect").unwrap(), Some(receipt.clone()));
    assert_eq!(
        core.capture_pdf_compound("collect", 1, &draft, &origin(), "one user note", None)
            .unwrap(),
        receipt
    );
    assert!(
        core.capture_pdf_compound("collect", 1, &draft, &origin(), "changed note", None)
            .is_err()
    );
    assert!(
        core.query_pdf(1)
            .unwrap_err()
            .to_string()
            .contains("complete compound query")
    );
    let record = core.query_pdf_complete(1).unwrap();
    assert_eq!(record.compound, Some(draft.clone()));
    assert_eq!(record.draft, draft.segments[0]);
    assert!(
        core.resolve_pdf(1, &draft.segments[0])
            .unwrap_err()
            .to_string()
            .contains("complete compound")
    );
    let selection = core.resolve_pdf_compound(1, &draft).unwrap();
    assert_eq!(selection.segments.len(), 3);
    assert_eq!(selection.exact, "upper\nlower\nthird");
    let mut changed = draft.clone();
    changed.segments[1] = segment(0, 1, 5, "ower");
    changed.exact = "upper\nower\nthird".into();
    assert!(changed.resolve(&snapshot()).is_ok());
    assert!(
        core.capture_pdf_compound("collect", 1, &changed, &origin(), "one user note", None)
            .is_err()
    );
    assert!(core.resolve_pdf_compound(1, &changed).is_err());
    let mut changed = draft.clone();
    changed.segments.swap(0, 1);
    changed.exact = "lower\nupper\nthird".into();
    assert!(changed.resolve(&snapshot()).is_ok());
    assert!(
        core.capture_pdf_compound("collect", 1, &changed, &origin(), "one user note", None)
            .is_err()
    );
    let archive = core.export_all().unwrap();
    assert_eq!(archive.schema_version, 3);
    assert_eq!(archive.pdf_captures.len(), 1);
    assert_eq!(archive.pdf_operations.len(), 2);
    assert_eq!(archive.commits.len(), 2);
    assert_eq!(
        archive
            .pdf_operations
            .iter()
            .filter(|entry| entry.kind == "pdf_capture")
            .count(),
        1
    );
    let next = core
        .capture_pdf(
            "single-after",
            1,
            &draft.segments[0],
            &origin(),
            "single",
            None,
        )
        .unwrap();
    assert_eq!(next.capture_id, 2);
    assert_eq!(next.commit_rev, 3);
    assert_eq!(
        core.resolve_pdf(next.capture_id, &draft.segments[0])
            .unwrap()
            .exact,
        "upper"
    );
    assert!(core.resolve_pdf_compound(next.capture_id, &draft).is_err());
    drop(core);
    let reopened = Core::open(&path).unwrap();
    assert_eq!(
        reopened
            .resolve_pdf_compound(1, &draft)
            .unwrap()
            .segments
            .len(),
        3
    );
    assert_eq!(reopened.pdf_receipt("collect").unwrap(), Some(receipt));
}

#[test]
fn archive_version_and_all_segment_relationships_are_strict_and_round_trip() {
    let tmp = Scratch::new();
    let mut core = imported(&tmp.join("source"));
    let text: qusor_capture_probe::contract::CaptureDraft = serde_json::from_value(serde_json::json!({"protocol_version":1,"source":{"namespace":"test","reference":"text-source","title":"Text source","public_uri":null,"external_return":"none"},"snapshot_text":"legacy text","selector":{"unit":"unicode_scalar","normalization":"qusor-raw-text-v1","start":0,"end":6,"exact":"legacy"}})).unwrap();
    core.commit("text", "vault", &text, &origin(), "text note", None)
        .unwrap();
    let legacy = core
        .capture_pdf(
            "legacy",
            1,
            &compound().segments[0],
            &origin(),
            "old note",
            None,
        )
        .unwrap();
    let archive2 = core.export_all().unwrap();
    assert_eq!(archive2.schema_version, 2);
    let legacy_json = serde_json::to_vec(&archive2.pdf_captures[0]).unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&legacy_json)
            .unwrap()
            .get("compound")
            .is_none()
    );
    let record = &archive2.pdf_captures[0];
    let historical_json = format!(
        "{{\"receipt\":{},\"draft\":{},\"provenance\":{},\"user_note\":\"old note\"}}",
        serde_json::to_string(&record.receipt).unwrap(),
        serde_json::to_string(&record.draft).unwrap(),
        serde_json::to_string(&record.provenance).unwrap()
    );
    assert_eq!(legacy_json, historical_json.as_bytes());
    let historical_input = sha256(
        &serde_json::to_vec(&(
            "pdf_capture",
            1i64,
            "vault",
            &compound().segments[0],
            &origin(),
            "old note",
        ))
        .unwrap(),
    );
    assert_eq!(
        archive2
            .pdf_operations
            .iter()
            .find(|op| op.operation_id == "legacy")
            .unwrap()
            .input_hash,
        historical_input
    );
    core.capture_pdf_compound("compound", 1, &compound(), &origin(), "new note", None)
        .unwrap();
    let archive3 = core.export_all().unwrap();
    assert_eq!(archive3.schema_version, 3);
    assert_eq!(
        serde_json::to_vec(&archive3.pdf_captures[0]).unwrap(),
        legacy_json
    );
    let mut restored = Core::open(tmp.join("restored")).unwrap();
    for case in [
        "version",
        "second-segment",
        "order",
        "projection",
        "drop-compound",
        "audit",
    ] {
        let mut bad = archive3.clone();
        match case {
            "version" => bad.schema_version = 2,
            "second-segment" => {
                let c = bad.pdf_captures[1].compound.as_mut().unwrap();
                c.segments[1] = segment(0, 1, 5, "ower");
                c.exact = "upper\nower\nthird".into();
            }
            "order" => {
                let c = bad.pdf_captures[1].compound.as_mut().unwrap();
                c.segments.swap(0, 1);
                c.exact = "lower\nupper\nthird".into();
            }
            "projection" => bad.pdf_captures[1].draft = segment(0, 0, 5, "lower"),
            "drop-compound" => {
                bad.pdf_captures[1].compound = None;
            }
            _ => bad.pdf_operations.last_mut().unwrap().index_job = "partial first segment".into(),
        };
        assert!(restored.restore_all(&bad, None).is_err(), "{case}");
        assert!(restored.export_all().unwrap().commits.is_empty());
    }
    restored.restore_all(&archive3, None).unwrap();
    assert_eq!(restored.export_all().unwrap(), archive3);
    assert_eq!(restored.pdf_receipt("legacy").unwrap(), Some(legacy));
    assert_eq!(
        restored.resolve_pdf_compound(2, &compound()).unwrap().exact,
        compound().exact
    );
    let mut legacy_target = Core::open(tmp.join("legacy-target")).unwrap();
    legacy_target.restore_all(&archive2, None).unwrap();
    assert_eq!(legacy_target.export_all().unwrap(), archive2);
}

#[test]
fn schema_two_metadata_migration_preserves_record_receipt_input_and_export_bytes() {
    let tmp = Scratch::new();
    let path = tmp.join("schema-two");
    let archive;
    {
        let mut core = imported(&path);
        core.capture_pdf(
            "single",
            1,
            &compound().segments[0],
            &origin(),
            "note",
            None,
        )
        .unwrap();
        archive = core.export_all().unwrap();
    }
    let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
    db.execute_batch("PRAGMA user_version=2").unwrap();
    drop(db);
    let core = Core::open(&path).unwrap();
    assert_eq!(core.export_all().unwrap(), archive);
    drop(core);
    let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
}

#[test]
fn process_compound_crash_child() {
    if let Ok(path) = std::env::var("QUSOR_COMPOUND_CORE_PATH") {
        let stage = std::env::var("QUSOR_COMPOUND_CORE_STAGE").unwrap();
        let point = std::env::var("QUSOR_COMPOUND_CORE_POINT").unwrap();
        if stage == "migration" {
            Core::open_migration_probe(&path, Some(&point)).unwrap();
            panic!("migration did not crash")
        };
        let mut core = Core::open(&path).unwrap();
        match stage.as_str() {
            "capture" => {
                core.capture_pdf_compound(
                    "collect",
                    1,
                    &compound(),
                    &origin(),
                    "note",
                    Some(&point),
                )
                .unwrap();
            }
            "restore" => {
                let archive =
                    fs::read(std::env::var("QUSOR_COMPOUND_CORE_ARCHIVE").unwrap()).unwrap();
                core.restore_all_json(&archive, Some(&point)).unwrap();
            }
            _ => panic!("unknown stage"),
        };
        panic!("did not crash");
    }
}
fn crash_child(path: &Path, stage: &str, point: &str, archive: Option<&Path>) {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "process_compound_crash_child", "--nocapture"])
        .env("QUSOR_COMPOUND_CORE_PATH", path)
        .env("QUSOR_COMPOUND_CORE_STAGE", stage)
        .env("QUSOR_COMPOUND_CORE_POINT", point);
    if let Some(archive) = archive {
        command.env("QUSOR_COMPOUND_CORE_ARCHIVE", archive);
    }
    assert_eq!(
        command.status().unwrap().code(),
        Some(86),
        "{stage}/{point}"
    );
}

#[test]
fn real_capture_and_restore_crashes_never_publish_a_partial_segment_set() {
    let tmp = Scratch::new();
    for point in [
        "before_transaction",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let path = tmp.join(&format!("capture-{point}"));
        drop(imported(&path));
        crash_child(&path, "capture", point, None);
        let mut core = Core::open(&path).unwrap();
        let archive = core.export_all().unwrap();
        assert_eq!(
            archive.pdf_captures.len(),
            usize::from(point == "after_commit")
        );
        assert_eq!(
            archive.commits.len(),
            1 + usize::from(point == "after_commit")
        );
        let receipt = core
            .capture_pdf_compound("collect", 1, &compound(), &origin(), "note", None)
            .unwrap();
        assert_eq!(receipt.capture_id, 1);
        assert_eq!(receipt.commit_rev, 2);
        assert_eq!(
            core.resolve_pdf_compound(1, &compound())
                .unwrap()
                .segments
                .len(),
            3
        );
    }
    let mut source = imported(&tmp.join("source"));
    source
        .capture_pdf_compound("collect", 1, &compound(), &origin(), "note", None)
        .unwrap();
    let archive = source.export_all().unwrap();
    let input = tmp.join("archive.json");
    fs::write(&input, serde_json::to_vec(&archive).unwrap()).unwrap();
    for point in [
        "after_blob_publish",
        "after_entities",
        "before_commit",
        "after_commit",
    ] {
        let path = tmp.join(&format!("restore-{point}"));
        crash_child(&path, "restore", point, Some(&input));
        let mut core = Core::open(&path).unwrap();
        assert_eq!(
            core.export_all().unwrap().commits.len(),
            if point == "after_commit" { 2 } else { 0 }
        );
        if point != "after_commit" {
            core.restore_all(&archive, None).unwrap();
        }
        assert_eq!(core.export_all().unwrap(), archive);
    }
}

#[test]
fn real_schema_two_migration_crashes_are_atomic_and_do_not_rewrite_legacy_data() {
    let tmp = Scratch::new();
    for point in ["before_migration_commit", "after_migration_commit"] {
        let path = tmp.join(point);
        let archive;
        {
            let mut core = imported(&path);
            core.capture_pdf("old", 1, &compound().segments[0], &origin(), "note", None)
                .unwrap();
            archive = core.export_all().unwrap();
        }
        let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
        db.execute_batch("PRAGMA user_version=2").unwrap();
        drop(db);
        crash_child(&path, "migration", point, None);
        let db = rusqlite::Connection::open(path.join("core.sqlite")).unwrap();
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            version,
            if point == "after_migration_commit" {
                3
            } else {
                2
            }
        );
        drop(db);
        let core = Core::open(&path).unwrap();
        assert_eq!(core.export_all().unwrap(), archive);
    }
}
