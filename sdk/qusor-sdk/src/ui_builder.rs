#![forbid(unsafe_code)]

//! Ergonomic builders for valid [`extension_api::ui`] declarative UI trees.

use extension_api::ui::{
    AccessibilityProps, AccessibilityRole, Align, NodeStyle, PLUGIN_UI_PROTOCOL, SemanticToken,
    SpacingScale, TypographyRole, UiComponent, UiNode, UiTree,
};

/// Wraps a root [`UiNode`] into a protocol-1 [`UiTree`].
pub fn ui_tree(revision: u64, root: UiNode) -> UiTree {
    UiTree {
        protocol_version: PLUGIN_UI_PROTOCOL,
        revision: revision.max(1),
        root,
    }
}

/// Builds a basic container or leaf [`UiNode`] with default styling.
pub fn node(id: impl Into<String>, component: UiComponent, children: Vec<UiNode>) -> UiNode {
    UiNode {
        id: id.into(),
        component,
        style: NodeStyle::default(),
        accessibility: AccessibilityProps::default(),
        children,
    }
}

/// Builds a vertical stack with spacing and padding.
pub fn vstack(
    id: impl Into<String>,
    gap: SpacingScale,
    padding: SpacingScale,
    children: Vec<UiNode>,
) -> UiNode {
    let mut item = node(id, UiComponent::VStack, children);
    item.style.gap = Some(gap);
    item.style.padding = Some(padding);
    item
}

/// Builds a horizontal stack with spacing and optional alignment.
pub fn hstack(
    id: impl Into<String>,
    gap: SpacingScale,
    align: Align,
    children: Vec<UiNode>,
) -> UiNode {
    let mut item = node(id, UiComponent::HStack, children);
    item.style.gap = Some(gap);
    item.style.align = Some(align);
    item
}

/// Builds a scroll container.
pub fn scroll(id: impl Into<String>, padding: SpacingScale, children: Vec<UiNode>) -> UiNode {
    let mut item = node(id, UiComponent::Scroll, children);
    item.style.padding = Some(padding);
    item
}

/// Builds a card container with padding and spacing.
pub fn card(
    id: impl Into<String>,
    gap: SpacingScale,
    padding: SpacingScale,
    children: Vec<UiNode>,
) -> UiNode {
    let mut item = node(id, UiComponent::Card, children);
    item.style.gap = Some(gap);
    item.style.padding = Some(padding);
    item
}

/// Builds a titled section container.
pub fn section(
    id: impl Into<String>,
    title: impl Into<String>,
    gap: SpacingScale,
    children: Vec<UiNode>,
) -> UiNode {
    let mut item = node(
        id,
        UiComponent::Section {
            title: title.into(),
        },
        children,
    );
    item.style.gap = Some(gap);
    item
}

/// Builds a styled text node.
pub fn text(
    id: impl Into<String>,
    content: impl Into<String>,
    typography: TypographyRole,
    tone: SemanticToken,
) -> UiNode {
    let mut item = node(
        id,
        UiComponent::Text {
            content: content.into(),
        },
        Vec::new(),
    );
    item.style.typography = Some(typography);
    item.style.foreground = Some(tone);
    item
}

/// Builds a semantic badge node.
pub fn badge(id: impl Into<String>, label: impl Into<String>, tone: SemanticToken) -> UiNode {
    node(
        id,
        UiComponent::Badge {
            label: label.into(),
            tone,
        },
        Vec::new(),
    )
}

/// Builds a key/value display row.
pub fn key_value(
    id: impl Into<String>,
    key: impl Into<String>,
    value: impl Into<String>,
) -> UiNode {
    node(
        id,
        UiComponent::KeyValue {
            key: key.into(),
            value: value.into(),
        },
        Vec::new(),
    )
}

/// Builds a progress indicator (`total` is clamped to `>= 1`).
pub fn progress(id: impl Into<String>, completed: u32, total: u32) -> UiNode {
    let safe_total = total.max(1);
    let safe_completed = completed.min(safe_total);
    node(
        id,
        UiComponent::Progress {
            completed: safe_completed,
            total: safe_total,
        },
        Vec::new(),
    )
}

/// Builds a list container.
pub fn list(id: impl Into<String>, gap: SpacingScale, children: Vec<UiNode>) -> UiNode {
    let mut item = node(id, UiComponent::List, children);
    item.style.gap = Some(gap);
    item
}

/// Builds a selectable list item with explicit accessibility label.
pub fn list_item(
    id: impl Into<String>,
    title: impl Into<String>,
    subtitle: Option<String>,
    selected: bool,
    action_id: impl Into<String>,
) -> UiNode {
    let title = title.into();
    let mut item = node(
        id,
        UiComponent::ListItem {
            title: title.clone(),
            subtitle,
            selected,
            action_id: Some(action_id.into()),
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(title),
        role: Some(AccessibilityRole::ListItem),
        ..AccessibilityProps::default()
    };
    item
}

/// Builds an action button with explicit accessibility metadata.
pub fn button(
    id: impl Into<String>,
    label: impl Into<String>,
    action_id: impl Into<String>,
    primary: bool,
    destructive: bool,
    disabled: bool,
) -> UiNode {
    let label = label.into();
    let mut item = node(
        id,
        UiComponent::Button {
            label: label.clone(),
            primary,
            destructive,
            disabled,
            action_id: action_id.into(),
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(label),
        role: Some(AccessibilityRole::Button),
        ..AccessibilityProps::default()
    };
    item
}

/// Builds an empty state placeholder.
pub fn empty_state(
    id: impl Into<String>,
    title: impl Into<String>,
    description: impl Into<String>,
) -> UiNode {
    node(
        id,
        UiComponent::EmptyState {
            title: title.into(),
            description: description.into(),
        },
        Vec::new(),
    )
}
