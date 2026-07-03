/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, hash_map::Entry};

use super::defaults::{
    binding_label, default_binding_specs, graph_view_confirm_binding, legacy_binding,
    toolbar_nav_back_binding, toolbar_nav_forward_binding, toolbar_nav_reload_binding,
    toolbar_submit_binding,
};
use super::{
    InputActionBindingDescriptor, InputBinding, InputBindingRemap, InputConflict, InputContext,
    action_id,
};

pub(super) enum BindingSlot {
    Routed(String),
    Conflict(Vec<String>),
}

pub struct InputBindingResolution {
    pub binding_label: String,
    pub context: InputContext,
    pub action_id: Option<String>,
    pub matched: bool,
    pub conflicted: bool,
}

pub struct InputRegistry {
    pub(super) bindings: HashMap<(InputContext, InputBinding), BindingSlot>,
}

impl InputRegistry {
    fn current_binding_for_action(
        &self,
        action_id: &str,
        context: InputContext,
    ) -> Option<InputBinding> {
        let normalized = action_id.to_ascii_lowercase();
        for ((entry_context, binding), slot) in &self.bindings {
            if *entry_context != context {
                continue;
            }
            if let BindingSlot::Routed(routed) = slot {
                if routed == &normalized {
                    return Some(binding.clone());
                }
            }
        }
        None
    }

    pub fn describe_bindable_actions(&self) -> Vec<InputActionBindingDescriptor> {
        default_binding_specs()
            .into_iter()
            .map(|spec| InputActionBindingDescriptor {
                action_id: spec.action_id.to_string(),
                display_name: spec.display_name,
                context: spec.context,
                section: spec.section,
                current_binding: self.current_binding_for_action(spec.action_id, spec.context),
                default_binding: Some(spec.binding),
            })
            .collect()
    }

    pub fn binding_display_labels_for_action(&self, action_id: &str) -> Vec<String> {
        self.describe_bindable_actions()
            .into_iter()
            .filter(|entry| entry.action_id == action_id)
            .filter_map(|entry| entry.current_binding.map(|binding| binding.display_label()))
            .collect()
    }

    pub fn register_binding(
        &mut self,
        binding: InputBinding,
        action_id: &str,
        context: InputContext,
    ) {
        let normalized_action_id = action_id.to_ascii_lowercase();
        match self.bindings.entry((context, binding)) {
            Entry::Vacant(entry) => {
                entry.insert(BindingSlot::Routed(normalized_action_id));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                BindingSlot::Routed(existing) if *existing == normalized_action_id => {}
                BindingSlot::Routed(existing) => {
                    let actions = vec![existing.clone(), normalized_action_id];
                    entry.insert(BindingSlot::Conflict(actions));
                }
                BindingSlot::Conflict(actions) => {
                    if !actions.contains(&normalized_action_id) {
                        actions.push(normalized_action_id);
                    }
                }
            },
        }
    }

    pub fn resolve(&self, binding: &InputBinding, context: InputContext) -> InputBindingResolution {
        let label = binding_label(binding, context);
        match self.bindings.get(&(context, binding.clone())) {
            Some(BindingSlot::Routed(action_id)) => InputBindingResolution {
                binding_label: label,
                context,
                matched: true,
                conflicted: false,
                action_id: Some(action_id.clone()),
            },
            Some(BindingSlot::Conflict(_)) => InputBindingResolution {
                binding_label: label,
                context,
                matched: false,
                conflicted: true,
                action_id: None,
            },
            None => InputBindingResolution {
                binding_label: label,
                context,
                matched: false,
                conflicted: false,
                action_id: None,
            },
        }
    }

    pub fn resolve_binding_id(&self, binding_id: &str) -> InputBindingResolution {
        if let Some((binding, context)) = legacy_binding(binding_id) {
            return self.resolve(&binding, context);
        }

        InputBindingResolution {
            binding_label: binding_id.to_ascii_lowercase(),
            context: InputContext::DialogOpen,
            action_id: None,
            matched: false,
            conflicted: false,
        }
    }

    pub fn remap_binding(
        &mut self,
        old: InputBinding,
        new: InputBinding,
        context: InputContext,
    ) -> Result<(), InputConflict> {
        if old == new {
            return match self.bindings.get(&(context, old.clone())) {
                Some(BindingSlot::Routed(_)) => Ok(()),
                Some(BindingSlot::Conflict(action_ids)) => Err(InputConflict::SourceConflict {
                    binding_label: binding_label(&old, context),
                    action_ids: action_ids.clone(),
                }),
                None => Err(InputConflict::MissingBinding {
                    binding_label: binding_label(&old, context),
                }),
            };
        }

        let old_key = (context, old.clone());
        let new_key = (context, new.clone());
        let old_slot =
            self.bindings
                .remove(&old_key)
                .ok_or_else(|| InputConflict::MissingBinding {
                    binding_label: binding_label(&old, context),
                })?;

        let action_id = match old_slot {
            BindingSlot::Routed(action_id) => action_id,
            BindingSlot::Conflict(action_ids) => {
                self.bindings
                    .insert(old_key, BindingSlot::Conflict(action_ids.clone()));
                return Err(InputConflict::SourceConflict {
                    binding_label: binding_label(&old, context),
                    action_ids,
                });
            }
        };

        match self.bindings.entry(new_key) {
            Entry::Vacant(entry) => {
                entry.insert(BindingSlot::Routed(action_id));
                Ok(())
            }
            Entry::Occupied(entry) => {
                let conflict = match entry.get() {
                    BindingSlot::Routed(existing) if *existing == action_id => None,
                    BindingSlot::Routed(existing) => Some(InputConflict::TargetConflict {
                        binding_label: binding_label(&new, context),
                        action_ids: vec![existing.clone(), action_id.clone()],
                    }),
                    BindingSlot::Conflict(action_ids) => Some(InputConflict::TargetConflict {
                        binding_label: binding_label(&new, context),
                        action_ids: action_ids.clone(),
                    }),
                };

                self.bindings
                    .insert(old_key, BindingSlot::Routed(action_id));
                match conflict {
                    Some(conflict) => Err(conflict),
                    None => Ok(()),
                }
            }
        }
    }

    pub fn with_remaps(remaps: &[InputBindingRemap]) -> Result<Self, InputConflict> {
        let mut registry = Self::default();
        for remap in remaps {
            registry.remap_binding(remap.old.clone(), remap.new.clone(), remap.context)?;
        }
        Ok(registry)
    }
}

impl Default for InputRegistry {
    fn default() -> Self {
        let mut registry = Self {
            bindings: HashMap::new(),
        };
        registry.register_binding(
            toolbar_submit_binding(),
            action_id::toolbar::SUBMIT,
            InputContext::OmnibarOpen,
        );
        registry.register_binding(
            graph_view_confirm_binding(),
            action_id::graph::VIEW_CONFIRM,
            InputContext::GraphView,
        );
        for spec in default_binding_specs() {
            registry.register_binding(spec.binding, spec.action_id, spec.context);
        }
        registry
    }
}
