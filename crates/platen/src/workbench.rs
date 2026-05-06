/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable workbench/frame model and selectors.

use std::collections::HashMap;

use graphshell_core::graph::{GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;
use serde::{Deserialize, Serialize};
use verso_tile::surface::SurfaceHostId;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable pane-to-graph binding consumed by hosts and frame projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneBinding {
    pub pane_id: PaneId,
    pub node: NodeKey,
    pub surface_host: Option<SurfaceHostId>,
}

/// One named frame/workbench composition.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameState {
    pub id: FrameId,
    pub label: String,
    pub root_view: Option<GraphViewId>,
    pub panes: Vec<PaneBinding>,
}

/// Select the currently active frame when it exists in the frame map.
pub fn select_active_frame<'a>(
    frames: &'a HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<&'a FrameState> {
    active_frame.and_then(|frame_id| frames.get(frame_id))
}

pub fn upsert_pane_binding(bindings: &mut Vec<PaneBinding>, binding: PaneBinding) -> bool {
    if let Some(existing) = bindings
        .iter_mut()
        .find(|existing| existing.pane_id == binding.pane_id)
    {
        if existing == &binding {
            return false;
        }
        *existing = binding;
        return true;
    }
    bindings.push(binding);
    true
}

pub fn remove_pane_binding(
    bindings: &mut Vec<PaneBinding>,
    pane_id: PaneId,
) -> Option<PaneBinding> {
    let index = bindings
        .iter()
        .position(|binding| binding.pane_id == pane_id)?;
    Some(bindings.remove(index))
}

pub fn set_binding_surface_host(
    bindings: &mut [PaneBinding],
    pane_id: PaneId,
    surface_host: Option<SurfaceHostId>,
) -> Option<bool> {
    let binding = bindings
        .iter_mut()
        .find(|binding| binding.pane_id == pane_id)?;
    let changed = binding.surface_host != surface_host;
    binding.surface_host = surface_host;
    Some(changed)
}

pub fn set_frame_root_view(frame: &mut FrameState, root_view: Option<GraphViewId>) -> bool {
    let changed = frame.root_view != root_view;
    frame.root_view = root_view;
    changed
}

pub fn assign_frame_pane(
    frame: &mut FrameState,
    view_id: GraphViewId,
    binding: PaneBinding,
) -> bool {
    let root_changed = set_frame_root_view(frame, Some(view_id));
    let pane_changed = upsert_pane_binding(&mut frame.panes, binding);
    root_changed || pane_changed
}

pub fn clear_frame_pane(frame: &mut FrameState, pane_id: PaneId) -> bool {
    remove_pane_binding(&mut frame.panes, pane_id).is_some()
}

/// Select the active root view for workbench-aware composition.
pub fn select_active_root_view(
    frames: &HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<GraphViewId> {
    select_active_frame(frames, active_frame).and_then(|frame| frame.root_view)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn select_active_frame_returns_requested_frame() {
        let frame_id = FrameId::new("main");
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: None,
            panes: Vec::new(),
        };
        let mut frames = HashMap::new();
        frames.insert(frame_id.clone(), frame.clone());

        let selected = select_active_frame(&frames, Some(&frame_id));

        assert_eq!(selected, Some(&frame));
    }

    #[test]
    fn select_active_frame_returns_none_when_missing() {
        let frames = HashMap::new();
        let frame_id = FrameId::new("missing");

        assert_eq!(select_active_frame(&frames, Some(&frame_id)), None);
        assert_eq!(select_active_frame(&frames, None), None);
    }

    #[test]
    fn select_active_root_view_returns_frame_root_view() {
        let frame_id = FrameId::new("main");
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(7));
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: Some(root_view),
            panes: Vec::new(),
        };
        let frames = HashMap::from([(frame_id.clone(), frame)]);

        assert_eq!(
            select_active_root_view(&frames, Some(&frame_id)),
            Some(root_view)
        );
    }

    #[test]
    fn upsert_pane_binding_replaces_existing_pane() {
        let pane_id = PaneId::from_uuid(uuid::Uuid::from_u128(9));
        let first = PaneBinding {
            pane_id,
            node: NodeKey::new(1),
            surface_host: None,
        };
        let second = PaneBinding {
            pane_id,
            node: NodeKey::new(2),
            surface_host: Some(SurfaceHostId::new("desktop")),
        };
        let mut bindings = vec![first];

        assert!(upsert_pane_binding(&mut bindings, second.clone()));
        assert_eq!(bindings, vec![second]);
    }

    #[test]
    fn assign_frame_pane_sets_root_view_and_binding() {
        let pane_id = PaneId::from_uuid(uuid::Uuid::from_u128(10));
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(11));
        let binding = PaneBinding {
            pane_id,
            node: NodeKey::new(3),
            surface_host: None,
        };
        let mut frame = FrameState::default();

        assert!(assign_frame_pane(&mut frame, root_view, binding.clone()));
        assert_eq!(frame.root_view, Some(root_view));
        assert_eq!(frame.panes, vec![binding]);
    }
}
