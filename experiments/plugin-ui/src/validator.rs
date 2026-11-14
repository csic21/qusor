//! Strict validation for declarative plugin UI trees, patches, and slot metadata.

use std::collections::BTreeSet;

use crate::schema::{
    PLUGIN_UI_PROTOCOL, SlotDescriptor, UiBudget, UiComponent, UiHostError, UiNode, UiPatch, UiTree,
};

const VALID_ICONS: &[&str] = &[
    "search", "speaker", "plus", "close", "chevron", "back", "forward", "list", "document", "book",
    "pen", "plug", "panel", "info", "clock", "flow", "sun", "moon", "more", "arrow", "chat",
    "grid", "folder", "trash", "edit", "download", "upload", "bookmark", "sliders",
];

#[derive(Default)]
struct WalkStats {
    node_count: usize,
    text_bytes: usize,
    event_bindings: usize,
    seen_ids: BTreeSet<String>,
}

/// Validates a bounded JSON tree payload and returns the decoded `UiTree`.
pub fn decode_and_validate_tree(bytes: &[u8], budget: UiBudget) -> Result<UiTree, UiHostError> {
    let budget = budget.clamp_to_ceiling();
    if bytes.len() > budget.max_payload_bytes {
        return Err(UiHostError::PayloadTooLarge {
            actual: bytes.len(),
            max: budget.max_payload_bytes,
        });
    }
    let tree: UiTree = serde_json::from_slice(bytes)
        .map_err(|err| UiHostError::SlotError(format!("invalid UI tree JSON: {err}")))?;
    validate_tree(&tree, budget)?;
    Ok(tree)
}

/// Validates a bounded JSON patch payload and returns the decoded `UiPatch`.
pub fn decode_and_validate_patch(bytes: &[u8], budget: UiBudget) -> Result<UiPatch, UiHostError> {
    let budget = budget.clamp_to_ceiling();
    if bytes.len() > budget.max_payload_bytes {
        return Err(UiHostError::PayloadTooLarge {
            actual: bytes.len(),
            max: budget.max_payload_bytes,
        });
    }
    let patch: UiPatch = serde_json::from_slice(bytes)
        .map_err(|err| UiHostError::SlotError(format!("invalid UI patch JSON: {err}")))?;
    validate_patch(&patch, budget)?;
    Ok(patch)
}

/// Validates a complete `UiTree` against protocol version, budgets, whitelist, and a11y rules.
pub fn validate_tree(tree: &UiTree, budget: UiBudget) -> Result<(), UiHostError> {
    let budget = budget.clamp_to_ceiling();
    if tree.protocol_version != PLUGIN_UI_PROTOCOL {
        return Err(UiHostError::UnsupportedProtocol(tree.protocol_version));
    }
    if tree.revision == 0 {
        return Err(UiHostError::RevisionMismatch {
            expected: 1,
            actual: 0,
        });
    }
    let mut stats = WalkStats::default();
    validate_node(&tree.root, 1, budget, &mut stats)
}

pub(crate) fn validate_opaque_id(value: &str, field: &'static str) -> Result<(), UiHostError> {
    if value.len() > extension_api::MAX_ID_BYTES
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(UiHostError::InvalidIdentifier {
            field,
            detail: format!("invalid {field}"),
        });
    }
    Ok(())
}

pub(crate) fn validate_namespace(
    value: &str,
    limit: usize,
    field: &'static str,
) -> Result<(), UiHostError> {
    let valid_segment = |segment: &str| {
        segment
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    };
    if value.len() > limit || !value.contains('.') || !value.split('.').all(valid_segment) {
        return Err(UiHostError::InvalidIdentifier {
            field,
            detail: format!("invalid {field} namespace"),
        });
    }
    Ok(())
}

/// Validates patch header invariants before tree application.
pub fn validate_patch(patch: &UiPatch, budget: UiBudget) -> Result<(), UiHostError> {
    let budget = budget.clamp_to_ceiling();
    if patch.protocol_version != PLUGIN_UI_PROTOCOL {
        return Err(UiHostError::UnsupportedProtocol(patch.protocol_version));
    }
    validate_opaque_id(&patch.mount_id, "mount_id")?;
    if patch.ops.is_empty() || patch.ops.len() > budget.max_patch_ops {
        return Err(UiHostError::BudgetExceeded {
            metric: "patch_ops",
            actual: patch.ops.len(),
            max: budget.max_patch_ops,
        });
    }
    if patch.next_revision != patch.base_revision.saturating_add(1) {
        return Err(UiHostError::RevisionMismatch {
            expected: patch.base_revision.saturating_add(1),
            actual: patch.next_revision,
        });
    }
    Ok(())
}

/// Validates a plugin manifest slot declaration.
pub fn validate_slot_descriptor(slot: &SlotDescriptor) -> Result<(), UiHostError> {
    validate_namespace(
        &slot.contribution_id,
        extension_api::MAX_CONTRIBUTION_ID_BYTES,
        "contribution_id",
    )?;
    validate_namespace(&slot.role, extension_api::MAX_ID_BYTES, "role")?;
    if slot.min_width_pt > 1200 {
        return Err(UiHostError::BudgetExceeded {
            metric: "min_width_pt",
            actual: usize::from(slot.min_width_pt),
            max: 1200,
        });
    }
    Ok(())
}

fn validate_node(
    node: &UiNode,
    depth: usize,
    budget: UiBudget,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    if depth > budget.max_depth {
        return Err(UiHostError::BudgetExceeded {
            metric: "depth",
            actual: depth,
            max: budget.max_depth,
        });
    }
    stats.node_count += 1;
    if stats.node_count > budget.max_nodes {
        return Err(UiHostError::BudgetExceeded {
            metric: "nodes",
            actual: stats.node_count,
            max: budget.max_nodes,
        });
    }

    validate_opaque_id(&node.id, "node_id")?;
    if !stats.seen_ids.insert(node.id.clone()) {
        return Err(UiHostError::DuplicateNodeId(node.id.clone()));
    }

    if !node.component.allows_children() && !node.children.is_empty() {
        return Err(UiHostError::InvalidComponentHierarchy {
            node_id: node.id.clone(),
            reason: "leaf component cannot have child nodes",
        });
    }
    if node.children.len() > budget.max_children_per_node {
        return Err(UiHostError::BudgetExceeded {
            metric: "children_per_node",
            actual: node.children.len(),
            max: budget.max_children_per_node,
        });
    }

    validate_component_props(&node.id, &node.component, budget, stats)?;
    validate_accessibility(node)?;

    for child in &node.children {
        validate_node(child, depth + 1, budget, stats)?;
    }
    Ok(())
}

fn check_clean_text(
    node_id: &str,
    text: &str,
    allow_newlines: bool,
    budget: UiBudget,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    let has_disallowed_control = text.chars().any(|ch| {
        if allow_newlines && (ch == '\n' || ch == '\r' || ch == '\t') {
            return false;
        }
        ch.is_control()
    });
    if has_disallowed_control {
        return Err(UiHostError::InvalidProps {
            node_id: node_id.to_owned(),
            reason: "text contains disallowed control characters",
        });
    }
    stats.text_bytes = stats.text_bytes.saturating_add(text.len());
    if stats.text_bytes > budget.max_text_bytes {
        return Err(UiHostError::BudgetExceeded {
            metric: "text_bytes",
            actual: stats.text_bytes,
            max: budget.max_text_bytes,
        });
    }
    Ok(())
}

fn validate_action_id(node_id: &str, action_id: &str) -> Result<(), UiHostError> {
    validate_opaque_id(action_id, "action_id").map_err(|_| UiHostError::InvalidProps {
        node_id: node_id.to_owned(),
        reason: "invalid action_id",
    })
}

fn validate_component_props(
    node_id: &str,
    component: &UiComponent,
    budget: UiBudget,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    for action_id in component.action_ids() {
        validate_action_id(node_id, action_id)?;
        stats.event_bindings += 1;
        if stats.event_bindings > budget.max_event_bindings {
            return Err(UiHostError::BudgetExceeded {
                metric: "event_bindings",
                actual: stats.event_bindings,
                max: budget.max_event_bindings,
            });
        }
    }

    match component {
        UiComponent::Section { title } => {
            check_clean_text(node_id, title, false, budget, stats)?;
        }
        UiComponent::Split { ratio_percent } => {
            if !(15..=85).contains(ratio_percent) {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "split ratio_percent must be within 15..=85",
                });
            }
        }
        UiComponent::Text { content } => {
            check_clean_text(node_id, content, true, budget, stats)?;
        }
        UiComponent::Badge { label, .. } => {
            if label.is_empty() {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "badge label must not be empty",
                });
            }
            check_clean_text(node_id, label, false, budget, stats)?;
        }
        UiComponent::EmptyState { title, description } => {
            check_clean_text(node_id, title, false, budget, stats)?;
            check_clean_text(node_id, description, true, budget, stats)?;
        }
        UiComponent::KeyValue { key, value } => {
            check_clean_text(node_id, key, false, budget, stats)?;
            check_clean_text(node_id, value, true, budget, stats)?;
        }
        UiComponent::Progress { completed, total } => {
            if *total == 0 || *completed > *total {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "progress requires total > 0 and completed <= total",
                });
            }
        }
        UiComponent::ListItem {
            title, subtitle, ..
        } => {
            check_clean_text(node_id, title, false, budget, stats)?;
            if let Some(sub) = subtitle {
                check_clean_text(node_id, sub, false, budget, stats)?;
            }
        }
        UiComponent::Button { label, .. } => {
            if label.is_empty() {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "button label must not be empty",
                });
            }
            check_clean_text(node_id, label, false, budget, stats)?;
        }
        UiComponent::IconButton { icon_name, .. } => {
            if !VALID_ICONS.contains(&icon_name.as_str()) {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "icon_name is outside host glyph whitelist",
                });
            }
        }
        UiComponent::TextInput {
            value,
            placeholder,
            multiline,
            ..
        } => {
            check_clean_text(node_id, value, *multiline, budget, stats)?;
            check_clean_text(node_id, placeholder, false, budget, stats)?;
        }
        UiComponent::Toggle { label, .. } => {
            check_clean_text(node_id, label, false, budget, stats)?;
        }
        UiComponent::Select {
            selected_id,
            options,
            ..
        } => {
            if options.is_empty() || options.len() > 64 {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "select requires 1..=64 options",
                });
            }
            let mut seen = BTreeSet::new();
            for opt in options {
                validate_opaque_id(&opt.id, "option_id").map_err(|_| {
                    UiHostError::InvalidProps {
                        node_id: node_id.to_owned(),
                        reason: "invalid select option id",
                    }
                })?;
                if !seen.insert(opt.id.as_str()) {
                    return Err(UiHostError::InvalidProps {
                        node_id: node_id.to_owned(),
                        reason: "duplicate select option id",
                    });
                }
                check_clean_text(node_id, &opt.label, false, budget, stats)?;
            }
            if !seen.contains(selected_id.as_str()) {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "selected_id must match one of the select options",
                });
            }
        }
        UiComponent::Tabs {
            active_tab_id,
            items,
            ..
        } => {
            if items.is_empty() || items.len() > 16 {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "tabs require 1..=16 items",
                });
            }
            let mut seen = BTreeSet::new();
            for item in items {
                validate_opaque_id(&item.id, "tab_id").map_err(|_| UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "invalid tab id",
                })?;
                if !seen.insert(item.id.as_str()) {
                    return Err(UiHostError::InvalidProps {
                        node_id: node_id.to_owned(),
                        reason: "duplicate tab id",
                    });
                }
                check_clean_text(node_id, &item.label, false, budget, stats)?;
            }
            if !seen.contains(active_tab_id.as_str()) {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "active_tab_id must match one of the tabs",
                });
            }
        }
        UiComponent::ControlledSurfaceRef { surface_id, .. } => {
            // Only opaque host-issued handle IDs are allowed; reject paths, URLs, or schemes.
            validate_opaque_id(surface_id, "surface_id").map_err(|_| {
                UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "surface_id must be an opaque host handle, not a path or URI",
                }
            })?;
        }
        UiComponent::VStack
        | UiComponent::HStack
        | UiComponent::Scroll
        | UiComponent::Card
        | UiComponent::Divider
        | UiComponent::List => {}
    }

    Ok(())
}

fn validate_accessibility(node: &UiNode) -> Result<(), UiHostError> {
    if let Some(label) = &node.accessibility.label
        && (label.trim().is_empty() || label.chars().any(char::is_control))
    {
        return Err(UiHostError::AccessibilityViolation {
            node_id: node.id.clone(),
            reason: "accessibility.label must be non-empty without control chars",
        });
    }
    if node.component.is_interactive() {
        let has_explicit = node
            .accessibility
            .label
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());
        let has_intrinsic_text = match &node.component {
            UiComponent::Button { label, .. } | UiComponent::Toggle { label, .. } => {
                !label.trim().is_empty()
            }
            _ => false,
        };
        if !has_explicit && !has_intrinsic_text {
            return Err(UiHostError::AccessibilityViolation {
                node_id: node.id.clone(),
                reason: "interactive component requires an accessibility label",
            });
        }
    }
    Ok(())
}
