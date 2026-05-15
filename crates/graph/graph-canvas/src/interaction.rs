/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Interaction state and canvas actions.
//!
//! The canvas emits typed `CanvasAction` values rather than mutating
//! application state directly. The host converts actions into its own
//! reducer/intent model.

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hash;

use crate::scripting::SceneObjectId;

/// Lasso selection state: an in-progress rectangular drag selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LassoState {
    /// The world-space origin where the lasso drag started.
    pub origin: Point2D<f32>,
    /// The world-space position of the current drag corner.
    pub current: Point2D<f32>,
}

/// Reference to a graph edge by its endpoint node identifiers, plus
/// an opaque caller-defined `tag` that disambiguates relations on
/// multi-relation pairs (see [`crate::packet::HitProxy::Edge::tag`]).
/// `tag = None` means "no disambiguation" — e.g. layout adapters that
/// don't carry tagged relations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeRef<N> {
    pub source: N,
    pub target: N,
    #[serde(default)]
    pub tag: Option<u32>,
}

/// Interaction state for one graph view.
///
/// Maintained by the canvas between frames. The host reads this to know what
/// is currently hovered, selected, or being dragged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionState<N: Eq + Hash> {
    pub hovered_node: Option<N>,
    pub hovered_edge: Option<EdgeRef<N>>,
    pub hovered_scene_object: Option<SceneObjectId>,
    pub selection: HashSet<N>,
    pub lasso: Option<LassoState>,
}

impl<N: Eq + Hash> Default for InteractionState<N> {
    fn default() -> Self {
        Self {
            hovered_node: None,
            hovered_edge: None,
            hovered_scene_object: None,
            selection: HashSet::new(),
            lasso: None,
        }
    }
}

/// A typed action emitted by the canvas in response to user input.
///
/// The host converts these into its own mutation model (intents, reducers,
/// etc.). The canvas never mutates graph truth directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CanvasAction<N> {
    HoverNode(Option<N>),
    HoverEdge(Option<EdgeRef<N>>),
    SelectNode(N),
    DeselectNode(N),
    ToggleSelectNode(N),
    ClearSelection,
    DragNode {
        node: N,
        delta: Vector2D<f32>,
    },
    LassoBegin {
        origin: Point2D<f32>,
    },
    LassoUpdate {
        current: Point2D<f32>,
    },
    LassoComplete {
        nodes: Vec<N>,
    },
    LassoCancel,
    PanCamera(Vector2D<f32>),
    ZoomCamera {
        factor: f32,
        focus: Point2D<f32>,
    },
    /// Seed the camera's inertial pan velocity (world units per second).
    /// Emitted on drag release when the terminal drag delta is non-trivial,
    /// so the host can `tick_inertia` each frame and coast the view.
    SetPanInertia(Vector2D<f32>),
    HoverSceneObject(Option<SceneObjectId>),
    ClickSceneObject(SceneObjectId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_state_default_is_empty() {
        let state: InteractionState<u32> = InteractionState::default();
        assert!(state.hovered_node.is_none());
        assert!(state.hovered_edge.is_none());
        assert!(state.hovered_scene_object.is_none());
        assert!(state.selection.is_empty());
        assert!(state.lasso.is_none());
    }

    #[test]
    fn serde_roundtrip_canvas_action() {
        let actions: Vec<CanvasAction<u32>> = vec![
            CanvasAction::HoverNode(Some(5)),
            CanvasAction::SelectNode(3),
            CanvasAction::DragNode {
                node: 1,
                delta: Vector2D::new(10.0, -5.0),
            },
            CanvasAction::LassoComplete {
                nodes: vec![0, 1, 2],
            },
            CanvasAction::PanCamera(Vector2D::new(-20.0, 15.0)),
            CanvasAction::ZoomCamera {
                factor: 1.5,
                focus: Point2D::new(400.0, 300.0),
            },
            CanvasAction::HoverSceneObject(Some(SceneObjectId(7))),
            CanvasAction::ClickSceneObject(SceneObjectId(3)),
        ];
        let json = serde_json::to_string(&actions).unwrap();
        let back: Vec<CanvasAction<u32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(actions, back);
    }

    #[test]
    fn serde_roundtrip_interaction_state() {
        let mut state = InteractionState::<u32>::default();
        state.hovered_node = Some(7);
        state.selection.insert(1);
        state.selection.insert(3);
        state.lasso = Some(LassoState {
            origin: Point2D::new(10.0, 20.0),
            current: Point2D::new(100.0, 150.0),
        });
        let json = serde_json::to_string(&state).unwrap();
        let back: InteractionState<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }
}
