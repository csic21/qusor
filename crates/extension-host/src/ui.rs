//! Host-side declarative plugin UI slot state machine, validator, atomic patch
//! applicator, generation-fenced event verifier, theme token resolver, and
//! `ui.slot` capability authorization.

use std::collections::BTreeMap;

use extension_api::{
    ApiError, ApiVersion, Capability, CapabilityRequest, ErrorCode, HOST_API_V0_7, Resource,
    is_supported_api,
    ui::{
        PLUGIN_UI_PROTOCOL, SemanticToken, SlotDescriptor, UiAction, UiBudgets, UiComponent,
        UiEventEnvelope, UiHostError, UiNode, UiPatch, UiPatchOp, UiSlotRegistration, UiTree,
        validate_event_envelope, validate_opaque_id, validate_patch, validate_slot_descriptor,
        validate_tree,
    },
};

use crate::{Host, Permit, Session};

/// Default consecutive fault threshold before a slot is circuit-broken until
/// replaced or reset by the host shell.
pub const DEFAULT_MAX_SLOT_FAULTS: u32 = 8;

impl Host {
    /// Authorizes a declarative UI slot mount or update request against this
    /// host's vault and the installation's `ui.slot` grant (Host API >= 0.7).
    pub fn authorize_ui_slot(
        &self,
        session: &Session,
        api_version: ApiVersion,
        request_id: impl Into<String>,
    ) -> Result<Permit, ApiError> {
        if !is_supported_api(api_version) || api_version < HOST_API_V0_7 {
            return Err(ApiError::new(
                ErrorCode::UnsupportedVersion,
                "ui.slot requires minimum Host API 0.7",
            ));
        }
        self.authorize(
            session,
            CapabilityRequest {
                api_version,
                request_id: request_id.into(),
                capability: Capability::UiSlot,
                resource: Resource::Vault {
                    vault_id: self.vault_id().to_owned(),
                },
            },
        )
    }
}

/// Host-side mounted UI slot instance with generation fencing, atomic patches,
/// and circuit-breaker fault accounting.
#[derive(Clone, Debug)]
pub struct MountedSlot {
    mount_id: String,
    generation: u64,
    provider_package: String,
    descriptor: SlotDescriptor,
    budgets: UiBudgets,
    active_tree: Option<UiTree>,
    frozen: bool,
    fault_count: u32,
    max_faults: u32,
}

impl MountedSlot {
    /// Creates a new mounted slot bound to a verified provider and region.
    pub fn mount(
        mount_id: impl Into<String>,
        provider_package: impl Into<String>,
        descriptor: SlotDescriptor,
        budgets: UiBudgets,
    ) -> Result<Self, UiHostError> {
        let mount_id = mount_id.into();
        validate_opaque_id(&mount_id, "mount_id")?;
        validate_slot_descriptor(&descriptor)?;
        Ok(Self {
            mount_id,
            generation: 1,
            provider_package: provider_package.into(),
            descriptor,
            budgets: budgets.clamp_to_ceiling(),
            active_tree: None,
            frozen: false,
            fault_count: 0,
            max_faults: DEFAULT_MAX_SLOT_FAULTS,
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

    pub fn budgets(&self) -> UiBudgets {
        self.budgets
    }

    pub fn active_tree(&self) -> Option<&UiTree> {
        self.active_tree.as_ref()
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn fault_count(&self) -> u32 {
        self.fault_count
    }

    pub fn is_faulted(&self) -> bool {
        self.fault_count >= self.max_faults
    }

    /// Freezes input/output on this slot (e.g. during ADR 0028 provider teardown).
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Resumes input/output on this slot when not circuit-broken.
    pub fn unfreeze(&mut self) {
        self.frozen = false;
    }

    /// Mounts an initial full tree snapshot after full validation.
    pub fn set_initial_tree(&mut self, tree: UiTree) -> Result<(), UiHostError> {
        self.ensure_accepting_updates()?;
        if let Err(err) = validate_tree(&tree, self.budgets) {
            self.record_fault();
            return Err(err);
        }
        self.active_tree = Some(tree);
        Ok(())
    }

    /// Atomically applies a `UiPatch`, rolling back completely on any error.
    pub fn apply_patch(&mut self, patch: &UiPatch) -> Result<(), UiHostError> {
        self.ensure_accepting_updates()?;
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
        if let Err(err) = validate_patch(patch, self.budgets) {
            self.record_fault();
            return Err(err);
        }

        let current_revision = self.active_tree.as_ref().map_or(0, |t| t.revision);
        if patch.base_revision != current_revision {
            self.record_fault();
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
                    self.record_fault();
                    return Err(UiHostError::SlotError(
                        "first patch on empty slot must start with replace_tree".into(),
                    ));
                }
            },
        };

        for op in &patch.ops {
            if let Err(err) = apply_op(&mut working, op) {
                self.record_fault();
                return Err(err);
            }
        }
        working.revision = patch.next_revision;

        if let Err(err) = validate_tree(&working, self.budgets) {
            self.record_fault();
            return Err(err);
        }

        self.active_tree = Some(working);
        Ok(())
    }

    /// Validates an outbound user event against current mount, generation, node, and action.
    pub fn build_event(
        &self,
        node_id: &str,
        action_id: &str,
        action: UiAction,
    ) -> Result<UiEventEnvelope, UiHostError> {
        if self.frozen {
            return Err(UiHostError::EventRejected {
                node_id: node_id.to_owned(),
                reason: "slot is frozen",
            });
        }
        if self.is_faulted() {
            return Err(UiHostError::SlotFaulted {
                mount_id: self.mount_id.clone(),
                fault_count: self.fault_count,
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
        validate_action_for_component(node_id, &node.component, action_id, &action)?;

        let envelope = UiEventEnvelope {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: self.mount_id.clone(),
            generation: self.generation,
            node_id: node_id.to_owned(),
            action_id: action_id.to_owned(),
            action,
        };
        validate_event_envelope(&envelope)?;
        Ok(envelope)
    }

    /// Verifies an event envelope before delivering it to the plugin runtime.
    pub fn verify_inbound_event(&self, envelope: &UiEventEnvelope) -> Result<(), UiHostError> {
        validate_event_envelope(envelope)?;
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
            envelope.action.clone(),
        )?;
        Ok(())
    }

    /// Freezes current input, clears the active tree, advances `generation`, and
    /// switches the slot to a replacement provider (ADR 0028).
    pub fn replace_provider(
        &mut self,
        new_provider_package: impl Into<String>,
        new_descriptor: SlotDescriptor,
    ) -> Result<u64, UiHostError> {
        validate_slot_descriptor(&new_descriptor)?;
        self.frozen = true;
        self.active_tree = None;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| UiHostError::SlotError("slot generation counter exhausted".into()))?;
        self.provider_package = new_provider_package.into();
        self.descriptor = new_descriptor;
        self.fault_count = 0;
        self.frozen = false;
        Ok(self.generation)
    }

    fn ensure_accepting_updates(&self) -> Result<(), UiHostError> {
        if self.frozen {
            return Err(UiHostError::SlotError("slot input/output is frozen".into()));
        }
        if self.is_faulted() {
            return Err(UiHostError::SlotFaulted {
                mount_id: self.mount_id.clone(),
                fault_count: self.fault_count,
            });
        }
        Ok(())
    }

    fn record_fault(&mut self) {
        self.fault_count = self.fault_count.saturating_add(1);
    }
}

/// Multi-slot declarative UI host controller enforcing budgets, validation,
/// atomic patch rollback, fault isolation, and generation fencing.
#[derive(Debug)]
pub struct PluginUiHost {
    budgets: UiBudgets,
    max_faults_per_slot: u32,
    slots: BTreeMap<String, MountedSlot>,
}

impl Default for PluginUiHost {
    fn default() -> Self {
        Self::new(UiBudgets::default())
    }
}

impl PluginUiHost {
    /// Creates a new declarative UI host enforcing `budgets.clamp_to_ceiling()`.
    pub fn new(budgets: UiBudgets) -> Self {
        Self {
            budgets: budgets.clamp_to_ceiling(),
            max_faults_per_slot: DEFAULT_MAX_SLOT_FAULTS,
            slots: BTreeMap::new(),
        }
    }

    /// Sets the consecutive fault threshold before a slot is circuit-broken.
    pub fn with_max_faults_per_slot(mut self, max_faults: u32) -> Self {
        self.max_faults_per_slot = max_faults.max(1);
        self
    }

    pub fn budgets(&self) -> UiBudgets {
        self.budgets
    }

    pub fn slot(&self, mount_id: &str) -> Option<&MountedSlot> {
        self.slots.get(mount_id)
    }

    pub fn slots(&self) -> &BTreeMap<String, MountedSlot> {
        &self.slots
    }

    /// Mounts a validated slot descriptor under `mount_id`.
    pub fn mount_slot(
        &mut self,
        mount_id: impl Into<String>,
        provider_package: impl Into<String>,
        descriptor: SlotDescriptor,
    ) -> Result<&MountedSlot, UiHostError> {
        let mount_id = mount_id.into();
        if self.slots.contains_key(&mount_id) {
            return Err(UiHostError::SlotAlreadyMounted(mount_id));
        }
        let mut slot = MountedSlot::mount(&mount_id, provider_package, descriptor, self.budgets)?;
        slot.max_faults = self.max_faults_per_slot;
        self.slots.insert(mount_id.clone(), slot);
        Ok(self.slots.get(&mount_id).expect("inserted slot"))
    }

    /// Mounts a slot from a validated `UiSlotRegistration` wire contract.
    pub fn mount_registered_slot(
        &mut self,
        mount_id: impl Into<String>,
        provider_package: impl Into<String>,
        registration: &UiSlotRegistration,
    ) -> Result<&MountedSlot, UiHostError> {
        registration
            .validate()
            .map_err(|err| UiHostError::SlotError(err.message))?;
        self.mount_slot(mount_id, provider_package, registration.descriptor.clone())
    }

    /// Unmounts and removes a slot, discarding its active tree and generation.
    pub fn unmount_slot(&mut self, mount_id: &str) -> Result<MountedSlot, UiHostError> {
        self.slots
            .remove(mount_id)
            .ok_or_else(|| UiHostError::SlotNotFound(mount_id.to_owned()))
    }

    /// Sets an initial validated `UiTree` snapshot on a mounted slot.
    pub fn set_initial_tree(&mut self, mount_id: &str, tree: UiTree) -> Result<(), UiHostError> {
        self.slot_mut(mount_id)?.set_initial_tree(tree)
    }

    /// Decodes a bounded JSON `UiTree` payload and mounts it on `mount_id`.
    pub fn apply_tree_json(&mut self, mount_id: &str, json: &[u8]) -> Result<(), UiHostError> {
        let slot = self.slot_mut(mount_id)?;
        slot.ensure_accepting_updates()?;
        match UiTree::from_slice_with_budgets(json, slot.budgets) {
            Ok(tree) => slot.set_initial_tree(tree),
            Err(err) => {
                slot.record_fault();
                Err(err)
            }
        }
    }

    /// Atomically applies a `UiPatch` to its target `patch.mount_id` slot.
    pub fn apply_patch(&mut self, patch: &UiPatch) -> Result<(), UiHostError> {
        self.slot_mut(&patch.mount_id)?.apply_patch(patch)
    }

    /// Decodes a bounded JSON `UiPatch` payload and applies it atomically.
    pub fn apply_patch_json(&mut self, json: &[u8]) -> Result<(), UiHostError> {
        let patch = UiPatch::from_slice_with_budgets(json, self.budgets)?;
        self.apply_patch(&patch)
    }

    /// Replaces the provider on an existing slot, clearing the tree and advancing
    /// `generation` so any late patches or events from the old provider fail closed.
    pub fn replace_provider(
        &mut self,
        mount_id: &str,
        new_provider_package: impl Into<String>,
        new_descriptor: SlotDescriptor,
    ) -> Result<u64, UiHostError> {
        self.slot_mut(mount_id)?
            .replace_provider(new_provider_package, new_descriptor)
    }

    /// Freezes a slot during teardown/transition.
    pub fn freeze_slot(&mut self, mount_id: &str) -> Result<(), UiHostError> {
        self.slot_mut(mount_id)?.freeze();
        Ok(())
    }

    /// Unfreezes a slot.
    pub fn unfreeze_slot(&mut self, mount_id: &str) -> Result<(), UiHostError> {
        self.slot_mut(mount_id)?.unfreeze();
        Ok(())
    }

    /// Constructs a generation-fenced `UiEventEnvelope` from host renderer interaction.
    pub fn build_event(
        &self,
        mount_id: &str,
        node_id: &str,
        action_id: &str,
        action: UiAction,
    ) -> Result<UiEventEnvelope, UiHostError> {
        self.slots
            .get(mount_id)
            .ok_or_else(|| UiHostError::SlotNotFound(mount_id.to_owned()))?
            .build_event(node_id, action_id, action)
    }

    /// Verifies an inbound `UiEventEnvelope` before forwarding it to the plugin runtime.
    pub fn verify_inbound_event(&self, envelope: &UiEventEnvelope) -> Result<(), UiHostError> {
        self.slots
            .get(&envelope.mount_id)
            .ok_or_else(|| UiHostError::SlotNotFound(envelope.mount_id.clone()))?
            .verify_inbound_event(envelope)
    }

    fn slot_mut(&mut self, mount_id: &str) -> Result<&mut MountedSlot, UiHostError> {
        self.slots
            .get_mut(mount_id)
            .ok_or_else(|| UiHostError::SlotNotFound(mount_id.to_owned()))
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

fn validate_action_for_component(
    node_id: &str,
    component: &UiComponent,
    action_id: &str,
    action: &UiAction,
) -> Result<(), UiHostError> {
    match (component, action) {
        (UiComponent::Button { .. } | UiComponent::IconButton { .. }, UiAction::Press) => Ok(()),
        (
            UiComponent::ListItem {
                action_id: Some(_), ..
            },
            UiAction::Press,
        ) => Ok(()),
        (UiComponent::Toggle { .. }, UiAction::Toggled { .. }) => Ok(()),
        (
            UiComponent::TextInput {
                on_change_action, ..
            },
            UiAction::TextChanged { .. },
        ) if action_id == on_change_action => Ok(()),
        (
            UiComponent::TextInput {
                on_submit_action: Some(submit),
                ..
            },
            UiAction::TextSubmitted { .. },
        ) if action_id == submit => Ok(()),
        (UiComponent::Select { options, .. }, UiAction::Selected { option_id }) => {
            if options.iter().any(|opt| opt.id == *option_id) {
                Ok(())
            } else {
                Err(UiHostError::EventRejected {
                    node_id: node_id.to_owned(),
                    reason: "selected option_id does not exist on select node",
                })
            }
        }
        (UiComponent::Tabs { items, .. }, UiAction::TabChanged { tab_id }) => {
            if items.iter().any(|item| item.id == *tab_id) {
                Ok(())
            } else {
                Err(UiHostError::EventRejected {
                    node_id: node_id.to_owned(),
                    reason: "tab_id does not exist on tabs node",
                })
            }
        }
        _ => Err(UiHostError::EventRejected {
            node_id: node_id.to_owned(),
            reason: "action kind does not match target component",
        }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Grant;
    use extension_api::{
        ApiRange, Contribution, ContributionKind, HOST_API_V0_6, Manifest, PackageVersion,
        ui::{
            AccessibilityProps, AccessibilityRole, ControlledSurfaceKind, HARD_MAX_DEPTH,
            HARD_MAX_NODES, HARD_MAX_PAYLOAD_BYTES, NodeStyle, SelectOption, SlotRegion,
            SpacingScale, TabOption, TypographyRole, UI_SLOT_PROTOCOL,
        },
    };

    fn descriptor(contribution_id: &str, role: &str) -> SlotDescriptor {
        SlotDescriptor {
            contribution_id: contribution_id.into(),
            region: SlotRegion::WorkspaceContent,
            role: role.into(),
            min_width_pt: 320,
            supports_compact: true,
        }
    }

    fn node(id: &str, component: UiComponent, children: Vec<UiNode>) -> UiNode {
        UiNode {
            id: id.into(),
            component,
            style: NodeStyle::default(),
            accessibility: AccessibilityProps::default(),
            children,
        }
    }

    fn sample_tree(revision: u64) -> UiTree {
        UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision,
            root: node(
                "root",
                UiComponent::VStack,
                vec![
                    UiNode {
                        id: "header".into(),
                        component: UiComponent::Text {
                            content: "Workspace".into(),
                        },
                        style: NodeStyle {
                            foreground: Some(SemanticToken::TextPrimary),
                            typography: Some(TypographyRole::Title),
                            padding: Some(SpacingScale::Md),
                            ..NodeStyle::default()
                        },
                        accessibility: AccessibilityProps {
                            role: Some(AccessibilityRole::Heading),
                            ..AccessibilityProps::default()
                        },
                        children: vec![],
                    },
                    node(
                        "progress",
                        UiComponent::Progress {
                            completed: 3,
                            total: 10,
                        },
                        vec![],
                    ),
                    node(
                        "submit-btn",
                        UiComponent::Button {
                            label: "Continue".into(),
                            primary: true,
                            destructive: false,
                            disabled: false,
                            action_id: "action.continue".into(),
                        },
                        vec![],
                    ),
                ],
            ),
        }
    }

    #[test]
    fn budgets_clamp_to_system_hard_ceilings() {
        let oversized = UiBudgets {
            max_payload_bytes: 10 * 1024 * 1024,
            max_nodes: 999_999,
            max_depth: 500,
            max_children_per_node: 5000,
            max_text_bytes: 10 * 1024 * 1024,
            max_patch_ops: 9999,
            max_event_bindings: 9999,
        };
        let clamped = oversized.clamp_to_ceiling();
        assert_eq!(clamped.max_payload_bytes, HARD_MAX_PAYLOAD_BYTES);
        assert_eq!(clamped.max_nodes, HARD_MAX_NODES);
        assert_eq!(clamped.max_depth, HARD_MAX_DEPTH);
    }

    #[test]
    fn rejects_budget_exceeded_nodes_depth_and_payload() {
        let mut host = PluginUiHost::new(UiBudgets {
            max_nodes: 3,
            max_depth: 4,
            max_children_per_node: 2,
            max_text_bytes: 128,
            max_payload_bytes: 512,
            max_patch_ops: 4,
            max_event_bindings: 2,
        });
        host.mount_slot(
            "slot-1",
            "org.example.widget",
            descriptor("org.example.widget.main", "workspace.main"),
        )
        .unwrap();

        // 3 children > max_children_per_node (2)
        let too_many_children = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "root",
                UiComponent::VStack,
                vec![
                    node("c1", UiComponent::Divider, vec![]),
                    node("c2", UiComponent::Divider, vec![]),
                    node("c3", UiComponent::Divider, vec![]),
                ],
            ),
        };
        assert_eq!(
            host.set_initial_tree("slot-1", too_many_children)
                .unwrap_err(),
            UiHostError::BudgetExceeded {
                metric: "children_per_node",
                actual: 3,
                max: 2,
            }
        );

        // 4 nodes across 2 levels > max_nodes (3), within max_children_per_node (2)
        let too_many_nodes = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "root",
                UiComponent::VStack,
                vec![
                    node(
                        "c1",
                        UiComponent::VStack,
                        vec![node("c1a", UiComponent::Divider, vec![])],
                    ),
                    node("c2", UiComponent::Divider, vec![]),
                ],
            ),
        };
        assert_eq!(
            host.set_initial_tree("slot-1", too_many_nodes).unwrap_err(),
            UiHostError::BudgetExceeded {
                metric: "nodes",
                actual: 4,
                max: 3,
            }
        );

        // Depth 4 > max_depth (3)
        let too_deep = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "d1",
                UiComponent::VStack,
                vec![node(
                    "d2",
                    UiComponent::VStack,
                    vec![node(
                        "d3",
                        UiComponent::VStack,
                        vec![node("d4", UiComponent::Divider, vec![])],
                    )],
                )],
            ),
        };
        // Node count here is 4 as well; test depth specifically with max_nodes = 10
        let mut depth_host = PluginUiHost::new(UiBudgets {
            max_depth: 3,
            ..UiBudgets::default()
        });
        depth_host
            .mount_slot(
                "slot-depth",
                "org.example.widget",
                descriptor("org.example.widget.main", "workspace.main"),
            )
            .unwrap();
        assert_eq!(
            depth_host
                .set_initial_tree("slot-depth", too_deep)
                .unwrap_err(),
            UiHostError::BudgetExceeded {
                metric: "depth",
                actual: 4,
                max: 3,
            }
        );

        // Payload bytes > 512
        let oversized_json = format!(
            "{{\"protocol_version\":1,\"revision\":1,\"root\":{{\"id\":\"r\",\"component\":{{\"kind\":\"text\",\"content\":\"{}\"}}}}}}",
            "a".repeat(600)
        );
        assert!(matches!(
            host.apply_tree_json("slot-1", oversized_json.as_bytes()),
            Err(UiHostError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_unknown_components_raw_css_html_urls_and_invalid_props() {
        let unknown_component = r#"{
            "protocol_version": 1,
            "revision": 1,
            "root": { "id": "r", "component": { "kind": "web_view", "url": "https://evil.example" } }
        }"#;
        assert!(UiTree::from_json(unknown_component).is_err());

        let raw_css_field = r#"{
            "protocol_version": 1,
            "revision": 1,
            "root": {
                "id": "r",
                "component": { "kind": "text", "content": "hi" },
                "style": { "css": "color: red;" }
            }
        }"#;
        assert!(UiTree::from_json(raw_css_field).is_err());

        let unknown_token = r##"{
            "protocol_version": 1,
            "revision": 1,
            "root": {
                "id": "r",
                "component": { "kind": "text", "content": "hi" },
                "style": { "background": "#ff0000" }
            }
        }"##;
        assert!(UiTree::from_json(unknown_token).is_err());

        // Arbitrary path/URL in ControlledSurfaceRef is rejected.
        let url_surface = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "surf",
                UiComponent::ControlledSurfaceRef {
                    surface_id: "file:///etc/passwd".into(),
                    surface_kind: ControlledSurfaceKind::PdfPagePreview,
                },
                vec![],
            ),
        };
        assert!(matches!(
            url_surface.validate(UiBudgets::default()),
            Err(UiHostError::InvalidProps { .. })
        ));

        // Leaf component with children rejected.
        let leaf_with_children = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "txt",
                UiComponent::Text {
                    content: "leaf".into(),
                },
                vec![node("child", UiComponent::Divider, vec![])],
            ),
        };
        assert!(matches!(
            leaf_with_children.validate(UiBudgets::default()),
            Err(UiHostError::InvalidComponentHierarchy { .. })
        ));

        // Invalid icon outside host whitelist rejected.
        let bad_icon = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: UiNode {
                id: "ico".into(),
                component: UiComponent::IconButton {
                    icon_name: "arbitrary-svg".into(),
                    disabled: false,
                    action_id: "act".into(),
                },
                style: NodeStyle::default(),
                accessibility: AccessibilityProps {
                    label: Some("Action".into()),
                    ..AccessibilityProps::default()
                },
                children: vec![],
            },
        };
        assert!(matches!(
            bad_icon.validate(UiBudgets::default()),
            Err(UiHostError::InvalidProps { .. })
        ));
    }

    #[test]
    fn enforces_accessibility_label_gate_on_interactive_nodes() {
        // IconButton has no intrinsic text label so explicit accessibility.label is mandatory.
        let missing_a11y = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "icon-btn",
                UiComponent::IconButton {
                    icon_name: "speaker".into(),
                    disabled: false,
                    action_id: "speak".into(),
                },
                vec![],
            ),
        };
        assert!(matches!(
            missing_a11y.validate(UiBudgets::default()),
            Err(UiHostError::AccessibilityViolation { .. })
        ));

        // Blank accessibility label rejected even on non-interactive nodes.
        let blank_a11y = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: UiNode {
                id: "txt".into(),
                component: UiComponent::Text {
                    content: "hello".into(),
                },
                style: NodeStyle::default(),
                accessibility: AccessibilityProps {
                    label: Some("   ".into()),
                    ..AccessibilityProps::default()
                },
                children: vec![],
            },
        };
        assert!(matches!(
            blank_a11y.validate(UiBudgets::default()),
            Err(UiHostError::AccessibilityViolation { .. })
        ));
    }

    #[test]
    fn atomic_patch_rolls_back_completely_on_failure_and_circuit_breaks() {
        let mut host = PluginUiHost::new(UiBudgets::default()).with_max_faults_per_slot(2);
        host.mount_slot(
            "slot-patch",
            "org.example.panel",
            descriptor("org.example.panel.view", "workspace.view"),
        )
        .unwrap();
        let initial = sample_tree(1);
        host.set_initial_tree("slot-patch", initial.clone())
            .unwrap();

        // Multi-op patch: op 1 changes header text, op 2 inserts into missing parent -> full rollback!
        let bad_patch = UiPatch {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-patch".into(),
            generation: 1,
            base_revision: 1,
            next_revision: 2,
            ops: vec![
                UiPatchOp::SetProps {
                    target_id: "header".into(),
                    component: UiComponent::Text {
                        content: "Mutated".into(),
                    },
                    style: NodeStyle::default(),
                    accessibility: AccessibilityProps::default(),
                },
                UiPatchOp::InsertChild {
                    parent_id: "missing-parent".into(),
                    index: 0,
                    node: node("extra", UiComponent::Divider, vec![]),
                },
            ],
        };
        assert!(matches!(
            host.apply_patch(&bad_patch),
            Err(UiHostError::NodeNotFound(_))
        ));
        assert_eq!(
            host.slot("slot-patch").unwrap().active_tree(),
            Some(&initial)
        );
        assert_eq!(host.slot("slot-patch").unwrap().fault_count(), 1);

        // Second fault hits circuit breaker threshold (max_faults = 2).
        let duplicate_id_patch = UiPatch {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-patch".into(),
            generation: 1,
            base_revision: 1,
            next_revision: 2,
            ops: vec![UiPatchOp::InsertChild {
                parent_id: "root".into(),
                index: 0,
                node: node("header", UiComponent::Divider, vec![]),
            }],
        };
        assert!(matches!(
            host.apply_patch(&duplicate_id_patch),
            Err(UiHostError::DuplicateNodeId(_))
        ));
        assert!(host.slot("slot-patch").unwrap().is_faulted());

        // Subsequent valid patch is blocked until provider replacement / recovery.
        let valid_patch = UiPatch {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-patch".into(),
            generation: 1,
            base_revision: 1,
            next_revision: 2,
            ops: vec![UiPatchOp::SetProps {
                target_id: "progress".into(),
                component: UiComponent::Progress {
                    completed: 5,
                    total: 10,
                },
                style: NodeStyle::default(),
                accessibility: AccessibilityProps::default(),
            }],
        };
        assert!(matches!(
            host.apply_patch(&valid_patch),
            Err(UiHostError::SlotFaulted { .. })
        ));
    }

    #[test]
    fn generation_fencing_drops_stale_patches_and_events_across_replace_and_unmount() {
        let mut host = PluginUiHost::default();
        host.mount_slot(
            "slot-gen",
            "org.example.first",
            descriptor("org.example.first.card", "workspace.card"),
        )
        .unwrap();
        host.set_initial_tree("slot-gen", sample_tree(1)).unwrap();

        let gen1_event = host
            .build_event("slot-gen", "submit-btn", "action.continue", UiAction::Press)
            .unwrap();
        host.verify_inbound_event(&gen1_event).unwrap();

        // Replace provider -> clears active tree and increments generation to 2.
        let new_gen = host
            .replace_provider(
                "slot-gen",
                "org.example.second",
                descriptor("org.example.second.card", "workspace.card"),
            )
            .unwrap();
        assert_eq!(new_gen, 2);
        assert!(host.slot("slot-gen").unwrap().active_tree().is_none());

        // Mount a new tree at generation 2 with the same button ID.
        host.set_initial_tree("slot-gen", sample_tree(1)).unwrap();

        // Late patch from generation 1 is rejected.
        let stale_patch = UiPatch {
            protocol_version: PLUGIN_UI_PROTOCOL,
            mount_id: "slot-gen".into(),
            generation: 1,
            base_revision: 1,
            next_revision: 2,
            ops: vec![UiPatchOp::SetProps {
                target_id: "header".into(),
                component: UiComponent::Text {
                    content: "Stale".into(),
                },
                style: NodeStyle::default(),
                accessibility: AccessibilityProps::default(),
            }],
        };
        assert_eq!(
            host.apply_patch(&stale_patch).unwrap_err(),
            UiHostError::StaleGeneration {
                expected: 2,
                actual: 1,
            }
        );

        // Late event from generation 1 is rejected.
        assert_eq!(
            host.verify_inbound_event(&gen1_event).unwrap_err(),
            UiHostError::StaleGeneration {
                expected: 2,
                actual: 1,
            }
        );

        // Unmount invalidates all subsequent patches/events for that mount_id.
        host.unmount_slot("slot-gen").unwrap();
        assert!(matches!(
            host.verify_inbound_event(&gen1_event),
            Err(UiHostError::SlotNotFound(_))
        ));
    }

    #[test]
    fn validates_select_tabs_and_event_kind_matching() {
        let mut host = PluginUiHost::default();
        host.mount_slot(
            "slot-controls",
            "org.example.controls",
            descriptor("org.example.controls.panel", "workspace.controls"),
        )
        .unwrap();
        let tree = UiTree {
            protocol_version: PLUGIN_UI_PROTOCOL,
            revision: 1,
            root: node(
                "root",
                UiComponent::VStack,
                vec![
                    UiNode {
                        id: "mode-select".into(),
                        component: UiComponent::Select {
                            selected_id: "a".into(),
                            options: vec![
                                SelectOption {
                                    id: "a".into(),
                                    label: "Alpha".into(),
                                },
                                SelectOption {
                                    id: "b".into(),
                                    label: "Beta".into(),
                                },
                            ],
                            disabled: false,
                            action_id: "act.select".into(),
                        },
                        style: NodeStyle::default(),
                        accessibility: AccessibilityProps {
                            label: Some("Mode".into()),
                            ..AccessibilityProps::default()
                        },
                        children: vec![],
                    },
                    UiNode {
                        id: "view-tabs".into(),
                        component: UiComponent::Tabs {
                            active_tab_id: "t1".into(),
                            items: vec![
                                TabOption {
                                    id: "t1".into(),
                                    label: "One".into(),
                                },
                                TabOption {
                                    id: "t2".into(),
                                    label: "Two".into(),
                                },
                            ],
                            action_id: "act.tab".into(),
                        },
                        style: NodeStyle::default(),
                        accessibility: AccessibilityProps {
                            label: Some("Views".into()),
                            ..AccessibilityProps::default()
                        },
                        children: vec![],
                    },
                ],
            ),
        };
        host.set_initial_tree("slot-controls", tree).unwrap();

        let ok_select = host
            .build_event(
                "slot-controls",
                "mode-select",
                "act.select",
                UiAction::Selected {
                    option_id: "b".into(),
                },
            )
            .unwrap();
        host.verify_inbound_event(&ok_select).unwrap();

        // Non-existent option_id rejected.
        assert!(matches!(
            host.build_event(
                "slot-controls",
                "mode-select",
                "act.select",
                UiAction::Selected {
                    option_id: "missing".into(),
                },
            ),
            Err(UiHostError::EventRejected { .. })
        ));

        // Mismatched action kind rejected.
        assert!(matches!(
            host.build_event("slot-controls", "view-tabs", "act.tab", UiAction::Press),
            Err(UiHostError::EventRejected { .. })
        ));
    }

    #[test]
    fn host_authorizes_ui_slot_capability_cumulatively_from_v0_7() {
        let mut policy = Host::new("vault-1", [Capability::UiSlot]).unwrap();
        let manifest = Manifest {
            manifest_version: 1,
            package_id: "org.example.slot".into(),
            publisher_id: "org.example".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 7,
                max_minor: 7,
            },
            required_capabilities: vec![Capability::UiSlot],
            optional_capabilities: vec![],
            contributions: vec![Contribution {
                id: "org.example.slot.main".into(),
                title: "Slot".into(),
                kind: ContributionKind::UiSlot,
                required_capabilities: vec![Capability::UiSlot],
            }],
        };
        let reg = UiSlotRegistration {
            slot_protocol: UI_SLOT_PROTOCOL,
            api_version: HOST_API_V0_7,
            descriptor: descriptor("org.example.slot.main", "workspace.main"),
            required_capabilities: vec![Capability::UiSlot],
        };
        reg.validate_for_manifest(&manifest).unwrap();

        let id = policy.register(manifest, "a".repeat(64)).unwrap();
        policy
            .replace_grants(
                id,
                vec![Grant {
                    capability: Capability::UiSlot,
                    resource: Resource::Vault {
                        vault_id: "vault-1".into(),
                    },
                }],
            )
            .unwrap();
        policy.enable(id).unwrap();
        let session = policy.open_session_for_api(id, HOST_API_V0_7).unwrap();
        let permit = policy
            .authorize_ui_slot(&session, HOST_API_V0_7, "req-ui-1")
            .unwrap();
        policy
            .dispatch(permit, |ctx| {
                assert_eq!(ctx.request.capability, Capability::UiSlot);
                Ok(())
            })
            .unwrap();

        // Older 0.6 session cannot authorize ui.slot.
        assert_eq!(
            policy
                .authorize_ui_slot(&session, HOST_API_V0_6, "req-ui-old")
                .unwrap_err()
                .code,
            ErrorCode::UnsupportedVersion
        );
    }

    #[test]
    fn theme_tokens_resolve_for_light_and_dark() {
        assert_eq!(
            resolve_theme_token(SemanticToken::SurfaceBase, false),
            "#FFFFFF"
        );
        assert_eq!(
            resolve_theme_token(SemanticToken::SurfaceBase, true),
            "#18191C"
        );
        assert_eq!(
            resolve_theme_token(SemanticToken::Primary, false),
            "#1768BE"
        );
        assert_eq!(resolve_theme_token(SemanticToken::Primary, true), "#78B6FF");
    }

    #[test]
    fn ui_slot_manifest_respects_cumulative_api_bounds() {
        let mut manifest = Manifest {
            manifest_version: 1,
            package_id: "com.example.ui-demo".into(),
            publisher_id: "com.example".into(),
            package_version: PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            host_api: ApiRange {
                major: 0,
                min_minor: 6,
                max_minor: 7,
            },
            required_capabilities: vec![Capability::UiSlot],
            optional_capabilities: vec![],
            contributions: vec![Contribution {
                id: "com.example.ui-demo.main".into(),
                title: "Main Slot".into(),
                kind: ContributionKind::UiSlot,
                required_capabilities: vec![Capability::UiSlot],
            }],
        };
        assert_eq!(
            manifest.validate().unwrap_err().code,
            extension_api::ErrorCode::InvalidManifest
        );

        manifest.host_api = ApiRange {
            major: 0,
            min_minor: 7,
            max_minor: 7,
        };
        manifest.validate().unwrap();
        assert!(!manifest.supports_api(HOST_API_V0_6));
        assert!(manifest.supports_api(extension_api::HOST_API_V0_7));
    }
}
