/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable workbench/frame model and selectors.

use std::collections::HashMap;

use graphshell_core::graph::{GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;
use serde::{Deserialize, Serialize};
pub use verso_tile::surface::TileSlot;
use verso_tile::surface::{SurfaceHostId, SurfacePlacementPlan, SurfaceSlotPlacement};

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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchProjection {
    pub active_frame: Option<FrameId>,
    pub frame_label: Option<String>,
    pub root_view: Option<GraphViewId>,
    pub panes: Vec<ProjectedPane>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedPane {
    pub pane_id: PaneId,
    pub node: NodeKey,
    pub surface_host: Option<SurfaceHostId>,
    pub is_primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangementSnapshot {
    pub container: ArrangementContainer,
    pub members: Vec<ArrangementMember>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangementContainer {
    Frame {
        id: FrameId,
        label: String,
        root_view: Option<GraphViewId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangementMember {
    pub pane_id: PaneId,
    pub node: NodeKey,
    pub surface_host: Option<SurfaceHostId>,
    pub slot: TileSlot,
}

/// Select the currently active frame when it exists in the frame map.
pub fn select_active_frame<'a>(
    frames: &'a HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<&'a FrameState> {
    active_frame.and_then(|frame_id| frames.get(frame_id))
}

pub fn project_frame(frame: &FrameState) -> WorkbenchProjection {
    WorkbenchProjection {
        active_frame: Some(frame.id.clone()),
        frame_label: Some(frame.label.clone()),
        root_view: frame.root_view,
        panes: frame
            .panes
            .iter()
            .enumerate()
            .map(|(index, binding)| ProjectedPane {
                pane_id: binding.pane_id,
                node: binding.node,
                surface_host: binding.surface_host.clone(),
                is_primary: index == 0,
            })
            .collect(),
    }
}

pub fn project_active_workbench(
    frames: &HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> WorkbenchProjection {
    select_active_frame(frames, active_frame)
        .map(project_frame)
        .unwrap_or_default()
}

pub fn snapshot_frame_arrangement(frame: &FrameState) -> ArrangementSnapshot {
    ArrangementSnapshot {
        container: ArrangementContainer::Frame {
            id: frame.id.clone(),
            label: frame.label.clone(),
            root_view: frame.root_view,
        },
        members: frame
            .panes
            .iter()
            .enumerate()
            .map(|(index, binding)| ArrangementMember {
                pane_id: binding.pane_id,
                node: binding.node,
                surface_host: binding.surface_host.clone(),
                slot: slot_for_index(index),
            })
            .collect(),
    }
}

pub fn slot_for_index(index: usize) -> TileSlot {
    if index == 0 {
        TileSlot::primary()
    } else {
        TileSlot::secondary(index)
    }
}

pub fn snapshot_active_arrangement(
    frames: &HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<ArrangementSnapshot> {
    select_active_frame(frames, active_frame).map(snapshot_frame_arrangement)
}

pub fn project_surface_placements(snapshot: &ArrangementSnapshot) -> SurfacePlacementPlan {
    let view = match &snapshot.container {
        ArrangementContainer::Frame { root_view, .. } => *root_view,
    };
    let mut plan = SurfacePlacementPlan::default();
    for member in &snapshot.members {
        if let Some(host) = member.surface_host.clone() {
            plan.push(SurfaceSlotPlacement::new(
                host,
                view,
                member.pane_id,
                member.slot,
            ));
        }
    }
    plan
}

pub fn project_active_surface_placements(
    frames: &HashMap<FrameId, FrameState>,
    active_frame: Option<&FrameId>,
) -> Option<SurfacePlacementPlan> {
    snapshot_active_arrangement(frames, active_frame)
        .map(|snapshot| project_surface_placements(&snapshot))
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

pub fn set_view_and_frame_surface_host(
    view_bindings: &mut [PaneBinding],
    frame: Option<&mut FrameState>,
    pane_id: PaneId,
    surface_host: Option<SurfaceHostId>,
) -> Option<bool> {
    let view_changed = set_binding_surface_host(view_bindings, pane_id, surface_host.clone())?;
    let frame_changed = if let Some(frame) = frame {
        set_binding_surface_host(&mut frame.panes, pane_id, surface_host)?
    } else {
        false
    };
    Some(view_changed || frame_changed)
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

pub fn assign_view_and_frame_pane(
    view_bindings: &mut Vec<PaneBinding>,
    frame: Option<&mut FrameState>,
    view_id: GraphViewId,
    binding: PaneBinding,
) -> bool {
    let view_changed = upsert_pane_binding(view_bindings, binding.clone());
    let frame_changed = frame
        .map(|frame| assign_frame_pane(frame, view_id, binding))
        .unwrap_or(false);
    view_changed || frame_changed
}

pub fn clear_frame_pane(frame: &mut FrameState, pane_id: PaneId) -> bool {
    remove_pane_binding(&mut frame.panes, pane_id).is_some()
}

pub fn remove_view_and_frame_pane(
    view_bindings: &mut Vec<PaneBinding>,
    frame: Option<&mut FrameState>,
    pane_id: PaneId,
) -> Option<PaneBinding> {
    let removed = remove_pane_binding(view_bindings, pane_id)?;
    if let Some(frame) = frame {
        clear_frame_pane(frame, pane_id);
    }
    Some(removed)
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
    fn project_active_workbench_returns_plain_frame_packet() {
        let frame_id = FrameId::new("main");
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(8));
        let pane = PaneBinding {
            pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(18)),
            node: NodeKey::new(9),
            surface_host: Some(SurfaceHostId::new("desktop")),
        };
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: Some(root_view),
            panes: vec![pane.clone()],
        };
        let frames = HashMap::from([(frame_id.clone(), frame)]);

        let projection = project_active_workbench(&frames, Some(&frame_id));

        assert_eq!(projection.active_frame, Some(frame_id));
        assert_eq!(projection.root_view, Some(root_view));
        assert_eq!(projection.panes.len(), 1);
        assert_eq!(projection.panes[0].pane_id, pane.pane_id);
        assert!(projection.panes[0].is_primary);
    }

    #[test]
    fn project_active_workbench_is_empty_when_frame_missing() {
        let frames = HashMap::new();
        let frame_id = FrameId::new("missing");

        let projection = project_active_workbench(&frames, Some(&frame_id));

        assert_eq!(projection, WorkbenchProjection::default());
    }

    #[test]
    fn snapshot_active_arrangement_returns_plain_membership_packet() {
        let frame_id = FrameId::new("main");
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(19));
        let first = PaneBinding {
            pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(20)),
            node: NodeKey::new(10),
            surface_host: Some(SurfaceHostId::new("desktop")),
        };
        let second = PaneBinding {
            pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(21)),
            node: NodeKey::new(11),
            surface_host: None,
        };
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: Some(root_view),
            panes: vec![first.clone(), second.clone()],
        };
        let frames = HashMap::from([(frame_id.clone(), frame)]);

        let snapshot = snapshot_active_arrangement(&frames, Some(&frame_id)).unwrap();

        assert_eq!(
            snapshot.container,
            ArrangementContainer::Frame {
                id: frame_id,
                label: "Main".to_string(),
                root_view: Some(root_view),
            }
        );
        assert_eq!(snapshot.members.len(), 2);
        assert_eq!(snapshot.members[0].pane_id, first.pane_id);
        assert_eq!(snapshot.members[0].slot.index, 0);
        assert!(snapshot.members[0].slot.is_primary);
        assert_eq!(snapshot.members[1].pane_id, second.pane_id);
        assert_eq!(snapshot.members[1].slot.index, 1);
        assert!(!snapshot.members[1].slot.is_primary);
    }

    #[test]
    fn project_active_surface_placements_filters_unhosted_members() {
        let frame_id = FrameId::new("main");
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(22));
        let first = PaneBinding {
            pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(23)),
            node: NodeKey::new(12),
            surface_host: Some(SurfaceHostId::new("desktop")),
        };
        let second = PaneBinding {
            pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(24)),
            node: NodeKey::new(13),
            surface_host: None,
        };
        let frame = FrameState {
            id: frame_id.clone(),
            label: "Main".to_string(),
            root_view: Some(root_view),
            panes: vec![first.clone(), second],
        };
        let frames = HashMap::from([(frame_id.clone(), frame)]);

        let plan = project_active_surface_placements(&frames, Some(&frame_id)).unwrap();

        assert_eq!(plan.len(), 1);
        let placement = plan.placement_for_pane(first.pane_id).unwrap();
        assert_eq!(placement.view, Some(root_view));
        assert_eq!(placement.slot, TileSlot::primary());
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

    #[test]
    fn assign_view_and_frame_pane_keeps_view_and_frame_in_sync() {
        let pane_id = PaneId::from_uuid(uuid::Uuid::from_u128(12));
        let root_view = GraphViewId::from_uuid(uuid::Uuid::from_u128(13));
        let binding = PaneBinding {
            pane_id,
            node: NodeKey::new(4),
            surface_host: Some(SurfaceHostId::new("desktop")),
        };
        let mut view_bindings = Vec::new();
        let mut frame = FrameState::default();

        assert!(assign_view_and_frame_pane(
            &mut view_bindings,
            Some(&mut frame),
            root_view,
            binding.clone(),
        ));
        assert_eq!(view_bindings, vec![binding.clone()]);
        assert_eq!(frame.root_view, Some(root_view));
        assert_eq!(frame.panes, vec![binding]);
    }

    #[test]
    fn remove_view_and_frame_pane_removes_from_both_collections() {
        let pane_id = PaneId::from_uuid(uuid::Uuid::from_u128(14));
        let binding = PaneBinding {
            pane_id,
            node: NodeKey::new(5),
            surface_host: None,
        };
        let mut view_bindings = vec![binding.clone()];
        let mut frame = FrameState {
            panes: vec![binding.clone()],
            ..FrameState::default()
        };

        let removed = remove_view_and_frame_pane(&mut view_bindings, Some(&mut frame), pane_id);

        assert_eq!(removed, Some(binding));
        assert!(view_bindings.is_empty());
        assert!(frame.panes.is_empty());
    }

    #[test]
    fn set_view_and_frame_surface_host_keeps_bindings_in_sync() {
        let pane_id = PaneId::from_uuid(uuid::Uuid::from_u128(15));
        let binding = PaneBinding {
            pane_id,
            node: NodeKey::new(6),
            surface_host: None,
        };
        let host = SurfaceHostId::new("desktop");
        let mut view_bindings = vec![binding.clone()];
        let mut frame = FrameState {
            panes: vec![binding],
            ..FrameState::default()
        };

        let changed = set_view_and_frame_surface_host(
            &mut view_bindings,
            Some(&mut frame),
            pane_id,
            Some(host.clone()),
        );

        assert_eq!(changed, Some(true));
        assert_eq!(view_bindings[0].surface_host, Some(host.clone()));
        assert_eq!(frame.panes[0].surface_host, Some(host));
    }
}
