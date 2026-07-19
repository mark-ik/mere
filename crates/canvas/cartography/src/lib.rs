// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # Cartography
//!
//! Non-destructive projection layer for the
//! [Mere](https://crates.io/crates/mere) browser.
//!
//! Cartography sits between **graph truth** + **intelligence signals**
//! on the input side, and **canvas swatches** on the output side. It
//! owns the *contracts* — the [`LayoutStrategy`] trait, the
//! [`Projection`] / [`Overlay`] / [`MinimapDescriptor`] vocabulary, and
//! the [`IntelligenceSignals`] narrow shape that firewalls cartography
//! from `embed`' internals. The strategies
//! themselves live in sibling crates (graph-layout, document-layout,
//! …).
//!
//! ## The strategy contract
//!
//! Cartography exposes the [`LayoutStrategy`] trait: one-shot, stateless
//! analytic projection. Picks: Phyllotaxis, Penrose, Radial, Grid,
//! Timeline, Kanban, L-system, Spectral, SemanticEmbedding. `project()`
//! produces a final projection in one call. Live force physics
//! (force-directed, the affinity force) is seiche's domain; the old
//! streaming-strategy contract was retired with the `SemanticEdgeWeight`
//! projection once the seiche affinity force reached parity.
//!
//! Strategies emit a [`Projection`]
//! output type so canvases consume one shape uniformly. See the
//! [cartography layer brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
//! for the full design.
//!
//! ## The framing
//!
//! Inputs:
//!
//! - [`kernel::graph::Graph`] — read-only reference.
//! - [`IntelligenceSignals`] — clusters, affinity, hot regions, bridge
//!   nodes, importance hints. Produced by `embed`
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
pub use strategy::LayoutStrategy;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
