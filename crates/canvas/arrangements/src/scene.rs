// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light scene snapshot the layouts read.
//!
//! These are the trimmed descendants of the retired `canvas-ir` scene
//! types: just the node/edge geometry a `Layout<N>` needs to place nodes.
//! The renderer path (netrender) carries its own richer scene model, so
//! the draw-side fields (scene objects, overlays, scene mode, projection,
//! per-edge colour/tag) that the old `CanvasSceneInput` carried are gone.

use euclid::default::Point2D;
use serde::{Deserialize, Serialize};

/// A graph node prepared for a layout pass.
///
/// Generic over `N`, the node identifier type. The caller decides what `N`
/// is (Mere uses `kernel::graph::NodeKey`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode<N> {
    /// Node identifier.
    pub id: N,
    /// Position in canonical 2D layout space.
    pub position: Point2D<f32>,
    /// Display radius in world units.
    pub radius: f32,
    /// Optional label (some layouts size buckets by it).
    #[serde(default)]
    pub label: Option<String>,
}

/// A graph edge prepared for a layout pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEdge<N> {
    /// Source node identifier.
    pub source: N,
    /// Target node identifier.
    pub target: N,
    /// Edge weight (spring strength for force-aware layouts).
    pub weight: f32,
}

impl<N> CanvasEdge<N> {
    /// Construct an edge with the default weight (1.0) — the common shape
    /// for layouts that only care about graph topology.
    pub fn untagged(source: N, target: N) -> Self {
        Self {
            source,
            target,
            weight: 1.0,
        }
    }
}

/// The scene snapshot a layout reads: nodes with positions + edges.
///
/// Positions are the *working* positions the caller is iterating (not graph
/// truth) — analytic layouts ignore them, streaming layouts read them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasSceneInput<N> {
    pub nodes: Vec<CanvasNode<N>>,
    pub edges: Vec<CanvasEdge<N>>,
}

impl<N> Default for CanvasSceneInput<N> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}
