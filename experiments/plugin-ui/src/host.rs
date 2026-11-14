//! Host-side slot mount, atomic patch applicator, generation-fenced event
//! router, theme token resolver, and `capability-composition` bridge.

use capability_composition::{
    CompositionLayer, CompositionPlan, CompositionProfile, Entry, ServiceId, ServiceProvision,
    ServiceRequirement, ServiceVersion, VersionRequirement,
};

use crate::{
    schema::{
        PLUGIN_UI_PROTOCOL, SemanticToken, SlotDescriptor, SlotRegion, UiBudget, UiEventEnvelope,
        UiEventKind, UiHostError, UiNode, UiPatch, UiPatchOp, UiTree,
    },
    validator::{validate_opaque_id, validate_patch, validate_slot_descriptor, validate_tree},
};

/// Service identifier representing host declarative UI slot support (`ui.slot`).
pub const UI_SLOT_SERVICE: &str = "ui.slot";

/// Registered plugin slot declaration bound to a package & capability service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotRegistration {
    pub entry_id: String,
    pub package_id: String,
    pub descriptor: SlotDescriptor,
    pub required_services: Vec<(String, u16, u16)>,
}

/// Resolves host + plugin UI slots via `capability-composition` before mounting.
pub fn resolve_slot_composition(
    profile_id: &str,
    slots: &[SlotRegistration],
) -> Result<CompositionPlan, UiHostError> {
    let ui_slot_svc = ServiceId::parse(UI_SLOT_SERVICE).map_err(map_comp_err)?;
    let host_entry = Entry::new("app.qusor.ui.host", "org.qusor.host")
        .map_err(map_comp_err)?
        .required(true)
        .providing(ServiceProvision::new(
            ui_slot_svc.clone(),
            ServiceVersion::new(1, 0),
        ));

    let mut entries = Vec::with_capacity(slots.len() + 1);
    entries.push(host_entry);

    for slot in slots {
        validate_slot_descriptor(&slot.descriptor)?;
        let mut entry = Entry::new(&slot.entry_id, &slot.package_id)
            .map_err(map_comp_err)?
            .requiring(ServiceRequirement::new(
                ui_slot_svc.clone(),
                VersionRequirement::new(1, 0),
            ));
        for (svc, major, min_minor) in &slot.required_services {
            entry = entry.requiring(ServiceRequirement::new(
                ServiceId::parse(svc).map_err(map_comp_err)?,
                VersionRequirement::new(*major, *min_minor),
            ));
        }
        // Each `(region, role)` pair can also be modeled as a provided slot service
        // when strict single-provider uniqueness is desired for that role.
        let role_service = format!(
            "slot.{}.{}",
            region_key(slot.descriptor.region),
            slot.descriptor.role
        );
        entry = entry.providing(ServiceProvision::new(
            ServiceId::parse(&role_service).map_err(map_comp_err)?,
            ServiceVersion::new(1, 0),
        ));
        entries.push(entry);
    }

    let layer = CompositionLayer::new("slots", entries).map_err(map_comp_err)?;
    let profile = CompositionProfile::new(profile_id)
        .map_err(map_comp_err)?
        .with_layer(layer)
        .map_err(map_comp_err)?;
    profile.resolve().map_err(map_comp_err)
}

fn map_comp_err(err: capability_composition::CompositionError) -> UiHostError {
    UiHostError::SlotError(err.to_string())
}

fn region_key(region: SlotRegion) -> &'static str {
    match region {
        SlotRegion::ShellWorkspaces => "shell_workspaces",
        SlotRegion::WorkspaceToolbar => "workspace_toolbar",
        SlotRegion::WorkspaceLeading => "workspace_leading",
        SlotRegion::WorkspaceContent => "workspace_content",
        SlotRegion::WorkspaceAuxiliary => "workspace_auxiliary",
        SlotRegion::WorkspaceContextActions => "workspace_context_actions",
        SlotRegion::WorkspaceObjectStatus => "workspace_object_status",
        SlotRegion::WorkspaceOutput => "workspace_output",
    }
}

/// Host-side mounted UI slot instance with generation fencing and atomic patches.
#[derive(Clone, Debug)]
pub struct MountedSlot {
    mount_id: String,
    generation: u64,
    provider_package: String,
    descriptor: SlotDescriptor,
    budget: UiBudget,
    active_tree: Option<UiTree>,
    frozen: bool,
    fault_count: u32,
}

impl MountedSlot {
    /// Creates a new mounted slot bound to a verified provider and region.
    pub fn mount(
        mount_id: impl Into<String>,
        provider_package: impl Into<String>,
        descriptor: SlotDescriptor,
        budget: UiBudget,
    ) -> Result<Self, UiHostError> {
        let mount_id = mount_id.into();
        validate_opaque_id(&mount_id, "mount_id")?;
        validate_slot_descriptor(&descriptor)?;
        Ok(Self {
            mount_id,
            generation: 1,
            provider_package: provider_package.into(),
            descriptor,
            budget: budget.clamp_to_ceiling(),
            active_tree: None,
            frozen: false,
            fault_count: 0,
        })
    }

    pub fn mount_id(&self) -> &str {
        &self.mount_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn provider_package(&self) -> &str {
        &self.provider_package
    }

    pub fn descriptor(&self) -> &SlotDescriptor {
        &self.descriptor
    }

    pub fn active_tree(&self) -> Option<&UiTree> {
        self.active_tree.as_ref()
    }

    pub fn fault_count(&self) -> u32 {
        self.fault_count
    }

    /// Mounts an initial full tree snapshot after full validation.
    pub fn set_initial_tree(&mut self, tree: UiTree) -> Result<(), UiHostError> {
        if self.frozen {
            return Err(UiHostError::SlotError("slot input/output is frozen".into()));
        }
        validate_tree(&tree, self.budget)?;
        self.active_tree = Some(tree);
        Ok(())
    }

    /// Atomically applies a `UiPatch`, rolling back completely on any error.
    pub fn apply_patch(&mut self, patch: &UiPatch) -> Result<(), UiHostError> {
        if self.frozen {
            return Err(UiHostError::SlotError(
                "slot is frozen during teardown".into(),
            ));
        }
        if patch.mount_id != self.mount_id {
            return Err(UiHostError::MountMismatch {
                expected: self.mount_id.clone(),
                actual: patch.mount_id.clone(),
            });
        }
        if patch.generation != self.generation {
            return Err(UiHostError::StaleGeneration {
                expected: self.generation,
                actual: patch.generation,
            });
        }
        if let Err(err) = validate_patch(patch, self.budget) {
            self.fault_count = self.fault_count.saturating_add(1);
            return Err(err);
        }

        let current_revision = self.active_tree.as_ref().map_or(0, |t| t.revision);
        if patch.base_revision != current_revision {
            self.fault_count = self.fault_count.saturating_add(1);
            return Err(UiHostError::RevisionMismatch {
                expected: current_revision,
                actual: patch.base_revision,
            });
        }

        let mut working = match self.active_tree.clone() {
            Some(tree) => tree,
            None => match patch.ops.first() {
                Some(UiPatchOp::ReplaceTree { root }) => UiTree {
                    protocol_version: PLUGIN_UI_PROTOCOL,
                    revision: 0,
                    root: root.clone(),
                },
                _ => {
                    self.fault_count = self.fault_count.saturating_add(1);
                    return Err(UiHostError::SlotError(
                        "first patch on empty slot must start with replace_tree".into(),
                    ));
                }
            },
        };

        for op in &patch.ops {
            if let Err(err) = apply_op(&mut working, op) {
                self.fault_count = self.fault_count.saturating_add(1);
                return Err(err);
            }
        }
        working.revision = patch.next_revision;

        if let Err(err) = validate_tree(&working, self.budget) {
            self.fault_count = self.fault_count.saturating_add(1);
            return Err(err);
        }

        self.active_tree = Some(working);
        Ok(())
    }

    /// Validates an incoming user event against current mount, generation, node, and action.
    pub fn build_event(
        &self,
        node_id: &str,
        action_id: &str,
        event: UiEventKind,
    ) -> Result<UiEventEnvelope, UiHostError> {
        if self.frozen {
            return Err(UiHostError::EventRejected {
                node_id: node_id.to_owned(),
                reason: "slot is frozen",
            });
        }
        let tree = self
            .active_tree
            .as_ref()
            .ok_or_else(|| UiHostError::EventRejected {
                node_id: node_id.to_owned(),
                reason: "no active UI tree",
            })?;
        let node = find_node(&tree.root, node_id).ok_or_else(|| UiHostError::EventRejected {
            node_id: node_id.to_owned(),
            reason: "target node not found in active tree",
        })?;
        if !node.component.is_interactive() || node.component.is_disabled() {
            return Err(UiHostError::EventRejected {
                node_id: node_id.to_owned(),
                reason: "node is not interactive or is disabled",
            });
        }
        if !node.component.action_ids().contains(&action_id) {
            return Err(UiHostError::EventRejected {
                node_id: node_id.to_owned(),
                reason: "action_id is not subscribed on node",
            });
        }
        Ok(UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: self.mount_id.clone(),
            generation: self.generation,
            node_id: node_id.to_owned(),
            action_id: action_id.to_owned(),
            event,
        })
    }

    /// Verifies an event envelope before delivering it to the plugin runtime.
    pub fn verify_inbound_event(&self, envelope: &UiEventEnvelope) -> Result<(), UiHostError> {
        if envelope.protocol_version != PLUGIN_UI_PROTOCOL {
            return Err(UiHostError::UnsupportedProtocol(envelope.protocol_version));
        }
        if envelope.mount_id != self.mount_id {
            return Err(UiHostError::MountMismatch {
                expected: self.mount_id.clone(),
                actual: envelope.mount_id.clone(),
            });
        }
        if envelope.generation != self.generation {
            return Err(UiHostError::StaleGeneration {
                expected: self.generation,
                actual: envelope.generation,
            });
        }
        self.build_event(
            &envelope.node_id,
            &envelope.action_id,
            envelope.event.clone(),
        )?;
        Ok(())
    }

    /// Freezes current input and switches the slot to a replacement provider,
    /// invalidating all prior generation events and patches (ADR 0028).
    pub fn replace_provider(
        &mut self,
        new_provider_package: impl Into<String>,
        new_descriptor: SlotDescriptor,
    ) -> Result<(), UiHostError> {
        validate_slot_descriptor(&new_descriptor)?;
        self.frozen = true;
        self.active_tree = None;
        self.generation = self.generation.saturating_add(1);
        self.provider_package = new_provider_package.into();
        self.descriptor = new_descriptor;
        self.fault_count = 0;
        self.frozen = false;
        Ok(())
    }
}

/// Resolves semantic tokens to `apps/qusor/theme.json` color values.
pub fn resolve_theme_token(token: SemanticToken, dark: bool) -> &'static str {
    match (token, dark) {
        (SemanticToken::SurfaceBase, false) => "#FFFFFF",
        (SemanticToken::SurfaceBase, true) => "#18191C",
        (SemanticToken::SurfaceToolbar, false) => "#FAFAFB",
        (SemanticToken::SurfaceToolbar, true) => "#202125",
        (SemanticToken::SurfaceInspector, false) => "#FAFAFB",
        (SemanticToken::SurfaceInspector, true) => "#202125",
        (SemanticToken::SurfaceControl, false) => "#FFFFFF",
        (SemanticToken::SurfaceControl, true) => "#35373C",
        (SemanticToken::SurfaceSelected, false) => "#E8EDFC",
        (SemanticToken::SurfaceSelected, true) => "#30445F",
        (SemanticToken::SurfaceHover, false) => "#E9ECF1",
        (SemanticToken::SurfaceHover, true) => "#30333A",
        (SemanticToken::SurfacePressed, false) => "#DBE0E8",
        (SemanticToken::SurfacePressed, true) => "#3D424D",
        (SemanticToken::SurfaceInset, false) => "#F7F8FA",
        (SemanticToken::SurfaceInset, true) => "#222429",
        (SemanticToken::SurfaceAccent, false) => "#EBEFFF",
        (SemanticToken::SurfaceAccent, true) => "#293D59",
        (SemanticToken::SurfaceDanger, false) => "#FDECEC",
        (SemanticToken::SurfaceDanger, true) => "#3D2224",
        (SemanticToken::BorderSubtle, false) => "#E3E5E9",
        (SemanticToken::BorderSubtle, true) => "#30333A",
        (SemanticToken::BorderControl, false) => "#C9CED7",
        (SemanticToken::BorderControl, true) => "#4C525E",
        (SemanticToken::BorderDanger, false) => "#F3B7B7",
        (SemanticToken::BorderDanger, true) => "#6E3639",
        (SemanticToken::TextPrimary, false) => "#25272B",
        (SemanticToken::TextPrimary, true) => "#EAECF0",
        (SemanticToken::TextSecondary, false) => "#686D76",
        (SemanticToken::TextSecondary, true) => "#A3AAB7",
        (SemanticToken::TextAccent, false) => "#3C63CC",
        (SemanticToken::TextAccent, true) => "#78B6FF",
        (SemanticToken::TextDanger, false) => "#C82829",
        (SemanticToken::TextDanger, true) => "#FF7A78",
        (SemanticToken::FocusRing, false) => "#007AFF",
        (SemanticToken::FocusRing, true) => "#78B6FF",
        (SemanticToken::Primary, false) => "#1768BE",
        (SemanticToken::Primary, true) => "#78B6FF",
        (SemanticToken::PrimaryForeground, false) => "#FFFFFF",
        (SemanticToken::PrimaryForeground, true) => "#18191C",
        (SemanticToken::Muted, false) => "#F4F4F6",
        (SemanticToken::Muted, true) => "#222429",
        (SemanticToken::MutedForeground, false) => "#686D76",
        (SemanticToken::MutedForeground, true) => "#A3AAB7",
    }
}

fn apply_op(tree: &mut UiTree, op: &UiPatchOp) -> Result<(), UiHostError> {
    match op {
        UiPatchOp::ReplaceTree { root } => {
            tree.root = root.clone();
            Ok(())
        }
        UiPatchOp::SetProps {
            target_id,
            component,
            style,
            accessibility,
        } => {
            let node = find_node_mut(&mut tree.root, target_id)
                .ok_or_else(|| UiHostError::NodeNotFound(target_id.clone()))?;
            node.component = component.clone();
            node.style = style.clone();
            node.accessibility = accessibility.clone();
            Ok(())
        }
        UiPatchOp::InsertChild {
            parent_id,
            index,
            node,
        } => {
            let parent = find_node_mut(&mut tree.root, parent_id)
                .ok_or_else(|| UiHostError::NodeNotFound(parent_id.clone()))?;
            if !parent.component.allows_children() {
                return Err(UiHostError::InvalidComponentHierarchy {
                    node_id: parent_id.clone(),
                    reason: "cannot insert child into leaf component",
                });
            }
            if *index > parent.children.len() {
                return Err(UiHostError::SlotError(format!(
                    "insert index {index} out of bounds for parent {parent_id}"
                )));
            }
            parent.children.insert(*index, node.clone());
            Ok(())
        }
        UiPatchOp::RemoveNode { target_id } => {
            if tree.root.id == *target_id {
                return Err(UiHostError::InvalidComponentHierarchy {
                    node_id: target_id.clone(),
                    reason: "cannot remove root node; use replace_tree",
                });
            }
            if !remove_child(&mut tree.root, target_id) {
                return Err(UiHostError::NodeNotFound(target_id.clone()));
            }
            Ok(())
        }
        UiPatchOp::ReplaceSubtree { target_id, node } => {
            let target = find_node_mut(&mut tree.root, target_id)
                .ok_or_else(|| UiHostError::NodeNotFound(target_id.clone()))?;
            *target = node.clone();
            Ok(())
        }
    }
}

fn find_node<'a>(node: &'a UiNode, target_id: &str) -> Option<&'a UiNode> {
    if node.id == target_id {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node(child, target_id))
}

fn find_node_mut<'a>(node: &'a mut UiNode, target_id: &str) -> Option<&'a mut UiNode> {
    if node.id == target_id {
        return Some(node);
    }
    node.children
        .iter_mut()
        .find_map(|child| find_node_mut(child, target_id))
}

fn remove_child(node: &mut UiNode, target_id: &str) -> bool {
    if let Some(pos) = node.children.iter().position(|c| c.id == target_id) {
        node.children.remove(pos);
        return true;
    }
    node.children
        .iter_mut()
        .any(|child| remove_child(child, target_id))
}
