use qusor_plugin_ui_probe::{
    host::{MountedSlot, SlotRegistration, resolve_slot_composition, resolve_theme_token},
    schema::{
        AccessibilityProps, AccessibilityRole, ControlledSurfaceKind, HARD_MAX_NODES, NodeStyle,
        PLUGIN_UI_PROTOCOL, SemanticToken, SlotDescriptor, SlotRegion, SpacingScale,
        TypographyRole, UiBudget, UiComponent, UiEventKind, UiHostError, UiNode, UiPatch,
        UiPatchOp, UiTree,
    },
    validator::{decode_and_validate_patch, decode_and_validate_tree, validate_tree},
};

fn node(id: &str, component: UiComponent, children: Vec<UiNode>) -> UiNode {
    UiNode {
        id: id.into(),
        component,
        style: NodeStyle::default(),
        accessibility: AccessibilityProps::default(),
        children,
    }
}

fn vocab_study_tree() -> UiTree {
    UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: UiNode {
            id: "root".into(),
            component: UiComponent::VStack,
            style: NodeStyle {
                background: Some(SemanticToken::SurfaceBase),
                padding: Some(SpacingScale::Lg),
                gap: Some(SpacingScale::Md),
                ..NodeStyle::default()
            },
            accessibility: AccessibilityProps {
                role: Some(AccessibilityRole::Region),
                label: Some("背单词学习卡片".into()),
                ..AccessibilityProps::default()
            },
            children: vec![
                node(
                    "card",
                    UiComponent::Card,
                    vec![
                        UiNode {
                            id: "word".into(),
                            component: UiComponent::Text {
                                content: "ephemeral".into(),
                            },
                            style: NodeStyle {
                                foreground: Some(SemanticToken::TextPrimary),
                                typography: Some(TypographyRole::Title),
                                ..NodeStyle::default()
                            },
                            accessibility: AccessibilityProps::default(),
                            children: Vec::new(),
                        },
                        node(
                            "progress",
                            UiComponent::Progress {
                                completed: 3,
                                total: 12,
                            },
                            Vec::new(),
                        ),
                    ],
                ),
                UiNode {
                    id: "reveal_btn".into(),
                    component: UiComponent::Button {
                        label: "显示释义".into(),
                        primary: true,
                        destructive: false,
                        disabled: false,
                        action_id: "reveal_meaning".into(),
                    },
                    style: NodeStyle {
                        background: Some(SemanticToken::Primary),
                        foreground: Some(SemanticToken::PrimaryForeground),
                        ..NodeStyle::default()
                    },
                    accessibility: AccessibilityProps {
                        label: Some("显示释义".into()),
                        hint: Some("展开当前单词的中文解释与例句".into()),
                        role: Some(AccessibilityRole::Button),
                        ..AccessibilityProps::default()
                    },
                    children: Vec::new(),
                },
            ],
        },
    }
}

fn vocab_slot_descriptor() -> SlotDescriptor {
    SlotDescriptor {
        contribution_id: "org.qusor.vocab.study-card".into(),
        region: SlotRegion::WorkspaceContent,
        role: "study.flashcard".into(),
        min_width_pt: 320,
        supports_compact: true,
    }
}

#[test]
fn valid_tree_patch_and_event_roundtrip() {
    let tree = vocab_study_tree();
    let json = serde_json::to_vec(&tree).expect("serialize");
    let decoded = decode_and_validate_tree(&json, UiBudget::default()).expect("valid tree");

    let mut slot = MountedSlot::mount(
        "mount_vocab_01",
        "org.qusor.vocab",
        vocab_slot_descriptor(),
        UiBudget::default(),
    )
    .expect("mount");
    slot.set_initial_tree(decoded).expect("initial tree");

    let patch_json = serde_json::to_vec(&UiPatch {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: "mount_vocab_01".into(),
        generation: 1,
        base_revision: 1,
        next_revision: 2,
        ops: vec![
            UiPatchOp::InsertChild {
                parent_id: "card".into(),
                index: 1,
                node: node(
                    "meaning",
                    UiComponent::Text {
                        content: "adj. 短暂的，朝生暮死的".into(),
                    },
                    Vec::new(),
                ),
            },
            UiPatchOp::SetProps {
                target_id: "reveal_btn".into(),
                component: UiComponent::Button {
                    label: "记住".into(),
                    primary: true,
                    destructive: false,
                    disabled: false,
                    action_id: "rate_good".into(),
                },
                style: NodeStyle::default(),
                accessibility: AccessibilityProps {
                    label: Some("标记记住".into()),
                    role: Some(AccessibilityRole::Button),
                    ..AccessibilityProps::default()
                },
            },
        ],
    })
    .expect("serialize patch");

    let patch = decode_and_validate_patch(&patch_json, UiBudget::default()).expect("patch valid");
    slot.apply_patch(&patch).expect("patch applies");
    assert_eq!(slot.active_tree().expect("active").revision, 2);

    let event = slot
        .build_event("reveal_btn", "rate_good", UiEventKind::Press)
        .expect("event built");
    slot.verify_inbound_event(&event).expect("event verified");
}

#[test]
fn rejects_unknown_component_and_raw_style_escape() {
    let webview_json = br#"{
        "protocol_version": 1,
        "revision": 1,
        "root": {
            "id": "root",
            "component": { "kind": "web_view", "url": "https://evil.example" }
        }
    }"#;
    assert!(matches!(
        decode_and_validate_tree(webview_json, UiBudget::default()),
        Err(UiHostError::SlotError(_))
    ));

    let raw_color_json = br##"{
        "protocol_version": 1,
        "revision": 1,
        "root": {
            "id": "root",
            "component": { "kind": "v_stack" },
            "style": { "background": "#FF00FF" }
        }
    }"##;
    assert!(matches!(
        decode_and_validate_tree(raw_color_json, UiBudget::default()),
        Err(UiHostError::SlotError(_))
    ));

    let unknown_field_json = br#"{
        "protocol_version": 1,
        "revision": 1,
        "root": {
            "id": "root",
            "component": { "kind": "v_stack" },
            "style": { "position": "fixed" }
        }
    }"#;
    assert!(matches!(
        decode_and_validate_tree(unknown_field_json, UiBudget::default()),
        Err(UiHostError::SlotError(_))
    ));
}

#[test]
fn rejects_missing_accessibility_label_and_invalid_icon() {
    let bad_input = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: node(
            "input",
            UiComponent::TextInput {
                value: String::new(),
                placeholder: "输入笔记".into(),
                multiline: false,
                disabled: false,
                on_change_action: "on_change".into(),
                on_submit_action: None,
            },
            Vec::new(),
        ),
    };
    assert!(matches!(
        validate_tree(&bad_input, UiBudget::default()),
        Err(UiHostError::AccessibilityViolation { .. })
    ));

    let bad_icon = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: UiNode {
            id: "btn".into(),
            component: UiComponent::IconButton {
                icon_name: "../../evil.svg".into(),
                disabled: false,
                action_id: "tap".into(),
            },
            style: NodeStyle::default(),
            accessibility: AccessibilityProps {
                label: Some("非法图标".into()),
                ..AccessibilityProps::default()
            },
            children: Vec::new(),
        },
    };
    assert!(matches!(
        validate_tree(&bad_icon, UiBudget::default()),
        Err(UiHostError::InvalidProps { .. })
    ));
}

#[test]
fn enforces_budgets_and_hard_ceiling_clamp() {
    let excessive = UiBudget {
        max_nodes: 99_999,
        max_depth: 200,
        ..UiBudget::default()
    };
    assert_eq!(excessive.clamp_to_ceiling().max_nodes, HARD_MAX_NODES);

    // Depth overflow under tight budget
    let deep = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: node(
            "n1",
            UiComponent::VStack,
            vec![node(
                "n2",
                UiComponent::VStack,
                vec![node("n3", UiComponent::Divider, Vec::new())],
            )],
        ),
    };
    let tight = UiBudget {
        max_depth: 2,
        ..UiBudget::default()
    };
    assert!(matches!(
        validate_tree(&deep, tight),
        Err(UiHostError::BudgetExceeded {
            metric: "depth",
            ..
        })
    ));

    // Duplicate node id rejected
    let dup = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: node(
            "same",
            UiComponent::VStack,
            vec![node("same", UiComponent::Divider, Vec::new())],
        ),
    };
    assert!(matches!(
        validate_tree(&dup, UiBudget::default()),
        Err(UiHostError::DuplicateNodeId(_))
    ));
}

#[test]
fn patch_failure_rolls_back_atomically() {
    let mut slot = MountedSlot::mount(
        "mount_vocab_01",
        "org.qusor.vocab",
        vocab_slot_descriptor(),
        UiBudget::default(),
    )
    .expect("mount");
    slot.set_initial_tree(vocab_study_tree())
        .expect("initial tree");

    // Op 1 is valid, Op 2 targets missing node -> whole patch must roll back.
    let bad_patch = UiPatch {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: "mount_vocab_01".into(),
        generation: 1,
        base_revision: 1,
        next_revision: 2,
        ops: vec![
            UiPatchOp::InsertChild {
                parent_id: "card".into(),
                index: 0,
                node: node("temp_badge", UiComponent::Divider, Vec::new()),
            },
            UiPatchOp::RemoveNode {
                target_id: "nonexistent".into(),
            },
        ],
    };
    assert!(matches!(
        slot.apply_patch(&bad_patch),
        Err(UiHostError::NodeNotFound(_))
    ));
    let active = slot.active_tree().expect("still active");
    assert_eq!(active.revision, 1);
    assert_eq!(active.root.children[0].children.len(), 2);
    assert_eq!(slot.fault_count(), 1);
}

#[test]
fn provider_replacement_fences_stale_generation_events_and_patches() {
    let mut slot = MountedSlot::mount(
        "mount_editor_01",
        "org.thirdparty.editor_a",
        SlotDescriptor {
            contribution_id: "org.thirdparty.editor-a.view".into(),
            region: SlotRegion::WorkspaceAuxiliary,
            role: "annotation.editor".into(),
            min_width_pt: 280,
            supports_compact: true,
        },
        UiBudget::default(),
    )
    .expect("mount");
    slot.set_initial_tree(vocab_study_tree()).expect("init");

    let stale_event = slot
        .build_event("reveal_btn", "reveal_meaning", UiEventKind::Press)
        .expect("event from gen 1");

    // Replace provider A with provider B (ADR 0028)
    slot.replace_provider(
        "org.thirdparty.editor_b",
        SlotDescriptor {
            contribution_id: "org.thirdparty.editor-b.view".into(),
            region: SlotRegion::WorkspaceAuxiliary,
            role: "annotation.editor".into(),
            min_width_pt: 280,
            supports_compact: true,
        },
    )
    .expect("replace");
    assert_eq!(slot.generation(), 2);
    assert!(slot.active_tree().is_none());

    // Old generation event and patch must be rejected cleanly.
    assert!(matches!(
        slot.verify_inbound_event(&stale_event),
        Err(UiHostError::StaleGeneration {
            expected: 2,
            actual: 1
        })
    ));
    let stale_patch = UiPatch {
        protocol_version: PLUGIN_UI_PROTOCOL,
        mount_id: "mount_editor_01".into(),
        generation: 1,
        base_revision: 1,
        next_revision: 2,
        ops: vec![UiPatchOp::RemoveNode {
            target_id: "card".into(),
        }],
    };
    assert!(matches!(
        slot.apply_patch(&stale_patch),
        Err(UiHostError::StaleGeneration {
            expected: 2,
            actual: 1
        })
    ));
}

#[test]
fn composition_bridge_and_controlled_surface_boundaries() {
    let slots = vec![
        SlotRegistration {
            entry_id: "org.qusor.vocab.slot".into(),
            package_id: "org.qusor.vocab".into(),
            descriptor: vocab_slot_descriptor(),
            required_services: vec![],
        },
        SlotRegistration {
            entry_id: "org.qusor.notes.slot".into(),
            package_id: "org.qusor.notes".into(),
            descriptor: SlotDescriptor {
                contribution_id: "org.qusor.notes.inspector".into(),
                region: SlotRegion::WorkspaceAuxiliary,
                role: "annotation.editor".into(),
                min_width_pt: 280,
                supports_compact: true,
            },
            required_services: vec![],
        },
    ];
    let plan = resolve_slot_composition("desktop_slots", &slots).expect("resolves");
    assert_eq!(plan.entries().len(), 3);
    assert_eq!(plan.entries()[0].entry().id().as_str(), "app.qusor.ui.host");

    // Duplicate provider for the same (region, role) must fail closed.
    let conflicting = vec![
        slots[1].clone(),
        SlotRegistration {
            entry_id: "org.other.notes.slot".into(),
            package_id: "org.other.notes".into(),
            descriptor: SlotDescriptor {
                contribution_id: "org.other.notes.inspector".into(),
                region: SlotRegion::WorkspaceAuxiliary,
                role: "annotation.editor".into(),
                min_width_pt: 280,
                supports_compact: true,
            },
            required_services: vec![],
        },
    ];
    assert!(matches!(
        resolve_slot_composition("desktop_slots", &conflicting),
        Err(UiHostError::SlotError(_))
    ));

    // ControlledSurfaceRef accepts opaque host handle and rejects file paths/URIs.
    let valid_surface = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: node(
            "pdf_slot",
            UiComponent::ControlledSurfaceRef {
                surface_id: "surf_pdf_01".into(),
                surface_kind: ControlledSurfaceKind::PdfPagePreview,
            },
            Vec::new(),
        ),
    };
    validate_tree(&valid_surface, UiBudget::default()).expect("opaque handle ok");

    let bad_surface = UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: 1,
        root: node(
            "pdf_slot",
            UiComponent::ControlledSurfaceRef {
                surface_id: "/Users/karl/secret.pdf".into(),
                surface_kind: ControlledSurfaceKind::PdfPagePreview,
            },
            Vec::new(),
        ),
    };
    assert!(matches!(
        validate_tree(&bad_surface, UiBudget::default()),
        Err(UiHostError::InvalidProps { .. })
    ));

    assert_eq!(
        resolve_theme_token(SemanticToken::Primary, false),
        "#1768BE"
    );
    assert_eq!(resolve_theme_token(SemanticToken::Primary, true), "#78B6FF");
}
