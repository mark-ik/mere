/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral workspace routing for graph views, frames, and panes.
//!
//! This is the first portable subset of the donor `workspace_routing` seam. It
//! binds graph nodes to portable pane IDs, keeps view and frame state aligned,
//! updates the GraphTree projection, and emits surface effects for host
//! adapters to consume later.

use graph_tree::{NavAction, Provenance};
use graphshell_core::graph::{GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;

use super::{
    FrameId, GraphWorkspace, PaneBinding, SurfaceEffect, SurfaceHostId, SurfaceRequest,
    WorkspaceEffect,
};

/// Host-neutral workspace routing intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRoutingIntent {
    RouteNodeToPane {
        view_id: GraphViewId,
        frame_id: Option<FrameId>,
        pane_id: PaneId,
        node: NodeKey,
        surface_host: Option<SurfaceHostId>,
    },
    RemovePaneRoute {
        view_id: GraphViewId,
        frame_id: Option<FrameId>,
        pane_id: PaneId,
    },
    SetPaneSurfaceHost {
        view_id: GraphViewId,
        frame_id: Option<FrameId>,
        pane_id: PaneId,
        surface_host: Option<SurfaceHostId>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceRoutingOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRoutingError {
    MissingNode(NodeKey),
    MissingFrame(FrameId),
    MissingGraphView(GraphViewId),
    MissingPane(PaneId),
}

pub type WorkspaceRoutingResult = Result<WorkspaceRoutingOutcome, WorkspaceRoutingError>;

pub fn reduce_workspace_routing_intent(
    workspace: &mut GraphWorkspace,
    intent: WorkspaceRoutingIntent,
) -> WorkspaceRoutingResult {
    match intent {
        WorkspaceRoutingIntent::RouteNodeToPane {
            view_id,
            frame_id,
            pane_id,
            node,
            surface_host,
        } => route_node_to_pane(workspace, view_id, frame_id, pane_id, node, surface_host),
        WorkspaceRoutingIntent::RemovePaneRoute {
            view_id,
            frame_id,
            pane_id,
        } => remove_pane_route(workspace, view_id, frame_id, pane_id),
        WorkspaceRoutingIntent::SetPaneSurfaceHost {
            view_id,
            frame_id,
            pane_id,
            surface_host,
        } => set_pane_surface_host(workspace, view_id, frame_id, pane_id, surface_host),
    }
}

fn route_node_to_pane(
    workspace: &mut GraphWorkspace,
    view_id: GraphViewId,
    frame_id: Option<FrameId>,
    pane_id: PaneId,
    node: NodeKey,
    surface_host: Option<SurfaceHostId>,
) -> WorkspaceRoutingResult {
    if workspace.domain.graph.get_node(node).is_none() {
        return Err(WorkspaceRoutingError::MissingNode(node));
    }
    if let Some(frame_id) = frame_id.as_ref()
        && !workspace.workbench.frames.contains_key(frame_id)
    {
        return Err(WorkspaceRoutingError::MissingFrame(frame_id.clone()));
    }

    let view = workspace.views.ensure_view(view_id);
    let prior_active_node = view.projection.active_node;
    view.tree.apply(NavAction::Attach {
        member: node,
        provenance: Provenance::Manual {
            source: prior_active_node,
            context: Some("pane-route".to_string()),
        },
    });
    view.tree.apply(NavAction::Activate(node));
    let projection_changed = view.projection.active_node != Some(node);
    view.projection.active_node = Some(node);

    let binding = PaneBinding {
        pane_id,
        node,
        surface_host: surface_host.clone(),
    };
    let mut state_changed = projection_changed | upsert_binding(&mut view.panes, binding.clone());

    if let Some(frame_id) = frame_id.as_ref() {
        let frame = workspace
            .workbench
            .frames
            .get_mut(frame_id)
            .expect("frame existence checked");
        if frame.root_view != Some(view_id) {
            frame.root_view = Some(view_id);
            state_changed = true;
        }
        state_changed |= upsert_binding(&mut frame.panes, binding);
    }

    if state_changed {
        workspace.workbench.has_unsaved_changes = true;
    }

    let mut effects_emitted = 0;
    if let Some(host) = surface_host {
        workspace.push_effect(WorkspaceEffect::RequestSurface(SurfaceEffect {
            host,
            view: Some(view_id),
            pane: Some(pane_id),
            request: SurfaceRequest::Present,
        }));
        effects_emitted = 1;
    }

    Ok(WorkspaceRoutingOutcome {
        state_changed,
        effects_emitted,
    })
}

fn remove_pane_route(
    workspace: &mut GraphWorkspace,
    view_id: GraphViewId,
    frame_id: Option<FrameId>,
    pane_id: PaneId,
) -> WorkspaceRoutingResult {
    let view = workspace
        .views
        .graph_views
        .get_mut(&view_id)
        .ok_or(WorkspaceRoutingError::MissingGraphView(view_id))?;
    let removed_view_binding = remove_binding(&mut view.panes, pane_id);
    let Some(removed_view_binding) = removed_view_binding else {
        return Err(WorkspaceRoutingError::MissingPane(pane_id));
    };
    view.tree
        .apply(NavAction::Dismiss(removed_view_binding.node));
    if view.projection.active_node == Some(removed_view_binding.node) {
        view.projection.active_node = None;
    }

    let mut state_changed = true;
    if let Some(frame_id) = frame_id.as_ref() {
        let frame = workspace
            .workbench
            .frames
            .get_mut(frame_id)
            .ok_or_else(|| WorkspaceRoutingError::MissingFrame(frame_id.clone()))?;
        state_changed |= remove_binding(&mut frame.panes, pane_id).is_some();
    }
    workspace.workbench.has_unsaved_changes = true;

    let mut effects_emitted = 0;
    if let Some(host) = removed_view_binding.surface_host {
        workspace.push_effect(WorkspaceEffect::RequestSurface(SurfaceEffect {
            host,
            view: Some(view_id),
            pane: Some(pane_id),
            request: SurfaceRequest::Retire,
        }));
        effects_emitted = 1;
    }

    Ok(WorkspaceRoutingOutcome {
        state_changed,
        effects_emitted,
    })
}

fn set_pane_surface_host(
    workspace: &mut GraphWorkspace,
    view_id: GraphViewId,
    frame_id: Option<FrameId>,
    pane_id: PaneId,
    surface_host: Option<SurfaceHostId>,
) -> WorkspaceRoutingResult {
    let view = workspace
        .views
        .graph_views
        .get_mut(&view_id)
        .ok_or(WorkspaceRoutingError::MissingGraphView(view_id))?;
    let Some(view_binding) = view
        .panes
        .iter_mut()
        .find(|binding| binding.pane_id == pane_id)
    else {
        return Err(WorkspaceRoutingError::MissingPane(pane_id));
    };
    let mut state_changed = view_binding.surface_host != surface_host;
    view_binding.surface_host = surface_host.clone();

    if let Some(frame_id) = frame_id.as_ref() {
        let frame = workspace
            .workbench
            .frames
            .get_mut(frame_id)
            .ok_or_else(|| WorkspaceRoutingError::MissingFrame(frame_id.clone()))?;
        if let Some(frame_binding) = frame
            .panes
            .iter_mut()
            .find(|binding| binding.pane_id == pane_id)
            && frame_binding.surface_host != surface_host
        {
            frame_binding.surface_host = surface_host.clone();
            state_changed = true;
        }
    }

    if state_changed {
        workspace.workbench.has_unsaved_changes = true;
    }

    Ok(WorkspaceRoutingOutcome {
        state_changed,
        effects_emitted: 0,
    })
}

fn upsert_binding(bindings: &mut Vec<PaneBinding>, binding: PaneBinding) -> bool {
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

fn remove_binding(bindings: &mut Vec<PaneBinding>, pane_id: PaneId) -> Option<PaneBinding> {
    let index = bindings
        .iter()
        .position(|binding| binding.pane_id == pane_id)?;
    Some(bindings.remove(index))
}

#[cfg(test)]
mod tests {
    use graphshell_core::geometry::PortablePoint;
    use uuid::Uuid;

    use super::*;
    use crate::app_state::graph_runtime::{GraphRuntimeIntent, reduce_graph_runtime_intent};
    use crate::app_state::{FrameState, SurfaceRequest};

    fn graph_view_id(value: u128) -> GraphViewId {
        GraphViewId::from_uuid(Uuid::from_u128(value))
    }

    fn pane_id(value: u128) -> PaneId {
        PaneId::from_uuid(Uuid::from_u128(value))
    }

    fn add_node(workspace: &mut GraphWorkspace, value: u128) -> NodeKey {
        reduce_graph_runtime_intent(
            workspace,
            GraphRuntimeIntent::AddNode {
                id: Uuid::from_u128(value),
                url: format!("https://example.test/{value}"),
                position: PortablePoint::new(0.0, 0.0),
            },
        )
        .unwrap()
        .node
        .unwrap()
    }

    #[test]
    fn routes_node_to_view_frame_and_surface_effect() {
        let mut workspace = GraphWorkspace::new();
        let node = add_node(&mut workspace, 1);
        workspace.drain_effects();
        let view_id = graph_view_id(2);
        let frame_id = FrameId::new("main");
        let pane_id = pane_id(3);
        let host = SurfaceHostId::new("desktop");
        workspace.workbench.frames.insert(
            frame_id.clone(),
            FrameState {
                id: frame_id.clone(),
                label: "Main".to_string(),
                root_view: None,
                panes: Vec::new(),
            },
        );

        let outcome = reduce_workspace_routing_intent(
            &mut workspace,
            WorkspaceRoutingIntent::RouteNodeToPane {
                view_id,
                frame_id: Some(frame_id.clone()),
                pane_id,
                node,
                surface_host: Some(host.clone()),
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert_eq!(
            workspace.views.graph_views[&view_id].projection.active_node,
            Some(node)
        );
        assert_eq!(workspace.views.graph_views[&view_id].panes.len(), 1);
        assert_eq!(
            workspace.workbench.frames[&frame_id].root_view,
            Some(view_id)
        );
        assert_eq!(workspace.workbench.frames[&frame_id].panes.len(), 1);
        assert!(workspace.workbench.has_unsaved_changes);
        let effects = workspace.drain_effects();
        let WorkspaceEffect::RequestSurface(effect) = &effects[0] else {
            panic!("expected surface effect");
        };
        assert_eq!(effect.host, host);
        assert_eq!(effect.request, SurfaceRequest::Present);
    }

    #[test]
    fn rerouting_same_pane_replaces_binding_without_duplicates() {
        let mut workspace = GraphWorkspace::new();
        let first = add_node(&mut workspace, 10);
        let second = add_node(&mut workspace, 11);
        workspace.drain_effects();
        let view_id = graph_view_id(12);
        let pane_id = pane_id(13);

        for node in [first, second] {
            reduce_workspace_routing_intent(
                &mut workspace,
                WorkspaceRoutingIntent::RouteNodeToPane {
                    view_id,
                    frame_id: None,
                    pane_id,
                    node,
                    surface_host: None,
                },
            )
            .unwrap();
        }

        let panes = &workspace.views.graph_views[&view_id].panes;
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].node, second);
    }

    #[test]
    fn remove_pane_route_retires_surface() {
        let mut workspace = GraphWorkspace::new();
        let node = add_node(&mut workspace, 20);
        workspace.drain_effects();
        let view_id = graph_view_id(21);
        let pane_id = pane_id(22);
        let host = SurfaceHostId::new("desktop");
        reduce_workspace_routing_intent(
            &mut workspace,
            WorkspaceRoutingIntent::RouteNodeToPane {
                view_id,
                frame_id: None,
                pane_id,
                node,
                surface_host: Some(host.clone()),
            },
        )
        .unwrap();
        workspace.drain_effects();

        let outcome = reduce_workspace_routing_intent(
            &mut workspace,
            WorkspaceRoutingIntent::RemovePaneRoute {
                view_id,
                frame_id: None,
                pane_id,
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert!(workspace.views.graph_views[&view_id].panes.is_empty());
        let effects = workspace.drain_effects();
        let WorkspaceEffect::RequestSurface(effect) = &effects[0] else {
            panic!("expected retire effect");
        };
        assert_eq!(effect.host, host);
        assert_eq!(effect.request, SurfaceRequest::Retire);
    }

    #[test]
    fn missing_node_route_is_rejected_without_view_creation() {
        let mut workspace = GraphWorkspace::new();
        let view_id = graph_view_id(30);
        let missing = NodeKey::new(999);

        let error = reduce_workspace_routing_intent(
            &mut workspace,
            WorkspaceRoutingIntent::RouteNodeToPane {
                view_id,
                frame_id: None,
                pane_id: pane_id(31),
                node: missing,
                surface_host: None,
            },
        )
        .unwrap_err();

        assert_eq!(error, WorkspaceRoutingError::MissingNode(missing));
        assert!(!workspace.views.graph_views.contains_key(&view_id));
    }
}
