use extension_api::{ApiRange, Capability, Manifest, PackageVersion};
use qusor_capture_probe::{
    contract::sha256, core::Core, pdf_compound::*, pdf_contract::*, pdf_host::PdfCaptureHost,
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
fn compound(f: &Fixture) -> PdfCompoundDraft {
    compound_for_draft(&f.draft)
}
fn compound_for_snapshot(snapshot: &PdfSnapshot) -> PdfCompoundDraft {
    compound_for_draft(&PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: sha256(b"hello"),
        unit: PDF_TEXT_UNIT.into(),
        start: 0,
        end: 5,
        exact: "hello".into(),
    })
}
fn compound_for_draft(draft: &PdfDraft) -> PdfCompoundDraft {
    let mut first = draft.clone();
    first.end = 2;
    first.exact = "he".into();
    let mut second = draft.clone();
    second.start = 3;
    second.exact = "lo".into();
    PdfCompoundDraft {
        protocol_version: 2,
        original_sha256: first.original_sha256.clone(),
        extraction_revision: first.extraction_revision.clone(),
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments: vec![first, second],
        exact: "he\nlo".into(),
    }
}
fn guest(mode: &str) -> Vec<u8> {
    let read=serde_json::json!({"api_version":{"major":0,"minor":1},"request_id":"read","capability":"selection.read","resource":{"kind":"selection","vault_id":"vault-a","selection_id":"active-pdf"}}).to_string();
    let escape = |s: &str| s.bytes().map(|b| format!("\\{b:02x}")).collect::<String>();
    let protocol = if mode == "legacy" { 1 } else { 2 };
    let prefix = format!(r#"{{"capture_protocol":{protocol},"request_id":"pdf-proposal","draft":"#);
    let mut attack = compound_for_snapshot(&snapshot(
        b"%PDF-1.7\nsynthetic trusted contract fixture\n%%EOF",
    ));
    if mode == "second" {
        attack.segments[1].start = 2;
        attack.segments[1].exact = "llo".into();
        attack.exact = "he\nllo".into();
    }
    if mode == "order" {
        attack.segments.reverse();
        attack.exact = "lo\nhe".into();
    }
    let attack_bytes = String::from_utf8(attack.canonical_bytes().unwrap()).unwrap();
    let mutate = if mode == "second" || mode == "order" {
        format!(
            "i32.const 8192 i32.const 45000 i32.const {} memory.copy i32.const {} local.set $n",
            attack_bytes.len(),
            attack_bytes.len()
        )
    } else {
        String::new()
    };
    wat::parse_str(format!(r#"(module(import "qusor" "call"(func $call(param i32 i32 i32 i32)(result i32)))(memory(export "memory") 2 2)(data(i32.const 0)"{}")(data(i32.const 16384)"{}")(data(i32.const 45000)"{}")(func(export "run")(result i32 i32)(local $n i32)i32.const 0 i32.const {} i32.const 8192 i32.const 4096 call $call local.set $n {mutate} i32.const {} i32.const 8192 local.get $n memory.copy i32.const {} local.get $n i32.add i32.const 125 i32.store8 i32.const 16384 i32.const {} local.get $n i32.add i32.const 1 i32.add i32.const 32768 i32.const 4096 call $call drop i32.const 32768 i32.const 0))"#,escape(&read),escape(&prefix),escape(&attack_bytes),read.len(),16384+prefix.len(),16384+prefix.len(),prefix.len())).unwrap()
}
fn fixture(mode: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "qusor-pdf-compound-host-{}-{}",
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
    PdfCaptureHost::open_compound(
        &f.root.join("packages"),
        &f.root.join("core"),
        "vault-a",
        "org.test.pdf",
        f.document_id,
        compound(f),
    )
    .unwrap()
}
#[test]
fn real_guest_reads_whole_compound_and_native_accepts_one_capture() {
    let f = fixture("relay");
    let h = open(&f);
    h.grant().unwrap();
    let p = h.propose().unwrap();
    assert!(p.draft().is_err());
    assert_eq!(p.compound().unwrap(), &compound(&f));
    assert_eq!(p.hash(), compound(&f).draft_hash().unwrap());
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "compound-save", "viewpoint").unwrap();
    let receipt = h.accept(a).unwrap();
    drop(h);
    let core = Core::open(f.root.join("core")).unwrap();
    assert!(core.query_pdf(receipt.capture_id).is_err());
    let saved = core.query_pdf_complete(receipt.capture_id).unwrap();
    assert_eq!(saved.compound.as_ref().unwrap(), &compound(&f));
    assert_eq!(
        core.resolve_pdf_compound(receipt.capture_id, &compound(&f))
            .unwrap()
            .exact,
        "he\nlo"
    );
    assert!(core.resolve_pdf(receipt.capture_id, &f.draft).is_err());
}
#[test]
fn altered_second_segment_and_legacy_envelope_are_rejected() {
    for mode in ["second", "order", "legacy"] {
        let f = fixture(mode);
        let h = open(&f);
        h.grant().unwrap();
        assert!(h.propose().is_err(), "{mode}");
        assert!(h.receipt("no-write").unwrap().is_none());
    }
}

#[test]
fn source_integrity_is_checked_before_guest_and_again_at_commit() {
    for stage in ["before-guest", "before-commit"] {
        let f = fixture("relay");
        let h = open(&f);
        h.grant().unwrap();
        let blob = f.root.join("core/blobs").join(&f.draft.original_sha256);
        let original = std::fs::read(&blob).unwrap();
        if stage == "before-guest" {
            std::fs::write(&blob, b"corrupted original").unwrap();
            assert!(h.propose().is_err());
        } else {
            let proposal = h.propose().unwrap();
            let hash = proposal.hash().to_owned();
            let approval = h
                .approve(proposal, &hash, "integrity-save", "note")
                .unwrap();
            std::fs::write(&blob, b"corrupted original").unwrap();
            assert!(h.accept(approval).is_err());
        }
        std::fs::write(blob, original).unwrap();
        drop(h);
        let core = Core::open(f.root.join("core")).unwrap();
        assert!(core.pdf_receipt("integrity-save").unwrap().is_none());
        let archive = core.export_all().unwrap();
        assert!(archive.pdf_captures.is_empty());
        assert_eq!(archive.commits.len(), 1);
    }
}
#[test]
fn compound_wire_cannot_enter_legacy_single_action() {
    let f = fixture("relay");
    let h = PdfCaptureHost::open(
        &f.root.join("packages"),
        &f.root.join("core"),
        "vault-a",
        "org.test.pdf",
        f.document_id,
        f.draft.clone(),
    )
    .unwrap();
    h.grant().unwrap();
    assert!(h.propose().is_err());
}
#[test]
fn whole_hash_order_and_native_boot_are_bound() {
    let f = fixture("relay");
    let h = open(&f);
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let mut reversed = compound(&f);
    reversed.segments.reverse();
    reversed.exact = "lo\nhe".into();
    assert!(
        h.approve(p, &reversed.draft_hash().unwrap(), "wrong-order", "")
            .is_err()
    );
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "old-boot", "").unwrap();
    drop(h);
    let h = open(&f);
    h.grant().unwrap();
    assert!(h.accept(a).is_err());
    assert!(h.receipt("old-boot").unwrap().is_none());
}
#[test]
fn compound_revoke_and_commit_serialize_at_gate() {
    let f = fixture("relay");
    let h = open(&f);
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "revoke-first", "").unwrap();
    let b = Arc::new(Barrier::new(2));
    let other = h.clone();
    let barrier = b.clone();
    let t = std::thread::spawn(move || {
        other.revoke().unwrap();
        barrier.wait();
    });
    b.wait();
    assert!(h.accept(a).is_err());
    t.join().unwrap();
    assert!(h.receipt("revoke-first").unwrap().is_none());
    h.grant().unwrap();
    let p = h.propose().unwrap();
    let hash = p.hash().to_owned();
    let a = h.approve(p, &hash, "commit-first", "").unwrap();
    let (tx, rx) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let other = h.clone();
    let t = std::thread::spawn(move || {
        other
            .accept_observed(a, || {
                tx.send(()).unwrap();
                wait.recv().unwrap();
            })
            .map(|r| r.capture_id)
            .unwrap()
    });
    rx.recv().unwrap();
    let other = h.clone();
    let r = std::thread::spawn(move || {
        other.revoke().unwrap();
        done.send(()).unwrap();
    });
    assert!(finished.recv_timeout(Duration::from_millis(30)).is_err());
    release.send(()).unwrap();
    let id = t.join().unwrap();
    r.join().unwrap();
    assert_eq!(h.receipt("commit-first").unwrap().unwrap().capture_id, id);
}
#[test]
fn compound_cannot_borrow_another_vault() {
    let f = fixture("relay");
    assert!(
        PdfCaptureHost::open_compound(
            &f.root.join("packages"),
            &f.root.join("core"),
            "vault-b",
            "org.test.pdf",
            f.document_id,
            compound(&f)
        )
        .is_err()
    );
}
