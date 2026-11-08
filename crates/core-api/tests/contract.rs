use core_api::contract::*;
fn draft(text: &str, exact: &str) -> CaptureDraft {
    CaptureDraft {
        protocol_version: 1,
        source: SourceDescriptor {
            namespace: "example.reader".into(),
            reference: "document-1".into(),
            title: "标题".into(),
            public_uri: Some("https://example.org/read/1".into()),
            external_return: ExternalReturn::SourceOnly,
        },
        snapshot_text: text.into(),
        selector: TextSelector {
            unit: TEXT_UNIT.into(),
            normalization: NORMALIZATION.into(),
            start: None,
            end: None,
            exact: exact.into(),
        },
    }
}
fn code<T: std::fmt::Debug>(value: Result<T>) -> ContractErrorCode {
    value.unwrap_err().code
}
#[test]
fn preserves_raw_text_and_scalar_byte_coordinates() {
    let value = draft("前\r\n中e\u{301}👩‍💻后", "中e\u{301}👩‍💻");
    let bytes = value.canonical_bytes().unwrap();
    assert_eq!(CaptureDraft::from_slice(&bytes).unwrap(), value);
    let range = value.resolved_selector().unwrap();
    assert_eq!((range.start, range.end), (3, 9));
    assert_eq!(
        &value.snapshot_text[range.byte_start as usize..range.byte_end as usize],
        range.text
    );
    assert_eq!(range.byte_start, 5);
    assert_eq!(
        value.snapshot_hash(),
        sha256(value.snapshot_text.as_bytes())
    );
}
#[test]
fn duplicate_quotes_never_choose_a_match() {
    let mut value = draft("aaa", "aa");
    assert_eq!(code(value.validate()), ContractErrorCode::Ambiguous);
    value.selector.start = Some(1);
    value.selector.end = Some(3);
    assert_eq!(value.resolved_selector().unwrap().start, 1);
    value.selector.exact = "ab".into();
    assert_eq!(code(value.validate()), ContractErrorCode::QuoteMismatch);
}
#[test]
fn graphemes_cannot_be_split_even_at_scalar_boundaries() {
    for (text, quote) in [("e\u{301}", "e"), ("👩‍💻", "👩"), ("🇨🇳", "🇨"), ("\r\n", "\r")]
    {
        assert_eq!(
            code(draft(text, quote).validate()),
            ContractErrorCode::InvalidBoundary
        );
    }
    assert!(draft("e\u{301}", "e\u{301}").validate().is_ok());
}
#[test]
fn malformed_ranges_and_missing_quotes_are_typed() {
    let mut value = draft("中文", "中");
    value.selector.start = Some(u64::MAX);
    assert_eq!(code(value.validate()), ContractErrorCode::InvalidSelector);
    value.selector.end = Some(u64::MAX);
    assert_eq!(code(value.validate()), ContractErrorCode::InvalidSelector);
    assert_eq!(
        code(draft("a", "b").validate()),
        ContractErrorCode::NotFound
    );
    assert_eq!(
        code(draft("a", "").validate()),
        ContractErrorCode::InvalidSelector
    );
    value = draft("a", "a");
    value.selector.normalization = "qusor-text-v1".into();
    assert_eq!(code(value.validate()), ContractErrorCode::InvalidSelector);
}
#[test]
fn anchor_requires_identical_version_and_bytes() {
    let value = draft("a\r\nb", "b");
    let anchor = value.anchor("snapshot-1").unwrap();
    assert_eq!(anchor.selector.start, Some(3));
    assert!(resolve_anchor(&value.snapshot_text, &anchor, "snapshot-1").is_ok());
    assert_eq!(
        code(resolve_anchor(&value.snapshot_text, &anchor, "snapshot-2")),
        ContractErrorCode::VersionMismatch
    );
    assert_eq!(
        code(resolve_anchor("a\nb", &anchor, "snapshot-1")),
        ContractErrorCode::HashMismatch
    );
}
#[test]
fn rejects_unknown_duplicate_and_authority_fields() {
    let bytes = draft("a", "a").canonical_bytes().unwrap();
    let json = String::from_utf8(bytes).unwrap();
    for bad in [
        json.replacen('{', "{\"user_note\":\"approve\",", 1),
        json.replacen('{', "{\"protocol_version\":1,", 1),
        json.replace("\"namespace\":", "\"approved\":true,\"namespace\":"),
    ] {
        assert_eq!(
            code(CaptureDraft::from_slice(bad.as_bytes())),
            ContractErrorCode::Malformed
        );
    }
    let mut value = draft("a", "a");
    value.protocol_version = 2;
    assert_eq!(
        code(value.validate()),
        ContractErrorCode::UnsupportedVersion
    );
}
#[test]
fn uri_metadata_has_no_credential_or_navigation_authority() {
    for uri in [
        "https://user:password@example.org/a",
        "https://example.org/a?token=secret",
        "https://example.org/a?public=1",
        "https://example.org/#fragment",
        "file:///private",
        "https://example.org/%0a",
        "https://example.org\\evil",
        "https://example.org:99999/a",
    ] {
        let mut value = draft("a", "a");
        value.source.public_uri = Some(uri.into());
        assert_eq!(
            code(value.validate()),
            ContractErrorCode::InvalidSource,
            "{uri}"
        );
    }
    let mut value = draft("a", "a");
    value.source.public_uri = None;
    assert!(value.validate().is_ok());
}
#[test]
fn request_budget_and_canonical_digest_are_deterministic() {
    let value = draft("a", "a");
    let request = CaptureRequest {
        capture_protocol: 1,
        request_id: "correlation-only".into(),
        draft: value.clone(),
    };
    assert_eq!(
        CaptureRequest::from_slice(&request.canonical_bytes().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        value.draft_hash().unwrap(),
        sha256(&value.canonical_bytes().unwrap())
    );
    assert_eq!(
        code(CaptureRequest::from_slice(&vec![b' '; MAX_WIRE_BYTES + 1])),
        ContractErrorCode::BudgetExceeded
    );
    let huge = draft(&"a".repeat(MAX_WIRE_BYTES), "a");
    let mut huge = huge;
    huge.selector.start = Some(0);
    huge.selector.end = Some(1);
    assert_eq!(code(huge.validate()), ContractErrorCode::BudgetExceeded);
}
