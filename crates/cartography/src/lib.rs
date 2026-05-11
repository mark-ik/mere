/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # Cartography
//!
//! Non-destructive projection layer for the
//! [Mere](https://crates.io/crates/mere) browser.
//!
//! Cartography sits between **graph truth** + **intelligence signals**
//! on the input side, and **canvas swatches** on the output side. It
//! owns the *contracts* — the [`LayoutStrategy`] /
//! [`StreamingLayoutStrategy`] traits, the [`Projection`] /
//! [`Overlay`] / [`MinimapDescriptor`] vocabulary, and the
//! [`IntelligenceSignals`] narrow shape that firewalls cartography
//! from `intelligence-embeddings`' internals. The strategies
//! themselves live in sibling crates (graph-layout, document-layout,
//! …).
//!
//! ## Dual strategy contract
//!
//! Cartography exposes **two** strategy traits because two kinds of
//! layout exist:
//!
//! - **Analytic** ([`LayoutStrategy`]) — one-shot, stateless. Picks:
//!   Phyllotaxis, Penrose, Radial, Grid, Timeline, Kanban, L-system,
//!   ClusterCollapsed (astroid). `project()` produces a final
//!   projection in one call.
//! - **Streaming** ([`StreamingLayoutStrategy`]) — iterative, state-
//!   carrying. Picks: ForceDirected, BarnesHut, SemanticEmbedding,
//!   any algorithm that converges over multiple frames.
//!
//! Strategies pick which trait fits. Both emit the same [`Projection`]
//! output type so canvases consume one shape uniformly. See the
//! [cartography layer brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
//! for the full design.
//!
//! ## The framing
//!
//! Inputs:
//!
//! - [`mere_kernel::graph::Graph`] — read-only reference.
//! - [`IntelligenceSignals`] — clusters, affinity, hot regions, bridge
//!   nodes, importance hints. Produced by `intelligence-embeddings`
//!   and consumed through this narrow contract type, not a direct
//!   dependency on the crate's internals.
//! - [`ViewIntent`] — what the user is trying to see right now: scale,
//!   dimension, focus, filter, form factor (orrery root, workbench
//!   swatch, volvelle radial, astroid hub-collapse, minimap thumbnail).
//!
//! Outputs:
//!
//! - [`Projection`] — positioned nodes + edges + overlays at a chosen
//!   layout, ready for a canvas swatch to render.
//! - [`Overlay`] variants — semantic emphases canvases apply on top of
//!   geometry (cluster halos, edge weights, activity heat, bridge
//!   emphasis, importance scaling).
//! - [`MinimapDescriptor`] — thumbnail-scale projection of any swatch.
//!
//! The graph stays canonical. Cartography is *representation*, not
//! truth.
//!
//! ## Status
//!
//! Pre-1.0. v0 ships contract types only — no strategy
//! implementations, no canvas integration.

#![doc(html_root_url = "https://docs.rs/cartography/0.0.1")]

pub mod adapters;
pub mod minimap;
pub mod overlay;
pub mod projection;
pub mod request;
pub mod signals;
pub mod strategy;

pub use minimap::{MinimapDescriptor, MinimapOverlayKind};
pub use overlay::Overlay;
pub use projection::{PositionedEdge, PositionedNode, Projection, ProjectionMetadata};
pub use request::{
    AxisValue, FormFactor, NodeFilter, ProjectionDimension, ProjectionRequest, TargetSize,
    ViewIntent,
};
pub use signals::{
    AffinityScores, BridgeNodes, Cluster, ClusterSet, ImportanceWeights, IntelligenceSignals,
    NodeEmbeddings,
};
pub use strategy::{LayoutStrategy, StreamingLayoutStrategy};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
