use core_api::{contract::sha256, pdf_compound::*, pdf_contract::*};
fn rect() -> PdfRect {
    PdfRect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 100.0,
    }
}
fn snapshot(text: &str) -> PdfSnapshot {
    let mut utf16 = 0;
    let characters = text
        .char_indices()
        .enumerate()
        .map(|(index, (byte, ch))| {
            let mapped = PdfCharacter {
                pdf_char_index: index as u64,
                unicode_value: ch as u32,
                mapping: "scalar".into(),
                generated: ch.is_whitespace(),
                scalar_start: index as u64,
                scalar_end: index as u64 + 1,
                utf8_start: byte as u64,
                utf8_end: (byte + ch.len_utf8()) as u64,
                utf16_start: utf16,
                utf16_end: utf16 + ch.len_utf16() as u64,
                pdf_bounds: if ch.is_whitespace() {
                    None
                } else {
                    Some(rect())
                },
            };
            utf16 = mapped.utf16_end;
            mapped
        })
        .collect();
    PdfSnapshot {
        protocol_version: 1,
        original_sha256: sha256(b"original PDF"),
        original_bytes: 12,
        extraction: ExtractionIdentity {
            provider: "pdfium".into(),
            algorithm: "qusor-pdfium-char-map-v1".into(),
            wrapper: "pdfium-render-0.9.5".into(),
            abi: "pdfium_8125".into(),
            library_sha256: sha256(b"library"),
            mapping_check: "unicode_scalar_validity_only".into(),
            normalization: PDF_NORMALIZATION.into(),
        },
        engine_report_sha256: sha256(b"report"),
        pages: vec![PdfPage {
            page_index: 0,
            engine_text: text.into(),
            text_sha256: sha256(text.as_bytes()),
            rotation_degrees: 90,
            media_box: rect(),
            crop_box: rect(),
            characters,
        }],
    }
}
fn compound(snapshot: &PdfSnapshot, ranges: &[(u32, u64, u64, &str)]) -> PdfCompoundDraft {
    let revision = snapshot.extraction_revision().unwrap();
    let segments = ranges
        .iter()
        .map(|(page, start, end, exact)| PdfDraft {
            protocol_version: 1,
            original_sha256: snapshot.original_sha256.clone(),
            extraction_revision: revision.clone(),
            page_index: *page,
            page_text_sha256: snapshot.pages[*page as usize].text_sha256.clone(),
            unit: PDF_TEXT_UNIT.into(),
            start: *start,
            end: *end,
            exact: (*exact).into(),
        })
        .collect::<Vec<_>>();
    let exact = segments
        .iter()
        .map(|s| s.exact.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    PdfCompoundDraft {
        protocol_version: 2,
        original_sha256: snapshot.original_sha256.clone(),
        extraction_revision: revision,
        selection_profile: PDF_SELECTION_PROFILE.into(),
        separator: "\n".into(),
        segments,
        exact,
    }
}
fn code<T: std::fmt::Debug>(value: Result<T>) -> PdfContractErrorCode {
    value.unwrap_err().code
}
fn rejoin(draft: &mut PdfCompoundDraft) {
    draft.exact = draft
        .segments
        .iter()
        .map(|s| s.exact.as_str())
        .collect::<Vec<_>>()
        .join(&draft.separator);
}
#[test]
fn rotated_lines_keep_explicit_order_without_fabricating_source_separators() {
    let snapshot = snapshot("lowerUPPER");
    let draft = compound(&snapshot, &[(0, 5, 10, "UPPER"), (0, 0, 5, "lower")]);
    let selection = draft.resolve(&snapshot).unwrap();
    assert_eq!(selection.exact, "UPPER\nlower");
    assert_eq!(selection.segments[0].start, 5);
    assert_eq!(selection.segments[1].start, 0);
    assert_eq!(
        selection
            .segments
            .iter()
            .map(|s| s.characters.len())
            .sum::<usize>(),
        10
    );
    assert_eq!(selection.draft, draft);
    assert!(!snapshot.pages[0].engine_text.contains('\n'));
    assert_eq!(
        PdfCompoundDraft::from_slice(&draft.canonical_bytes().unwrap()).unwrap(),
        draft
    );
    assert_eq!(
        draft.draft_hash().unwrap(),
        sha256(&draft.canonical_bytes().unwrap())
    );
}
#[test]
fn pairwise_overlap_and_duplicate_ranges_are_rejected_but_adjacent_are_valid() {
    let snapshot = snapshot("aaaaaa");
    let initial = compound(
        &snapshot,
        &[(0, 0, 2, "aa"), (0, 2, 4, "aa"), (0, 4, 6, "aa")],
    );
    assert!(initial.resolve(&snapshot).is_ok());
    let mut draft = initial.clone();
    draft.segments[2] = draft.segments[0].clone();
    assert_eq!(
        code(draft.validate()),
        PdfContractErrorCode::InvalidSelector
    );
    draft = initial;
    draft.segments[2].start = 1;
    draft.segments[2].end = 3;
    assert_eq!(
        code(draft.validate()),
        PdfContractErrorCode::InvalidSelector
    );
}
#[test]
fn cross_page_order_is_explicit_and_local_ranges_can_repeat_on_different_pages() {
    let mut snapshot = snapshot("ab");
    let mut page = snapshot.pages[0].clone();
    page.page_index = 1;
    snapshot.pages.push(page);
    let initial = compound(&snapshot, &[(0, 0, 1, "a"), (1, 0, 1, "a")]);
    let selection = initial.resolve(&snapshot).unwrap();
    assert_eq!(selection.exact, "a\na");
    assert_eq!(selection.segments[1].page_index, 1);
    let mut draft = initial;
    draft.segments.reverse();
    assert_eq!(
        code(draft.validate()),
        PdfContractErrorCode::InvalidSelector
    );
}
#[test]
fn version_profile_separator_segment_count_and_composed_quote_are_strict() {
    let snapshot = snapshot("ab");
    let initial = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "b")]);
    let mut draft = initial.clone();
    draft.protocol_version = 1;
    assert_eq!(
        code(draft.validate()),
        PdfContractErrorCode::UnsupportedVersion
    );
    for change in 0..5 {
        let mut draft = initial.clone();
        match change {
            0 => draft.selection_profile = "canonical-reading-order".into(),
            1 => draft.separator = " ".into(),
            2 => draft.segments.truncate(1),
            3 => draft.segments = vec![draft.segments[0].clone(); 9],
            _ => draft.exact = "ab".into(),
        }
        assert_eq!(
            code(draft.validate()),
            if change == 4 {
                PdfContractErrorCode::QuoteMismatch
            } else {
                PdfContractErrorCode::InvalidSelector
            }
        );
    }
}
#[test]
fn every_segment_is_bound_to_the_same_immutable_tuple() {
    let snapshot = snapshot("ab");
    let initial = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "b")]);
    for change in 0..5 {
        let mut draft = initial.clone();
        let hash = sha256(b"wrong hash");
        match change {
            0 => draft.original_sha256 = hash,
            1 => draft.extraction_revision = hash,
            2 => draft.segments[1].original_sha256 = hash,
            3 => draft.segments[1].extraction_revision = hash,
            _ => draft.segments[1].page_text_sha256 = hash,
        }
        assert_eq!(
            code(draft.resolve(&snapshot)),
            PdfContractErrorCode::HashMismatch
        );
    }
    let mut draft = initial;
    draft.segments[1].exact = "x".into();
    rejoin(&mut draft);
    assert_eq!(
        code(draft.resolve(&snapshot)),
        PdfContractErrorCode::QuoteMismatch
    );
}
#[test]
fn each_segment_enforces_grapheme_and_mapping_rules() {
    let snapshot = snapshot("e\u{301}x");
    let draft = compound(&snapshot, &[(0, 0, 1, "e"), (0, 2, 3, "x")]);
    assert_eq!(
        code(draft.resolve(&snapshot)),
        PdfContractErrorCode::InvalidBoundary
    );
    let mut snapshot = self::snapshot("a�");
    snapshot.pages[0].characters[1].unicode_value = 0;
    snapshot.pages[0].characters[1].mapping = "unknown".into();
    let draft = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "�")]);
    assert_eq!(
        code(draft.resolve(&snapshot)),
        PdfContractErrorCode::UnknownMapping
    );
    let snapshot = self::snapshot("a😀");
    let draft = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "😀")]);
    let selection = draft.resolve(&snapshot).unwrap();
    assert_eq!(
        (
            selection.segments[1].byte_start,
            selection.segments[1].byte_end
        ),
        (1, 5)
    );
    assert_eq!(
        (
            selection.segments[1].utf16_start,
            selection.segments[1].utf16_end
        ),
        (1, 3)
    );
}
#[test]
fn typed_json_rejects_unknown_duplicate_and_native_authority_fields() {
    let snapshot = snapshot("ab");
    let draft = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "b")]);
    let json = String::from_utf8(draft.canonical_bytes().unwrap()).unwrap();
    for bad in [
        json.replacen('{', "{\"protocol_version\":2,", 1),
        json.replacen('{', "{\"approved\":true,", 1),
        json.replace("\"unit\":", "\"unit\":\"unicode_scalar\",\"unit\":"),
        json.replace("\"start\":", "\"pixel_quad\":[],\"start\":"),
    ] {
        assert_eq!(
            code(PdfCompoundDraft::from_slice(bad.as_bytes())),
            PdfContractErrorCode::Malformed
        );
    }
    let request = PdfCompoundCaptureRequest {
        capture_protocol: 2,
        request_id: "native-correlation".into(),
        draft,
    };
    let bytes = request.canonical_bytes().unwrap();
    assert_eq!(
        PdfCompoundCaptureRequest::from_slice(&bytes).unwrap(),
        request
    );
    let json = String::from_utf8(bytes).unwrap();
    for field in ["document_id", "user_note", "operation_id"] {
        let bad = json.replacen('{', &format!("{{\"{field}\":\"guest\","), 1);
        assert_eq!(
            code(PdfCompoundCaptureRequest::from_slice(bad.as_bytes())),
            PdfContractErrorCode::Malformed
        );
    }
    let mut request = request;
    request.capture_protocol = 1;
    assert_eq!(
        code(request.validate()),
        PdfContractErrorCode::UnsupportedVersion
    );
}
#[test]
fn both_actual_draft_and_envelope_must_fit_the_wire_budget() {
    let snapshot = snapshot("ab");
    let initial = compound(&snapshot, &[(0, 0, 1, "a"), (0, 1, 2, "b")]);
    assert_eq!(
        code(PdfCompoundDraft::from_slice(&vec![
            b' ';
            MAX_PDF_REQUEST_BYTES + 1
        ])),
        PdfContractErrorCode::BudgetExceeded
    );
    assert_eq!(
        code(PdfCompoundCaptureRequest::from_slice(&vec![
            b' ';
            MAX_PDF_REQUEST_BYTES
                + 1
        ])),
        PdfContractErrorCode::BudgetExceeded
    );
    let mut largest = None;
    for len in 1..1100 {
        let mut draft = initial.clone();
        for (index, segment) in draft.segments.iter_mut().enumerate() {
            segment.start = (index * len) as u64;
            segment.end = ((index + 1) * len) as u64;
            segment.exact = "a".repeat(len);
        }
        rejoin(&mut draft);
        if draft.canonical_bytes().is_err() {
            break;
        }
        largest = Some(draft);
    }
    let draft = largest.unwrap();
    assert!(draft.canonical_bytes().unwrap().len() <= MAX_PDF_REQUEST_BYTES);
    let request = PdfCompoundCaptureRequest {
        capture_protocol: 2,
        request_id: "x".repeat(128),
        draft,
    };
    assert_eq!(
        code(request.canonical_bytes()),
        PdfContractErrorCode::BudgetExceeded
    );
}
