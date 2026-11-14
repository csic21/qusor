// TypeScript wire mirrors for `crates/extension-api/src/ui.rs`.
// Keep field names and tagged union discriminators aligned 1:1 with the Rust schema.

export const PLUGIN_UI_PROTOCOL = 1 as const;
export const UI_SLOT_PROTOCOL = 1 as const;

export type SemanticToken =
    | "surface_base"
    | "surface_toolbar"
    | "surface_inspector"
    | "surface_control"
    | "surface_selected"
    | "surface_hover"
    | "surface_pressed"
    | "surface_inset"
    | "surface_accent"
    | "surface_danger"
    | "border_subtle"
    | "border_control"
    | "border_danger"
    | "text_primary"
    | "text_secondary"
    | "text_accent"
    | "text_danger"
    | "focus_ring"
    | "primary"
    | "primary_foreground"
    | "muted"
    | "muted_foreground";

export type SpacingScale = "none" | "xs" | "sm" | "md" | "lg" | "xl";

export type TypographyRole =
    | "caption"
    | "body"
    | "body_strong"
    | "subtitle"
    | "title"
    | "mono";

export type Align = "start" | "center" | "end" | "stretch";

export interface NodeStyle {
    background?: SemanticToken;
    foreground?: SemanticToken;
    border?: SemanticToken;
    padding?: SpacingScale;
    gap?: SpacingScale;
    align?: Align;
    typography?: TypographyRole;
    flex_grow?: boolean;
    compact_hide?: boolean;
}

export type AccessibilityRole =
    | "button"
    | "tab"
    | "text_field"
    | "checkbox"
    | "switch"
    | "list"
    | "list_item"
    | "heading"
    | "status"
    | "region";

export interface AccessibilityProps {
    label?: string;
    hint?: string;
    role?: AccessibilityRole;
    selected?: boolean;
    expanded?: boolean;
    busy?: boolean;
}

export interface SelectOption {
    id: string;
    label: string;
}

export interface TabOption {
    id: string;
    label: string;
}

export type ControlledSurfaceKind =
    | "reader_document"
    | "pdf_page_preview"
    | "markdown_preview";

export type UiComponent =
    | { kind: "v_stack" }
    | { kind: "h_stack" }
    | { kind: "scroll" }
    | { kind: "card" }
    | { kind: "section"; title: string }
    | { kind: "split"; ratio_percent: number }
    | { kind: "text"; content: string }
    | { kind: "badge"; label: string; tone: SemanticToken }
    | { kind: "divider" }
    | { kind: "empty_state"; title: string; description: string }
    | { kind: "key_value"; key: string; value: string }
    | { kind: "progress"; completed: number; total: number }
    | { kind: "list" }
    | {
          kind: "list_item";
          title: string;
          subtitle?: string;
          selected?: boolean;
          action_id?: string;
      }
    | {
          kind: "button";
          label: string;
          primary?: boolean;
          destructive?: boolean;
          disabled?: boolean;
          action_id: string;
      }
    | {
          kind: "icon_button";
          icon_name: string;
          disabled?: boolean;
          action_id: string;
      }
    | {
          kind: "text_input";
          value: string;
          placeholder: string;
          multiline?: boolean;
          disabled?: boolean;
          on_change_action: string;
          on_submit_action?: string;
      }
    | {
          kind: "toggle";
          label: string;
          checked: boolean;
          disabled?: boolean;
          action_id: string;
      }
    | {
          kind: "select";
          selected_id: string;
          options: SelectOption[];
          disabled?: boolean;
          action_id: string;
      }
    | {
          kind: "tabs";
          active_tab_id: string;
          items: TabOption[];
          action_id: string;
      }
    | {
          kind: "controlled_surface_ref";
          surface_id: string;
          surface_kind: ControlledSurfaceKind;
      };

export interface UiNode {
    id: string;
    component: UiComponent;
    style?: NodeStyle;
    accessibility?: AccessibilityProps;
    children?: UiNode[];
}

export interface UiTree {
    protocol_version: number;
    revision: number;
    root: UiNode;
}

export type UiPatchOp =
    | { op: "replace_tree"; root: UiNode }
    | {
          op: "set_props";
          target_id: string;
          component: UiComponent;
          style?: NodeStyle;
          accessibility?: AccessibilityProps;
      }
    | { op: "insert_child"; parent_id: string; index: number; node: UiNode }
    | { op: "remove_node"; target_id: string }
    | { op: "replace_subtree"; target_id: string; node: UiNode };

export interface UiPatch {
    protocol_version: number;
    mount_id: string;
    generation: number;
    base_revision: number;
    next_revision: number;
    ops: UiPatchOp[];
}

export type UiAction =
    | { kind: "press" }
    | { kind: "text_changed"; value: string }
    | { kind: "text_submitted"; value: string }
    | { kind: "toggled"; checked: boolean }
    | { kind: "selected"; option_id: string }
    | { kind: "tab_changed"; tab_id: string };

export interface UiEventEnvelope {
    protocol_version: number;
    mount_id: string;
    generation: number;
    node_id: string;
    action_id: string;
    action: UiAction;
}

export type SlotRegion =
    | "shell_workspaces"
    | "workspace_toolbar"
    | "workspace_leading"
    | "workspace_content"
    | "workspace_auxiliary"
    | "workspace_context_actions"
    | "workspace_object_status"
    | "workspace_output";

export interface SlotDescriptor {
    contribution_id: string;
    region: SlotRegion;
    role: string;
    min_width_pt: number;
    supports_compact: boolean;
}

export interface UiSlotRegistration {
    slot_protocol: number;
    api_version: { major: number; minor: number };
    descriptor: SlotDescriptor;
    required_capabilities?: string[];
}

export interface UiBudgets {
    max_payload_bytes: number;
    max_nodes: number;
    max_depth: number;
    max_children_per_node: number;
    max_text_bytes: number;
    max_patch_ops: number;
    max_event_bindings: number;
}
