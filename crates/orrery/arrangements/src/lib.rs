/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # arrangements
//!
//! Deterministic graph arrangements — the configurable
//! ArrangementRelation design space. Each layout reads a light
//! [`scene::CanvasSceneInput`] snapshot and returns per-node position
//! deltas for the caller to apply.
//!
//! The catalog is the *non-physics* half of graph layout: aperiodic
//! tilings ([`penrose`]), fractal paths ([`l_system`]), spirals and
//! grids ([`static_layouts`]), axial boards/timelines ([`axial`]), and
//! semantic projections ([`semantic_embedding`]). Live force physics
//! lives in `gyre` (rapier-backed); the Barnes-Hut approximation it uses
//! for large graphs was harvested out of this crate's old
//! `force_directed`/`barnes_hut` modules.
//!
//! The [`Layout<N>`](Layout) trait is delta-returning (not mutating):
//! each `step()` reads the current scene, advances internal state by
//! `dt`, and returns a map of node id to displacement. The caller writes
//! those deltas back to its own position store.
//!
//! This shape is framework-agnostic, allocation-visible, and WASM-clean —
//! no `std::time`, no egui, no petgraph.
//!
//! Cartography adapters wrapping each `Layout<N>` impl with the
//! [`cartography::LayoutStrategy`] / [`cartography::StreamingLayoutStrategy`]
//! contracts live in [`adapters`]; consumers depend on `arrangements`
//! directly to opt in.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use euclid::default::Vector2D;
use serde::{Deserialize, Serialize};

use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

pub mod camera;
pub mod scene;

pub mod curves;
pub use curves::{DegreeWeighting, Falloff, ProximityFalloff, SimilarityCurve};

pub mod static_layouts;
pub use static_layouts::{
    Grid, GridColumns, GridConfig, GridTraversal, Phyllotaxis, PhyllotaxisConfig,
    PhyllotaxisRadiusCurve, Radial, RadialAngularPolicy, RadialConfig, RadialUnreachablePolicy,
    SpiralOrientation, StaticLayoutState, angles,
};

pub mod semantic_embedding;
pub use semantic_embedding::{
    EmbeddingFallback, SemanticEdgeWeight, SemanticEdgeWeightConfig, SemanticEdgeWeightState,
    SemanticEmbedding, SemanticEmbeddingConfig,
};

pub mod l_system;
pub use l_system::{IterationDepth, LSystem, LSystemConfig, LSystemGrammar};

pub mod penrose;
pub use penrose::{
    NodeAssignmentStrategy, Penrose, PenroseConfig, PenroseVariant, SubdivisionCount,
    UnusedVertexPolicy,
};

pub mod axial;
pub use axial::{Kanban, KanbanConfig, Timeline, TimelineConfig};

pub mod registry;
pub use registry::{
    BuiltinProvider, DynLayout, ErasedState, LayoutCapability, LayoutCategory, LayoutId,
    LayoutProvenance, LayoutProvider, LayoutRegistry, RegisterError, register_builtins,
};

pub mod adapters;

/// A host-provided axis coordinate for layouts that project onto one or
/// two explicit axes (Timeline, Kanban, future axial variants).
#[derive(Debug, Clone, PartialEq)]
pub enum AxisValue {
    /// Numeric coordinate. Ordered relatively; layouts map to world units
    /// via their own scale config.
    Numeric(f64),
    /// Categorical tag. Groups nodes into buckets by tag; layouts use
    /// stable bucket ordering derived from config.
    Categorical(String),
}

/// Shared persistent state for stateless layout passes — the analytic
/// layouts and semantic embedding only need a step counter (they recompute
/// targets from scratch each call rather than accumulating displacement).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StatelessPassState {
    pub step_count: u64,
}

/// Out-of-band inputs that a layout step may consume.
///
/// Computed by the caller ahead of time; passed by reference to every step.
/// Extending this struct does not churn the `Layout` trait surface.
#[derive(Debug, Default, Clone)]
pub struct LayoutExtras<N>
where
    N: Clone + Eq + Hash,
{
    /// Nodes whose positions must not be moved by the layout. Pinned nodes
    /// still contribute to forces on other nodes; they just do not receive
    /// a delta themselves.
    pub pinned: HashSet<N>,

    /// Registrable-domain grouping per node. Nodes absent from the map are
    /// treated as unclustered. Read by domain-aware assignment strategies.
    pub domain_by_node: HashMap<N, String>,

    /// Precomputed pairwise semantic similarity in `[0.0, 1.0]`. Keys are
    /// unordered pairs — store both `(a, b)` and `(b, a)` if callers want
    /// asymmetric lookups, or keep one order and have the reader normalize.
    /// Used by `SemanticEdgeWeight`.
    pub semantic_similarity: HashMap<(N, N), f32>,

    /// Host-provided 2D coordinates per node (from UMAP / t-SNE / PCA /
    /// any ML pipeline). Coordinate space is arbitrary; layouts scale
    /// through their own config. Used by `SemanticEmbedding`.
    pub embedding_by_node: HashMap<N, euclid::default::Point2D<f32>>,

    /// Host-provided per-node axis coordinates for axial layouts (Timeline,
    /// Kanban, future variants). Nodes absent from the map get layout-
    /// specific fallback treatment.
    pub axis_value_by_node: HashMap<N, AxisValue>,

    /// Nodes the user is actively dragging this frame. Distinct from
    /// `pinned` (persistent user intent that a node not move) — `dragging`
    /// is transient ("user has their finger on this one right now").
    /// Layouts that don't care about drag state ignore this slot.
    pub dragging: HashSet<N>,
}

/// A graph layout that advances node positions one step at a time.
///
/// The layout does not own or mutate the scene; it reads it. Positions are
/// applied by the caller via the returned delta map. Nodes absent from the
/// returned map keep their current positions.
pub trait Layout<N>
where
    N: Clone + Eq + Hash,
{
    /// Serializable persistent state for this layout (damping history,
    /// displacement accumulators, iteration counters).
    type State: Default + Clone + Serialize + for<'de> Deserialize<'de>;

    /// Advance one frame. Returns per-node position deltas in world units.
    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        dt: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>>;

    /// True when the layout has reached a low-energy state and can be
    /// auto-paused. Default: never — caller drives explicit pause.
    fn is_converged(&self, _state: &Self::State) -> bool {
        false
    }
}
