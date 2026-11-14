#![forbid(unsafe_code)]

//! Ergonomic builders for valid [`extension_api::ui`] declarative UI trees.

use extension_api::ui::{
    AccessibilityProps, AccessibilityRole, Align, ControlledSurfaceKind, MAX_SPLIT_RATIO_PERCENT,
    MIN_SPLIT_RATIO_PERCENT, NodeStyle, PLUGIN_UI_PROTOCOL, SelectOption, SemanticToken,
    SpacingScale, TabOption, TypographyRole, UiComponent, UiNode, UiTree,
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

/// Builds a two-pane split container with `ratio_percent` clamped to `10..=90`.
pub fn split(
    id: impl Into<String>,
    ratio_percent: u8,
    gap: SpacingScale,
    children: Vec<UiNode>,
) -> UiNode {
    let mut item = node(
        id,
        UiComponent::Split {
            ratio_percent: ratio_percent.clamp(MIN_SPLIT_RATIO_PERCENT, MAX_SPLIT_RATIO_PERCENT),
        },
        children,
    );
    item.style.gap = Some(gap);
    item
}

/// Builds a horizontal or vertical visual divider.
pub fn divider(id: impl Into<String>) -> UiNode {
    node(id, UiComponent::Divider, Vec::new())
}

/// Builds an icon button using a whitelisted host glyph name and accessibility label.
pub fn icon_button(
    id: impl Into<String>,
    icon_name: impl Into<String>,
    label: impl Into<String>,
    action_id: impl Into<String>,
    disabled: bool,
) -> UiNode {
    let icon_name = icon_name.into();
    let label = fallback_accessible_label(label.into(), &icon_name, "Action");
    let mut item = node(
        id,
        UiComponent::IconButton {
            icon_name,
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

/// Builds a text input field with explicit accessibility label and `TextField` role.
#[allow(clippy::too_many_arguments)]
pub fn input(
    id: impl Into<String>,
    label: impl Into<String>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    multiline: bool,
    disabled: bool,
    on_change_action: impl Into<String>,
    on_submit_action: Option<String>,
) -> UiNode {
    let placeholder = placeholder.into();
    let label = fallback_accessible_label(label.into(), &placeholder, "Text input");
    let mut item = node(
        id,
        UiComponent::TextInput {
            value: value.into(),
            placeholder,
            multiline,
            disabled,
            on_change_action: on_change_action.into(),
            on_submit_action,
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(label),
        role: Some(AccessibilityRole::TextField),
        ..AccessibilityProps::default()
    };
    item
}

/// Builds a boolean toggle switch with explicit accessibility label.
pub fn toggle(
    id: impl Into<String>,
    label: impl Into<String>,
    checked: bool,
    disabled: bool,
    action_id: impl Into<String>,
) -> UiNode {
    let label = label.into();
    let a11y_label = fallback_accessible_label(label.clone(), "", "Toggle");
    let mut item = node(
        id,
        UiComponent::Toggle {
            label,
            checked,
            disabled,
            action_id: action_id.into(),
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(a11y_label),
        role: Some(AccessibilityRole::Switch),
        ..AccessibilityProps::default()
    };
    item
}

/// Constructs one [`SelectOption`] entry for [`select`].
pub fn select_option(id: impl Into<String>, label: impl Into<String>) -> SelectOption {
    SelectOption {
        id: id.into(),
        label: label.into(),
    }
}

/// Builds a picker / select control with explicit accessibility metadata.
pub fn select(
    id: impl Into<String>,
    label: impl Into<String>,
    selected_id: impl Into<String>,
    options: Vec<SelectOption>,
    disabled: bool,
    action_id: impl Into<String>,
) -> UiNode {
    let a11y_label = fallback_accessible_label(label.into(), "", "Select option");
    let mut item = node(
        id,
        UiComponent::Select {
            selected_id: selected_id.into(),
            options,
            disabled,
            action_id: action_id.into(),
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(a11y_label),
        role: Some(AccessibilityRole::Button),
        ..AccessibilityProps::default()
    };
    item
}

/// Constructs one [`TabOption`] entry for [`tabs`].
pub fn tab_option(id: impl Into<String>, label: impl Into<String>) -> TabOption {
    TabOption {
        id: id.into(),
        label: label.into(),
    }
}

/// Builds a tab bar control with explicit accessibility metadata.
pub fn tabs(
    id: impl Into<String>,
    label: impl Into<String>,
    active_tab_id: impl Into<String>,
    items: Vec<TabOption>,
    action_id: impl Into<String>,
) -> UiNode {
    let a11y_label = fallback_accessible_label(label.into(), "", "Navigation tabs");
    let mut item = node(
        id,
        UiComponent::Tabs {
            active_tab_id: active_tab_id.into(),
            items,
            action_id: action_id.into(),
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        label: Some(a11y_label),
        role: Some(AccessibilityRole::Tab),
        ..AccessibilityProps::default()
    };
    item
}

/// Builds an opaque host-controlled native surface reference (never a path or URL).
pub fn controlled_surface_ref(
    id: impl Into<String>,
    surface_id: impl Into<String>,
    surface_kind: ControlledSurfaceKind,
) -> UiNode {
    let mut item = node(
        id,
        UiComponent::ControlledSurfaceRef {
            surface_id: surface_id.into(),
            surface_kind,
        },
        Vec::new(),
    );
    item.accessibility = AccessibilityProps {
        role: Some(AccessibilityRole::Region),
        ..AccessibilityProps::default()
    };
    item
}

/// Marks a node to grow and fill remaining parent axis space.
pub fn with_flex_grow(mut node: UiNode) -> UiNode {
    node.style.flex_grow = true;
    node
}

/// Marks a node to hide when the slot renders in compact mode.
pub fn with_compact_hide(mut node: UiNode) -> UiNode {
    node.style.compact_hide = true;
    node
}

fn fallback_accessible_label(primary: String, secondary: &str, default_label: &str) -> String {
    if !primary.trim().is_empty() {
        primary
    } else if !secondary.trim().is_empty() {
        secondary.to_owned()
    } else {
        default_label.to_owned()
    }
}
