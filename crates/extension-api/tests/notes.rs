use extension_api::{notes::*, *};

fn request(operation: NotesOperation) -> NotesRequest {
    NotesRequest {
        notes_protocol: NOTES_PROTOCOL,
        api_version: HOST_API_V0_4,
        request_id: "request".into(),
        operation,
    }
}

fn summary(id: i64) -> NoteSummary {
    NoteSummary {
        capture_id: id,
        commit_rev: id,
        document_id: 1,
        document_title: "Source".into(),
        page_index: 2,
        quote_preview: "hello".into(),
        quote_bytes: 5,
        quote_truncated: false,
        note_preview: "thought".into(),
        note_bytes: 7,
        note_truncated: false,
        compound: false,
    }
}

fn page(notes: Vec<NoteSummary>, next: Option<&str>) -> NotesResponse {
    NotesResponse {
        request_id: "request".into(),
        result: NotesResult::Page {
            notes,
            next_cursor: next.map(str::to_owned),
        },
    }
}

fn body(text: &str, offset: u32, total: u64, next: Option<u32>) -> NotesResponse {
    NotesResponse {
        request_id: "request".into(),
        result: NotesResult::Body(NoteBody {
            capture_id: 1,
            commit_rev: 2,
            document_id: 1,
            field: NoteField::Note,
            offset,
            total_bytes: total,
            text: text.into(),
            next_offset: next,
        }),
    }
}

fn notes_manifest(min_minor: u16) -> Manifest {
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
            min_minor,
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

#[test]
fn requests_round_trip_with_canonical_bounds() {
    for operation in [
        NotesOperation::Page {
            cursor: None,
            limit: MAX_NOTE_PAGE,
        },
        NotesOperation::Page {
            cursor: Some("n1:*:7:3".into()),
            limit: 1,
        },
        NotesOperation::Body {
            capture_id: 3,
            field: NoteField::Quote,
            offset: 0,
            max_bytes: MAX_NOTE_CHUNK_BYTES,
        },
    ] {
        let request = request(operation);
        let json = request.to_json().unwrap();
        assert_eq!(NotesRequest::from_json(&json).unwrap(), request);
    }
}

#[test]
fn requests_reject_wrong_protocol_version_and_budgets() {
    let mut wrong_protocol = request(NotesOperation::Page {
        cursor: None,
        limit: 1,
    });
    wrong_protocol.notes_protocol = 2;
    assert_eq!(
        wrong_protocol.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    let mut wrong_api = request(NotesOperation::Page {
        cursor: None,
        limit: 1,
    });
    wrong_api.api_version = HOST_API_V0_3;
    assert_eq!(
        wrong_api.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    for limit in [0, MAX_NOTE_PAGE + 1] {
        assert_eq!(
            request(NotesOperation::Page {
                cursor: None,
                limit
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for cursor in [
        "".to_owned(),
        "with space".to_owned(),
        "n1:\u{0}:1:2".to_owned(),
        "x".repeat(MAX_NOTE_CURSOR_BYTES + 1),
    ] {
        assert_eq!(
            request(NotesOperation::Page {
                cursor: Some(cursor),
                limit: 1
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
    }

    for max_bytes in [0, MIN_NOTE_CHUNK_BYTES - 1, MAX_NOTE_CHUNK_BYTES + 1] {
        assert_eq!(
            request(NotesOperation::Body {
                capture_id: 1,
                field: NoteField::Note,
                offset: 0,
                max_bytes,
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for (capture_id, offset) in [(0, 0), (-1, 0), (1, MAX_NOTE_OFFSET + 1)] {
        assert_eq!(
            request(NotesOperation::Body {
                capture_id,
                field: NoteField::Note,
                offset,
                max_bytes: MIN_NOTE_CHUNK_BYTES,
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
    }
}

#[test]
fn unknown_request_and_response_fields_are_rejected() {
    let json = r#"{"notes_protocol":1,"api_version":{"major":0,"minor":4},"request_id":"r","operation":{"kind":"page","cursor":null,"limit":1,"vault":"forged"}}"#;
    assert_eq!(
        NotesRequest::from_json(json).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let response = r#"{"request_id":"r","result":{"kind":"error","error":{"code":"not_found","message":"missing"},"vault":"forged"}}"#;
    assert_eq!(
        NotesResponse::from_json(response).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn responses_validate_previews_cursors_and_chunks() {
    let response = page(vec![summary(1)], Some("n1:*:2:1"));
    assert_eq!(
        NotesResponse::from_json(&response.to_json().unwrap()).unwrap(),
        response
    );

    assert_eq!(
        page(vec![summary(1); usize::from(MAX_NOTE_PAGE) + 1], None)
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::BudgetExceeded
    );

    let mut oversized = summary(1);
    oversized.quote_preview = "q".repeat(MAX_NOTE_PREVIEW_BYTES + 1);
    assert_eq!(
        page(vec![oversized], None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let mut inconsistent = summary(1);
    inconsistent.note_truncated = true;
    assert_eq!(
        page(vec![inconsistent], None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let mut control = summary(1);
    control.document_title = "bad\n".into();
    assert_eq!(
        page(vec![control], None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    assert_eq!(
        page(vec![summary(1)], Some("bad cursor"))
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );

    assert_eq!(
        body("abc", 0, 5, None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        body("abc", 2, 6, None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        body("abc", 0, 3, Some(3)).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        body(
            &"x".repeat(usize::from(MAX_NOTE_CHUNK_BYTES) + 1),
            0,
            4096,
            None
        )
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::BudgetExceeded
    );

    let mut control = summary(1);
    control.note_preview = "bad\u{0}".into();
    assert_eq!(
        page(vec![control], None).validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let first = body("abc", 0, 6, Some(3));
    assert!(first.validate().is_ok());
    let last = body("def", 3, 6, None);
    assert!(last.validate().is_ok());
    let empty_end = body("", 6, 6, None);
    assert!(empty_end.validate().is_ok());
}

#[test]
fn a_full_page_stays_inside_the_public_wire_budget() {
    let mut notes = Vec::new();
    for id in 1..=i64::from(MAX_NOTE_PAGE) {
        let mut note = summary(id);
        note.document_title = "t".repeat(MAX_NOTE_TITLE_BYTES);
        note.quote_preview = "q".repeat(MAX_NOTE_PREVIEW_BYTES);
        note.quote_bytes = MAX_NOTE_PREVIEW_BYTES as u64;
        note.note_preview = "n".repeat(MAX_NOTE_PREVIEW_BYTES);
        note.note_bytes = MAX_NOTE_PREVIEW_BYTES as u64;
        notes.push(note);
    }
    let json = page(notes, Some("n1:*:9:8")).to_json().unwrap();
    assert!(json.len() <= MAX_REQUEST_BYTES);
    assert!(NotesResponse::from_json(&json).is_ok());

    // Control characters inflate JSON escaping: the adapter must shrink the
    // chunk until the encoded response fits, and the wire rejects the oversize.
    let escaped = body(
        &"\u{0}".repeat(usize::from(MAX_NOTE_CHUNK_BYTES)),
        0,
        2048,
        None,
    );
    assert_eq!(
        escaped.to_json().unwrap_err().code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn notes_vocabulary_is_api_0_4_only() {
    assert!(capability_available_in(
        Capability::NotesRead,
        HOST_API_V0_4
    ));
    assert!(!capability_available_in(
        Capability::NotesRead,
        HOST_API_V0_3
    ));
    assert!(!capability_available_in(
        Capability::ReaderSurface,
        HOST_API_V0_4
    ));
    assert!(capability_resource_valid(
        Capability::NotesRead,
        &Resource::Vault {
            vault_id: "vault".into()
        },
        HOST_API_V0_4
    ));
    assert!(!capability_resource_valid(
        Capability::NotesRead,
        &Resource::Document {
            vault_id: "vault".into(),
            document_id: "document".into()
        },
        HOST_API_V0_4
    ));

    let manifest = notes_manifest(4);
    manifest.validate().unwrap();
    assert!(manifest.supports_api(HOST_API_V0_4));
    assert_eq!(
        manifest.negotiate_api(&SUPPORTED_HOST_APIS),
        Some(HOST_API_V0_4)
    );
    assert!(is_supported_api(HOST_API_V0_4));

    let too_old = notes_manifest(2);
    assert_eq!(
        too_old.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    let mut workspace_without_capability = notes_manifest(4);
    workspace_without_capability.contributions[0]
        .required_capabilities
        .clear();
    assert_eq!(
        workspace_without_capability.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    let request = CapabilityRequest {
        api_version: HOST_API_V0_4,
        request_id: "request".into(),
        capability: Capability::NotesRead,
        resource: Resource::Vault {
            vault_id: "vault".into(),
        },
    };
    request.validate().unwrap();
    let mut old = request.clone();
    old.api_version = HOST_API_V0_3;
    assert_eq!(
        old.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );
}
