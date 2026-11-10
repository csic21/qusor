#![cfg(unix)]

use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use learning_application::KnowledgeApplication;
use qusor_capture_probe::notes_bridge::NotesReadBridge;
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
            "qusor-notes-read-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn original() -> Vec<u8> {
    b"%PDF-1.7\nSynthetic immutable Core fixture, parser validated independently.\n".to_vec()
}

fn snapshot() -> PdfSnapshot {
    let original = original();
    serde_json::from_value(serde_json::json!({
        "protocol_version": 1,
        "original_sha256": sha256(&original),
        "original_bytes": original.len(),
        "extraction": {
            "provider": "pdfium",
            "algorithm": "qusor-pdfium-char-map-v1",
            "wrapper": "pdfium-render-0.9.5",
            "abi": "pdfium_8125",
            "library_sha256": "a".repeat(64),
            "mapping_check": "unicode_scalar_validity_only",
            "normalization": PDF_NORMALIZATION
        },
        "engine_report_sha256": "b".repeat(64),
        "pages": [{
            "page_index": 0,
            "engine_text": "hello",
            "text_sha256": sha256(b"hello"),
            "rotation_degrees": 0,
            "media_box": {"left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0},
            "crop_box": {"left": 0.0, "bottom": 0.0, "right": 100.0, "top": 100.0},
            "characters": "hello".chars().enumerate().map(|(i, c)| serde_json::json!({
                "pdf_char_index": i,
                "unicode_value": c as u32,
                "mapping": "scalar",
                "generated": false,
                "scalar_start": i,
                "scalar_end": i + 1,
                "utf8_start": i,
                "utf8_end": i + 1,
                "utf16_start": i,
                "utf16_end": i + 1,
                "pdf_bounds": {"left": i as f32, "bottom": 1.0, "right": i as f32 + 1.0, "top": 2.0}
            })).collect::<Vec<_>>()
        }]
    }))
    .unwrap()
}

fn draft(snapshot: &PdfSnapshot) -> PdfDraft {
    PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(b"hello"),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 5,
        exact: "hello".into(),
    }
}

fn origin() -> Provenance {
    Provenance {
        package_id: "ordinary.notes".into(),
        publisher_id: "ordinary.author".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    }
}

fn manifest() -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.test.notes".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 4,
            max_minor: 4,
        },
        required_capabilities: vec![Capability::NotesRead],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.test.notes.workspace".into(),
            title: "Notes".into(),
            kind: ContributionKind::NotesWorkspace,
            required_capabilities: vec![Capability::NotesRead],
        }],
    }
}

fn page_request(cursor: Option<&str>, limit: u16) -> NotesRequest {
    NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_4,
        request_id: "page-request".into(),
        operation: NotesOperation::Page {
            cursor: cursor.map(str::to_owned),
            limit,
        },
    }
}

fn body_request(capture_id: i64, offset: u32, max_bytes: u16) -> NotesRequest {
    NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_4,
        request_id: "body-request".into(),
        operation: NotesOperation::Body {
            capture_id,
            field: NoteField::Note,
            offset,
            max_bytes,
        },
    }
}

fn read(
    host: &mut Host,
    session: &Session,
    bridge: &NotesReadBridge<SqliteStore>,
    request: &NotesRequest,
) -> NotesResponse {
    let permit = host.authorize_notes(session, request).unwrap();
    host.dispatch(permit, |context| {
        assert_eq!(context.request.capability, Capability::NotesRead);
        Ok(bridge.respond(request))
    })
    .unwrap()
}

fn setup(
    notes: &[&str],
) -> (
    Scratch,
    Host,
    Session,
    NotesReadBridge<SqliteStore>,
    Vec<i64>,
) {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(SqliteStore::open(&scratch.0).unwrap());
    let original = original();
    let snapshot = snapshot();
    let selection = draft(&snapshot);
    let imported = app
        .import_document("import", "vault", "Source", &original, &snapshot)
        .unwrap();
    let mut ids = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        ids.push(
            app.capture_pdf(
                &format!("capture-{index}"),
                imported.document_id,
                &selection,
                &origin(),
                note,
            )
            .unwrap()
            .capture_id,
        );
    }
    let mut host = Host::new("vault", [Capability::NotesRead]).unwrap();
    let id = host.register(manifest(), "a".repeat(64)).unwrap();
    host.replace_grants(
        id,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        }],
    )
    .unwrap();
    host.enable(id).unwrap();
    let session = host.open_session_for_api(id, HOST_API_V0_4).unwrap();
    (
        scratch,
        host,
        session,
        NotesReadBridge::new(app, "vault"),
        ids,
    )
}

#[test]
fn pages_and_body_chunks_read_real_core_notes() {
    let long = "想法😀\u{0}".repeat(30);
    let (_scratch, mut host, session, bridge, ids) =
        setup(&["first thought", &long, "third thought"]);
    assert_eq!(bridge.vault(), "vault");

    let first = read(&mut host, &session, &bridge, &page_request(None, 2));
    let NotesResult::Page { notes, next_cursor } = &first.result else {
        panic!("expected a page: {first:?}");
    };
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].capture_id, ids[2]);
    assert_eq!(notes[1].capture_id, ids[1]);
    assert!(next_cursor.is_some());
    assert!(
        notes
            .iter()
            .all(|note| note.note_preview.len() <= MAX_NOTE_PREVIEW_BYTES)
    );
    assert!(notes[1].note_truncated);
    assert!(notes[1].note_bytes > notes[1].note_preview.len() as u64);
    let encoded = first.to_json().unwrap();
    assert!(encoded.len() <= MAX_REQUEST_BYTES);
    assert_eq!(NotesResponse::from_json(&encoded).unwrap(), first);

    let second = read(
        &mut host,
        &session,
        &bridge,
        &page_request(next_cursor.as_deref(), 2),
    );
    let NotesResult::Page { notes, next_cursor } = &second.result else {
        panic!("expected a page: {second:?}");
    };
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].capture_id, ids[0]);
    assert!(next_cursor.is_none());

    let mut offset = 0;
    let mut rebuilt = String::new();
    loop {
        let response = read(
            &mut host,
            &session,
            &bridge,
            &body_request(ids[1], offset, MAX_NOTE_CHUNK_BYTES),
        );
        let NotesResult::Body(body) = &response.result else {
            panic!("expected a body chunk: {response:?}");
        };
        assert!(response.to_json().unwrap().len() <= MAX_REQUEST_BYTES);
        rebuilt.push_str(&body.text);
        match body.next_offset {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(rebuilt, long);
}

#[test]
fn control_characters_shrink_the_body_chunk_to_fit() {
    let note = "\u{0}".repeat(1000);
    let (_scratch, mut host, session, bridge, ids) = setup(&[&note]);
    let response = read(
        &mut host,
        &session,
        &bridge,
        &body_request(ids[0], 0, MAX_NOTE_CHUNK_BYTES),
    );
    let NotesResult::Body(body) = &response.result else {
        panic!("expected a shrunk body chunk: {response:?}");
    };
    assert!(!body.text.is_empty());
    assert!(body.text.len() < note.len());
    assert!(body.text.len() <= usize::from(MAX_NOTE_CHUNK_BYTES));
    assert!(body.next_offset.is_some());
    assert!(response.to_json().unwrap().len() <= MAX_REQUEST_BYTES);
    assert!(NotesResponse::from_json(&response.to_json().unwrap()).is_ok());
}

#[test]
fn read_errors_are_explicit_wire_codes() {
    let (_scratch, mut host, session, bridge, _) = setup(&["one"]);
    let missing = read(&mut host, &session, &bridge, &body_request(999, 0, 8));
    assert_eq!(
        missing.result,
        NotesResult::Error {
            error: ApiError::new(ErrorCode::NotFound, "note read failed")
        }
    );
    let bad_cursor = read(
        &mut host,
        &session,
        &bridge,
        &page_request(Some("n1:*:999:999"), 2),
    );
    assert_eq!(
        bad_cursor.result,
        NotesResult::Error {
            error: ApiError::new(ErrorCode::InvalidRequest, "note read failed")
        }
    );
}

#[test]
fn a_bridge_bound_to_another_vault_returns_not_found() {
    let (_scratch, mut host, session, _bridge, ids) = setup(&["one"]);
    let other_scratch = Scratch::new();
    let other = NotesReadBridge::new(
        KnowledgeApplication::new(SqliteStore::open(&other_scratch.0).unwrap()),
        "other-vault",
    );
    let response = read(&mut host, &session, &other, &page_request(None, 2));
    assert!(matches!(response.result, NotesResult::Page { ref notes, .. } if notes.is_empty()));
    let body = read(&mut host, &session, &other, &body_request(ids[0], 0, 8));
    assert!(matches!(
        body.result,
        NotesResult::Error {
            error: ApiError {
                code: ErrorCode::NotFound,
                ..
            }
        }
    ));
}
