use extension_api::{
    ApiRange, ApiVersion, Capability, Contribution, ContributionKind, ErrorCode, HOST_API_V0_6,
    HOST_API_V0_7, HOST_API_V0_8, Manifest, PackageVersion, Resource, SUPPORTED_HOST_APIS,
    notes::{NOTES_PROTOCOL, NotesOperation, NotesRequest},
    orthogonal::{
        CAP_CORE_CAPTURE_PROPOSE, CAP_CORE_KNOWLEDGE_READ, CAP_CORE_KNOWLEDGE_WRITE,
        CAP_CORE_VOCAB_STUDY, CORE_KNOWLEDGE_PROTOCOL, CapabilityId, CapabilityRequirement,
        CoreKnowledgeOperation, CoreKnowledgeRequest, DocumentFormat, MAX_SERVICE_FRAME_BYTES,
        ServiceEndpoint,
    },
};
use extension_host::{
    Grant, Host, OrthogonalGrant,
    router::{ExtensionRouter, ServiceCallRequest},
};

fn digest(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

fn vault(id: &str) -> Resource {
    Resource::Vault {
        vault_id: id.into(),
    }
}

fn doc(vault_id: &str, document_id: &str) -> Resource {
    Resource::Document {
        vault_id: vault_id.into(),
        document_id: document_id.into(),
    }
}

fn cap(id: &str) -> CapabilityId {
    CapabilityId::parse(id).unwrap()
}

fn req(
    id: &str,
    major: u16,
    min_minor: u16,
    max_minor: Option<u16>,
    optional: bool,
) -> CapabilityRequirement {
    CapabilityRequirement {
        id: cap(id),
        major,
        min_minor,
        max_minor,
        optional,
    }
}

fn v2_manifest(
    package_id: &str,
    orthogonal_capabilities: Vec<CapabilityRequirement>,
    provides: Vec<ServiceEndpoint>,
    requires: Vec<CapabilityRequirement>,
) -> Manifest {
    Manifest {
        manifest_version: 2,
        package_id: package_id.into(),
        publisher_id: "org.test".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 8,
            max_minor: 8,
        },
        required_capabilities: vec![],
        optional_capabilities: vec![],
        contributions: vec![],
        orthogonal_capabilities,
        provides,
        requires,
    }
}

fn ck_request(id: &str, operation: CoreKnowledgeOperation) -> CoreKnowledgeRequest {
    CoreKnowledgeRequest {
        protocol_version: CORE_KNOWLEDGE_PROTOCOL,
        api_version: HOST_API_V0_8,
        request_id: id.into(),
        operation,
    }
}

#[test]
fn host_api_0_8_is_cumulative_and_dispatches_core_knowledge_with_doc_scope() {
    let mut host = Host::new(
        "vault-1",
        [
            Capability::NotesRead,
            Capability::LibraryRead,
            Capability::CapturePropose,
        ],
    )
    .unwrap();
    host.configure_orthogonal_support(cap(CAP_CORE_KNOWLEDGE_WRITE), 1, 1)
        .unwrap();

    // Legacy v1 notes package continues working cumulatively alongside v2 packages.
    let v1_notes = Manifest {
        manifest_version: 1,
        package_id: "org.legacy.notes".into(),
        publisher_id: "org.legacy".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 4,
            max_minor: 8,
        },
        required_capabilities: vec![Capability::NotesRead],
        optional_capabilities: vec![],
        contributions: vec![Contribution {
            id: "org.legacy.notes.view".into(),
            title: "Notes".into(),
            kind: ContributionKind::NotesWorkspace,
            required_capabilities: vec![Capability::NotesRead],
        }],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    };
    let legacy_id = host.register(v1_notes, digest('1')).unwrap();
    host.replace_grants(
        legacy_id,
        vec![Grant {
            capability: Capability::NotesRead,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(legacy_id).unwrap();
    let legacy_session = host
        .open_negotiated_session(legacy_id, &SUPPORTED_HOST_APIS)
        .unwrap();
    assert_eq!(host.session_api(&legacy_session).unwrap(), HOST_API_V0_8);
    let legacy_permit = host
        .authorize_notes(
            &legacy_session,
            &NotesRequest {
                notes_protocol: NOTES_PROTOCOL,
                api_version: HOST_API_V0_8,
                request_id: "legacy-req-1".into(),
                operation: NotesOperation::Page {
                    cursor: None,
                    limit: 5,
                },
            },
        )
        .unwrap();
    host.dispatch(legacy_permit, |ctx| {
        assert_eq!(ctx.request.capability, Capability::NotesRead);
        Ok(())
    })
    .unwrap();

    // v2 package with read + document-scoped write + capture.
    let v2_pkg = v2_manifest(
        "org.modern.editor",
        vec![
            req(CAP_CORE_KNOWLEDGE_READ, 1, 0, Some(1), false),
            req(CAP_CORE_KNOWLEDGE_WRITE, 1, 1, Some(1), false),
            req(CAP_CORE_CAPTURE_PROPOSE, 1, 0, None, false),
        ],
        vec![],
        vec![],
    );
    let v2_id = host.register(v2_pkg, digest('2')).unwrap();
    host.replace_orthogonal_grants(
        v2_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_WRITE),
                min_minor: 1,
                resource: doc("vault-1", "doc-allowed"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_CAPTURE_PROPOSE),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(v2_id).unwrap();

    let session = host.open_orthogonal_session(v2_id).unwrap();
    let table = host.session_capabilities(&session).unwrap();
    assert!(table.is_active(&cap(CAP_CORE_KNOWLEDGE_READ)));
    assert_eq!(
        table
            .get(&cap(CAP_CORE_KNOWLEDGE_WRITE))
            .unwrap()
            .negotiated_minor,
        Some(1)
    );

    // Read on any document under vault-1 succeeds.
    let read_permit = host
        .authorize_core_knowledge(
            &session,
            &ck_request(
                "ck-read-1",
                CoreKnowledgeOperation::DocumentChunk {
                    format: DocumentFormat::Epub,
                    document_id: 42,
                    section_index: 0,
                    offset: 0,
                    max_bytes: 512,
                },
            ),
            doc("vault-1", "doc-other"),
        )
        .unwrap();
    host.dispatch_orthogonal(read_permit, |ctx| {
        assert_eq!(ctx.capability, cap(CAP_CORE_KNOWLEDGE_READ));
        assert_eq!(ctx.negotiated_major, 1);
        assert_eq!(ctx.negotiated_minor, 0);
        Ok(())
    })
    .unwrap();

    // Write on allowed document succeeds, but other document or vault-wide fails closed.
    let write_ok = host
        .authorize_core_knowledge(
            &session,
            &ck_request(
                "ck-write-1",
                CoreKnowledgeOperation::UpdateCaptureNote {
                    format: DocumentFormat::Pdf,
                    capture_id: 7,
                    note: "Updated".into(),
                },
            ),
            doc("vault-1", "doc-allowed"),
        )
        .unwrap();
    host.dispatch_orthogonal(write_ok, |ctx| {
        assert_eq!(ctx.capability, cap(CAP_CORE_KNOWLEDGE_WRITE));
        assert_eq!(ctx.negotiated_minor, 1);
        Ok(())
    })
    .unwrap();

    assert_eq!(
        host.authorize_core_knowledge(
            &session,
            &ck_request(
                "ck-write-2",
                CoreKnowledgeOperation::DeleteCapture {
                    format: DocumentFormat::Pdf,
                    capture_id: 7,
                },
            ),
            doc("vault-1", "doc-other"),
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );
    assert_eq!(
        host.authorize_core_knowledge(
            &session,
            &ck_request(
                "ck-write-3",
                CoreKnowledgeOperation::DeleteDocument {
                    format: DocumentFormat::Epub,
                    document_id: 7,
                },
            ),
            vault("vault-1"),
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );

    // Capture epub selection succeeds under core.capture.propose.
    let capture_permit = host
        .authorize_core_knowledge(
            &session,
            &ck_request(
                "ck-cap-1",
                CoreKnowledgeOperation::CaptureEpubSelection {
                    operation_id: "op-1".into(),
                    document_id: 42,
                    chapter_index: 2,
                    start: 10,
                    end: 40,
                    quote: "exact snippet".into(),
                    note: "margin note".into(),
                },
            ),
            doc("vault-1", "doc-other"),
        )
        .unwrap();
    host.dispatch_orthogonal(capture_permit, |ctx| {
        assert_eq!(ctx.capability, cap(CAP_CORE_CAPTURE_PROPOSE));
        Ok(())
    })
    .unwrap();
}

#[test]
fn per_capability_negotiation_degrades_optional_independently_and_blocks_missing_required() {
    let mut host = Host::new("vault-1", [Capability::NotesRead]).unwrap();
    // Host supports vocab only at minor 0, but package optionally asks for >= 1.2.
    host.configure_orthogonal_support(cap(CAP_CORE_VOCAB_STUDY), 1, 0)
        .unwrap();

    let manifest = v2_manifest(
        "org.test.optional",
        vec![
            req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false),
            req(CAP_CORE_VOCAB_STUDY, 1, 2, Some(3), true),
        ],
        vec![],
        vec![req("ext.missing.translator", 1, 0, None, true)],
    );
    let id = host.register(manifest, digest('a')).unwrap();
    host.replace_orthogonal_grants(
        id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_VOCAB_STUDY),
                min_minor: 2,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(id).unwrap();

    let session = host
        .open_negotiated_session(id, &SUPPORTED_HOST_APIS)
        .unwrap();
    let table = host.session_capabilities(&session).unwrap();
    let read_entry = table.get(&cap(CAP_CORE_KNOWLEDGE_READ)).unwrap();
    assert!(!read_entry.degraded);
    assert_eq!(read_entry.negotiated_minor, Some(0));

    let vocab_entry = table.get(&cap(CAP_CORE_VOCAB_STUDY)).unwrap();
    assert!(vocab_entry.optional);
    assert!(vocab_entry.degraded);
    assert_eq!(vocab_entry.negotiated_minor, None);

    let ext_entry = table.get(&cap("ext.missing.translator")).unwrap();
    assert!(ext_entry.optional);
    assert!(ext_entry.degraded);
    assert_eq!(ext_entry.negotiated_minor, None);

    // Degraded optional capability fails authorization, while required read works.
    assert_eq!(
        host.authorize_orthogonal(
            &session,
            cap(CAP_CORE_VOCAB_STUDY),
            "req-vocab",
            vault("vault-1"),
        )
        .unwrap_err()
        .code,
        ErrorCode::CapabilityUnavailable
    );
    let permit = host
        .authorize_orthogonal(
            &session,
            cap(CAP_CORE_KNOWLEDGE_READ),
            "req-read",
            vault("vault-1"),
        )
        .unwrap();
    host.dispatch_orthogonal(permit, |_| Ok(())).unwrap();

    // Missing required orthogonal capability blocks opening a session.
    let req_missing = v2_manifest(
        "org.test.reqmissing",
        vec![req("host.storage.kv", 1, 0, None, false)],
        vec![],
        vec![],
    );
    let req_id = host.register(req_missing, digest('b')).unwrap();
    host.replace_orthogonal_grants(
        req_id,
        vec![OrthogonalGrant {
            capability: cap("host.storage.kv"),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(req_id).unwrap();
    assert_eq!(
        host.open_orthogonal_session(req_id).unwrap_err().code,
        ErrorCode::CapabilityUnavailable
    );
}

#[test]
fn read_only_packages_cannot_grant_or_exercise_core_knowledge_write() {
    let mut host = Host::new(
        "vault-1",
        [
            Capability::NotesRead,
            Capability::LibraryRead,
            Capability::SelectionRead,
            Capability::DocumentMetadataRead,
        ],
    )
    .unwrap();
    host.configure_orthogonal_support(cap(CAP_CORE_KNOWLEDGE_WRITE), 1, 0)
        .unwrap();

    let legacy_ro = Manifest {
        manifest_version: 1,
        package_id: "org.ro.legacy".into(),
        publisher_id: "org.ro".into(),
        package_version: PackageVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        host_api: ApiRange {
            major: 0,
            min_minor: 6,
            max_minor: 8,
        },
        required_capabilities: vec![Capability::NotesRead, Capability::LibraryRead],
        optional_capabilities: vec![Capability::DocumentMetadataRead],
        contributions: vec![],
        orthogonal_capabilities: vec![],
        provides: vec![],
        requires: vec![],
    };
    assert!(
        !legacy_ro
            .effective_orthogonal_requirements()
            .iter()
            .any(|item| item.id.as_str() == CAP_CORE_KNOWLEDGE_WRITE)
    );
    let legacy_id = host.register(legacy_ro, digest('c')).unwrap();

    // Cannot grant core.knowledge.write to legacy read-only package.
    assert_eq!(
        host.replace_orthogonal_grants(
            legacy_id,
            vec![OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_WRITE),
                min_minor: 0,
                resource: vault("vault-1"),
            }],
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );

    host.replace_grants(
        legacy_id,
        vec![
            Grant {
                capability: Capability::NotesRead,
                resource: vault("vault-1"),
            },
            Grant {
                capability: Capability::LibraryRead,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(legacy_id).unwrap();
    let legacy_session = host.open_orthogonal_session(legacy_id).unwrap();
    assert_eq!(
        host.authorize_core_knowledge(
            &legacy_session,
            &ck_request(
                "ro-write-1",
                CoreKnowledgeOperation::UpdateCaptureNote {
                    format: DocumentFormat::Pdf,
                    capture_id: 1,
                    note: "forbidden".into(),
                },
            ),
            vault("vault-1"),
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );

    // Also forbidden for v2 read-only package.
    let v2_ro = v2_manifest(
        "org.ro.v2",
        vec![req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false)],
        vec![],
        vec![],
    );
    let v2_id = host.register(v2_ro, digest('d')).unwrap();
    assert_eq!(
        host.replace_orthogonal_grants(
            v2_id,
            vec![OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_WRITE),
                min_minor: 0,
                resource: vault("vault-1"),
            }],
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );
    host.replace_orthogonal_grants(
        v2_id,
        vec![OrthogonalGrant {
            capability: cap(CAP_CORE_KNOWLEDGE_READ),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(v2_id).unwrap();
    let v2_session = host.open_orthogonal_session(v2_id).unwrap();
    assert_eq!(
        host.authorize_core_knowledge(
            &v2_session,
            &ck_request(
                "ro-write-2",
                CoreKnowledgeOperation::DeleteDocument {
                    format: DocumentFormat::Epub,
                    document_id: 1,
                },
            ),
            vault("vault-1"),
        )
        .unwrap_err()
        .code,
        ErrorCode::PermissionDenied
    );

    // 0.7-negotiated session also cannot invoke 0.8 orthogonal APIs.
    let v07_session = host.open_session_for_api(legacy_id, HOST_API_V0_7).unwrap();
    assert_eq!(
        host.authorize_orthogonal(
            &v07_session,
            cap(CAP_CORE_KNOWLEDGE_READ),
            "old-sess",
            vault("vault-1"),
        )
        .unwrap_err()
        .code,
        ErrorCode::UnsupportedVersion
    );
    let _ = ApiVersion { major: 0, minor: 6 };
    let _ = HOST_API_V0_6;
}

#[test]
fn extension_router_enforces_caller_provider_intersection_bounds_and_cycles() {
    let mut host = Host::new("vault-1", [Capability::NotesRead]).unwrap();
    host.configure_orthogonal_support(cap(CAP_CORE_KNOWLEDGE_WRITE), 1, 0)
        .unwrap();
    host.configure_orthogonal_support(cap(CAP_CORE_CAPTURE_PROPOSE), 1, 0)
        .unwrap();

    // Provider holds BOTH read and write; Caller holds ONLY read + service access.
    let provider_manifest = v2_manifest(
        "org.acme.provider",
        vec![
            req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false),
            req(CAP_CORE_KNOWLEDGE_WRITE, 1, 0, None, false),
        ],
        vec![ServiceEndpoint {
            service: cap("ext.acme.summarizer"),
            major: 1,
            minor: 2,
        }],
        vec![],
    );
    let caller_manifest = v2_manifest(
        "org.acme.caller",
        vec![req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false)],
        vec![],
        vec![req("ext.acme.summarizer", 1, 0, Some(2), false)],
    );

    let provider_id = host.register(provider_manifest, digest('1')).unwrap();
    host.replace_orthogonal_grants(
        provider_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_WRITE),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(provider_id).unwrap();

    let caller_id = host.register(caller_manifest, digest('2')).unwrap();
    host.replace_orthogonal_grants(
        caller_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap("ext.acme.summarizer"),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(caller_id).unwrap();

    let mut router = ExtensionRouter::new(&host);
    let plan = router
        .register_installations(&host, &[provider_id, caller_id])
        .unwrap();
    assert_eq!(plan.entries().len(), 2);

    let caller_session = host.open_orthogonal_session(caller_id).unwrap();

    // Reject empty and oversized opaque frame payloads.
    assert_eq!(
        router
            .authorize_service_call(
                &host,
                &caller_session,
                ServiceCallRequest {
                    api_version: HOST_API_V0_8,
                    request_id: "call-empty".into(),
                    service: cap("ext.acme.summarizer"),
                    resource: doc("vault-1", "doc-1"),
                    requested_capabilities: vec![],
                    payload: vec![],
                },
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        router
            .authorize_service_call(
                &host,
                &caller_session,
                ServiceCallRequest {
                    api_version: HOST_API_V0_8,
                    request_id: "call-big".into(),
                    service: cap("ext.acme.summarizer"),
                    resource: doc("vault-1", "doc-1"),
                    requested_capabilities: vec![],
                    payload: vec![b'x'; MAX_SERVICE_FRAME_BYTES + 1],
                },
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );

    // Caller CANNOT escalate to provider's core.knowledge.write!
    assert_eq!(
        router
            .authorize_service_call(
                &host,
                &caller_session,
                ServiceCallRequest {
                    api_version: HOST_API_V0_8,
                    request_id: "call-escalate".into(),
                    service: cap("ext.acme.summarizer"),
                    resource: doc("vault-1", "doc-1"),
                    requested_capabilities: vec![cap(CAP_CORE_KNOWLEDGE_WRITE)],
                    payload: b"summarize".to_vec(),
                },
            )
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );

    // Unfiltered intersection also contains only caller ∩ provider = {core.knowledge.read}.
    let permit = router
        .authorize_service_call(
            &host,
            &caller_session,
            ServiceCallRequest {
                api_version: HOST_API_V0_8,
                request_id: "call-ok".into(),
                service: cap("ext.acme.summarizer"),
                resource: doc("vault-1", "doc-1"),
                requested_capabilities: vec![],
                payload: b"{\"doc\":1}".to_vec(),
            },
        )
        .unwrap();
    assert_eq!(permit.delegated_capabilities().len(), 1);
    assert!(
        permit
            .delegated_capabilities()
            .contains(&cap(CAP_CORE_KNOWLEDGE_READ))
    );
    assert!(
        !permit
            .delegated_capabilities()
            .contains(&cap(CAP_CORE_KNOWLEDGE_WRITE))
    );

    router
        .dispatch_service_call(&host, permit, |call| {
            assert_eq!(call.payload, b"{\"doc\":1}");
            assert_eq!(call.negotiated_major, 1);
            assert_eq!(call.negotiated_minor, 2);
            assert!(
                call.delegated_capabilities
                    .contains(&cap(CAP_CORE_KNOWLEDGE_READ))
            );
            assert!(
                !call
                    .delegated_capabilities
                    .contains(&cap(CAP_CORE_KNOWLEDGE_WRITE))
            );
            Ok(())
        })
        .unwrap();

    // Multi-hop chain: Caller -> Provider -> Leaf; Leaf lacks capture, Provider holds write+capture+read.
    // Final intersection must be {read}, and forwarding loop back to Provider/Caller fails.
    let leaf_manifest = v2_manifest(
        "org.acme.leaf",
        vec![req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false)],
        vec![ServiceEndpoint {
            service: cap("ext.acme.leaf"),
            major: 1,
            minor: 0,
        }],
        vec![],
    );
    let mid_manifest = v2_manifest(
        "org.acme.mid",
        vec![
            req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false),
            req(CAP_CORE_KNOWLEDGE_WRITE, 1, 0, None, false),
            req(CAP_CORE_CAPTURE_PROPOSE, 1, 0, None, false),
        ],
        vec![ServiceEndpoint {
            service: cap("ext.acme.mid"),
            major: 1,
            minor: 0,
        }],
        vec![req("ext.acme.leaf", 1, 0, None, false)],
    );
    let top_manifest = v2_manifest(
        "org.acme.top",
        vec![
            req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false),
            req(CAP_CORE_CAPTURE_PROPOSE, 1, 0, None, false),
        ],
        vec![],
        vec![req("ext.acme.mid", 1, 0, None, false)],
    );

    let leaf_id = host.register(leaf_manifest, digest('3')).unwrap();
    host.replace_orthogonal_grants(
        leaf_id,
        vec![OrthogonalGrant {
            capability: cap(CAP_CORE_KNOWLEDGE_READ),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(leaf_id).unwrap();

    let mid_id = host.register(mid_manifest, digest('4')).unwrap();
    host.replace_orthogonal_grants(
        mid_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_WRITE),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_CAPTURE_PROPOSE),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap("ext.acme.leaf"),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(mid_id).unwrap();

    let top_id = host.register(top_manifest, digest('5')).unwrap();
    host.replace_orthogonal_grants(
        top_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap(CAP_CORE_CAPTURE_PROPOSE),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap("ext.acme.mid"),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(top_id).unwrap();

    router
        .register_installations(&host, &[leaf_id, mid_id, top_id])
        .unwrap();
    let top_session = host.open_orthogonal_session(top_id).unwrap();
    let mid_session = host.open_orthogonal_session(mid_id).unwrap();

    let hop1 = router
        .authorize_service_call(
            &host,
            &top_session,
            ServiceCallRequest {
                api_version: HOST_API_V0_8,
                request_id: "hop-1".into(),
                service: cap("ext.acme.mid"),
                resource: vault("vault-1"),
                requested_capabilities: vec![],
                payload: b"step1".to_vec(),
            },
        )
        .unwrap();
    router
        .dispatch_service_call(&host, hop1, |parent_call| {
            assert!(
                parent_call
                    .delegated_capabilities
                    .contains(&cap(CAP_CORE_KNOWLEDGE_READ))
            );
            assert!(
                parent_call
                    .delegated_capabilities
                    .contains(&cap(CAP_CORE_CAPTURE_PROPOSE))
            );
            assert!(
                !parent_call
                    .delegated_capabilities
                    .contains(&cap(CAP_CORE_KNOWLEDGE_WRITE))
            );

            // Mid cannot forward core.knowledge.write (absent from top ∩ mid) to leaf.
            assert_eq!(
                router
                    .authorize_forwarded_call(
                        &host,
                        parent_call,
                        &mid_session,
                        ServiceCallRequest {
                            api_version: HOST_API_V0_8,
                            request_id: "hop-2-esc".into(),
                            service: cap("ext.acme.leaf"),
                            resource: vault("vault-1"),
                            requested_capabilities: vec![cap(CAP_CORE_KNOWLEDGE_WRITE)],
                            payload: b"esc".to_vec(),
                        },
                    )
                    .unwrap_err()
                    .code,
                ErrorCode::PermissionDenied
            );

            let hop2 = router
                .authorize_forwarded_call(
                    &host,
                    parent_call,
                    &mid_session,
                    ServiceCallRequest {
                        api_version: HOST_API_V0_8,
                        request_id: "hop-2-ok".into(),
                        service: cap("ext.acme.leaf"),
                        resource: vault("vault-1"),
                        requested_capabilities: vec![],
                        payload: b"step2".to_vec(),
                    },
                )
                .unwrap();
            assert_eq!(hop2.call_chain(), &[top_id, mid_id, leaf_id]);
            assert_eq!(hop2.delegated_capabilities().len(), 1);
            assert!(
                hop2.delegated_capabilities()
                    .contains(&cap(CAP_CORE_KNOWLEDGE_READ))
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn cyclic_dependencies_are_rejected_and_uninstall_revocation_fences_routes() {
    let mut host = Host::new("vault-1", []).unwrap();

    // Cyclic dependencies A <-> B must be rejected by capability_composition.
    let cyc_a = v2_manifest(
        "org.cycle.a",
        vec![],
        vec![ServiceEndpoint {
            service: cap("ext.cycle.a"),
            major: 1,
            minor: 0,
        }],
        vec![req("ext.cycle.b", 1, 0, None, false)],
    );
    let cyc_b = v2_manifest(
        "org.cycle.b",
        vec![],
        vec![ServiceEndpoint {
            service: cap("ext.cycle.b"),
            major: 1,
            minor: 0,
        }],
        vec![req("ext.cycle.a", 1, 0, None, false)],
    );
    let cyc_a_id = host.register(cyc_a, digest('a')).unwrap();
    let cyc_b_id = host.register(cyc_b, digest('b')).unwrap();
    // Register and enable mutually dependent pair after enabling: note we register B first so A can resolve provider if needed,
    // wait: enable checks orthogonal_grants.
    host.replace_orthogonal_grants(
        cyc_a_id,
        vec![OrthogonalGrant {
            capability: cap("ext.cycle.b"),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.replace_orthogonal_grants(
        cyc_b_id,
        vec![OrthogonalGrant {
            capability: cap("ext.cycle.a"),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(cyc_a_id).unwrap();
    host.enable(cyc_b_id).unwrap();

    let mut cyc_router = ExtensionRouter::new(&host);
    let err = cyc_router
        .register_installations(&host, &[cyc_a_id, cyc_b_id])
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidState);
    assert!(err.message.contains("cycle"));
    assert!(cyc_router.plan().is_none());

    // Live route deactivation on grant revocation, disable, and unregister.
    let prov = v2_manifest(
        "org.fence.prov",
        vec![req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false)],
        vec![ServiceEndpoint {
            service: cap("ext.fence.svc"),
            major: 1,
            minor: 0,
        }],
        vec![],
    );
    let cons = v2_manifest(
        "org.fence.cons",
        vec![req(CAP_CORE_KNOWLEDGE_READ, 1, 0, None, false)],
        vec![],
        vec![req("ext.fence.svc", 1, 0, None, false)],
    );
    let prov_id = host.register(prov, digest('c')).unwrap();
    let cons_id = host.register(cons, digest('d')).unwrap();
    host.configure_orthogonal_support(cap(CAP_CORE_KNOWLEDGE_READ), 1, 0)
        .unwrap();
    host.replace_orthogonal_grants(
        prov_id,
        vec![OrthogonalGrant {
            capability: cap(CAP_CORE_KNOWLEDGE_READ),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.replace_orthogonal_grants(
        cons_id,
        vec![
            OrthogonalGrant {
                capability: cap(CAP_CORE_KNOWLEDGE_READ),
                min_minor: 0,
                resource: vault("vault-1"),
            },
            OrthogonalGrant {
                capability: cap("ext.fence.svc"),
                min_minor: 0,
                resource: vault("vault-1"),
            },
        ],
    )
    .unwrap();
    host.enable(prov_id).unwrap();
    host.enable(cons_id).unwrap();

    let mut router = ExtensionRouter::new(&host);
    router
        .register_installations(&host, &[prov_id, cons_id])
        .unwrap();
    let cons_session = host.open_orthogonal_session(cons_id).unwrap();

    // 1. Grant revocation on provider invalidates in-flight ServiceCallPermit via epoch fence.
    let inflight = router
        .authorize_service_call(
            &host,
            &cons_session,
            ServiceCallRequest {
                api_version: HOST_API_V0_8,
                request_id: "fence-1".into(),
                service: cap("ext.fence.svc"),
                resource: vault("vault-1"),
                requested_capabilities: vec![],
                payload: b"ping".to_vec(),
            },
        )
        .unwrap();
    host.replace_orthogonal_grants(prov_id, vec![]).unwrap();
    assert_eq!(
        router
            .dispatch_service_call(&host, inflight, |_| Ok(()))
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );

    // Syncing router deactivates the stale provider node and its service route.
    router.sync_with_host(&host).unwrap();
    assert!(!router.is_installation_active(prov_id));
    assert!(!router.service(&cap("ext.fence.svc")).unwrap().active);

    // Re-enable and re-register, then test uninstall/unregister fencing.
    host.replace_orthogonal_grants(
        prov_id,
        vec![OrthogonalGrant {
            capability: cap(CAP_CORE_KNOWLEDGE_READ),
            min_minor: 0,
            resource: vault("vault-1"),
        }],
    )
    .unwrap();
    host.enable(prov_id).unwrap();
    router
        .register_installations(&host, &[prov_id, cons_id])
        .unwrap();
    let inflight2 = router
        .authorize_service_call(
            &host,
            &cons_session,
            ServiceCallRequest {
                api_version: HOST_API_V0_8,
                request_id: "fence-2".into(),
                service: cap("ext.fence.svc"),
                resource: vault("vault-1"),
                requested_capabilities: vec![],
                payload: b"ping2".to_vec(),
            },
        )
        .unwrap();

    host.unregister(prov_id).unwrap();
    assert_eq!(
        router
            .dispatch_service_call(&host, inflight2, |_| Ok(()))
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );
    router.sync_with_host(&host).unwrap();
    assert!(router.service(&cap("ext.fence.svc")).is_none());

    assert_eq!(
        router
            .authorize_service_call(
                &host,
                &cons_session,
                ServiceCallRequest {
                    api_version: HOST_API_V0_8,
                    request_id: "fence-3".into(),
                    service: cap("ext.fence.svc"),
                    resource: vault("vault-1"),
                    requested_capabilities: vec![],
                    payload: b"ping3".to_vec(),
                },
            )
            .unwrap_err()
            .code,
        ErrorCode::CapabilityUnavailable
    );
}
