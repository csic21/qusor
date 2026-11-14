//! Wire schema for QUSOR Candidate A declarative native plugin UI protocol.

use serde::{Deserialize, Serialize};

/// Current declarative UI tree protocol version.
pub const PLUGIN_UI_PROTOCOL: u16 = 1;

/// Hard upper bounds that no host profile may exceed.
pub const HARD_MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const HARD_MAX_NODES: usize = 512;
pub const HARD_MAX_DEPTH: usize = 16;
pub const HARD_MAX_CHILDREN: usize = 128;
pub const HARD_MAX_TEXT_BYTES: usize = 64 * 1024;
pub const HARD_MAX_PATCH_OPS: usize = 256;
pub const HARD_MAX_EVENT_BINDINGS: usize = 256;

/// Host-enforced resource budgets for a mounted plugin UI slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiBudget {
    pub max_payload_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_children_per_node: usize,
    pub max_text_bytes: usize,
    pub max_patch_ops: usize,
    pub max_event_bindings: usize,
}

impl Default for UiBudget {
    fn default() -> Self {
        Self {
            max_payload_bytes: HARD_MAX_PAYLOAD_BYTES,
            max_nodes: HARD_MAX_NODES,
            max_depth: HARD_MAX_DEPTH,
            max_children_per_node: HARD_MAX_CHILDREN,
            max_text_bytes: HARD_MAX_TEXT_BYTES,
            max_patch_ops: HARD_MAX_PATCH_OPS,
            max_event_bindings: HARD_MAX_EVENT_BINDINGS,
        }
    }
}

impl UiBudget {
    /// Clamps any profile-provided budget so it never exceeds system hard ceilings.
    pub fn clamp_to_ceiling(self) -> Self {
        Self {
            max_payload_bytes: self.max_payload_bytes.clamp(1024, HARD_MAX_PAYLOAD_BYTES),
            max_nodes: self.max_nodes.clamp(1, HARD_MAX_NODES),
            max_depth: self.max_depth.clamp(1, HARD_MAX_DEPTH),
            max_children_per_node: self.max_children_per_node.clamp(1, HARD_MAX_CHILDREN),
            max_text_bytes: self.max_text_bytes.clamp(256, HARD_MAX_TEXT_BYTES),
            max_patch_ops: self.max_patch_ops.clamp(1, HARD_MAX_PATCH_OPS),
            max_event_bindings: self.max_event_bindings.clamp(1, HARD_MAX_EVENT_BINDINGS),
        }
    }
}

/// Semantic theme tokens aligned with `apps/qusor/theme.json`.
///
/// Plugins reference semantic roles only; raw hex/RGB strings are not part of
/// the schema so light/dark mode and user themes remain host-governed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticToken {
    SurfaceBase,
    SurfaceToolbar,
    SurfaceInspector,
    SurfaceControl,
    SurfaceSelected,
    SurfaceHover,
    SurfacePressed,
    SurfaceInset,
    SurfaceAccent,
    SurfaceDanger,
    BorderSubtle,
    BorderControl,
    BorderDanger,
    TextPrimary,
    TextSecondary,
    TextAccent,
    TextDanger,
    FocusRing,
    Primary,
    PrimaryForeground,
    Muted,
    MutedForeground,
}

/// Bounded spacing scale mapped to host platform points/dp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingScale {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

/// Bounded typography hierarchy aligned with host design tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypographyRole {
    Caption,
    Body,
    BodyStrong,
    Subtitle,
    Title,
    Mono,
}

/// Layout alignment along main or cross axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// Controlled style properties allowed on declarative nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<SemanticToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<SemanticToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<SemanticToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<SpacingScale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<SpacingScale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typography: Option<TypographyRole>,
    #[serde(default)]
    pub flex_grow: bool,
    #[serde(default)]
    pub compact_hide: bool,
}

/// Semantic accessibility metadata required on interactive nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessibilityProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AccessibilityRole>,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub busy: bool,
}

/// Explicit accessibility roles understood by NSH platform backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityRole {
    Button,
    Tab,
    TextField,
    Checkbox,
    Switch,
    List,
    ListItem,
    Heading,
    Status,
    Region,
}

/// Whitelisted declarative UI components.
///
/// WebView, raw HTML, arbitrary script, and unguarded canvas/shader nodes are
/// excluded by design (ADR 0042).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiComponent {
    VStack,
    HStack,
    Scroll,
    Card,
    Section {
        title: String,
    },
    Split {
        ratio_percent: u8,
    },
    Text {
        content: String,
    },
    Badge {
        label: String,
        tone: SemanticToken,
    },
    Divider,
    EmptyState {
        title: String,
        description: String,
    },
    KeyValue {
        key: String,
        value: String,
    },
    Progress {
        completed: u32,
        total: u32,
    },
    List,
    ListItem {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        selected: bool,
    },
    Button {
        label: String,
        #[serde(default)]
        primary: bool,
        #[serde(default)]
        destructive: bool,
        #[serde(default)]
        disabled: bool,
        action_id: String,
    },
    IconButton {
        icon_name: String,
        #[serde(default)]
        disabled: bool,
        action_id: String,
    },
    TextInput {
        value: String,
        placeholder: String,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        disabled: bool,
        on_change_action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_submit_action: Option<String>,
    },
    Toggle {
        label: String,
        checked: bool,
        #[serde(default)]
        disabled: bool,
        action_id: String,
    },
    Select {
        selected_id: String,
        options: Vec<SelectOption>,
        #[serde(default)]
        disabled: bool,
        action_id: String,
    },
    Tabs {
        active_tab_id: String,
        items: Vec<TabOption>,
        action_id: String,
    },
    /// Opaque host-managed native surface reference (e.g. `reader.surface` or
    /// `pdf.page_view`) issued by a trusted host broker, never a raw path/URL.
    ControlledSurfaceRef {
        surface_id: String,
        surface_kind: ControlledSurfaceKind,
    },
}

impl UiComponent {
    /// Returns whether user interaction is directly attached to this node.
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            Self::Button { .. }
                | Self::IconButton { .. }
                | Self::TextInput { .. }
                | Self::Toggle { .. }
                | Self::Select { .. }
                | Self::Tabs { .. }
        )
    }

    /// Returns whether this component is disabled.
    pub fn is_disabled(&self) -> bool {
        match self {
            Self::Button { disabled, .. }
            | Self::IconButton { disabled, .. }
            | Self::TextInput { disabled, .. }
            | Self::Toggle { disabled, .. }
            | Self::Select { disabled, .. } => *disabled,
            _ => false,
        }
    }

    /// Returns whether the node can accept child nodes.
    pub fn allows_children(&self) -> bool {
        matches!(
            self,
            Self::VStack
                | Self::HStack
                | Self::Scroll
                | Self::Card
                | Self::Section { .. }
                | Self::Split { .. }
                | Self::List
                | Self::ListItem { .. }
        )
    }

    /// Returns all action IDs subscribed by this component.
    pub fn action_ids(&self) -> Vec<&str> {
        match self {
            Self::Button { action_id, .. }
            | Self::IconButton { action_id, .. }
            | Self::Toggle { action_id, .. }
            | Self::Select { action_id, .. }
            | Self::Tabs { action_id, .. } => vec![action_id.as_str()],
            Self::TextInput {
                on_change_action,
                on_submit_action,
                ..
            } => {
                let mut ids = vec![on_change_action.as_str()];
                if let Some(submit) = on_submit_action {
                    ids.push(submit.as_str());
                }
                ids
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TabOption {
    pub id: String,
    pub label: String,
}

/// Allowed host-controlled native capability surfaces embeddable in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlledSurfaceKind {
    ReaderDocument,
    PdfPagePreview,
    MarkdownPreview,
}

/// A single node in the declarative UI tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub id: String,
    pub component: UiComponent,
    #[serde(default)]
    pub style: NodeStyle,
    #[serde(default)]
    pub accessibility: AccessibilityProps,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UiNode>,
}

/// Complete declarative UI tree snapshot for a single plugin slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiTree {
    pub protocol_version: u16,
    pub revision: u64,
    pub root: UiNode,
}

/// Incremental patch envelope bound to an active host mount and generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPatch {
    pub protocol_version: u16,
    pub mount_id: String,
    pub generation: u64,
    pub base_revision: u64,
    pub next_revision: u64,
    pub ops: Vec<UiPatchOp>,
}

/// Atomic tree mutation operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiPatchOp {
    ReplaceTree {
        root: UiNode,
    },
    SetProps {
        target_id: String,
        component: UiComponent,
        #[serde(default)]
        style: NodeStyle,
        #[serde(default)]
        accessibility: AccessibilityProps,
    },
    InsertChild {
        parent_id: String,
        index: usize,
        node: UiNode,
    },
    RemoveNode {
        target_id: String,
    },
    ReplaceSubtree {
        target_id: String,
        node: UiNode,
    },
}

/// Event payload kind sent from host NSH renderer back to the plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiEventKind {
    Press,
    TextChanged { value: String },
    TextSubmitted { value: String },
    Toggled { checked: bool },
    Selected { option_id: String },
    TabChanged { tab_id: String },
}

/// Generation-fenced user event emitted by the host to a mounted plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiEventEnvelope {
    pub protocol_version: u16,
    pub mount_id: String,
    pub generation: u64,
    pub node_id: String,
    pub action_id: String,
    pub event: UiEventKind,
}

/// ADR 0028 workspace regions where a plugin UI slot may be placed by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRegion {
    ShellWorkspaces,
    WorkspaceToolbar,
    WorkspaceLeading,
    WorkspaceContent,
    WorkspaceAuxiliary,
    WorkspaceContextActions,
    WorkspaceObjectStatus,
    WorkspaceOutput,
}

/// Declarative slot contribution metadata published in a plugin manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDescriptor {
    pub contribution_id: String,
    pub region: SlotRegion,
    pub role: String,
    pub min_width_pt: u16,
    pub supports_compact: bool,
}

/// Errors produced by UI schema validation, patch application, or event fencing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiHostError {
    UnsupportedProtocol(u16),
    PayloadTooLarge {
        actual: usize,
        max: usize,
    },
    InvalidIdentifier {
        field: &'static str,
        detail: String,
    },
    DuplicateNodeId(String),
    NodeNotFound(String),
    BudgetExceeded {
        metric: &'static str,
        actual: usize,
        max: usize,
    },
    AccessibilityViolation {
        node_id: String,
        reason: &'static str,
    },
    InvalidComponentHierarchy {
        node_id: String,
        reason: &'static str,
    },
    InvalidProps {
        node_id: String,
        reason: &'static str,
    },
    MountMismatch {
        expected: String,
        actual: String,
    },
    StaleGeneration {
        expected: u64,
        actual: u64,
    },
    RevisionMismatch {
        expected: u64,
        actual: u64,
    },
    EventRejected {
        node_id: String,
        reason: &'static str,
    },
    SlotError(String),
}
