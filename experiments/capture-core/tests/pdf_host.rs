use extension_api::{ApiRange, Capability, Manifest, PackageVersion};
use qusor_capture_probe::{
    contract::sha256, core::Core, pdf_contract::*, pdf_host::PdfCaptureHost,
};
use qusor_package_probe::{
    package::{PackageFile, build_signed, publisher_key, verify},
    store::Store,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    root: PathBuf,
    document_id: i64,
    draft: PdfDraft,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn snapshot(original: &[u8]) -> PdfSnapshot {
    let bounds = PdfRect {
        left: 0.,
        bottom: 0.,
        right: 100.,
        top: 100.,
    };
    PdfSnapshot {
        protocol_version: 1,
        original_sha256: sha256(original),
        original_bytes: original.len() as u64,
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
        pages: vec![PdfPage {
            page_index: 0,
            engine_text: "hello".into(),
            text_sha256: sha256(b"hello"),
            rotation_degrees: 0,
            media_box: bounds.clone(),
            crop_box: bounds,
            characters: "hello"
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
                    pdf_bounds: Some(PdfRect {
                        left: i as f32,
                        bottom: 1.,
                        right: i as f32 + 1.,
                        top: 2.,
                    }),
                })
                .collect(),
        }],
    }
}
fn guest(mode: &str) -> Vec<u8> {
    let resource = if mode == "wrong-selection" {
        "other-selection"
    } else {
        "active-pdf"
    };
    let capability = if mode == "metadata" {
        "document.metadata.read"
    } else {
        "selection.read"
    };
    let read=serde_json::json!({"api_version":{"major":0,"minor":1},"request_id":"pdf-read","capability":capability,"resource":{"kind":"selection","vault_id":"vault-a","selection_id":resource}}).to_string();
    let escape = |s: &str| s.bytes().map(|b| format!("\\{b:02x}")).collect::<String>();
    let prefix = r#"{"capture_protocol":1,"request_id":"pdf-proposal","draft":"#;
    let read_call = if mode == "skip-read" {
        "i32.const 0 local.set $n".to_owned()
    } else {
        format!(
            "i32.const 0 i32.const {} i32.const 8192 i32.const 4096 call $call local.set $n",
            read.len()
        )
    };
    let mutate = if mode == "mutate-valid" {
        "i32.const 8192 local.get $n i32.add i32.const 7 i32.sub i32.const 72 i32.store8"
    } else if mode == "mutate" {
        "i32.const 8192 i32.const 0 i32.store8"
    } else {
        ""
    };
    wat::parse_str(format!(r#"(module(import "qusor" "call"(func $call(param i32 i32 i32 i32)(result i32)))(memory(export "memory") 2 2)(data(i32.const 0)"{}")(data(i32.const 16384)"{}")(func(export "run")(result i32 i32)(local $n i32){read_call} {mutate} i32.const {} i32.const 8192 local.get $n memory.copy i32.const {} local.get $n i32.add i32.const 125 i32.store8 i32.const 16384 i32.const {} local.get $n i32.add i32.const 1 i32.add i32.const 32768 i32.const 4096 call $call drop i32.const 32768 i32.const 0))"#,escape(&read),escape(prefix),16384+prefix.len(),16384+prefix.len(),prefix.len())).unwrap()
}
fn fixture(mode: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "qusor-pdf-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let manifest = Manifest {
        manifest_version: 1,
        package_id: "org.test.pdf".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 1,
            max_minor: 1,
        },
        required_capabilities: vec![],
        optional_capabilities: vec![Capability::SelectionRead, Capability::CapturePropose],
        contributions: vec![],
    };
    let signed = build_signed(
        manifest,
        "module.wasm",
        vec![PackageFile {
            path: "module.wasm".into(),
            mime: "application/wasm".into(),
            bytes: guest(mode),
        }],
        &[8; 32],
    )
    .unwrap();
    let package = verify(&signed, &publisher_key(&[8; 32])).unwrap();
    Store::open(&root.join("packages"))
        .unwrap()
        .install("install", 0, &package, None)
        .unwrap();
    let original = b"%PDF-1.7\nsynthetic trusted contract fixture\n%%EOF";
    let snapshot = snapshot(original);
    snapshot.validate().unwrap();
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
    let mut core = Core::open(root.join("core")).unwrap();
    let receipt = core
        .import_document("pdf-import", "vault-a", "Test", original, &snapshot, None)
        .unwrap();
    drop(core);
    Fixture {
        root,
        document_id: receipt.document_id,
        draft,
    }
}
fn open(f: &Fixture) -> PdfCaptureHost {
    PdfCaptureHost::open(
        &f.root.join("packages"),
        &f.root.join("core"),
        "vault-a",
        "org.test.pdf",
        f.document_id,
        f.draft.clone(),
    )
    .unwrap()
}
#[test]
fn plugin_reads_current_selection_then_proposes_and_native_accepts() {
    let f = fixture("relay");
    let host = open(&f);
    assert!(host.propose().is_err());
    host.grant().unwrap();
    let p = host.propose().unwrap();
    assert_eq!(p.draft().unwrap(), &f.draft);
    let hash = p.hash().to_owned();
    let a = host.approve(p, &hash, "pdf-save", "my note").unwrap();
    let receipt = host.accept(a).unwrap();
    assert_eq!(host.receipt("pdf-save").unwrap().unwrap(), receipt);
}
#[test]
fn broker_rejects_other_resources_mutation_and_skip_read() {
    for mode in [
        "wrong-selection",
        "metadata",
        "mutate",
        "mutate-valid",
        "skip-read",
    ] {
        let f = fixture(mode);
        let host = open(&f);
        host.grant().unwrap();
        assert!(host.propose().is_err(), "{mode}");
        assert!(host.receipt("never").unwrap().is_none());
    }
}
#[test]
fn trusted_selection_cannot_cross_vault_document_or_snapshot() {
    let f = fixture("relay");
    assert!(
        PdfCaptureHost::open(
            &f.root.join("packages"),
            &f.root.join("core"),
            "vault-b",
            "org.test.pdf",
            f.document_id,
            f.draft.clone()
        )
        .is_err()
    );
    let mut changed = f.draft.clone();
    changed.original_sha256 = "f".repeat(64);
    assert!(
        PdfCaptureHost::open(
            &f.root.join("packages"),
            &f.root.join("core"),
            "vault-a",
            "org.test.pdf",
            f.document_id,
            changed
        )
        .is_err()
    );
    assert!(
        PdfCaptureHost::open(
            &f.root.join("packages"),
            &f.root.join("core"),
            "vault-a",
            "org.test.pdf",
            f.document_id + 1,
            f.draft.clone()
        )
        .is_err()
    );
}
#[test]
fn revoke_and_commit_have_real_serial_gate_order() {
    let f = fixture("relay");
    let host = open(&f);
    host.grant().unwrap();
    let p = host.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = host.approve(p, &hash, "revoke-first", "").unwrap();
    let b = Arc::new(Barrier::new(2));
    let other = host.clone();
    let barrier = b.clone();
    let revoke = std::thread::spawn(move || {
        other.revoke().unwrap();
        barrier.wait();
    });
    b.wait();
    assert!(host.accept(a).is_err());
    revoke.join().unwrap();
    assert!(host.receipt("revoke-first").unwrap().is_none());
    host.grant().unwrap();
    let p = host.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = host.approve(p, &hash, "commit-first", "").unwrap();
    let (tx, rx) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let other = host.clone();
    let commit = std::thread::spawn(move || {
        other
            .accept_observed(a, || {
                tx.send(()).unwrap();
                wait.recv().unwrap();
            })
            .map(|r| r.capture_id)
            .unwrap()
    });
    rx.recv().unwrap();
    let other = host.clone();
    let revoke = std::thread::spawn(move || {
        other.revoke().unwrap();
        done.send(()).unwrap();
    });
    assert!(finished.recv_timeout(Duration::from_millis(30)).is_err());
    release.send(()).unwrap();
    let id = commit.join().unwrap();
    revoke.join().unwrap();
    assert_eq!(
        host.receipt("commit-first").unwrap().unwrap().capture_id,
        id
    );
}
#[test]
fn text_host_cannot_service_pdf_selection_read() {
    let f = fixture("relay");
    let host = qusor_capture_probe::host::CaptureHost::open(
        &f.root.join("packages"),
        &f.root.join("core"),
        "vault-a",
        "org.test.pdf",
    )
    .unwrap();
    host.grant().unwrap();
    assert!(host.propose().is_err());
}

#[test]
fn uninstall_does_not_remove_saved_pdf_or_its_selection() {
    let f = fixture("relay");
    let host = open(&f);
    host.grant().unwrap();
    let p = host.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = host
        .approve(p, &hash, "survive-uninstall", "separate note")
        .unwrap();
    let saved = host.accept(a).unwrap();
    drop(host);
    let core = Core::open(f.root.join("core")).unwrap();
    let before = serde_json::to_value(core.export_all().unwrap()).unwrap();
    let selection = core.resolve_pdf(saved.capture_id, &f.draft).unwrap();
    drop(core);
    let mut store = Store::open(&f.root.join("packages")).unwrap();
    let (installed, _) = store.frozen_package_for_action("org.test.pdf").unwrap();
    store
        .uninstall("uninstall", installed.revision, "org.test.pdf", None)
        .unwrap();
    drop(store);
    assert!(
        PdfCaptureHost::open(
            &f.root.join("packages"),
            &f.root.join("core"),
            "vault-a",
            "org.test.pdf",
            f.document_id,
            f.draft.clone()
        )
        .is_err()
    );
    let core = Core::open(f.root.join("core")).unwrap();
    assert_eq!(core.query_pdf(saved.capture_id).unwrap().receipt, saved);
    assert_eq!(
        core.resolve_pdf(saved.capture_id, &f.draft).unwrap(),
        selection
    );
    assert_eq!(
        serde_json::to_value(core.export_all().unwrap()).unwrap(),
        before
    );
}

#[test]
fn pdf_approval_is_not_reusable_after_reboot_or_grant_replacement() {
    let f = fixture("relay");
    let host = open(&f);
    host.grant().unwrap();
    let p = host.propose().unwrap();
    let hash = p.hash().to_owned();
    assert!(host.approve(p, "unreviewed", "save", "").is_err());
    let p = host.propose().unwrap();
    let a = host.approve(p, &hash, "old-boot", "").unwrap();
    drop(host);
    let host = open(&f);
    host.grant().unwrap();
    assert!(host.accept(a).is_err());
    assert!(host.receipt("old-boot").unwrap().is_none());
    let p = host.propose().unwrap();
    let a = host.approve(p, &hash, "old-epoch", "").unwrap();
    host.grant().unwrap();
    assert!(host.accept(a).is_err());
    assert!(host.receipt("old-epoch").unwrap().is_none());
}
