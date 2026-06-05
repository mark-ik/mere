/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Adapters that wrap `graph-canvas`'s existing `Layout<N>` impls as
//! cartography [`crate::StreamingLayoutStrategy`] / [`crate::LayoutStrategy`].
//!
//! Each adapter is a thin translation layer:
//!
//! 1. Read a [`crate::ProjectionRequest`] (graph + signals + intent).
//! 2. Build graph-canvas-shaped inputs (`CanvasSceneInput`,
//!    `CanvasViewport`, `LayoutExtras`) from the request — or, for
//!    analytic strategies, compute target positions directly from
//!    node ordinals.
//! 3. Call the existing layout (`Layout::step()` for streaming
//!    adapters; analytic adapters bypass it).
//! 4. Translate output into a [`crate::Projection`].
//!
//! Positions live in the adapter's `State`, not in the graph itself —
//! cartography contracts forbid mutating graph truth. The canvas
//! caller is responsible for committing positions back to the graph
//! if/when it wants them persisted.
//!
//! ## Current adapters
//!
//! - [`ForceDirectedAdapter`] — wraps
//!   [`canvas_ir::layout::ForceDirected`] as a
//!   [`crate::StreamingLayoutStrategy`].
//! - [`PhyllotaxisAdapter`] — wraps
//!   [`canvas_ir::layout::Phyllotaxis`] as a
//!   [`crate::LayoutStrategy`] (analytic — closed-form target
//!   positions, no iteration).
//!
//! Future adapters land here for `BarnesHut`, `SemanticEmbedding`,
//! `SemanticEdgeWeight` (streaming) and `Penrose`, `Radial`, `Grid`,
//! `Timeline`, `Kanban`, `LSystem` (analytic).

pub mod barnes_hut;
pub mod force_directed;
pub mod grid;
pub mod kanban;
pub mod lsystem;
pub mod penrose;
pub mod phyllotaxis;
pub mod radial;
pub mod semantic_edge_weight;
pub mod semantic_embedding;
pub mod shared;
pub mod timeline;

pub use barnes_hut::{BarnesHutAdapter, BarnesHutAdapterState};
pub use force_directed::{ForceDirectedAdapter, ForceDirectedAdapterState};
pub use grid::GridAdapter;
pub use kanban::KanbanAdapter;
pub use lsystem::LSystemAdapter;
pub use penrose::PenroseAdapter;
pub use phyllotaxis::PhyllotaxisAdapter;
pub use radial::RadialAdapter;
pub use semantic_edge_weight::{SemanticEdgeWeightAdapter, SemanticEdgeWeightAdapterState};
pub use semantic_embedding::SemanticEmbeddingAdapter;
pub use shared::run_one_step;
pub use timeline::TimelineAdapter;
