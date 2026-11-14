//! Declarative native plugin UI tree protocol (`PLUGIN_UI_PROTOCOL = 1`).
//!
//! Defines the strict, unknown-field-rejecting wire contracts for Candidate A
//! (declarative native UI tree + controlled host slots): `UiTree`, `UiNode`,
//! `UiPatch`, `UiAction`, `UiEventEnvelope`, `UiSlotRegistration`, `UiBudgets`,
//! semantic theme tokens, accessibility metadata, and whitelisted components.
//!
//! WebView, raw HTML, arbitrary script, raw CSS strings, and arbitrary external
//! URLs are excluded from the wire schema by design (ADR 0042 / ADR 0052).

use std::{collections::BTreeSet, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    ApiError, ApiVersion, Capability, ContributionKind, ErrorCode, HOST_API_V0_7, MAX_ID_BYTES,
    Manifest, decode_json, encode_json,
};

/// Current declarative UI tree wire protocol version.
pub const PLUGIN_UI_PROTOCOL: u16 = 1;

/// Current UI slot registration wire protocol version.
pub const UI_SLOT_PROTOCOL: u16 = 1;

/// Hard upper bounds that no host profile may exceed.
pub const HARD_MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const HARD_MAX_NODES: usize = 512;
pub const HARD_MAX_DEPTH: usize = 16;
pub const HARD_MAX_CHILDREN: usize = 128;
pub const HARD_MAX_TEXT_BYTES: usize = 64 * 1024;
pub const HARD_MAX_PATCH_OPS: usize = 256;
pub const HARD_MAX_EVENT_BINDINGS: usize = 256;

/// Allowed inclusive range for `UiComponent::Split::ratio_percent`.
pub const MIN_SPLIT_RATIO_PERCENT: u8 = 10;
pub const MAX_SPLIT_RATIO_PERCENT: u8 = 90;

/// Maximum declared `min_width_pt` on a slot descriptor.
pub const MAX_SLOT_MIN_WIDTH_PT: u16 = 1200;

/// Maximum serialized registration size in bytes.
pub const MAX_SLOT_REGISTRATION_BYTES: usize = 8 * 1024;

/// Maximum serialized event envelope size in bytes.
pub const MAX_EVENT_ENVELOPE_BYTES: usize = 16 * 1024;

/// Host glyph whitelist matching `apps/qusor/ui.tsx`.
pub const VALID_ICON_NAMES: &[&str] = &[
    "search", "speaker", "plus", "close", "chevron", "back", "forward", "list", "document", "book",
    "pen", "plug", "panel", "info", "clock", "flow", "sun", "moon", "more", "arrow", "chat",
    "grid", "folder", "trash", "edit", "download", "upload", "bookmark", "sliders",
];

/// Host-enforced resource budgets for a mounted plugin UI slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiBudgets {
    pub max_payload_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_children_per_node: usize,
    pub max_text_bytes: usize,
    pub max_patch_ops: usize,
    pub max_event_bindings: usize,
}

/// Alias retained for prototype source readability.
pub type UiBudget = UiBudgets;

impl Default for UiBudgets {
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

impl UiBudgets {
    /// Clamps any host-profile budget so it never exceeds system hard ceilings.
    pub fn clamp_to_ceiling(self) -> Self {
        Self {
            max_payload_bytes: self.max_payload_bytes.clamp(512, HARD_MAX_PAYLOAD_BYTES),
            max_nodes: self.max_nodes.clamp(1, HARD_MAX_NODES),
            max_depth: self.max_depth.clamp(1, HARD_MAX_DEPTH),
            max_children_per_node: self.max_children_per_node.clamp(1, HARD_MAX_CHILDREN),
            max_text_bytes: self.max_text_bytes.clamp(128, HARD_MAX_TEXT_BYTES),
            max_patch_ops: self.max_patch_ops.clamp(1, HARD_MAX_PATCH_OPS),
            max_event_bindings: self.max_event_bindings.clamp(1, HARD_MAX_EVENT_BINDINGS),
        }
    }
}

/// Semantic theme tokens aligned with `apps/qusor/theme.json`.
///
/// Plugins reference semantic roles only; raw hex/RGB/CSS values are excluded
/// from the schema so light/dark mode and user themes remain host-governed.
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

/// Option entry for `UiComponent::Select`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
}

/// Tab entry for `UiComponent::Tabs`.
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

/// Whitelisted declarative UI components.
///
/// WebView, raw HTML, arbitrary script, raw CSS, and unguarded canvas/shader
/// nodes are excluded by design (ADR 0042).
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
        #[serde(default)]
        selected: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action_id: Option<String>,
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
    /// Opaque host-managed native surface reference issued by a trusted host
    /// broker after capability verification, never a raw path or URL.
    ControlledSurfaceRef {
        surface_id: String,
        surface_kind: ControlledSurfaceKind,
    },
}

impl UiComponent {
    /// Returns whether user interaction is attached to this node.
    pub fn is_interactive(&self) -> bool {
        match self {
            Self::Button { .. }
            | Self::IconButton { .. }
            | Self::TextInput { .. }
            | Self::Toggle { .. }
            | Self::Select { .. }
            | Self::Tabs { .. } => true,
            Self::ListItem { action_id, .. } => action_id.is_some(),
            _ => false,
        }
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
            Self::ListItem {
                action_id: Some(action_id),
                ..
            } => vec![action_id.as_str()],
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

impl UiTree {
    /// Validates this tree against protocol invariants, budgets, and accessibility rules.
    pub fn validate(&self, budgets: UiBudgets) -> Result<(), UiHostError> {
        validate_tree(self, budgets)
    }

    /// Decodes a bounded UTF-8 JSON snapshot using default hard budgets.
    pub fn from_slice(json: &[u8]) -> Result<Self, UiHostError> {
        Self::from_slice_with_budgets(json, UiBudgets::default())
    }

    /// Decodes and validates a bounded UTF-8 JSON snapshot against `budgets`.
    pub fn from_slice_with_budgets(json: &[u8], budgets: UiBudgets) -> Result<Self, UiHostError> {
        decode_and_validate_tree(json, budgets)
    }

    /// Decodes and validates a JSON string snapshot using default hard budgets.
    pub fn from_json(json: &str) -> Result<Self, UiHostError> {
        Self::from_slice(json.as_bytes())
    }

    /// Validates and encodes this tree to compact JSON within `HARD_MAX_PAYLOAD_BYTES`.
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate(UiBudgets::default())?;
        encode_json(self, HARD_MAX_PAYLOAD_BYTES, ErrorCode::InvalidRequest)
    }
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

impl UiPatch {
    /// Validates patch header invariants before tree application.
    pub fn validate(&self, budgets: UiBudgets) -> Result<(), UiHostError> {
        validate_patch(self, budgets)
    }

    /// Decodes and validates a bounded UTF-8 JSON patch using default hard budgets.
    pub fn from_slice(json: &[u8]) -> Result<Self, UiHostError> {
        Self::from_slice_with_budgets(json, UiBudgets::default())
    }

    /// Decodes and validates a bounded UTF-8 JSON patch against `budgets`.
    pub fn from_slice_with_budgets(json: &[u8], budgets: UiBudgets) -> Result<Self, UiHostError> {
        decode_and_validate_patch(json, budgets)
    }

    /// Decodes and validates a JSON string patch using default hard budgets.
    pub fn from_json(json: &str) -> Result<Self, UiHostError> {
        Self::from_slice(json.as_bytes())
    }

    /// Validates and encodes this patch to compact JSON within `HARD_MAX_PAYLOAD_BYTES`.
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate(UiBudgets::default())?;
        encode_json(self, HARD_MAX_PAYLOAD_BYTES, ErrorCode::InvalidRequest)
    }
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

/// Action payload emitted when a user interacts with a declarative UI control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiAction {
    Press,
    TextChanged { value: String },
    TextSubmitted { value: String },
    Toggled { checked: bool },
    Selected { option_id: String },
    TabChanged { tab_id: String },
}

/// Alias retained for prototype source compatibility.
pub type UiEventKind = UiAction;

/// Generation-fenced user event emitted by the host NSH renderer to a plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiEventEnvelope {
    pub protocol_version: u16,
    pub mount_id: String,
    pub generation: u64,
    pub node_id: String,
    pub action_id: String,
    pub action: UiAction,
}

impl UiEventEnvelope {
    /// Validates envelope wire syntax and identifier bounds.
    pub fn validate(&self) -> Result<(), UiHostError> {
        validate_event_envelope(self)
    }

    /// Decodes and validates a bounded UTF-8 JSON event envelope.
    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let envelope: Self =
            decode_json(json, MAX_EVENT_ENVELOPE_BYTES, ErrorCode::InvalidRequest)?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Decodes and validates a JSON string event envelope.
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    /// Validates and encodes this envelope to compact JSON.
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_EVENT_ENVELOPE_BYTES, ErrorCode::InvalidRequest)
    }
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

/// Declarative slot contribution metadata published by a plugin UI slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDescriptor {
    pub contribution_id: String,
    pub region: SlotRegion,
    pub role: String,
    pub min_width_pt: u16,
    pub supports_compact: bool,
}

impl SlotDescriptor {
    /// Validates namespace identifiers and layout width constraints.
    pub fn validate(&self) -> Result<(), UiHostError> {
        validate_slot_descriptor(self)
    }
}

/// Signed slot registration wire contract binding a `SlotDescriptor` to
/// negotiated Host API & declared `ui.slot` manifest contributions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiSlotRegistration {
    pub slot_protocol: u16,
    pub api_version: ApiVersion,
    pub descriptor: SlotDescriptor,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<Capability>,
}

impl UiSlotRegistration {
    /// Validates protocol version, descriptor syntax, and required capabilities.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.slot_protocol != UI_SLOT_PROTOCOL
            || !crate::is_supported_api(self.api_version)
            || self.api_version < HOST_API_V0_7
        {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "ui.slot registration requires protocol 1 and minimum Host API 0.7",
            ));
        }
        self.descriptor.validate()?;
        let mut unique = BTreeSet::new();
        for cap in &self.required_capabilities {
            if !unique.insert(*cap) {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    "duplicate required capability in slot registration",
                ));
            }
            if !crate::capability_available_in(*cap, self.api_version) {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedVersion,
                    "slot registration capability unavailable in negotiated API",
                ));
            }
        }
        Ok(())
    }

    /// Validates that this registration binds to a compatible `ContributionKind::UiSlot`
    /// entry in `manifest` with `Capability::UiSlot`.
    pub fn validate_for_manifest(&self, manifest: &Manifest) -> Result<(), ApiError> {
        self.validate()?;
        manifest.validate()?;
        if !manifest.supports_api(self.api_version) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "manifest does not support registration api_version",
            ));
        }
        let matching = manifest.contributions.iter().find(|c| {
            c.id == self.descriptor.contribution_id
                && c.kind == ContributionKind::UiSlot
                && c.required_capabilities.contains(&Capability::UiSlot)
        });
        let Some(contribution) = matching else {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "slot registration does not match a declared ui_slot contribution",
            ));
        };
        if self
            .required_capabilities
            .iter()
            .any(|cap| !contribution.required_capabilities.contains(cap))
        {
            return Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "slot registration requests capability absent from contribution",
            ));
        }
        Ok(())
    }

    /// Decodes and validates a bounded UTF-8 JSON slot registration.
    pub fn from_slice(json: &[u8]) -> Result<Self, ApiError> {
        let value: Self =
            decode_json(json, MAX_SLOT_REGISTRATION_BYTES, ErrorCode::InvalidRequest)?;
        value.validate()?;
        Ok(value)
    }

    /// Decodes and validates a JSON string slot registration.
    pub fn from_json(json: &str) -> Result<Self, ApiError> {
        Self::from_slice(json.as_bytes())
    }

    /// Validates and encodes this registration to compact JSON.
    pub fn to_json(&self) -> Result<String, ApiError> {
        self.validate()?;
        encode_json(self, MAX_SLOT_REGISTRATION_BYTES, ErrorCode::InvalidRequest)
    }
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
    SlotNotFound(String),
    SlotAlreadyMounted(String),
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
    SlotFaulted {
        mount_id: String,
        fault_count: u32,
    },
    SlotError(String),
}

impl fmt::Display for UiHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProtocol(v) => write!(f, "unsupported UI protocol version {v}"),
            Self::PayloadTooLarge { actual, max } => {
                write!(f, "UI payload size {actual} exceeds limit {max}")
            }
            Self::InvalidIdentifier { field, detail } => {
                write!(f, "invalid {field}: {detail}")
            }
            Self::DuplicateNodeId(id) => write!(f, "duplicate UI node id: {id}"),
            Self::NodeNotFound(id) => write!(f, "UI node not found: {id}"),
            Self::SlotNotFound(id) => write!(f, "UI slot not found: {id}"),
            Self::SlotAlreadyMounted(id) => write!(f, "UI slot already mounted: {id}"),
            Self::BudgetExceeded {
                metric,
                actual,
                max,
            } => {
                write!(f, "UI budget exceeded for {metric}: {actual} > {max}")
            }
            Self::AccessibilityViolation { node_id, reason } => {
                write!(f, "accessibility violation on {node_id}: {reason}")
            }
            Self::InvalidComponentHierarchy { node_id, reason } => {
                write!(f, "invalid component hierarchy on {node_id}: {reason}")
            }
            Self::InvalidProps { node_id, reason } => {
                write!(f, "invalid component props on {node_id}: {reason}")
            }
            Self::MountMismatch { expected, actual } => {
                write!(f, "mount mismatch: expected {expected}, got {actual}")
            }
            Self::StaleGeneration { expected, actual } => {
                write!(f, "stale generation: expected {expected}, got {actual}")
            }
            Self::RevisionMismatch { expected, actual } => {
                write!(f, "revision mismatch: expected {expected}, got {actual}")
            }
            Self::EventRejected { node_id, reason } => {
                write!(f, "UI event rejected for {node_id}: {reason}")
            }
            Self::SlotFaulted {
                mount_id,
                fault_count,
            } => write!(
                f,
                "UI slot {mount_id} circuit-broken after {fault_count} faults"
            ),
            Self::SlotError(msg) => write!(f, "UI slot error: {msg}"),
        }
    }
}

impl Error for UiHostError {}

impl From<UiHostError> for ApiError {
    fn from(err: UiHostError) -> Self {
        let code = match &err {
            UiHostError::UnsupportedProtocol(_) => ErrorCode::UnsupportedVersion,
            UiHostError::PayloadTooLarge { .. }
            | UiHostError::BudgetExceeded { .. }
            | UiHostError::SlotFaulted { .. } => ErrorCode::BudgetExceeded,
            UiHostError::StaleGeneration { .. }
            | UiHostError::RevisionMismatch { .. }
            | UiHostError::MountMismatch { .. } => ErrorCode::StaleContext,
            UiHostError::SlotNotFound(_) | UiHostError::NodeNotFound(_) => ErrorCode::NotFound,
            UiHostError::SlotAlreadyMounted(_) => ErrorCode::InvalidState,
            _ => ErrorCode::InvalidRequest,
        };
        ApiError::new(code, err.to_string())
    }
}

#[derive(Default)]
struct WalkStats {
    node_count: usize,
    text_bytes: usize,
    event_bindings: usize,
    seen_ids: BTreeSet<String>,
}

/// Validates a bounded JSON tree payload and returns the decoded `UiTree`.
pub fn decode_and_validate_tree(bytes: &[u8], budgets: UiBudgets) -> Result<UiTree, UiHostError> {
    let budgets = budgets.clamp_to_ceiling();
    if bytes.len() > budgets.max_payload_bytes {
        return Err(UiHostError::PayloadTooLarge {
            actual: bytes.len(),
            max: budgets.max_payload_bytes,
        });
    }
    let tree: UiTree = serde_json::from_slice(bytes)
        .map_err(|_| UiHostError::SlotError("invalid UI tree JSON shape or value".into()))?;
    validate_tree(&tree, budgets)?;
    Ok(tree)
}

/// Validates a bounded JSON patch payload and returns the decoded `UiPatch`.
pub fn decode_and_validate_patch(bytes: &[u8], budgets: UiBudgets) -> Result<UiPatch, UiHostError> {
    let budgets = budgets.clamp_to_ceiling();
    if bytes.len() > budgets.max_payload_bytes {
        return Err(UiHostError::PayloadTooLarge {
            actual: bytes.len(),
            max: budgets.max_payload_bytes,
        });
    }
    let patch: UiPatch = serde_json::from_slice(bytes)
        .map_err(|_| UiHostError::SlotError("invalid UI patch JSON shape or value".into()))?;
    validate_patch(&patch, budgets)?;
    Ok(patch)
}

/// Validates a complete `UiTree` against protocol version, budgets, whitelist, and a11y rules.
pub fn validate_tree(tree: &UiTree, budgets: UiBudgets) -> Result<(), UiHostError> {
    let budgets = budgets.clamp_to_ceiling();
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
    validate_node(&tree.root, 1, budgets, &mut stats)
}

/// Validates patch header invariants before tree application.
pub fn validate_patch(patch: &UiPatch, budgets: UiBudgets) -> Result<(), UiHostError> {
    let budgets = budgets.clamp_to_ceiling();
    if patch.protocol_version != PLUGIN_UI_PROTOCOL {
        return Err(UiHostError::UnsupportedProtocol(patch.protocol_version));
    }
    validate_opaque_id(&patch.mount_id, "mount_id")?;
    if patch.generation == 0 {
        return Err(UiHostError::StaleGeneration {
            expected: 1,
            actual: 0,
        });
    }
    if patch.ops.is_empty() || patch.ops.len() > budgets.max_patch_ops {
        return Err(UiHostError::BudgetExceeded {
            metric: "patch_ops",
            actual: patch.ops.len(),
            max: budgets.max_patch_ops,
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

/// Validates a plugin manifest slot descriptor.
pub fn validate_slot_descriptor(slot: &SlotDescriptor) -> Result<(), UiHostError> {
    validate_namespace(
        &slot.contribution_id,
        crate::MAX_CONTRIBUTION_ID_BYTES,
        "contribution_id",
    )?;
    validate_namespace(&slot.role, MAX_ID_BYTES, "role")?;
    if slot.min_width_pt > MAX_SLOT_MIN_WIDTH_PT {
        return Err(UiHostError::BudgetExceeded {
            metric: "min_width_pt",
            actual: usize::from(slot.min_width_pt),
            max: usize::from(MAX_SLOT_MIN_WIDTH_PT),
        });
    }
    Ok(())
}

/// Validates an inbound or outbound `UiEventEnvelope` syntax and identifier bounds.
pub fn validate_event_envelope(envelope: &UiEventEnvelope) -> Result<(), UiHostError> {
    if envelope.protocol_version != PLUGIN_UI_PROTOCOL {
        return Err(UiHostError::UnsupportedProtocol(envelope.protocol_version));
    }
    if envelope.generation == 0 {
        return Err(UiHostError::StaleGeneration {
            expected: 1,
            actual: 0,
        });
    }
    validate_opaque_id(&envelope.mount_id, "mount_id")?;
    validate_opaque_id(&envelope.node_id, "node_id")?;
    validate_opaque_id(&envelope.action_id, "action_id")?;
    match &envelope.action {
        UiAction::Press | UiAction::Toggled { .. } => Ok(()),
        UiAction::TextChanged { value } | UiAction::TextSubmitted { value } => {
            let mut stats = WalkStats::default();
            check_clean_text(
                &envelope.node_id,
                value,
                true,
                UiBudgets::default(),
                &mut stats,
            )
        }
        UiAction::Selected { option_id } => validate_opaque_id(option_id, "option_id"),
        UiAction::TabChanged { tab_id } => validate_opaque_id(tab_id, "tab_id"),
    }
}

/// Checks an opaque identifier (`[A-Za-z0-9][A-Za-z0-9._:-]*`, bounded to `MAX_ID_BYTES`).
pub fn validate_opaque_id(value: &str, field: &'static str) -> Result<(), UiHostError> {
    if value.len() > MAX_ID_BYTES
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

fn validate_namespace(value: &str, limit: usize, field: &'static str) -> Result<(), UiHostError> {
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

fn validate_node(
    node: &UiNode,
    depth: usize,
    budgets: UiBudgets,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    if depth > budgets.max_depth {
        return Err(UiHostError::BudgetExceeded {
            metric: "depth",
            actual: depth,
            max: budgets.max_depth,
        });
    }
    stats.node_count += 1;
    if stats.node_count > budgets.max_nodes {
        return Err(UiHostError::BudgetExceeded {
            metric: "nodes",
            actual: stats.node_count,
            max: budgets.max_nodes,
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
    if node.children.len() > budgets.max_children_per_node {
        return Err(UiHostError::BudgetExceeded {
            metric: "children_per_node",
            actual: node.children.len(),
            max: budgets.max_children_per_node,
        });
    }

    validate_component_props(&node.id, &node.component, budgets, stats)?;
    validate_accessibility(node, budgets, stats)?;

    for child in &node.children {
        validate_node(child, depth + 1, budgets, stats)?;
    }
    Ok(())
}

fn check_clean_text(
    node_id: &str,
    text: &str,
    allow_newlines: bool,
    budgets: UiBudgets,
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
    if stats.text_bytes > budgets.max_text_bytes {
        return Err(UiHostError::BudgetExceeded {
            metric: "text_bytes",
            actual: stats.text_bytes,
            max: budgets.max_text_bytes,
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
    budgets: UiBudgets,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    for action_id in component.action_ids() {
        validate_action_id(node_id, action_id)?;
        stats.event_bindings += 1;
        if stats.event_bindings > budgets.max_event_bindings {
            return Err(UiHostError::BudgetExceeded {
                metric: "event_bindings",
                actual: stats.event_bindings,
                max: budgets.max_event_bindings,
            });
        }
    }

    match component {
        UiComponent::Section { title } => {
            check_clean_text(node_id, title, false, budgets, stats)?;
        }
        UiComponent::Split { ratio_percent } => {
            if !(MIN_SPLIT_RATIO_PERCENT..=MAX_SPLIT_RATIO_PERCENT).contains(ratio_percent) {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "split ratio_percent must be within 10..=90",
                });
            }
        }
        UiComponent::Text { content } => {
            check_clean_text(node_id, content, true, budgets, stats)?;
        }
        UiComponent::Badge { label, .. } => {
            if label.trim().is_empty() {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "badge label must not be empty",
                });
            }
            check_clean_text(node_id, label, false, budgets, stats)?;
        }
        UiComponent::EmptyState { title, description } => {
            check_clean_text(node_id, title, false, budgets, stats)?;
            check_clean_text(node_id, description, true, budgets, stats)?;
        }
        UiComponent::KeyValue { key, value } => {
            check_clean_text(node_id, key, false, budgets, stats)?;
            check_clean_text(node_id, value, true, budgets, stats)?;
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
            check_clean_text(node_id, title, false, budgets, stats)?;
            if let Some(sub) = subtitle {
                check_clean_text(node_id, sub, false, budgets, stats)?;
            }
        }
        UiComponent::Button { label, .. } => {
            if label.trim().is_empty() {
                return Err(UiHostError::InvalidProps {
                    node_id: node_id.to_owned(),
                    reason: "button label must not be empty",
                });
            }
            check_clean_text(node_id, label, false, budgets, stats)?;
        }
        UiComponent::IconButton { icon_name, .. } => {
            if !VALID_ICON_NAMES.contains(&icon_name.as_str()) {
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
            check_clean_text(node_id, value, *multiline, budgets, stats)?;
            check_clean_text(node_id, placeholder, false, budgets, stats)?;
        }
        UiComponent::Toggle { label, .. } => {
            check_clean_text(node_id, label, false, budgets, stats)?;
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
                check_clean_text(node_id, &opt.label, false, budgets, stats)?;
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
                check_clean_text(node_id, &item.label, false, budgets, stats)?;
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

fn validate_accessibility(
    node: &UiNode,
    budgets: UiBudgets,
    stats: &mut WalkStats,
) -> Result<(), UiHostError> {
    if let Some(label) = &node.accessibility.label {
        if label.trim().is_empty() || label.chars().any(char::is_control) {
            return Err(UiHostError::AccessibilityViolation {
                node_id: node.id.clone(),
                reason: "accessibility.label must be non-empty without control chars",
            });
        }
        check_clean_text(&node.id, label, false, budgets, stats)?;
    }
    if let Some(hint) = &node.accessibility.hint {
        check_clean_text(&node.id, hint, false, budgets, stats)?;
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
            UiComponent::ListItem { title, .. } => !title.trim().is_empty(),
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
