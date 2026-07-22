// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters that wrap this crate's `Layout<N>` impls as cartography
//! [`cartography::LayoutStrategy`] implementations.
//!
//! Each adapter is a thin translation layer:
//!
//! 1. Read a [`cartography::request::ProjectionRequest`] (graph + signals + intent).
//! 2. Build the layout's [`crate::scene::CanvasSceneInput`] /
//!    [`crate::camera::CanvasViewport`] / [`crate::LayoutExtras`] from the
//!    request — or, for analytic strategies, compute target positions
//!    directly from node ordinals.
//! 3. Call the layout (analytic adapters compute target positions via the
//!    origin trick in [`shared`]).
//! 4. Translate output into a [`cartography::projection::Projection`].
//!
//! Positions live in the adapter's `State`, not in the graph itself —
//! cartography contracts forbid mutating graph truth. The caller commits
//! positions back to the graph if/when it wants them persisted.
//!
//! ## Adapters
//!
//! - Analytic (closed-form, no iteration): [`GridAdapter`],
//!   [`PhyllotaxisAdapter`], [`RadialAdapter`], [`PenroseAdapter`],
//!   [`LSystemAdapter`], [`TimelineAdapter`], [`KanbanAdapter`],
//!   [`SpectralAdapter`], [`SemanticEmbeddingAdapter`].
//!
//! Live force physics (force-directed, Barnes-Hut, the pairwise affinity force)
//! is `seiche`'s domain, not an arrangement adapter. The old streaming
//! `SemanticEdgeWeightAdapter` (similarity-driven iterative projection) was
//! retired once `seiche`'s `AffinitySpring` reached parity:
//! affinity now clusters at seiche's cost rather than as a projection. (Graph
//! signals — P4.)

pub mod grid;
pub mod kanban;
pub mod lsystem;
pub mod penrose;
pub mod phyllotaxis;
pub mod radial;
pub mod semantic_embedding;
pub mod shared;
pub mod spectral;
pub mod timeline;

pub use grid::GridAdapter;
pub use kanban::KanbanAdapter;
pub use lsystem::LSystemAdapter;
pub use penrose::PenroseAdapter;
pub use phyllotaxis::{PhyllotaxisAdapter, SpiralOrdering};
pub use radial::RadialAdapter;
pub use semantic_embedding::SemanticEmbeddingAdapter;
pub use spectral::SpectralAdapter;
pub use timeline::TimelineAdapter;
