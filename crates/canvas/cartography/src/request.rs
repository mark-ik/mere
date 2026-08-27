// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Request side: graph + signals + intent → projection request.

use std::collections::HashMap;

use kernel::geometry::PortableSize;
use kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};

use crate::signals::IntelligenceSignals;

/// Inputs to a single projection call.
///
/// The borrow lifetimes communicate the contract: cartography sees
/// graph truth and intelligence signals as immutable references; it
/// never mutates either. The view intent is owned because it carries
/// per-call configuration (target size, focus node, filter), not
/// long-lived state.
///
/// Not `Debug` because `Graph` is not `Debug` (its `petgraph` backing
/// type is opaque on purpose). Consumers wanting to log a request
/// should log the `intent` and signal-presence flags separately.
#[derive(Clone)]
pub struct ProjectionRequest<'a> {
    pub graph: &'a kernel::graph::Graph,
    pub signals: &'a IntelligenceSignals,
    pub intent: ViewIntent,
}

/// What the user is trying to see right now.
///
/// `ViewIntent` is the **only** call-site way to tell a strategy what
/// kind of map it should produce. Strategies should treat it as
/// declarative: the same intent should yield the same projection shape
/// for the same `(graph, signals)` pair.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewIntent {
    pub form_factor: FormFactor,
    pub dimension: ProjectionDimension,
    pub focus: Option<NodeKey>,
    pub filter: Option<NodeFilter>,
    pub target_size: TargetSize,
    /// Per-node axis values for axial strategies (Timeline, Kanban).
    /// Lives on the intent (not on [`IntelligenceSignals`]) because
    /// "this node's place on the time axis is X" is a *view-config*
    /// decision about how to map graph nodes to an axis, not an
    /// emergent statistical signal.
    #[serde(default)]
    pub axis_values: Option<HashMap<NodeKey, AxisValue>>,
    /// Per-node visual extents `(w, h)` in target-size units — what each
    /// node's representation occupies, so extent-aware strategies can
    /// space placements to clear them (the projection-proofs P1 finding:
    /// a position-only contract cannot avoid overlap). View-config like
    /// `axis_values`: the host measures, the strategy places. Strategies
    /// that ignore extents behave as before (additive contract).
    #[serde(default)]
    pub extents: Option<HashMap<NodeKey, (f32, f32)>>,
}

/// Host-provided axis coordinate for axial strategies that project onto
/// one or two explicit axes (Timeline, Kanban, future axial variants).
///
/// Mirrors [`graph_layout::AxisValue`] so the cartography
/// contract stays decoupled from graph-canvas's internals; the
/// adapter for each axial strategy maps between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AxisValue {
    /// Numeric coordinate. Ordered relatively; strategies map to world
    /// units via their own scale config.
    Numeric(f64),
    /// Categorical tag. Groups nodes into buckets by tag; strategies
    /// pick bucket ordering from their config.
    Categorical(String),
}

/// What surface the projection is destined for.
///
/// Strategies can specialize on this — a `Minimap` form factor should
/// strip detail overlays; a `Volvelle` form factor should prefer
/// radial layouts; an `Astroid` form factor expects hub-collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FormFactor {
    /// Default: a full-size graph view (orrery root, workbench tile).
    #[default]
    Canvas,
    /// The cosmos form: a whole mere seen at once, special-cased so
    /// strategies can emphasize map-shape over network-shape. See lexicon:
    /// *orrery* (a form factor, narrowed 2026-07-26; the tier-1 sense it used
    /// to carry is now *your root mere*).
    Orrery,
    /// Expanded-moot radial form. See lexicon: *volvelle*.
    Volvelle,
    /// Hub-collapsed view. See lexicon: *astroid*.
    Astroid,
    /// Thumbnail/minimap projection. Strategies strip detail and emit
    /// a [`crate::MinimapDescriptor`] on the resulting
    /// [`crate::Projection`].
    Minimap,
}

/// Which projection dimension to render in.
///
/// `(x, y)` is identical across all variants; the `z` axis (when
/// applicable) is metadata-driven, not strategy-driven. This is the
/// *position parity contract* inherited from the graphshell field
/// algebra plan and the Mere field algebra plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectionDimension {
    #[default]
    TwoD,
    /// 2.5D — top-down with depth illusion. `z` is decorative.
    TwoPointFive,
    /// Discrete depth layers. `z` is quantized.
    Isometric,
    /// Full reorientable 3D. `z` is semantic.
    ThreeD,
}

/// Subset filter applied before strategy projection.
///
/// Cartography is responsible for materializing the filtered subgraph
/// reference handed to the strategy; the strategy operates on the
/// already-filtered set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeFilter {
    /// Only nodes within `hops` of `center` in the undirected graph.
    Neighborhood { center: NodeKey, hops: u32 },
    /// Only nodes carrying any of the listed tag strings.
    Tagged(Vec<String>),
    /// Only the explicit node set (used for "show selection" views).
    Explicit(Vec<NodeKey>),
}

/// Target render size for the projection.
///
/// `Pixels` for canvas-bound projections, `Logical` for minimaps and
/// abstract diagrams. Strategies pick spring constants / scaling /
/// label density based on this.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum TargetSize {
    /// Default: render at a logical reference size (480×360).
    #[default]
    Default,
    /// Pixels — for direct canvas binding at known DP dimensions.
    Pixels { width: u32, height: u32 },
    /// Logical — for minimap thumbnails and resolution-independent
    /// projections.
    Logical { width: f32, height: f32 },
}

impl ViewIntent {
    /// Convenience builder for the most common case: a canvas-bound
    /// view at known pixel dimensions.
    pub fn canvas_pixels(width: u32, height: u32) -> Self {
        Self {
            form_factor: FormFactor::Canvas,
            dimension: ProjectionDimension::TwoD,
            focus: None,
            filter: None,
            target_size: TargetSize::Pixels { width, height },
            axis_values: None,
            extents: None,
        }
    }

    /// Convenience builder for a minimap.
    pub fn minimap(width: f32, height: f32) -> Self {
        Self {
            form_factor: FormFactor::Minimap,
            dimension: ProjectionDimension::TwoD,
            focus: None,
            filter: None,
            target_size: TargetSize::Logical { width, height },
            axis_values: None,
            extents: None,
        }
    }

    pub fn with_focus(mut self, focus: NodeKey) -> Self {
        self.focus = Some(focus);
        self
    }

    pub fn with_filter(mut self, filter: NodeFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_axis_values(mut self, values: HashMap<NodeKey, AxisValue>) -> Self {
        self.axis_values = Some(values);
        self
    }
}

impl TargetSize {
    /// Logical-unit dimensions for whichever variant we are.
    /// Pixel-bound sizes round to f32 for layout math.
    pub fn logical_size(self) -> PortableSize {
        match self {
            TargetSize::Default => PortableSize::new(480.0, 360.0),
            TargetSize::Pixels { width, height } => PortableSize::new(width as f32, height as f32),
            TargetSize::Logical { width, height } => PortableSize::new(width, height),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_intent_canvas_pixels_builder_sets_pixel_target() {
        let intent = ViewIntent::canvas_pixels(800, 600);
        assert_eq!(intent.form_factor, FormFactor::Canvas);
        assert_eq!(intent.dimension, ProjectionDimension::TwoD);
        assert!(intent.focus.is_none());
        assert_eq!(
            intent.target_size,
            TargetSize::Pixels {
                width: 800,
                height: 600
            }
        );
    }

    #[test]
    fn view_intent_minimap_builder_sets_minimap_form_factor() {
        let intent = ViewIntent::minimap(120.0, 90.0);
        assert_eq!(intent.form_factor, FormFactor::Minimap);
        assert_eq!(
            intent.target_size,
            TargetSize::Logical {
                width: 120.0,
                height: 90.0
            }
        );
    }

    #[test]
    fn target_size_default_logical_size_is_reference() {
        let s = TargetSize::Default.logical_size();
        assert_eq!(s.width, 480.0);
        assert_eq!(s.height, 360.0);
    }
}
