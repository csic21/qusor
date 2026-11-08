use core_api::{contract::sha256, pdf_contract::*};

fn rect() -> PdfRect {
    PdfRect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 200.0,
    }
}
fn snapshot(text: &str) -> PdfSnapshot {
    let mut utf16 = 0;
    let characters = text
        .char_indices()
        .enumerate()
        .map(|(index, (byte, scalar))| {
            let value = PdfCharacter {
                pdf_char_index: index as u64,
                unicode_value: scalar as u32,
                mapping: "scalar".into(),
                generated: scalar.is_whitespace(),
                scalar_start: index as u64,
                scalar_end: index as u64 + 1,
                utf8_start: byte as u64,
                utf8_end: (byte + scalar.len_utf8()) as u64,
                utf16_start: utf16,
                utf16_end: utf16 + scalar.len_utf16() as u64,
                pdf_bounds: if scalar.is_whitespace() {
                    None
                } else {
                    Some(rect())
                },
            };
            utf16 = value.utf16_end;
            value
        })
        .collect();
    PdfSnapshot {
        protocol_version: 1,
        original_sha256: sha256(b"immutable PDF bytes"),
        original_bytes: b"immutable PDF bytes".len() as u64,
        extraction: ExtractionIdentity {
            provider: "pdfium".into(),
            algorithm: "qusor-pdfium-char-map-v1".into(),
            wrapper: "pdfium-render-0.9.5".into(),
            abi: "pdfium_8125".into(),
            library_sha256: sha256(b"native library"),
            mapping_check: "unicode_scalar_validity_only".into(),
            normalization: PDF_NORMALIZATION.into(),
        },
        engine_report_sha256: sha256(b"render report"),
        pages: vec![PdfPage {
            page_index: 0,
            engine_text: text.into(),
            text_sha256: sha256(text.as_bytes()),
            rotation_degrees: 0,
            media_box: rect(),
            crop_box: rect(),
            characters,
        }],
    }
}
fn draft(snapshot: &PdfSnapshot, start: u64, end: u64, exact: &str) -> PdfDraft {
    PdfDraft {
        protocol_version: 1,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: snapshot.extraction_revision().unwrap(),
        page_index: 0,
        page_text_sha256: snapshot.pages[0].text_sha256.clone(),
        unit: PDF_TEXT_UNIT.into(),
        start,
        end,
        exact: exact.into(),
    }
}
fn code<T: std::fmt::Debug>(value: Result<T>) -> PdfContractErrorCode {
    value.unwrap_err().code
}

#[test]
fn immutable_raw_stream_and_every_coordinate_round_trip() {
    let snapshot = snapshot("前\r\n中e\u{301}👩‍💻后");
    let selection = snapshot
        .resolve(&draft(&snapshot, 3, 9, "中e\u{301}👩‍💻"))
        .unwrap();
    assert_eq!((selection.start, selection.end), (3, 9));
    assert_eq!((selection.byte_start, selection.byte_end), (5, 22));
    assert_eq!((selection.utf16_start, selection.utf16_end), (3, 11));
    assert_eq!(selection.characters.len(), 6);
    assert_eq!(selection.characters[0].pdf_bounds, Some(rect()));
    let bytes = snapshot.canonical_bytes().unwrap();
    assert_eq!(PdfSnapshot::from_slice(&bytes).unwrap(), snapshot);
    assert_eq!(
        PdfDraft::from_slice(&selection.anchor.canonical_bytes().unwrap()).unwrap(),
        selection.anchor
    );
    assert!(snapshot.validate_original(b"immutable PDF bytes").is_ok());
    assert_eq!(
        code(snapshot.validate_original(b"modified PDF bytes")),
        PdfContractErrorCode::OriginalMismatch
    );
}
#[test]
fn semantic_revision_excludes_only_render_report_provenance() {
    let initial = snapshot("same text");
    let revision = initial.extraction_revision().unwrap();
    let mut changed = initial.clone();
    changed.engine_report_sha256 = sha256(b"different DPI or rendered pixels");
    assert_ne!(
        initial.canonical_bytes().unwrap(),
        changed.canonical_bytes().unwrap()
    );
    assert_eq!(revision, changed.extraction_revision().unwrap());
    changed.pages[0].media_box.left = -0.0;
    assert_eq!(revision, changed.extraction_revision().unwrap());
    changed.extraction.library_sha256 = sha256(b"different native build");
    assert_ne!(revision, changed.extraction_revision().unwrap());
    changed = initial.clone();
    changed.pages[0].rotation_degrees = 90;
    assert_ne!(revision, changed.extraction_revision().unwrap());
    changed = initial.clone();
    changed.pages[0].crop_box.left = 1.0;
    assert_ne!(revision, changed.extraction_revision().unwrap());
    changed = initial.clone();
    changed.pages[0].characters[0]
        .pdf_bounds
        .as_mut()
        .unwrap()
        .left = 2.0;
    assert_ne!(revision, changed.extraction_revision().unwrap());
    changed = initial.clone();
    changed.original_sha256 = sha256(b"new original");
    assert_ne!(revision, changed.extraction_revision().unwrap());
    assert_ne!(
        revision,
        snapshot("same text!").extraction_revision().unwrap()
    );
}
#[test]
fn all_hash_constraints_are_required_without_fallback() {
    let snapshot = snapshot("repeat repeat");
    let initial = draft(&snapshot, 7, 13, "repeat");
    assert_eq!(resolve_pdf(&snapshot, &initial).unwrap().start, 7);
    for field in 0..3 {
        let mut changed = initial.clone();
        let bad = sha256(b"mismatch");
        match field {
            0 => changed.original_sha256 = bad,
            1 => changed.extraction_revision = bad,
            _ => changed.page_text_sha256 = bad,
        }
        assert_eq!(
            code(snapshot.resolve(&changed)),
            PdfContractErrorCode::HashMismatch
        );
    }
    let mut changed = initial.clone();
    changed.exact = "Repeat".into();
    assert_eq!(
        code(snapshot.resolve(&changed)),
        PdfContractErrorCode::QuoteMismatch
    );
    changed = initial;
    changed.start = 0;
    changed.end = 5;
    assert_eq!(
        code(snapshot.resolve(&changed)),
        PdfContractErrorCode::QuoteMismatch
    );
}
#[test]
fn refuses_partial_graphemes_including_emoji_flags_and_crlf() {
    for (text, end, quote) in [
        ("e\u{301}", 1, "e"),
        ("👩‍💻", 1, "👩"),
        ("🇨🇳", 1, "🇨"),
        ("\r\n", 1, "\r"),
    ] {
        let snapshot = snapshot(text);
        assert_eq!(
            code(snapshot.resolve(&draft(&snapshot, 0, end, quote))),
            PdfContractErrorCode::InvalidBoundary
        );
    }
}
#[test]
fn validates_all_map_offsets_and_raw_unicode() {
    let initial = snapshot("中😀");
    for field in 0..9 {
        let mut changed = initial.clone();
        let character = &mut changed.pages[0].characters[1];
        match field {
            0 => character.pdf_char_index = 0,
            1 => character.unicode_value = 'a' as u32,
            2 => character.mapping = "guessed".into(),
            3 => character.scalar_start = 0,
            4 => character.scalar_end = u64::MAX,
            5 => character.utf8_start = 0,
            6 => character.utf8_end = 1,
            7 => character.utf16_start = 99,
            _ => character.utf16_end = 4,
        }
        assert_eq!(
            code(changed.validate()),
            PdfContractErrorCode::InvalidMap,
            "field {field}"
        );
    }
    let mut changed = initial;
    changed.pages[0].characters.pop();
    assert_eq!(code(changed.validate()), PdfContractErrorCode::InvalidMap);
}
#[test]
fn unknown_mappings_are_explicit_evidence_never_exact_anchors() {
    let mut snapshot = snapshot("a�b");
    snapshot.pages[0].characters[1].unicode_value = 0;
    snapshot.pages[0].characters[1].mapping = "unknown".into();
    snapshot.validate().unwrap();
    assert_eq!(
        code(snapshot.resolve(&draft(&snapshot, 1, 2, "�"))),
        PdfContractErrorCode::UnknownMapping
    );
    assert!(snapshot.resolve(&draft(&snapshot, 2, 3, "b")).is_ok());
    snapshot.pages[0].characters[1].mapping = "scalar".into();
    assert_eq!(code(snapshot.validate()), PdfContractErrorCode::InvalidMap);
}
#[test]
fn geometry_is_finite_ordered_pdf_space_and_generated_gaps_stay_explicit() {
    let initial = snapshot("a b");
    for field in 0..5 {
        let mut changed = initial.clone();
        match field {
            0 => changed.pages[0].crop_box.right = 0.0,
            1 => changed.pages[0].media_box.top = f32::INFINITY,
            2 => {
                changed.pages[0].characters[0]
                    .pdf_bounds
                    .as_mut()
                    .unwrap()
                    .left = f32::NAN
            }
            3 => changed.pages[0].characters[1].pdf_bounds = Some(rect()),
            _ => changed.pages[0].characters[0].pdf_bounds = None,
        }
        assert_eq!(
            code(changed.validate()),
            PdfContractErrorCode::InvalidGeometry
        );
    }
    let selection = initial.resolve(&draft(&initial, 0, 3, "a b")).unwrap();
    assert!(selection.characters[1].generated);
    assert!(selection.characters[1].pdf_bounds.is_none());
}
#[test]
fn strict_json_and_profile_reject_unknown_duplicate_and_render_fields() {
    let initial = snapshot("a");
    let json = String::from_utf8(initial.canonical_bytes().unwrap()).unwrap();
    for bad in [
        json.replacen('{', "{\"protocol_version\":1,", 1),
        json.replacen('{', "{\"title\":\"plugin claims\",", 1),
        json.replace(
            "\"pdf_char_index\":",
            "\"pixel_quad\":null,\"pdf_char_index\":",
        ),
    ] {
        assert_eq!(
            code(PdfSnapshot::from_slice(bad.as_bytes())),
            PdfContractErrorCode::Malformed
        );
    }
    let mut changed = initial.clone();
    changed.protocol_version = 2;
    assert_eq!(
        code(changed.validate()),
        PdfContractErrorCode::UnsupportedVersion
    );
    changed = initial.clone();
    changed.extraction.mapping_check = "verified_text_semantics".into();
    assert_eq!(
        code(changed.validate()),
        PdfContractErrorCode::InvalidIdentity
    );
    changed = initial;
    changed.pages[0].text_sha256 = sha256(b"incorrect");
    assert_eq!(code(changed.validate()), PdfContractErrorCode::HashMismatch);
}
#[test]
fn no_text_pages_and_invalid_selectors_are_explicit() {
    let snapshot = snapshot("");
    snapshot.validate().unwrap();
    assert_eq!(
        code(snapshot.resolve(&draft(&snapshot, 0, 1, "a"))),
        PdfContractErrorCode::NoText
    );
    let snapshot = self::snapshot("a");
    let initial = draft(&snapshot, 0, 1, "a");
    for field in 0..5 {
        let mut changed = initial.clone();
        match field {
            0 => changed.unit = "utf16".into(),
            1 => changed.start = 1,
            2 => changed.end = u64::MAX,
            3 => changed.exact.clear(),
            _ => changed.page_index = 15,
        }
        assert_eq!(
            code(snapshot.resolve(&changed)),
            PdfContractErrorCode::InvalidSelector
        );
    }
}
#[test]
fn budgeted_request_has_no_approval_identity_or_storage_fields() {
    let snapshot = snapshot("a");
    let request = PdfCaptureRequest {
        capture_protocol: 1,
        request_id: "guest-correlation".into(),
        draft: draft(&snapshot, 0, 1, "a"),
    };
    let bytes = request.canonical_bytes().unwrap();
    assert_eq!(PdfCaptureRequest::from_slice(&bytes).unwrap(), request);
    assert_eq!(
        request.draft.draft_hash().unwrap(),
        sha256(&request.draft.canonical_bytes().unwrap())
    );
    let json = String::from_utf8(bytes).unwrap();
    for field in ["user_note", "approved", "document_id", "operation_id"] {
        let bad = json.replacen('{', &format!("{{\"{field}\":\"guest\","), 1);
        assert_eq!(
            code(PdfCaptureRequest::from_slice(bad.as_bytes())),
            PdfContractErrorCode::Malformed
        );
    }
    assert_eq!(
        code(PdfCaptureRequest::from_slice(&vec![
            b' ';
            MAX_PDF_REQUEST_BYTES
                + 1
        ])),
        PdfContractErrorCode::BudgetExceeded
    );
    let mut large = request;
    large.draft.exact = "a".repeat(MAX_PDF_REQUEST_BYTES);
    assert_eq!(
        code(large.canonical_bytes()),
        PdfContractErrorCode::BudgetExceeded
    );
    let mut many = snapshot.clone();
    many.pages = vec![snapshot.pages[0].clone(); MAX_PDF_PAGES + 1];
    assert_eq!(code(many.validate()), PdfContractErrorCode::BudgetExceeded);
    let mut many = snapshot;
    many.pages[0].characters = vec![many.pages[0].characters[0].clone(); MAX_PDF_CHARACTERS + 1];
    assert_eq!(code(many.validate()), PdfContractErrorCode::BudgetExceeded);
    many.original_bytes = MAX_PDF_BYTES as u64 + 1;
    assert_eq!(code(many.validate()), PdfContractErrorCode::BudgetExceeded);
}
