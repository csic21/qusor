//! Persistent install lifecycle for the ordinary notes package: install into the
//! verified store, reopen the store, run the frozen module against real Core,
//! disable the Host, then tombstone the installation.
#![cfg(unix)]

use core_api::{
    contract::sha256,
    knowledge::Provenance,
    pdf_contract::{PDF_NORMALIZATION, PDF_TEXT_UNIT, PdfDraft, PdfSnapshot},
};
use extension_api::{notes::*, *};
use extension_host::{Grant, Host, Session};
use extension_package::package::{
    PackageFile, VerifiedPackage, build_signed_for_host, publisher_key, verify_for_host,
};
use extension_store::Store;
use learning_application::KnowledgeApplication;
use qusor_capture_probe::notes_bridge::NotesReadBridge;
use runtime_wasm::{RuntimeError, run_managed_raw};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage_sqlite::SqliteStore;

const MANIFEST: &str = include_str!("../../../plugins/notes/manifest.legacy.json");
const GUEST: &str = include_str!("../../../plugins/notes/guest.wat");
const SEED: [u8; 32] = [0x33; 32];

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "qusor-notes-install-{}-{}",
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

fn read_through_guest(
    host: &mut Host,
    session: &Session,
    bridge: &NotesReadBridge<SqliteStore>,
    module: &[u8],
) -> NotesResponse {
    let result = run_managed_raw(module, |bytes| {
        let request =
            NotesRequest::from_slice(bytes).map_err(|_| RuntimeError::new("invalid_request"))?;
        let permit = host
            .authorize_notes(session, &request)
            .map_err(|_| RuntimeError::new("permission_denied"))?;
        let response = host
            .dispatch(permit, |context| {
                assert_eq!(context.vault_id, "vault");
                Ok(bridge.respond(&request))
            })
            .map_err(|_| RuntimeError::new("dispatch_failed"))?;
        response
            .to_json()
            .map(String::into_bytes)
            .map_err(|_| RuntimeError::new("invalid_response"))
    })
    .unwrap();
    NotesResponse::from_json(&result.output).unwrap()
}

fn page_request() -> NotesRequest {
    NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_4,
        request_id: "notes-page".into(),
        operation: NotesOperation::Page {
            cursor: None,
            limit: MAX_NOTE_PAGE,
        },
    }
}

fn install_host(verified: &VerifiedPackage) -> (Host, Session) {
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
    let session = host
        .open_session_for_api(installation, HOST_API_V0_4)
        .unwrap();
    (host, session)
}

#[test]
fn an_installed_notes_package_survives_reopen_and_stops_after_disable() {
    let scratch = Scratch::new();
    let store_path = scratch.0.join("store");
    let (verified, _) = signed_package();
    let digest = verified.digest().to_owned();
    {
        let mut store = Store::open_for_host(&store_path, &SUPPORTED_HOST_APIS).unwrap();
        let receipt = store
            .install("install-notes", 0, &verified)
            .expect("install the ordinary notes package");
        assert_eq!(receipt.digest, digest);
        // Installation persists the artifact disabled; enabling is a separate
        // Host policy action, never a manifest-derived grant.
        assert_eq!(receipt.status, "disabled");
        let (frozen_receipt, frozen) = store.frozen_package_for_action("org.qusor.notes").unwrap();
        assert_eq!(frozen_receipt.installation_id, receipt.installation_id);
        assert_eq!(frozen.digest(), digest);
    }

    let (_core_scratch, app, ids) = setup_core(&["installed note", "想法 from store"]);
    let store = Store::open_for_host(&store_path, &SUPPORTED_HOST_APIS).unwrap();
    let (receipt, frozen) = store.frozen_package_for_action("org.qusor.notes").unwrap();
    assert_eq!(receipt.digest, digest);
    assert_eq!(frozen.raw_bytes(), verified.raw_bytes());
    drop(store);

    let (mut host, session) = install_host(&frozen);
    let bridge = NotesReadBridge::new(app, "vault");
    let response = read_through_guest(&mut host, &session, &bridge, frozen.module_bytes());
    let NotesResult::Page { notes, .. } = &response.result else {
        panic!("expected a real Core page: {response:?}");
    };
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].capture_id, ids[1]);

    let installation = host
        .contributions()
        .first()
        .map(|(id, _)| *id)
        .expect("one installation");
    host.disable(installation).unwrap();
    assert!(host.authorize_notes(&session, &page_request()).is_err());
    assert!(
        host.open_session_for_api(installation, HOST_API_V0_4)
            .is_err()
    );
    drop(host);
}

#[test]
fn uninstall_tombstones_the_package_without_touching_core_notes() {
    let scratch = Scratch::new();
    let store_path = scratch.0.join("store");
    let (verified, _) = signed_package();
    let (_core_scratch, app, ids) = setup_core(&["survives uninstall"]);
    let mut store = Store::open_for_host(&store_path, &SUPPORTED_HOST_APIS).unwrap();
    let receipt = store.install("install-notes", 0, &verified).unwrap();
    store
        .uninstall("uninstall-notes", receipt.revision, "org.qusor.notes")
        .unwrap();
    drop(store);

    let store = Store::open_for_host(&store_path, &SUPPORTED_HOST_APIS).unwrap();
    assert!(store.frozen_package_for_action("org.qusor.notes").is_err());
    let bridge = NotesReadBridge::new(app, "vault");
    let page = bridge.respond(&page_request());
    let NotesResult::Page { notes, .. } = &page.result else {
        panic!("expected Core notes independent of the plugin: {page:?}");
    };
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].capture_id, ids[0]);
}
