use extension_api::{
    ApiRange, ApiVersion, ErrorCode, HOST_API_V0_7, HOST_API_V0_8, MAX_ID_BYTES, Manifest,
    SUPPORTED_HOST_APIS,
    notes::NoteField,
    orthogonal::{
        CAP_CORE_CAPTURE_PROPOSE, CAP_CORE_KNOWLEDGE_READ, CAP_CORE_KNOWLEDGE_WRITE,
        CAP_CORE_VOCAB_STUDY, CAP_HOST_STORAGE_KV, CAP_HOST_UI_SLOT, CAP_NATIVE_DOCUMENT_RENDER,
        CORE_KNOWLEDGE_PROTOCOL, CapabilityId, CapabilityRequirement, CoreKnowledgeOperation,
        CoreKnowledgeRequest, CoreKnowledgeResponse, CoreKnowledgeResult, DocumentFormat,
        KnowledgeDocumentSummary, KnowledgePdfMetadata, KnowledgeTocEntry,
        MAX_KNOWLEDGE_NOTE_BYTES, MAX_KNOWLEDGE_PAGE, MAX_KNOWLEDGE_QUOTE_BYTES, ServiceEndpoint,
    },
};
use serde_json::json;

#[test]
fn capability_id_validates_syntax_and_fails_closed_on_unknown_namespaces() {
    for standard in [
        CAP_CORE_KNOWLEDGE_READ,
        CAP_CORE_KNOWLEDGE_WRITE,
        CAP_CORE_CAPTURE_PROPOSE,
        CAP_CORE_VOCAB_STUDY,
        CAP_HOST_UI_SLOT,
        CAP_HOST_STORAGE_KV,
        CAP_NATIVE_DOCUMENT_RENDER,
    ] {
        let id = CapabilityId::parse(standard).unwrap();
        assert!(id.is_standard());
        assert!(!id.is_extension());
        assert_eq!(id, standard);
    }

    for ext in [
        "ext.notes.search",
        "ext.org_example.ai_summary-v2",
        "ext.com.example.sync.feed",
    ] {
        let id = CapabilityId::parse(ext).unwrap();
        assert!(!id.is_standard());
        assert!(id.is_extension());
    }

    let max_ext = format!("ext.p.{}", "a".repeat(MAX_ID_BYTES - 6));
    assert_eq!(max_ext.len(), MAX_ID_BYTES);
    assert!(CapabilityId::parse(&max_ext).is_ok());

    let too_long = format!("{max_ext}a");
    for bad in [
        "",
        &too_long,
        "Core.knowledge.read",
        "core.knowledge.read ",
        ".core.knowledge.read",
        "core.knowledge.read.",
        "-ext.a.b",
        "ext.a.b_",
        "ext.a..b",
        "ext.a--b.c",
        "ext.a__b.c",
        "ext.a.-b",
        "ext.a._b",
        "ext.0a.b",
        "ext.onlytwo",
        "core.unknown.read",
        "host.secret.exec",
        "ai.magic.run",
        "notes.read",
        "ext.a/b.c",
    ] {
        assert_eq!(
            CapabilityId::parse(bad).unwrap_err().code,
            ErrorCode::InvalidManifest,
            "unexpectedly accepted {bad:?}"
        );
        let quoted = serde_json::to_string(bad).unwrap();
        assert!(serde_json::from_str::<CapabilityId>(&quoted).is_err());
    }
}

#[test]
fn v1_manifests_preserve_json_and_never_project_write_from_readonly_caps() {
    let raw_v1 = r#"{
      "manifest_version": 1,
      "package_id": "com.qusor.readonly",
      "publisher_id": "com.qusor",
      "package_version": {"major": 1, "minor": 0, "patch": 0},
      "host_api": {"major": 0, "min_minor": 6, "max_minor": 8},
      "required_capabilities": [
        "selection.read",
        "document.metadata.read",
        "notes.read",
        "library.read"
      ],
      "optional_capabilities": ["vocab.read"],
      "contributions": []
    }"#;
    let parsed = Manifest::from_json(raw_v1).unwrap();
    assert_eq!(parsed.manifest_version, 1);
    assert!(parsed.orthogonal_capabilities.is_empty());
    assert!(parsed.provides.is_empty());
    assert!(parsed.requires.is_empty());

    let reencoded = parsed.to_json().unwrap();
    assert!(!reencoded.contains("orthogonal_capabilities"));
    assert!(!reencoded.contains("provides"));
    assert!(!reencoded.contains("requires"));
    assert_eq!(Manifest::from_json(&reencoded).unwrap(), parsed);

    let effective = parsed.effective_orthogonal_requirements();
    assert_eq!(effective.len(), 2);
    assert!(
        effective
            .iter()
            .any(|req| req.id == CAP_CORE_KNOWLEDGE_READ && !req.optional)
    );
    assert!(
        effective
            .iter()
            .any(|req| req.id == CAP_CORE_VOCAB_STUDY && req.optional)
    );
    assert!(
        !effective
            .iter()
            .any(|req| req.id == CAP_CORE_KNOWLEDGE_WRITE)
    );

    // v1 manifest must reject v2 orthogonal fields if someone injects them.
    let mut bad_v1 = serde_json::from_str::<serde_json::Value>(raw_v1).unwrap();
    bad_v1["orthogonal_capabilities"] = json!([{
        "id": "core.knowledge.read",
        "major": 1,
        "min_minor": 0
    }]);
    assert_eq!(
        Manifest::from_json(&bad_v1.to_string()).unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}

#[test]
fn v2_manifest_validates_deduplicates_and_rejects_invalid_service_seams() {
    let raw_v2 = r#"{
      "manifest_version": 2,
      "package_id": "org.example.assistant",
      "publisher_id": "org.example",
      "package_version": {"major": 2, "minor": 1, "patch": 0},
      "host_api": {"major": 0, "min_minor": 8, "max_minor": 8},
      "required_capabilities": ["notes.read"],
      "optional_capabilities": [],
      "contributions": [],
      "orthogonal_capabilities": [
        {"id": "core.knowledge.read", "major": 1, "min_minor": 1, "max_minor": 3, "optional": false},
        {"id": "host.storage.kv", "major": 1, "min_minor": 0, "optional": true}
      ],
      "provides": [
        {"service": "ext.example.summarize", "major": 1, "minor": 2}
      ],
      "requires": [
        {"id": "ext.notes.index", "major": 1, "min_minor": 0, "max_minor": 2, "optional": false}
      ]
    }"#;
    let parsed = Manifest::from_json(raw_v2).unwrap();
    assert!(!parsed.supports_api(HOST_API_V0_7));
    assert!(parsed.supports_api(HOST_API_V0_8));
    assert_eq!(
        parsed.negotiate_api(&SUPPORTED_HOST_APIS),
        Some(HOST_API_V0_8)
    );

    let effective = parsed.effective_orthogonal_requirements();
    assert_eq!(effective.len(), 3);
    let read_req = effective
        .iter()
        .find(|req| req.id == CAP_CORE_KNOWLEDGE_READ)
        .unwrap();
    assert_eq!(read_req.major, 1);
    assert_eq!(read_req.min_minor, 1);
    assert_eq!(read_req.max_minor, Some(3));
    assert!(!read_req.optional);

    // Cannot provide standard host/core capabilities.
    let mut hijack = parsed.clone();
    hijack.provides = vec![ServiceEndpoint {
        service: CapabilityId::parse(CAP_CORE_KNOWLEDGE_READ).unwrap(),
        major: 1,
        minor: 0,
    }];
    assert_eq!(
        hijack.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    // Cannot require a service it provides (direct self-loop).
    let mut self_loop = parsed.clone();
    self_loop.requires.push(CapabilityRequirement {
        id: CapabilityId::parse("ext.example.summarize").unwrap(),
        major: 1,
        min_minor: 0,
        max_minor: None,
        optional: false,
    });
    assert_eq!(
        self_loop.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    // Inverted requirement range fails closed.
    let mut inverted = parsed.clone();
    inverted.orthogonal_capabilities[0].min_minor = 4;
    assert_eq!(
        inverted.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );

    // v2 requires host_api min_minor >= 8.
    let mut low_api = parsed;
    low_api.host_api = ApiRange {
        major: 0,
        min_minor: 7,
        max_minor: 8,
    };
    assert_eq!(
        low_api.validate().unwrap_err().code,
        ErrorCode::InvalidManifest
    );
}

#[test]
fn core_knowledge_wire_enforces_capabilities_bounds_and_provenance_isolation() {
    let read_ops = [
        CoreKnowledgeOperation::DocumentsPage {
            format: Some(DocumentFormat::Epub),
            cursor: None,
            limit: MAX_KNOWLEDGE_PAGE,
        },
        CoreKnowledgeOperation::DocumentToc {
            format: DocumentFormat::Epub,
            document_id: 1,
        },
        CoreKnowledgeOperation::DocumentChunk {
            format: DocumentFormat::Pdf,
            document_id: 1,
            section_index: 0,
            offset: 0,
            max_bytes: 512,
        },
        CoreKnowledgeOperation::SavedSelections {
            format: DocumentFormat::Epub,
            document_id: 1,
            section_index: 2,
        },
        CoreKnowledgeOperation::NotesPage {
            format: None,
            document_id: Some(1),
            cursor: Some("c1".into()),
            limit: 4,
        },
        CoreKnowledgeOperation::NoteBody {
            format: DocumentFormat::Pdf,
            capture_id: 9,
            field: NoteField::Note,
            offset: 0,
            max_bytes: 256,
        },
        CoreKnowledgeOperation::PdfMetadata { document_id: 3 },
    ];
    for op in read_ops {
        assert_eq!(op.required_capability(), CAP_CORE_KNOWLEDGE_READ);
        assert!(!op.is_write());
        let req = CoreKnowledgeRequest {
            protocol_version: CORE_KNOWLEDGE_PROTOCOL,
            api_version: HOST_API_V0_8,
            request_id: "req-read".into(),
            operation: op,
        };
        let encoded = req.to_json().unwrap();
        assert_eq!(CoreKnowledgeRequest::from_json(&encoded).unwrap(), req);
    }

    let write_ops = [
        CoreKnowledgeOperation::UpdateCaptureNote {
            format: DocumentFormat::Epub,
            capture_id: 5,
            note: "updated".into(),
        },
        CoreKnowledgeOperation::DeleteCapture {
            format: DocumentFormat::Pdf,
            capture_id: 5,
        },
        CoreKnowledgeOperation::DeleteDocument {
            format: DocumentFormat::Epub,
            document_id: 5,
        },
    ];
    for op in write_ops {
        assert_eq!(op.required_capability(), CAP_CORE_KNOWLEDGE_WRITE);
        assert!(op.is_write());
        let req = CoreKnowledgeRequest {
            protocol_version: CORE_KNOWLEDGE_PROTOCOL,
            api_version: HOST_API_V0_8,
            request_id: "req-write".into(),
            operation: op,
        };
        assert!(req.validate().is_ok());
    }

    let capture_req = CoreKnowledgeRequest {
        protocol_version: CORE_KNOWLEDGE_PROTOCOL,
        api_version: HOST_API_V0_8,
        request_id: "req-cap".into(),
        operation: CoreKnowledgeOperation::CaptureEpubSelection {
            operation_id: "op-1".into(),
            document_id: 7,
            chapter_index: 3,
            start: 10,
            end: 24,
            quote: "selected text".into(),
            note: "my note".into(),
        },
    };
    assert_eq!(
        capture_req.operation.required_capability(),
        CAP_CORE_CAPTURE_PROPOSE
    );
    assert!(!capture_req.operation.is_write());
    let json_text = capture_req.to_json().unwrap();
    assert_eq!(
        CoreKnowledgeRequest::from_json(&json_text).unwrap(),
        capture_req
    );

    // Caller cannot inject provenance or installation identity into CaptureEpubSelection.
    let mut forged = serde_json::from_str::<serde_json::Value>(&json_text).unwrap();
    for field in [
        "provenance",
        "installation_id",
        "package_id",
        "publisher_id",
        "vault_id",
    ] {
        let mut attempt = forged.clone();
        attempt["operation"][field] = json!("spoofed");
        assert_eq!(
            CoreKnowledgeRequest::from_json(&attempt.to_string())
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    forged["api_version"] = json!({"major": 0, "minor": 7});
    assert_eq!(
        CoreKnowledgeRequest::from_json(&forged.to_string())
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedVersion
    );

    // Oversized quote/note fail closed.
    let mut bad_quote = capture_req.clone();
    bad_quote.operation = CoreKnowledgeOperation::CaptureEpubSelection {
        operation_id: "op-2".into(),
        document_id: 7,
        chapter_index: 0,
        start: 1,
        end: 2,
        quote: "q".repeat(MAX_KNOWLEDGE_QUOTE_BYTES + 1),
        note: String::new(),
    };
    assert_eq!(
        bad_quote.validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let bad_note = CoreKnowledgeRequest {
        protocol_version: CORE_KNOWLEDGE_PROTOCOL,
        api_version: HOST_API_V0_8,
        request_id: "req-note".into(),
        operation: CoreKnowledgeOperation::UpdateCaptureNote {
            format: DocumentFormat::Epub,
            capture_id: 1,
            note: "n".repeat(MAX_KNOWLEDGE_NOTE_BYTES + 1),
        },
    };
    assert_eq!(
        bad_note.validate().unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    // Response round-trips cleanly.
    for result in [
        CoreKnowledgeResult::DocumentsPage {
            documents: vec![KnowledgeDocumentSummary {
                document_id: 1,
                format: DocumentFormat::Epub,
                commit_rev: 2,
                title: "Book".into(),
                section_count: 12,
            }],
            next_cursor: None,
        },
        CoreKnowledgeResult::DocumentToc {
            format: DocumentFormat::Epub,
            document_id: 1,
            entries: vec![KnowledgeTocEntry {
                title: "Chapter 1".into(),
                section_index: 0,
                depth: 0,
            }],
        },
        CoreKnowledgeResult::PdfMetadata(KnowledgePdfMetadata {
            document_id: 1,
            commit_rev: 3,
            title: "Paper".into(),
            original_sha256: "a".repeat(64),
            original_bytes: 1024,
            extraction_revision: "b".repeat(64),
            page_count: 10,
        }),
    ] {
        let resp = CoreKnowledgeResponse {
            request_id: "resp-1".into(),
            result,
        };
        let encoded = resp.to_json().unwrap();
        assert_eq!(CoreKnowledgeResponse::from_json(&encoded).unwrap(), resp);
    }
    let _ = ApiVersion { major: 0, minor: 8 };
}
