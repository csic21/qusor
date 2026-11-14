use extension_api::{vocab::*, *};

fn request(operation: VocabOperation) -> VocabRequest {
    VocabRequest {
        vocab_protocol: VOCAB_PROTOCOL,
        api_version: HOST_API_V0_5,
        request_id: "request".into(),
        operation,
    }
}

fn deck(id: i64) -> VocabDeck {
    VocabDeck {
        wordbook_id: id,
        title: "CET-4".into(),
        total: 10,
        due: 3,
        new: 2,
    }
}

fn card(id: i64) -> VocabCardView {
    VocabCardView {
        card_id: id,
        term: "abandon".into(),
        meaning: "放弃".into(),
        example: "Do not abandon the plan.".into(),
        stage: VocabStage::New,
    }
}

fn outcome() -> VocabReviewOutcome {
    VocabReviewOutcome {
        card_id: 1,
        stage: VocabStage::Review,
        due_at_ms: 1_767_484_800_000,
        scheduled_minutes: 3 * 24 * 60,
        remaining_due: 2,
        remaining_new: 8,
    }
}

fn response(result: VocabResult) -> VocabResponse {
    VocabResponse {
        request_id: "request".into(),
        result,
    }
}

fn manifest(min_minor: u16, max_minor: u16) -> Manifest {
    Manifest {
        manifest_version: 1,
        package_id: "org.test.vocab".into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor,
            max_minor,
        },
        required_capabilities: vec![Capability::VocabRead, Capability::VocabReview],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.test.vocab.workspace".into(),
            title: "Vocabulary".into(),
            kind: ContributionKind::VocabWorkspace,
            required_capabilities: vec![Capability::VocabRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    }
}

#[test]
fn requests_round_trip_with_canonical_bounds() {
    for operation in [
        VocabOperation::Decks {},
        VocabOperation::Queue {
            wordbook_id: 1,
            limit: MAX_VOCAB_PAGE,
        },
        VocabOperation::Review {
            card_id: 2,
            rating: VocabRating::Good,
        },
    ] {
        let request = request(operation);
        let json = request.to_json().unwrap();
        assert_eq!(VocabRequest::from_json(&json).unwrap(), request);
    }
}

#[test]
fn requests_reject_wrong_protocol_versions_and_ids() {
    let mut wrong_protocol = request(VocabOperation::Decks {});
    wrong_protocol.vocab_protocol = 2;
    assert_eq!(
        wrong_protocol.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    let mut wrong_api = request(VocabOperation::Decks {});
    wrong_api.api_version = HOST_API_V0_4;
    assert_eq!(
        wrong_api.validate().unwrap_err().code,
        ErrorCode::UnsupportedVersion
    );

    for limit in [0, MAX_VOCAB_PAGE + 1] {
        assert_eq!(
            request(VocabOperation::Queue {
                wordbook_id: 1,
                limit
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::BudgetExceeded
        );
    }
    for wordbook_id in [0, -1] {
        assert_eq!(
            request(VocabOperation::Queue {
                wordbook_id,
                limit: 1
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
    }
    for card_id in [0, -1] {
        assert_eq!(
            request(VocabOperation::Review {
                card_id,
                rating: VocabRating::Hard
            })
            .validate()
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
    }

    let mut bad_request_id = request(VocabOperation::Decks {});
    bad_request_id.request_id = "bad id".into();
    assert_eq!(
        bad_request_id.validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn unknown_request_and_response_fields_are_rejected() {
    let forged_request = r#"{"vocab_protocol":1,"api_version":{"major":0,"minor":5},"request_id":"r","operation":{"kind":"decks","vault":"forged"}}"#;
    assert_eq!(
        VocabRequest::from_json(forged_request).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let forged_response = r#"{"request_id":"r","result":{"kind":"review","card_id":1,"stage":"review","due_at_ms":1,"scheduled_minutes":1,"remaining_due":0,"remaining_new":0,"vault":"forged"}}"#;
    assert_eq!(
        VocabResponse::from_json(forged_response).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn responses_validate_budgets_counters_and_text() {
    let decks = response(VocabResult::Decks {
        decks: vec![deck(1)],
    });
    assert_eq!(
        VocabResponse::from_json(&decks.to_json().unwrap()).unwrap(),
        decks
    );

    assert_eq!(
        response(VocabResult::Decks {
            decks: vec![deck(1); MAX_VOCAB_DECKS + 1],
        })
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::BudgetExceeded
    );

    assert_eq!(
        response(VocabResult::Queue {
            cards: vec![card(1); usize::from(MAX_VOCAB_PAGE) + 1],
            remaining_due: 0,
            remaining_new: 0,
        })
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::BudgetExceeded
    );

    let mut inconsistent = deck(1);
    inconsistent.due = inconsistent.total + 1;
    assert_eq!(
        response(VocabResult::Decks {
            decks: vec![inconsistent]
        })
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::InvalidRequest
    );

    let mut long_term = card(1);
    long_term.term = "t".repeat(MAX_TERM_BYTES + 1);
    assert_eq!(
        response(VocabResult::Queue {
            cards: vec![long_term],
            remaining_due: 0,
            remaining_new: 0,
        })
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::InvalidRequest
    );

    let mut control = card(1);
    control.meaning = "bad\u{0}".into();
    assert_eq!(
        response(VocabResult::Queue {
            cards: vec![control],
            remaining_due: 0,
            remaining_new: 0,
        })
        .validate()
        .unwrap_err()
        .code,
        ErrorCode::InvalidRequest
    );

    let mut new_stage = outcome();
    new_stage.stage = VocabStage::New;
    assert_eq!(
        response(VocabResult::Review(new_stage))
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );

    let mut zero_interval = outcome();
    zero_interval.scheduled_minutes = 0;
    assert_eq!(
        response(VocabResult::Review(zero_interval))
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );

    let error = response(VocabResult::Error {
        error: ApiError::new(ErrorCode::NotFound, "missing"),
    });
    assert_eq!(
        VocabResponse::from_json(&error.to_json().unwrap()).unwrap(),
        error
    );
}

#[test]
fn a_full_page_and_escaped_text_stay_inside_the_public_wire_budget() {
    let mut cards = Vec::new();
    for id in 1..=i64::from(MAX_VOCAB_PAGE) {
        let mut entry = card(id);
        entry.term = "t".repeat(MAX_TERM_BYTES);
        entry.meaning = "m".repeat(MAX_MEANING_BYTES);
        entry.example = "e".repeat(MAX_EXAMPLE_BYTES);
        cards.push(entry);
    }
    let json = response(VocabResult::Queue {
        cards,
        remaining_due: 3,
        remaining_new: 7,
    })
    .to_json()
    .unwrap();
    assert!(json.len() <= MAX_REQUEST_BYTES);
    assert!(VocabResponse::from_json(&json).is_ok());

    // Quotes and newlines inflate JSON escaping; the wire rejects the oversize page.
    let mut escaped = Vec::new();
    for id in 1..=i64::from(MAX_VOCAB_PAGE) {
        escaped.push(VocabCardView {
            card_id: id,
            term: "\"".repeat(MAX_TERM_BYTES),
            meaning: "\"".repeat(MAX_MEANING_BYTES),
            example: "\n".repeat(MAX_EXAMPLE_BYTES),
            stage: VocabStage::Review,
        });
    }
    assert_eq!(
        response(VocabResult::Queue {
            cards: escaped,
            remaining_due: 0,
            remaining_new: 0,
        })
        .to_json()
        .unwrap_err()
        .code,
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn vocab_vocabulary_is_api_0_5_only() {
    assert!(is_supported_api(HOST_API_V0_5));
    assert!(SUPPORTED_HOST_APIS.contains(&HOST_API_V0_5));
    for capability in [Capability::VocabRead, Capability::VocabReview] {
        assert!(capability_available_in(capability, HOST_API_V0_5));
        assert!(!capability_available_in(capability, HOST_API_V0_4));
        assert!(capability_resource_valid(
            capability,
            &Resource::Vault {
                vault_id: "vault".into()
            },
            HOST_API_V0_5
        ));
        assert!(!capability_resource_valid(
            capability,
            &Resource::Document {
                vault_id: "vault".into(),
                document_id: "document".into()
            },
            HOST_API_V0_5
        ));
    }
    assert!(capability_available_in(
        Capability::NotesRead,
        HOST_API_V0_5
    ));
    assert!(!capability_available_in(
        Capability::LibraryRead,
        HOST_API_V0_5
    ));
}

#[test]
fn manifests_require_the_matching_vocab_vocabulary() {
    let valid = manifest(5, 5);
    assert!(valid.validate().is_ok());
    assert!(valid.supports_api(HOST_API_V0_5));
    assert!(!valid.supports_api(HOST_API_V0_4));
    assert_eq!(
        valid.negotiate_api(&[HOST_API_V0_4, HOST_API_V0_5]),
        Some(HOST_API_V0_5)
    );

    let mut too_old = manifest(4, 4);
    too_old.contributions.clear();
    assert_eq!(
        too_old.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    let mut non_vocab_on_0_5 = manifest(5, 5);
    non_vocab_on_0_5.required_capabilities = vec![Capability::NotesRead];
    non_vocab_on_0_5.contributions.clear();
    assert!(non_vocab_on_0_5.validate().is_ok());
    assert!(non_vocab_on_0_5.supports_api(HOST_API_V0_5));

    let mut workspace_without_read = manifest(5, 5);
    workspace_without_read.required_capabilities = vec![Capability::VocabReview];
    workspace_without_read.contributions[0].required_capabilities = vec![Capability::VocabReview];
    assert_eq!(
        workspace_without_read.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    let mut read_only = manifest(5, 5);
    read_only.required_capabilities = vec![Capability::VocabRead];
    read_only.optional_capabilities = vec![Capability::VocabReview];
    assert!(read_only.validate().is_ok());
    assert!(read_only.supports_api(HOST_API_V0_5));
}
