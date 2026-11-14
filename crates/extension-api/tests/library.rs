use extension_api::{library::*, *};

fn request(operation: LibraryOperation) -> LibraryRequest {
    LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_6,
        request_id: "request".into(),
        operation,
    }
}

fn request_v7(operation: LibraryOperation) -> LibraryRequest {
    LibraryRequest {
        library_protocol: LIBRARY_PROTOCOL,
        api_version: HOST_API_V0_7,
        request_id: "request".into(),
        operation,
    }
}

fn document(id: i64) -> LibraryDocument {
    LibraryDocument {
        document_id: id,
        commit_rev: id,
        title: "Source".into(),
        original_sha256: "a".repeat(64),
        original_bytes: 1024,
        extraction_revision: "b".repeat(64),
        page_count: 2,
    }
}

fn page(documents: Vec<LibraryDocument>, next: Option<&str>) -> LibraryResponse {
    LibraryResponse {
        request_id: "request".into(),
        result: LibraryResult::Page {
            documents,
            next_cursor: next.map(str::to_owned),
        },
    }
}

fn manifest(min_minor: u16) -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.test.library".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor,
            max_minor: 7,
        },
        required_capabilities: vec![Capability::LibraryRead],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.test.library.workspace".into(),
            title: "Library".into(),
            kind: ContributionKind::LibraryWorkspace,
            required_capabilities: vec![Capability::LibraryRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}

#[test]
fn requests_round_trip_with_canonical_bounds() {
    for operation in [
        LibraryOperation::Page {
            cursor: None,
            limit: MAX_LIBRARY_PAGE,
        },
        LibraryOperation::Page {
            cursor: Some("l1:7:3".into()),
            limit: 1,
        },
        LibraryOperation::PageText {
            document_id: 2,
            page_index: 0,
            offset: 0,
            max_bytes: MAX_LIBRARY_TEXT_CHUNK,
        },
        LibraryOperation::CaptureSelection {
            document_id: 1,
            page_index: 0,
            start: 0,
            end: 5,
            note: "mark this thought".into(),
        },
    ] {
        let request = request(operation);
        assert_eq!(
            LibraryRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );
    }
}

#[test]
fn saved_selections_round_trip_only_with_api_0_7() {
    let operation = LibraryOperation::SavedSelections {
        document_id: 1,
        page_index: 0,
    };
    let v7 = request_v7(operation.clone());
    assert_eq!(
        LibraryRequest::from_json(&v7.to_json().unwrap()).unwrap(),
        v7
    );
    let mut old = request(operation);
    old.api_version = HOST_API_V0_6;
    assert_eq!(
        old.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    let response = LibraryResponse {
        request_id: "request".into(),
        result: LibraryResult::SavedSelections {
            document_id: 1,
            page_index: 0,
            ranges: vec![SavedSelectionRange {
                capture_id: 2,
                start: 1,
                end: 4,
            }],
        },
    };
    assert_eq!(
        LibraryResponse::from_json(&response.to_json().unwrap()).unwrap(),
        response
    );
    let mut bad = response.clone();
    if let LibraryResult::SavedSelections { ranges, .. } = &mut bad.result {
        ranges[0].end = ranges[0].start;
    }
    assert_eq!(bad.validate().unwrap_err().code, ErrorCode::InvalidRequest);

    let mut accepted = response.clone();
    if let LibraryResult::SavedSelections { ranges, .. } = &mut accepted.result {
        *ranges = (0..MAX_LIBRARY_SAVED_SELECTIONS)
            .map(|start| SavedSelectionRange {
                capture_id: start as i64 + 1,
                start: start as u32,
                end: start as u32 + 1,
            })
            .collect();
    }
    accepted.validate().unwrap();
    if let LibraryResult::SavedSelections { ranges, .. } = &mut accepted.result {
        ranges.push(SavedSelectionRange {
            capture_id: 65,
            start: 64,
            end: 65,
        });
    }
    assert_eq!(
        accepted.validate().unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn capture_note_accepts_newlines_and_rejects_controls_and_oversize() {
    let capture = |note: String| {
        request(LibraryOperation::CaptureSelection {
            document_id: 1,
            page_index: 0,
            start: 0,
            end: 5,
            note,
        })
    };
    assert!(capture("first line\nsecond line".into()).validate().is_ok());
    for note in [
        "\t".into(),
        "\r".into(),
        "x".repeat(MAX_LIBRARY_NOTE_BYTES + 1),
    ] {
        assert_eq!(
            capture(note).validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }
}

#[test]
fn requests_reject_wrong_versions_budgets_and_cursors() {
    let mut wrong_protocol = request(LibraryOperation::Page {
        cursor: None,
        limit: 1,
    });
    wrong_protocol.library_protocol = 2;
    assert_eq!(
        wrong_protocol.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    let mut wrong_api = request(LibraryOperation::Page {
        cursor: None,
        limit: 1,
    });
    wrong_api.api_version = HOST_API_V0_5;
    assert_eq!(
        wrong_api.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    for limit in [0, MAX_LIBRARY_PAGE + 1] {
        assert_eq!(
            request(LibraryOperation::Page {
                cursor: None,
                limit
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for max_bytes in [0, MIN_LIBRARY_TEXT_CHUNK - 1, MAX_LIBRARY_TEXT_CHUNK + 1] {
        assert_eq!(
            request(LibraryOperation::PageText {
                document_id: 1,
                page_index: 0,
                offset: 0,
                max_bytes,
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for page_index in [MAX_LIBRARY_PAGES, MAX_LIBRARY_PAGES + 1] {
        assert_eq!(
            request(LibraryOperation::PageText {
                document_id: 1,
                page_index,
                offset: 0,
                max_bytes: 4,
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for (start, end) in [(2, 2), (2, 1), (0, MAX_LIBRARY_SELECTION_SCALARS + 1)] {
        assert!(
            request(LibraryOperation::CaptureSelection {
                document_id: 1,
                page_index: 0,
                start,
                end,
                note: String::new(),
            })
            .validate()
            .is_err()
        );
    }
    for cursor in [
        "".to_owned(),
        "with space".to_owned(),
        "l1:\u{0}:1".to_owned(),
        "x".repeat(MAX_LIBRARY_CURSOR_BYTES + 1),
    ] {
        assert_eq!(
            request(LibraryOperation::Page {
                cursor: Some(cursor),
                limit: 1
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
    }
}

#[test]
fn unknown_fields_and_forged_vault_are_rejected() {
    let request = r#"{"library_protocol":1,"api_version":{"major":0,"minor":6},"request_id":"r","operation":{"kind":"page","cursor":null,"limit":1,"vault":"forged"}}"#;
    assert_eq!(
        LibraryRequest::from_json(request).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let response = r#"{"request_id":"r","result":{"kind":"error","error":{"code":"not_found","message":"missing"},"vault":"forged"}}"#;
    assert_eq!(
        LibraryResponse::from_json(response).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn responses_validate_documents_cursors_and_budgets() {
    let response = page(vec![document(1)], Some("l1:2:1"));
    assert_eq!(
        LibraryResponse::from_json(&response.to_json().unwrap()).unwrap(),
        response
    );
    assert_eq!(
        page(vec![document(1); usize::from(MAX_LIBRARY_PAGE) + 1], None)
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );

    let mut oversized_title = document(1);
    oversized_title.title = "t".repeat(MAX_LIBRARY_TITLE_BYTES + 1);
    assert_eq!(
        page(vec![oversized_title], None)
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    let mut bad_digest = document(1);
    bad_digest.original_sha256 = "A".repeat(64);
    assert_eq!(
        page(vec![bad_digest], None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut bad_bytes = document(1);
    bad_bytes.original_bytes = MAX_LIBRARY_ORIGINAL_BYTES + 1;
    assert_eq!(
        page(vec![bad_bytes], None).validate().unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
    let mut bad_pages = document(1);
    bad_pages.page_count = 0;
    assert_eq!(
        page(vec![bad_pages], None).validate().unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
    assert_eq!(
        page(vec![document(1)], Some("bad cursor"))
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    let text = LibraryResponse {
        request_id: "request".into(),
        result: LibraryResult::PageText {
            document_id: 1,
            page_index: 0,
            offset: 0,
            total_bytes: 5,
            text: "hello".into(),
            next_offset: None,
        },
    };
    assert!(text.validate().is_ok());
    let mut bad_text = text.clone();
    if let LibraryResult::PageText { text, .. } = &mut bad_text.result {
        *text = "hello!".into();
    }
    assert_eq!(
        bad_text.validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let capture = LibraryResponse {
        request_id: "request".into(),
        result: LibraryResult::Capture {
            capture_id: 1,
            commit_rev: 2,
        },
    };
    assert!(capture.validate().is_ok());
}

#[test]
fn library_vocabulary_supports_api_0_6_and_0_7() {
    assert!(is_supported_api(HOST_API_V0_6));
    assert!(is_supported_api(HOST_API_V0_7));
    assert!(SUPPORTED_HOST_APIS.contains(&HOST_API_V0_6));
    assert!(SUPPORTED_HOST_APIS.contains(&HOST_API_V0_7));
    assert!(capability_available_in(
        Capability::LibraryRead,
        HOST_API_V0_6
    ));
    assert!(capability_available_in(
        Capability::LibraryRead,
        HOST_API_V0_7
    ));
    for version in SUPPORTED_HOST_APIS
        .into_iter()
        .filter(|version| *version < HOST_API_V0_6)
    {
        assert!(!capability_available_in(Capability::LibraryRead, version));
    }
    assert!(manifest(6).supports_api(HOST_API_V0_6));
    assert!(manifest(7).supports_api(HOST_API_V0_7));
    let mut not_07 = manifest(7);
    not_07.host_api.max_minor = 6;
    assert!(!not_07.supports_api(HOST_API_V0_7));
    assert_eq!(
        manifest(7).negotiate_api(&[HOST_API_V0_5, HOST_API_V0_6, HOST_API_V0_7]),
        Some(HOST_API_V0_7)
    );
}

#[test]
fn manifests_require_the_matching_library_vocabulary() {
    let valid = manifest(7);
    valid.validate().unwrap();

    let wrong_version = manifest(5);
    assert_eq!(
        wrong_version.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    let mut missing_declaration = manifest(7);
    missing_declaration.required_capabilities.clear();
    missing_declaration.contributions[0]
        .required_capabilities
        .clear();
    assert_eq!(
        missing_declaration.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
    let mut wrong_contribution = manifest(7);
    wrong_contribution.contributions[0]
        .required_capabilities
        .clear();
    assert_eq!(
        wrong_contribution.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}
