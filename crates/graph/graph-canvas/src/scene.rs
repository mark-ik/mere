/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene input types: the data a host provides to graph-canvas for scene
//! derivation.
//!
//! These are canvas-facing derived carriers. They do not own graph truth —
//! the host converts from its graph model into these types before handing
//! them to the canvas.

use euclid::default::Point2D;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

use crate::projection::ProjectionMode;
use crate::scripting::{SceneObjectHitShape, SceneObjectId};

/// Runtime mode for the graph canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SceneMode {
    /// Read-only navigation: pan, zoom, hover, select.
    #[default]
    Browse,
    /// User can reposition nodes and edit the graph layout.
    Arrange,
    /// Physics simulation is active (Rapier-driven).
    Simulate,
}

/// A graph node prepared for the canvas.
///
/// Generic over `N`, the node identifier type. The host decides what `N` is
/// (e.g. `petgraph::graph::NodeIndex`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode<N> {
    /// Node identifier.
    pub id: N,
    /// Position in canonical 2D layout space.
    pub position: Point2D<f32>,
    /// Display radius in world units.
    pub radius: f32,
    /// Optional label for rendering.
    pub label: Option<String>,
}

/// A graph edge prepared for the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEdge<N> {
    /// Source node identifier.
    pub source: N,
    /// Target node identifier.
    pub target: N,
    /// Edge weight (used for visual thickness, opacity, etc.).
    pub weight: f32,
}

/// A scene object that is not a graph node or edge (e.g. scripted avatar,
/// prop, annotation, widget).
///
/// The host runs scripts (via Wasmtime/Extism or native code), collects their
/// output, and packs it into these structs for inclusion in
/// `CanvasSceneInput.scene_objects`. The canvas derives them into draw items
/// and hit proxies alongside nodes and edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasSceneObject {
    /// Unique identifier for this scene object.
    pub id: SceneObjectId,
    /// Position in canonical 2D layout space.
    pub position: Point2D<f32>,
    /// Draw items produced by the script. Positions are relative to the
    /// object's position (object-local coordinates).
    pub draw_items: Vec<crate::packet::SceneDrawItem>,
    /// Interaction surface shape. `None` means the object is not interactive.
    pub hit_shape: Option<SceneObjectHitShape>,
    /// Overlay items drawn on top of the scene. Positions are relative to
    /// the object's position.
    pub overlay_items: Vec<crate::packet::SceneDrawItem>,
}

/// An overlay item (e.g. debug label, selection rect, minimap indicator).
/// Placeholder for Phase 3+ overlay composition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasOverlayItem {
    pub position: Point2D<f32>,
    pub kind: String,
}

/// Opaque view identifier. The host assigns these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewId(pub u64);

/// Complete scene input packet provided by the host for one graph view.
///
/// This is the primary entry point for scene derivation. The canvas consumes
/// this and produces a `ProjectedScene`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasSceneInput<N> {
    pub view_id: ViewId,
    pub nodes: Vec<CanvasNode<N>>,
    pub edges: Vec<CanvasEdge<N>>,
    pub scene_objects: Vec<CanvasSceneObject>,
    pub overlays: Vec<CanvasOverlayItem>,
    pub scene_mode: SceneMode,
    pub projection: ProjectionMode,
}

impl<N: Default> Default for CanvasSceneInput<N> {
    fn default() -> Self {
        Self {
            view_id: ViewId(0),
            nodes: Vec::new(),
            edges: Vec::new(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::default(),
            projection: ProjectionMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{Color, SceneDrawItem};

    #[test]
    fn scene_mode_default_is_browse() {
        assert_eq!(SceneMode::default(), SceneMode::Browse);
    }

    #[test]
    fn canvas_node_construction() {
        let node = CanvasNode {
            id: 42u32,
            position: Point2D::new(100.0, 200.0),
            radius: 16.0,
            label: Some("example".into()),
        };
        assert_eq!(node.id, 42);
        assert_eq!(node.radius, 16.0);
    }

    #[test]
    fn canvas_scene_object_construction() {
        let obj = CanvasSceneObject {
            id: SceneObjectId(1),
            position: Point2D::new(50.0, 50.0),
            draw_items: vec![SceneDrawItem::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                fill: Color::new(1.0, 0.0, 0.0, 1.0),
                stroke: None,
            }],
            hit_shape: Some(SceneObjectHitShape::Circle { radius: 12.0 }),
            overlay_items: vec![],
        };
        assert_eq!(obj.id, SceneObjectId(1));
        assert!(obj.hit_shape.is_some());
    }

    #[test]
    fn serde_roundtrip_scene_input() {
        let input = CanvasSceneInput {
            view_id: ViewId(1),
            nodes: vec![
                CanvasNode {
                    id: 0u32,
                    position: Point2D::new(10.0, 20.0),
                    radius: 8.0,
                    label: None,
                },
                CanvasNode {
                    id: 1,
                    position: Point2D::new(50.0, 60.0),
                    radius: 12.0,
                    label: Some("node-1".into()),
                },
            ],
            edges: vec![CanvasEdge {
                source: 0,
                target: 1,
                weight: 1.0,
            }],
            scene_objects: vec![CanvasSceneObject {
                id: SceneObjectId(10),
                position: Point2D::new(30.0, 40.0),
                draw_items: vec![],
                hit_shape: None,
                overlay_items: vec![],
            }],
            overlays: vec![],
            scene_mode: SceneMode::Arrange,
            projection: ProjectionMode::TwoD,
        };
        let json = serde_json::to_string(&input).unwrap();
        let back: CanvasSceneInput<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(input, back);
    }
}
