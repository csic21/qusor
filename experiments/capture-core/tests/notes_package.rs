//! End-to-end: an ordinary signed notes package from `plugins/notes` is built
//! outside the repository, verified, installed into the Host, granted
//! `notes.read`, executed by the Wasm runtime, and answered by real Core reads.
#![cfg(unix)]

use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::{notes::*, *};
use extension_host::{Grant, Host};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use learning_application::KnowledgeApplication;
use qusor_capture_probe::notes_bridge::NotesReadBridge;
use runtime_wasm::{RuntimeError, run_managed_raw};
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const MANIFEST: &str = include_str!("../../../plugins/notes/manifest.json");
const GUEST: &str = include_str!("../../../plugins/notes/guest.wat");
const SEED: [u8; 32] = [0x33; 32];

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-notes-package-{}-{}",
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

fn setup_core(notes: &[&str]) -> (Scratch, KnowledgeApplication<SqliteStore>, Vec<i64>) {
    let scratch = Scratch::new();
    let mut app = KnowledgeApplication::new(SqliteStore::open(&scratch.0).unwrap());
    let original = original();
    let snapshot = snapshot();
    let selection = draft(&snapshot);
    let imported = app
        .import_document("import", "vault", "Source", &original, &snapshot)
        .unwrap();
    let origin = Provenance {
        package_id: "org.qusor.notes".into(),
        publisher_id: "org.qusor".into(),
        package_digest: "a".repeat(64),
        installation_id: 1,
    };
    let mut ids = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        ids.push(
            app.capture_pdf(
                &format!("capture-{index}"),
                imported.document_id,
                &selection,
                &origin,
                note,
            )
            .unwrap()
            .capture_id,
        );
    }
    (scratch, app, ids)
}

fn signed_package() -> (VerifiedPackage, Vec<u8>) {
    let manifest = Manifest::from_slice(MANIFEST.as_bytes()).unwrap();
    let wasm = wat::parse_str(GUEST).unwrap();
    let bytes = build_signed_for_host(
        manifest,
        "notes.wasm",
        vec![PackageFile {
            path: "notes.wasm".into(),
            mime: "application/wasm".into(),
            bytes: wasm,
        }],
        &SEED,
        &SUPPORTED_HOST_APIS,
    )
    .unwrap();
    let verified = verify_for_host(&bytes, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).unwrap();
    (verified, bytes)
}

#[test]
fn an_ordinary_signed_notes_package_reads_real_core_through_the_host() {
    let (_scratch, app, ids) = setup_core(&["first thought", "想法😀 from the guest"]);
    let (verified, package_bytes) = signed_package();

    assert_eq!(verified.manifest().package_id, "org.qusor.notes");
    assert_eq!(
        verified.manifest().required_capabilities,
        vec![Capability::NotesRead]
    );
    assert_eq!(
        verified.manifest().contributions[0].kind,
        ContributionKind::NotesWorkspace
    );
    assert_eq!(verified.digest(), sha256(&package_bytes));

    let mut host = Host::new("vault", [Capability::NotesRead]).unwrap();
    let installation = host
        .register(verified.manifest().clone(), verified.digest())
        .unwrap();
    host.replace_grants(
        installation,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: Resource::Vault {
                vault_id: "vault".into(),
            },
        }],
    )
    .unwrap();
    host.enable(installation).unwrap();
    assert!(host.contributions().iter().any(|(_, contribution)| {
        contribution.kind == ContributionKind::NotesWorkspace
            && contribution.id == "org.qusor.notes.workspace"
    }));
    let session = host
        .open_session_for_api(installation, HOST_API_V0_4)
        .unwrap();
    let bridge = NotesReadBridge::new(app, "vault");

    let seen = RefCell::new(Vec::new());
    let result = run_managed_raw(verified.module_bytes(), |bytes| {
        let request =
            NotesRequest::from_slice(bytes).map_err(|_| RuntimeError::new("invalid_request"))?;
        seen.borrow_mut().push(request.operation.clone());
        let permit = host
            .authorize_notes(&session, &request)
            .map_err(|_| RuntimeError::new("permission_denied"))?;
        let response = host
            .dispatch(permit, |context| {
                assert_eq!(context.vault_id, "vault");
                assert_eq!(context.request.capability, Capability::NotesRead);
                Ok(bridge.respond(&request))
            })
            .map_err(|_| RuntimeError::new("dispatch_failed"))?;
        response
            .to_json()
            .map(String::into_bytes)
            .map_err(|_| RuntimeError::new("invalid_response"))
    })
    .unwrap();

    assert_eq!(result.calls, 1);
    assert!(result.fuel_consumed > 0);
    assert_eq!(
        seen.borrow().as_slice(),
        &[NotesOperation::Page {
            cursor: None,
            limit: MAX_NOTE_PAGE
        }]
    );
    let response = NotesResponse::from_json(&result.output).unwrap();
    let NotesResult::Page { notes, next_cursor } = &response.result else {
        panic!("expected a real Core page from the ordinary package: {response:?}");
    };
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].capture_id, ids[1]);
    assert_eq!(notes[1].capture_id, ids[0]);
    assert!(notes[0].note_preview.contains("想法😀"));
    assert!(
        notes
            .iter()
            .all(|note| note.note_preview.len() <= MAX_NOTE_PREVIEW_BYTES)
    );
    assert!(next_cursor.is_none());
}

#[test]
fn another_key_or_tampered_package_bytes_are_rejected() {
    let (_, package_bytes) = signed_package();
    assert!(
        verify_for_host(
            &package_bytes,
            &publisher_key(&[0x44; 32]),
            &SUPPORTED_HOST_APIS
        )
        .is_err()
    );
    let mut tampered = package_bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(verify_for_host(&tampered, &publisher_key(&SEED), &SUPPORTED_HOST_APIS).is_err());
}
