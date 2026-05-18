/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # graph-layout
//!
//! Graph layout algorithms that operate on `CanvasSceneInput` snapshots
//! and return per-node position deltas for the host to apply.
//!
//! Extracted from `graph_canvas::layout` per the [cartography layer
//! brief](../../../../design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
//! §9 step 4 — the sibling-crate move. `graph-canvas` is now the
//! renderer; this crate is the layout subsystem. `graph-canvas` still
//! defines the `CanvasViewport` / `CanvasSceneInput` primitive shapes
//! the layout impls operate on; this crate depends on `graph-canvas`
//! one-way for those primitives.
//!
//! The `Layout<N>` trait is delta-returning (not mutating): each
//! `step()` reads the current scene, advances internal state by `dt`,
//! and returns a map of node id to displacement. The host writes
//! those deltas back to its own position store (petgraph in
//! graphshell proper; other carriers for future hosts).
//!
//! This shape is framework-agnostic, allocation-visible, and
//! WASM-clean — no `std::time`, no egui, no petgraph.
//!
//! Cartography adapters wrapping each `Layout<N>` impl with the
//! `LayoutStrategy` / `StreamingLayoutStrategy` contracts live in
//! [`adapters`]. They retired cartography's prior `graph-canvas-
//! adapters` feature gate; consumers depend on `graph-layout`
//! directly to opt in.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use euclid::default::Vector2D;
use serde::{Deserialize, Serialize};

use graph_canvas::camera::CanvasViewport;
use graph_canvas::scene::CanvasSceneInput;

pub mod barnes_hut;
pub mod curves;
pub mod extras;
pub mod force_directed;
pub mod static_layouts;

pub use barnes_hut::{BarnesHut, BarnesHutConfig};
pub use curves::{DegreeWeighting, Falloff, ProximityFalloff, SimilarityCurve};
pub use extras::{
    DegreeRepulsion, DegreeRepulsionConfig, DomainClustering, DomainClusteringConfig,
    FrameAffinity, FrameAffinityConfig, FrameRegion, HubPull, HubPullConfig, SemanticClustering,
    SemanticClusteringConfig, StatelessPassState, TargetPolicy,
};
pub use force_directed::{ForceDirected, ForceDirectedState};
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

pub mod physics_config;
pub use physics_config::{
    GraphPhysicsTuning, apply_graph_physics_tuning, default_graph_physics_state,
};

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

/// Out-of-band inputs that a layout step may consume.
///
/// Computed by the host ahead of time; passed by reference to every step.
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

    /// Registrable-domain grouping per node. Used by `DomainClustering` to
    /// pull same-domain members toward a shared centroid. Nodes absent from
    /// the map are treated as unclustered.
    pub domain_by_node: HashMap<N, String>,

    /// Precomputed pairwise semantic similarity in `[0.0, 1.0]`. Keys are
    /// unordered pairs — store both `(a, b)` and `(b, a)` if callers want
    /// asymmetric lookups, or keep one order and have the reader normalize.
    /// Used by `SemanticClustering` and `SemanticEdgeWeight`.
    pub semantic_similarity: HashMap<(N, N), f32>,

    /// Frame-affinity regions derived from the host's arrangement relations.
    /// Each region is an anchor with a set of member nodes and a centroid;
    /// members are pulled toward the centroid. Used by `FrameAffinity`.
    pub frame_regions: Vec<FrameRegion<N>>,

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
    /// Persistent-world physics adapters use this to drive the body
    /// kinematically for the duration of the drag while preserving
    /// momentum when the drag concludes. Layouts that don't care about
    /// drag state ignore this slot.
    pub dragging: HashSet<N>,
}

/// A graph layout that advances node positions one step at a time.
///
/// The layout does not own or mutate the scene; it reads it. Positions are
/// applied by the host via the returned delta map. Nodes absent from the
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
