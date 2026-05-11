/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projection: the output handed to canvas swatches.

use mere_kernel::geometry::{PortablePoint, PortableRect};
use mere_kernel::graph::{EdgeKey, NodeKey};
use serde::{Deserialize, Serialize};

use crate::minimap::MinimapDescriptor;
use crate::overlay::Overlay;

/// Output of a strategy projection.
///
/// Carries everything a canvas swatch needs to render the result:
/// positioned nodes, positioned edges, overlay vocabulary, and an
/// optional minimap descriptor.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Projection {
    pub nodes: Vec<PositionedNode>,
    pub edges: Vec<PositionedEdge>,
    pub overlays: Vec<Overlay>,
    pub minimap: Option<MinimapDescriptor>,
    /// Total content bounds for this projection. May exceed the
    /// requested `TargetSize` (canvas decides scroll/zoom strategy).
    pub content_bounds: PortableRect,
    /// Strategy-specific metadata. Free-form to keep the contract
    /// open to strategies that need to communicate state out-of-band.
    pub metadata: ProjectionMetadata,
}

impl Projection {
    /// An empty projection — what strategies should return for an
    /// empty graph or fully-filtered-out input.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Per-projection metadata that doesn't fit elsewhere.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectionMetadata {
    /// `projection_id()` of the strategy that produced this
    /// projection. Set by the strategy; consumers can inspect to
    /// route per-strategy debug overlays.
    pub strategy_id: Option<String>,
    /// Whether the strategy considers this projection settled
    /// (force-directed converged, tree fully placed, etc.).
    /// Hosts can use this to suppress redundant re-projection.
    pub settled: bool,
}

/// One node with a positioned point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionedNode {
    pub node: NodeKey,
    pub position: PortablePoint,
    /// Suggested visual radius in projection-space units. Strategies
    /// that don't care about radius emit 0.0; canvases substitute a
    /// theme-default.
    pub radius: f32,
}

/// One edge with a positioned start/end. Edges may pass through
/// additional waypoints for routed strategies (orthogonal routing,
/// bundled edges) — represented here as a polyline; strategies that
/// don't route emit just `[from, to]`.
///
/// `edge` is `Option<EdgeKey>` because some strategies derive edges
/// from node identity rather than from a stable graph edge (e.g.
/// analytic strategies that draw spring connections without going
/// through the graph's edge table; the kernel graph's `edges()`
/// iterator yields `EdgeView`s that don't carry an `EdgeKey` because
/// one `EdgeKey` can project to multiple `EdgeView`s by edge-kind).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionedEdge {
    pub edge: Option<EdgeKey>,
    pub from: NodeKey,
    pub to: NodeKey,
    pub path: Vec<PortablePoint>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_empty_has_no_geometry() {
        let p = Projection::empty();
        assert!(p.nodes.is_empty());
        assert!(p.edges.is_empty());
        assert!(p.overlays.is_empty());
        assert!(p.minimap.is_none());
    }
}
